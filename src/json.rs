//! JSON, lu puis réécrit **exactement** comme Python l'écrit.
//!
//! ## Pourquoi un analyseur écrit à la main
//!
//! Le registre est chaîné sur une forme canonique :
//!
//! ```text
//! json.dumps(corps, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
//! ```
//!
//! Vérifier une chaîne, c'est donc **reproduire cette chaîne d'octets**. Une
//! bibliothèque générique lit le JSON très bien et le réécrit à sa manière —
//! ce qui suffit à faire échouer la vérification d'un registre intact. Le
//! travail n'est pas d'analyser du JSON, il est de le rendre à l'identique.
//!
//! ## Là où ça se joue : les flottants
//!
//! Le registre contient `2.676e-05` et `6.659999999999999e-05`. Rust écrirait
//! `0.00002676` avec `{}`, et `2.676e-5` avec `{:e}` — un chiffre d'exposant,
//! sans signe. Trois formes, un seul octet correct.
//!
//! Les règles de Python, **relevées sur l'interpréteur le 31/10/2026** plutôt
//! que supposées :
//!
//! | valeur | sortie |
//! |---|---|
//! | `1e15` | `1000000000000000.0` |
//! | `1e16` | `1e+16` |
//! | `1e-4` | `0.0001` |
//! | `1e-5` | `1e-05` |
//! | `1e100` | `1e+100` |
//! | `1.0` | `1.0` |
//!
//! Donc : notation scientifique si l'exposant décimal est **≥ 16 ou ≤ −5** ;
//! exposant signé sur **au moins deux chiffres** ; et `.0` ajouté aux
//! flottants entiers rendus en notation fixe.

use std::collections::BTreeMap;
use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq)]
pub enum Valeur {
    Nul,
    Bool(bool),
    /// Les entiers restent entiers : `3` et `3.0` ne s'écrivent pas pareil,
    /// et confondre les deux casserait une empreinte sur un champ `calls`.
    Entier(i64),
    Flottant(f64),
    Texte(String),
    Liste(Vec<Valeur>),
    /// `BTreeMap` et non `HashMap` : les clés sortent triées, ce que
    /// `sort_keys=True` exige, sans qu'on ait à y penser à l'écriture.
    Objet(BTreeMap<String, Valeur>),
}

// ── Lecture ──────────────────────────────────────────────────────────────

pub struct Lecteur<'a> {
    octets: &'a [u8],
    i: usize,
}

pub fn lire(texte: &str) -> Result<Valeur, String> {
    let mut l = Lecteur { octets: texte.as_bytes(), i: 0 };
    l.sauter_blancs();
    let v = l.valeur()?;
    l.sauter_blancs();
    if l.i != l.octets.len() {
        return Err(format!("du texte suit la valeur, à l'octet {}", l.i));
    }
    Ok(v)
}

impl<'a> Lecteur<'a> {
    fn sauter_blancs(&mut self) {
        while self.i < self.octets.len()
            && matches!(self.octets[self.i], b' ' | b'\t' | b'\n' | b'\r')
        {
            self.i += 1;
        }
    }

    fn octet(&self) -> Result<u8, String> {
        self.octets
            .get(self.i)
            .copied()
            .ok_or_else(|| "fin de texte inattendue".to_string())
    }

    fn attendre(&mut self, c: u8) -> Result<(), String> {
        if self.octet()? != c {
            return Err(format!(
                "attendu {:?} à l'octet {}, trouvé {:?}",
                c as char, self.i, self.octet()? as char
            ));
        }
        self.i += 1;
        Ok(())
    }

    fn valeur(&mut self) -> Result<Valeur, String> {
        match self.octet()? {
            b'{' => self.objet(),
            b'[' => self.liste(),
            b'"' => Ok(Valeur::Texte(self.texte()?)),
            b't' => self.mot("true").map(|_| Valeur::Bool(true)),
            b'f' => self.mot("false").map(|_| Valeur::Bool(false)),
            b'n' => self.mot("null").map(|_| Valeur::Nul),
            _ => self.nombre(),
        }
    }

    fn mot(&mut self, attendu: &str) -> Result<(), String> {
        if self.octets[self.i..].starts_with(attendu.as_bytes()) {
            self.i += attendu.len();
            Ok(())
        } else {
            Err(format!("attendu {attendu} à l'octet {}", self.i))
        }
    }

    fn objet(&mut self) -> Result<Valeur, String> {
        self.attendre(b'{')?;
        let mut m = BTreeMap::new();
        self.sauter_blancs();
        if self.octet()? == b'}' {
            self.i += 1;
            return Ok(Valeur::Objet(m));
        }
        loop {
            self.sauter_blancs();
            let cle = self.texte()?;
            self.sauter_blancs();
            self.attendre(b':')?;
            self.sauter_blancs();
            let v = self.valeur()?;
            m.insert(cle, v);
            self.sauter_blancs();
            match self.octet()? {
                b',' => self.i += 1,
                b'}' => {
                    self.i += 1;
                    return Ok(Valeur::Objet(m));
                }
                autre => return Err(format!("attendu , ou }} , trouvé {:?}", autre as char)),
            }
        }
    }

    fn liste(&mut self) -> Result<Valeur, String> {
        self.attendre(b'[')?;
        let mut v = Vec::new();
        self.sauter_blancs();
        if self.octet()? == b']' {
            self.i += 1;
            return Ok(Valeur::Liste(v));
        }
        loop {
            self.sauter_blancs();
            v.push(self.valeur()?);
            self.sauter_blancs();
            match self.octet()? {
                b',' => self.i += 1,
                b']' => {
                    self.i += 1;
                    return Ok(Valeur::Liste(v));
                }
                autre => return Err(format!("attendu , ou ] , trouvé {:?}", autre as char)),
            }
        }
    }

    fn texte(&mut self) -> Result<String, String> {
        self.attendre(b'"')?;
        let mut s = String::new();
        loop {
            let c = self.octet()?;
            self.i += 1;
            match c {
                b'"' => return Ok(s),
                b'\\' => {
                    let e = self.octet()?;
                    self.i += 1;
                    match e {
                        b'"' => s.push('"'),
                        b'\\' => s.push('\\'),
                        b'/' => s.push('/'),
                        b'b' => s.push('\u{8}'),
                        b'f' => s.push('\u{c}'),
                        b'n' => s.push('\n'),
                        b'r' => s.push('\r'),
                        b't' => s.push('\t'),
                        b'u' => {
                            let point = self.quatre_hex()?;
                            // Les paires de substitution UTF-16 : un
                            // caractère hors du plan de base arrive en deux
                            // échappements, et les recoller est la seule
                            // façon de retrouver le texte d'origine.
                            if (0xD800..0xDC00).contains(&point) {
                                self.attendre(b'\\')?;
                                self.attendre(b'u')?;
                                let bas = self.quatre_hex()?;
                                let complet = 0x10000
                                    + ((point - 0xD800) << 10)
                                    + (bas - 0xDC00);
                                s.push(
                                    char::from_u32(complet)
                                        .ok_or("paire de substitution invalide")?,
                                );
                            } else {
                                s.push(
                                    char::from_u32(point)
                                        .ok_or("point de code invalide")?,
                                );
                            }
                        }
                        autre => {
                            return Err(format!("échappement inconnu \\{}", autre as char))
                        }
                    }
                }
                _ => {
                    // Octet brut : on recompose le caractère UTF-8 entier.
                    let debut = self.i - 1;
                    let taille = match c {
                        0x00..=0x7F => 1,
                        0xC0..=0xDF => 2,
                        0xE0..=0xEF => 3,
                        _ => 4,
                    };
                    self.i = debut + taille;
                    let morceau = std::str::from_utf8(&self.octets[debut..self.i])
                        .map_err(|e| format!("UTF-8 invalide : {e}"))?;
                    s.push_str(morceau);
                }
            }
        }
    }

    fn quatre_hex(&mut self) -> Result<u32, String> {
        let fin = self.i + 4;
        let s = std::str::from_utf8(
            self.octets.get(self.i..fin).ok_or("échappement \\u tronqué")?,
        )
        .map_err(|_| "échappement \\u non ASCII")?;
        self.i = fin;
        u32::from_str_radix(s, 16).map_err(|_| format!("\\u{s} n'est pas hexadécimal"))
    }

    fn nombre(&mut self) -> Result<Valeur, String> {
        let debut = self.i;
        if self.octet()? == b'-' {
            self.i += 1;
        }
        let mut flottant = false;
        while self.i < self.octets.len() {
            match self.octets[self.i] {
                b'0'..=b'9' => self.i += 1,
                b'.' | b'e' | b'E' | b'+' | b'-' => {
                    flottant = true;
                    self.i += 1;
                }
                _ => break,
            }
        }
        let brut = std::str::from_utf8(&self.octets[debut..self.i])
            .map_err(|_| "nombre non ASCII")?;
        if flottant {
            brut.parse::<f64>()
                .map(Valeur::Flottant)
                .map_err(|e| format!("{brut} n'est pas un nombre : {e}"))
        } else {
            brut.parse::<i64>()
                .map(Valeur::Entier)
                .map_err(|e| format!("{brut} n'est pas un entier : {e}"))
        }
    }
}

// ── Écriture canonique ───────────────────────────────────────────────────

pub fn canonique(v: &Valeur) -> String {
    let mut s = String::new();
    ecrire(v, &mut s);
    s
}

fn ecrire(v: &Valeur, out: &mut String) {
    match v {
        Valeur::Nul => out.push_str("null"),
        Valeur::Bool(true) => out.push_str("true"),
        Valeur::Bool(false) => out.push_str("false"),
        Valeur::Entier(n) => {
            let _ = write!(out, "{n}");
        }
        Valeur::Flottant(x) => out.push_str(&flottant_comme_python(*x)),
        Valeur::Texte(s) => ecrire_texte(s, out),
        Valeur::Liste(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                ecrire(item, out);
            }
            out.push(']');
        }
        Valeur::Objet(m) => {
            out.push('{');
            for (i, (cle, val)) in m.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                ecrire_texte(cle, out);
                out.push(':');
                ecrire(val, out);
            }
            out.push('}');
        }
    }
}

/// `ensure_ascii=False` : les accents sortent tels quels. Seuls s'échappent
/// le guillemet, la barre inverse et les caractères de contrôle — et Python
/// nomme `\b \t \n \f \r` plutôt que de les écrire en `\u00XX`.
fn ecrire_texte(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Le `repr` de Python pour un flottant fini.
///
/// `{:e}` de Rust donne la plus courte représentation qui fait l'aller-retour,
/// sous la forme `m.mmme±X`. On la décompose, puis on choisit la notation
/// selon le seuil relevé sur Python : scientifique si l'exposant décimal est
/// ≥ 16 ou ≤ −5.
pub fn flottant_comme_python(x: f64) -> String {
    if x == 0.0 {
        return if x.is_sign_negative() { "-0.0".into() } else { "0.0".into() };
    }
    let negatif = x < 0.0;
    let brut = format!("{:e}", x.abs()); // ex. "2.676e-5", "1e22"
    let (mantisse, exposant) = brut.split_once('e').expect("{:e} produit toujours un e");
    let exposant: i32 = exposant.parse().expect("exposant entier");
    let chiffres: String = mantisse.chars().filter(|c| *c != '.').collect();

    let corps = if exposant >= 16 || exposant <= -5 {
        let m = if chiffres.len() == 1 {
            chiffres.clone()
        } else {
            format!("{}.{}", &chiffres[..1], &chiffres[1..])
        };
        let signe = if exposant < 0 { '-' } else { '+' };
        format!("{m}e{signe}{:02}", exposant.abs())
    } else if exposant < 0 {
        // 0.000xyz : des zéros devant les chiffres significatifs.
        let zeros = (-exposant - 1) as usize;
        format!("0.{}{}", "0".repeat(zeros), chiffres)
    } else {
        let avant = (exposant + 1) as usize;
        if chiffres.len() <= avant {
            // Entier : Python ajoute toujours « .0 ».
            format!("{}{}.0", chiffres, "0".repeat(avant - chiffres.len()))
        } else {
            format!("{}.{}", &chiffres[..avant], &chiffres[avant..])
        }
    };

    if negatif {
        format!("-{corps}")
    } else {
        corps
    }
}
