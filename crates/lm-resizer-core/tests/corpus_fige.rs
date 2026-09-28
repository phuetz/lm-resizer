//! Un corpus figé, et ce qu'on exige de la compression sur chacun de ses cas.
//!
//! # Pourquoi un corpus figé
//!
//! Un taux de compression mesuré sur ce qui passait ce jour-là ne se compare à
//! rien. Les cas ci-dessous sont écrits une fois et ne bougent plus : quand un
//! chiffre change, c'est le code qui a changé, pas l'échantillon.
//!
//! La liste vient d'une contre-revue d'Astra, qui demandait exactement cela :
//! sorties courtes et longues, succès et erreurs, Unicode et séquences ANSI, et
//! un gros diff dont le défaut n'est que dans la partie finale — le cas où une
//! compression naïve garde le début et jette précisément ce qui comptait.
//!
//! # Ce qu'on exige, et ce qu'on ne promet pas
//!
//! Trois propriétés, vérifiées sur chaque cas :
//!
//! 1. **Ne jamais grossir.** Une sortie plus grosse que l'entrée n'est pas une
//!    compression, marqueur de récupération compris.
//! 2. **Rester récupérable.** Quand la compression annonce une clé, ce que le
//!    magasin rend doit être l'original **octet pour octet**, vérifié par
//!    SHA-256 et non par longueur.
//! 3. **Ne pas paniquer.** Aucune entrée, si hostile soit-elle.
//!
//! On mesure aussi les jetons, avec le tokenizer nommé — mais **on ne promet
//! aucune équivalence entre octets et jetons**. Les deux sont rapportés côte à
//! côte parce qu'ils ne mesurent pas la même chose : la facture se compte en
//! jetons, la mémoire en octets, et un gain sur l'un n'est pas un gain sur
//! l'autre.
//!
//! # Ce que ce corpus mesure, et ce qu'il ne mesure pas
//!
//! Il mesure **le pipeline seul** — la chaîne qui s'applique à un contenu selon
//! son type détecté. Il ne mesure **pas** les filtres par commande
//! (`git status`, `rg`, `cargo test`, les filtres TOML…), qui vivent dans le
//! binaire et s'appliquent *avant* le pipeline sur le chemin `exec`.
//!
//! Le premier relevé le montre sans ambiguïté : **un seul cas sur quatorze est
//! réduit par le pipeline** — un tableau JSON de 17 Ko, ramené de 50,1 % en
//! octets et 46,1 % en jetons. Les treize autres ressortent identiques, y
//! compris une recherche de 34 Ko, un diff de 56 Ko et une ligne de 200 Ko.
//!
//! Ce n'est pas une anomalie, c'est un partage des rôles : l'essentiel du gain
//! de lm-resizer vient des filtres par commande, pas du pipeline. Mais cela se
//! dit, parce qu'on pourrait croire l'inverse en lisant les chiffres globaux —
//! et parce qu'un pipeline qui laisse passer un diff de 56 Ko intact est une
//! piste de travail, pas un acquis.
//!
//! L'écart mesuré entre octets et jetons sur le seul cas compressé — 50,1 %
//! contre 46,1 % — illustre au passage pourquoi les deux sont rapportés : ils
//! ne bougent pas du même pas.

use lm_resizer_core::ccr::{CcrStore, InMemoryCcrStore};
use lm_resizer_core::default_pipeline;
use lm_resizer_core::tokenizer::get_tokenizer;
use lm_resizer_core::transforms::{detect_content_type, CompressionContext, PipelineResult};
use sha2::{Digest, Sha256};

/// Le tokenizer employé pour tous les décomptes de ce fichier. Nommé plutôt
/// qu'implicite : un chiffre de jetons sans tokenizer ne veut rien dire.
const TOKENIZER: &str = "gpt-4o";

fn empreinte(texte: &str) -> String {
    let mut h = Sha256::new();
    h.update(texte.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

struct Cas {
    nom: &'static str,
    contenu: String,
}

fn corpus() -> Vec<Cas> {
    let mut cas = Vec::new();

    cas.push(Cas {
        nom: "vide",
        contenu: String::new(),
    });
    cas.push(Cas {
        nom: "un seul caractere",
        contenu: "x".to_string(),
    });
    cas.push(Cas {
        nom: "sortie courte de commande",
        contenu: "ok\n".to_string(),
    });
    cas.push(Cas {
        nom: "succes de suite de tests",
        contenu: "test result: ok. 128 passed; 0 failed; 0 ignored\n".to_string(),
    });
    cas.push(Cas {
        nom: "echec de suite de tests",
        contenu: format!(
            "{}test result: FAILED. 120 passed; 8 failed; 0 ignored\n",
            "thread 'un::test' panicked at src/a.rs:12:5:\nassertion failed\n".repeat(8)
        ),
    });
    cas.push(Cas {
        nom: "recherche volumineuse",
        contenu: (0..600)
            .map(|n| format!("src/module{n}.rs:{n}:une ligne de resultat plutot longue\n"))
            .collect(),
    });
    cas.push(Cas {
        nom: "json volumineux",
        contenu: format!(
            "[{}]",
            (0..400)
                .map(|n| format!(r#"{{"id":{n},"nom":"element {n}","actif":true}}"#))
                .collect::<Vec<_>>()
                .join(",")
        ),
    });
    cas.push(Cas {
        nom: "unicode et accents",
        contenu: "éàüçñ 中文 日本語 🙂🚀\nligne accentuée répétée\n".repeat(120),
    });
    cas.push(Cas {
        nom: "sequences ANSI",
        contenu: (0..200)
            .map(|n| format!("\u{1b}[31merreur {n}\u{1b}[0m \u{1b}[1mgras\u{1b}[0m\n"))
            .collect(),
    });
    cas.push(Cas {
        nom: "fins de ligne Windows",
        contenu: "premiere\r\ndeuxieme\r\ntroisieme\r\n".repeat(200),
    });

    // Le cas qui piège une compression naïve : un très gros diff dont le seul
    // défaut est dans la toute dernière partie. Garder le début revient à jeter
    // la seule information qui comptait.
    let mut diff = String::from("diff --git a/gros.rs b/gros.rs\n--- a/gros.rs\n+++ b/gros.rs\n");
    for n in 0..1500 {
        diff.push_str(&format!("+    let variable_{n} = calcul({n});\n"));
    }
    diff.push_str("+    // DEFAUT: division par zero possible ici\n");
    diff.push_str("+    let resultat = total / diviseur;\n");
    cas.push(Cas {
        nom: "gros diff, defaut en fin",
        contenu: diff,
    });

    cas.push(Cas {
        nom: "ligne unique tres longue",
        contenu: "a".repeat(200_000),
    });
    cas.push(Cas {
        nom: "octet nul et marque d'ordre",
        contenu: "\u{feff}debut\u{0}milieu\nfin\n".to_string(),
    });
    cas.push(Cas {
        nom: "texte deja compact",
        contenu: "Réponse brève, rien à gagner.".to_string(),
    });

    cas
}

fn compresser(contenu: &str, store: &dyn CcrStore) -> PipelineResult {
    let pipeline = default_pipeline();
    let detection = detect_content_type(contenu);
    let ctx = CompressionContext {
        query: "corpus fige".to_string(),
        token_budget: None,
    };
    pipeline.run(contenu, detection.content_type, &ctx, store)
}

#[test]
fn le_corpus_ne_grossit_jamais_et_reste_recuperable() {
    let tokenizer = get_tokenizer(TOKENIZER);
    let mut lignes = Vec::new();
    let mut fautes = Vec::new();

    for cas in corpus() {
        let store = InMemoryCcrStore::default();
        let resultat = compresser(&cas.contenu, &store);

        // 1. Ne jamais grossir.
        if resultat.output.len() > cas.contenu.len() {
            fautes.push(format!(
                "« {} » a grossi : {} octets pour {} en entree",
                cas.nom,
                resultat.output.len(),
                cas.contenu.len()
            ));
        }

        // 2. Rester recuperable, verifie par empreinte et non par longueur.
        for cle in &resultat.cache_keys {
            match store.get(cle) {
                Some(rendu) => {
                    if empreinte(&rendu) != empreinte(&cas.contenu) {
                        fautes.push(format!(
                            "« {} » : ce que le magasin rend ne correspond pas a l'original",
                            cas.nom
                        ));
                    }
                }
                None => fautes.push(format!(
                    "« {} » annonce la cle {cle} mais le magasin ne la connait pas",
                    cas.nom
                )),
            }
        }

        // 3. Octets ET jetons, cote a cote, sans promettre d'equivalence.
        let o_avant = cas.contenu.len();
        let o_apres = resultat.output.len();
        let j_avant = tokenizer.count_text(&cas.contenu);
        let j_apres = tokenizer.count_text(&resultat.output);
        let pct = |a: usize, b: usize| {
            if a == 0 {
                0.0
            } else {
                (1.0 - b as f64 / a as f64) * 100.0
            }
        };
        lignes.push(format!(
            "{:<28} octets {:>8} -> {:>8} ({:>5.1} %)   jetons {:>7} -> {:>7} ({:>5.1} %)",
            cas.nom,
            o_avant,
            o_apres,
            pct(o_avant, o_apres),
            j_avant,
            j_apres,
            pct(j_avant, j_apres)
        ));
    }

    eprintln!("\n=== corpus fige — tokenizer {TOKENIZER} ===");
    for l in &lignes {
        eprintln!("{l}");
    }
    eprintln!(
        "\nLes pourcentages d'octets et de jetons ne sont pas interchangeables\n\
         et aucune equivalence n'est promise entre eux.\n"
    );

    assert!(fautes.is_empty(), "\n{}", fautes.join("\n"));
}

#[test]
fn aucune_entree_du_corpus_ne_fait_paniquer() {
    // Séparé du test précédent : si une entrée panique, on veut savoir laquelle
    // sans que les assertions de l'autre test masquent la panique.
    for cas in corpus() {
        let store = InMemoryCcrStore::default();
        let _ = compresser(&cas.contenu, &store);
    }
}

#[test]
fn la_compression_est_deterministe_sur_tout_le_corpus() {
    // Deux exécutions sur la même entrée doivent rendre le même octet, sans
    // quoi aucun cache ne vaut rien et aucune mesure n'est reproductible.
    for cas in corpus() {
        let a = compresser(&cas.contenu, &InMemoryCcrStore::default());
        let b = compresser(&cas.contenu, &InMemoryCcrStore::default());
        assert_eq!(
            empreinte(&a.output),
            empreinte(&b.output),
            "« {} » ne rend pas le meme resultat deux fois",
            cas.nom
        );
    }
}
