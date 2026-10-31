//! La vérification d'un registre : re-chaîner le fichier entier.
//!
//! Les règles sont celles de `glassbox/ledger.py`, et elles tiennent en trois
//! phrases :
//!
//! 1. le `prev` d'un enregistrement est le `chain` du précédent (vide pour le
//!    premier) ;
//! 2. son `chain` vaut `sha256(prev + forme_canonique(corps))` ;
//! 3. tout champ hors des neuf du corps et des deux de structure n'est couvert
//!    par **aucune** empreinte — et ça se dit, au lieu de se taire.
//!
//! La troisième est la moins évidente et la plus importante : un vérificateur
//! qui ignore un champ inconnu valide un registre auquel on a ajouté quelque
//! chose. Il ne peut pas se porter garant d'un champ qu'il ne connaît pas.
//!
//! **La position est comptée à partir de un et toujours nommée.** « Quelque
//! chose ne va pas quelque part » n'est pas un constat.

use std::collections::BTreeMap;

use crate::json::{self, Valeur};
use crate::sha256::sha256_hex;

/// Les neuf champs que l'empreinte couvre, dans l'ordre déclaré par Glass Box.
pub const CLES_CORPS: [&str; 9] = [
    "name",
    "started_at",
    "agent_version",
    "evidence",
    "asks",
    "model_calls",
    "actions",
    "outcome",
    "rule_digest",
];

/// Les deux champs qui portent la chaîne elle-même.
pub const CLES_STRUCTURE: [&str; 2] = ["prev", "chain"];

/// Sans ceux-là, une ligne n'est pas un enregistrement. `asks` et
/// `rule_digest` manquent aux registres écrits avant leur existence, donc ils
/// ne sont pas exigés.
const CLES_REQUISES: [&str; 7] = [
    "name", "started_at", "agent_version", "evidence", "actions", "prev", "chain",
];

pub struct Rapport {
    pub enregistrements: usize,
    pub problemes: Vec<String>,
}

impl Rapport {
    pub fn intact(&self) -> bool {
        self.problemes.is_empty()
    }
}

fn texte(m: &BTreeMap<String, Valeur>, cle: &str) -> Option<String> {
    match m.get(cle) {
        Some(Valeur::Texte(s)) => Some(s.clone()),
        _ => None,
    }
}

pub fn verifier(contenu: &str) -> Rapport {
    let mut problemes = Vec::new();
    let mut precedent: Option<String> = None;
    let mut position = 0usize;

    for (numero, ligne) in contenu.lines().enumerate() {
        let ligne = ligne.trim();
        if ligne.is_empty() {
            continue;
        }
        position += 1;

        let valeur = match json::lire(ligne) {
            Ok(v) => v,
            Err(e) => {
                problemes.push(format!(
                    "ligne {} : ce n'est pas du JSON ({e})",
                    numero + 1
                ));
                continue;
            }
        };
        let Valeur::Objet(champs) = &valeur else {
            problemes.push(format!("ligne {} : ce n'est pas un objet", numero + 1));
            continue;
        };

        let manquantes: Vec<&str> = CLES_REQUISES
            .iter()
            .filter(|c| !champs.contains_key(**c))
            .copied()
            .collect();
        if !manquantes.is_empty() {
            problemes.push(format!(
                "ligne {} : il manque {} — ce n'est pas un enregistrement",
                numero + 1,
                manquantes.join(", ")
            ));
            continue;
        }

        let nom = texte(champs, "name").unwrap_or_else(|| "?".into());

        let mut corps = BTreeMap::new();
        for cle in CLES_CORPS {
            if let Some(v) = champs.get(cle) {
                corps.insert(cle.to_string(), v.clone());
            }
        }
        let canonique = json::canonique(&Valeur::Objet(corps));

        let prev_lu = match champs.get("prev") {
            Some(Valeur::Texte(s)) => Some(s.clone()),
            Some(Valeur::Nul) | None => None,
            autre => {
                problemes.push(format!(
                    "enregistrement {position} ({nom}) : prev n'est ni un texte ni null ({autre:?})"
                ));
                None
            }
        };

        if prev_lu != precedent {
            problemes.push(format!(
                "enregistrement {position} ({nom}) ne suit pas celui d'avant : \
                 un enregistrement a été modifié, supprimé ou inséré"
            ));
        } else {
            let matiere = format!("{}{}", precedent.clone().unwrap_or_default(), canonique);
            let calcule = format!("sha256:{}", sha256_hex(matiere.as_bytes()));
            if Some(&calcule) != champs.get("chain").and_then(|v| match v {
                Valeur::Texte(s) => Some(s),
                _ => None,
            }) {
                problemes.push(format!(
                    "enregistrement {position} ({nom}) a été modifié depuis son écriture"
                ));
            }
        }

        let connues: Vec<&str> = CLES_CORPS.iter().chain(CLES_STRUCTURE.iter()).copied().collect();
        let mut inconnues: Vec<&String> = champs
            .keys()
            .filter(|c| !connues.contains(&c.as_str()))
            .collect();
        inconnues.sort();
        if !inconnues.is_empty() {
            let liste: Vec<&str> = inconnues.iter().map(|s| s.as_str()).collect();
            problemes.push(format!(
                "enregistrement {position} ({nom}) porte {} — qu'aucune empreinte ne couvre : \
                 ce vérificateur ne peut pas se porter garant d'un champ qu'il ne connaît pas",
                liste.join(", ")
            ));
        }

        precedent = texte(champs, "chain");
    }

    Rapport { enregistrements: position, problemes }
}
