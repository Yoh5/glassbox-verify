//! `glassbox-verify` — un second avis sur un registre Glass Box.
//!
//! ## Pourquoi un deuxième vérificateur, dans une autre langue
//!
//! Glass Box dit : *« ne me croyez pas, relancez »*. Mais jusqu'ici, relancer
//! voulait dire exécuter **le code de celui qui a écrit le registre**. Si sa
//! fonction d'empreinte a un défaut, son vérificateur a le même, et les deux
//! s'accordent sur une réponse fausse.
//!
//! Ce binaire ne partage pas une ligne avec lui. Il relit la spécification —
//! `sha256(prev + json.dumps(corps, sort_keys=True, separators=(",",":"),
//! ensure_ascii=False))` — et la réimplémente. Quand les deux tombent
//! d'accord, l'accord veut dire quelque chose.
//!
//! ## Ce que ça a coûté de réimplémenter
//!
//! Presque tout tient dans le formatage des flottants. Un registre contient
//! `2.676e-05` ; Rust écrit `0.00002676` par défaut. Un octet d'écart, et un
//! registre intact est déclaré falsifié. Les règles de Python ont donc été
//! **relevées sur l'interpréteur**, pas supposées, et un corpus de conformité
//! les fige : 19 formes et 3 enregistrements réels, dans `tests/`.
//!
//! ## Usage
//!
//! ```text
//! glassbox-verify registre.jsonl
//! glassbox-verify a.jsonl b.jsonl
//! ```
//!
//! Code de sortie 0 si tout tient, 1 si un enregistrement ne tient pas,
//! 2 si un fichier est illisible.

mod json;
mod ledger;
mod sha256;

use std::process::ExitCode;

fn main() -> ExitCode {
    let fichiers: Vec<String> = std::env::args().skip(1).collect();
    if fichiers.is_empty() {
        eprintln!("usage : glassbox-verify <registre.jsonl> [...]");
        eprintln!();
        eprintln!("Re-chaîne un registre Glass Box et nomme le premier");
        eprintln!("enregistrement qui ne tient pas.");
        return ExitCode::from(2);
    }

    let mut casse = false;
    let mut illisible = false;

    for chemin in &fichiers {
        let contenu = match std::fs::read_to_string(chemin) {
            Ok(c) => c,
            Err(e) => {
                println!("{chemin} : illisible — {e}");
                illisible = true;
                continue;
            }
        };

        let rapport = ledger::verifier(&contenu);
        if rapport.intact() {
            println!(
                "{chemin} : INTACT — {} enregistrement(s) re-chaîné(s), aucun écart.",
                rapport.enregistrements
            );
        } else {
            casse = true;
            println!(
                "{chemin} : ROMPU — {} enregistrement(s) lu(s), {} problème(s) :",
                rapport.enregistrements,
                rapport.problemes.len()
            );
            for p in &rapport.problemes {
                println!("    {p}");
            }
        }
    }

    if illisible {
        ExitCode::from(2)
    } else if casse {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
