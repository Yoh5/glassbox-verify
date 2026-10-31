//! Le juge : Python a écrit ce que la forme canonique doit valoir, et chaque
//! cas est confronté octet pour octet.
//!
//! Le corpus est produit par le dépôt Glass Box lui-même, donc il ne peut pas
//! diverger de ce qui chaîne les vrais registres : si Python change de
//! formatage, ce test tombe au lieu de laisser le binaire valider des
//! empreintes fausses.
//!
//! **Le corpus est lu par notre propre analyseur, et c'est voulu.** On
//! pourrait y voir une circularité — un analyseur qui se juge lui-même — mais
//! la vérité de référence n'est pas produite ici : elle est dans le champ
//! `canonique`, écrit par Python. Si notre lecture est fausse, la comparaison
//! échoue. Un analyseur cassé ne peut pas se donner raison.

use std::fs;
use std::path::Path;

#[path = "../src/json.rs"]
mod json;
#[path = "../src/sha256.rs"]
mod sha256;

use json::Valeur;

fn texte(v: &Valeur, cle: &str) -> String {
    let Valeur::Objet(m) = v else { panic!("objet attendu") };
    match m.get(cle) {
        Some(Valeur::Texte(s)) => s.clone(),
        autre => panic!("{cle} absent ou non textuel : {autre:?}"),
    }
}

fn corpus() -> Valeur {
    let chemin = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/conformite.json");
    let brut = fs::read_to_string(&chemin)
        .unwrap_or_else(|e| panic!("corpus de conformité illisible ({}) : {e}", chemin.display()));
    json::lire(&brut).expect("le corpus lui-même doit se lire")
}

#[test]
fn la_forme_canonique_est_celle_de_python() {
    let c = corpus();
    let Valeur::Objet(racine) = &c else { panic!("objet attendu") };
    let Some(Valeur::Liste(cas)) = racine.get("valeurs") else {
        panic!("le corpus n'a pas de liste « valeurs »")
    };

    for cas in cas {
        let nom = texte(cas, "nom");
        let attendu = texte(cas, "canonique");
        let empreinte = texte(cas, "sha256");

        // On relit la forme canonique produite par Python, puis on la réécrit.
        // Le tour complet doit rendre exactement la même chaîne d'octets.
        let valeur = json::lire(&attendu)
            .unwrap_or_else(|e| panic!("[{nom}] relecture impossible : {e}"));
        let rendu = json::canonique(&valeur);

        assert_eq!(rendu, attendu, "[{nom}] la forme canonique diverge");
        assert_eq!(
            sha256::sha256_hex(rendu.as_bytes()),
            empreinte,
            "[{nom}] l'empreinte diverge"
        );
    }
    assert!(cas.len() >= 15, "corpus trop maigre : {} cas", cas.len());
}

#[test]
fn la_chaine_se_recalcule_sur_de_vrais_enregistrements() {
    let c = corpus();
    let Valeur::Objet(racine) = &c else { panic!("objet attendu") };
    let Some(Valeur::Liste(maillons)) = racine.get("chaine") else {
        panic!("le corpus n'a pas de liste « chaine »")
    };

    // Les neuf champs que l'empreinte couvre, dans l'ordre où Glass Box les
    // déclare. Tout autre champ d'un enregistrement n'est couvert par aucune
    // empreinte — c'est au vérificateur de le dire, pas de l'ignorer.
    const CLES_CORPS: [&str; 9] = [
        "name", "started_at", "agent_version", "evidence", "asks",
        "model_calls", "actions", "outcome", "rule_digest",
    ];

    let mut precedent: Option<String> = None;
    for maillon in maillons {
        let ligne = texte(maillon, "ligne");
        let attendu = texte(maillon, "chain_attendu");
        let canonique_attendu = texte(maillon, "canonique_du_corps");

        let enregistrement = json::lire(&ligne).expect("ligne de registre lisible");
        let Valeur::Objet(champs) = &enregistrement else { panic!("objet attendu") };

        let mut corps = std::collections::BTreeMap::new();
        for cle in CLES_CORPS {
            if let Some(v) = champs.get(cle) {
                corps.insert(cle.to_string(), v.clone());
            }
        }
        let canonique = json::canonique(&Valeur::Objet(corps));
        assert_eq!(
            canonique, canonique_attendu,
            "la forme canonique du corps diverge de celle de Python"
        );

        let matiere = format!("{}{}", precedent.clone().unwrap_or_default(), canonique);
        let calcule = format!("sha256:{}", sha256::sha256_hex(matiere.as_bytes()));
        assert_eq!(calcule, attendu, "la chaîne recalculée diverge");

        precedent = Some(texte(maillon, "chain_ecrit"));
    }
    assert!(!maillons.is_empty(), "aucun enregistrement dans le corpus");
}
