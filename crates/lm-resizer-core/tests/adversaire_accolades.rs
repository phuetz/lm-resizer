// Contre-vérification indépendante : un compteur d'accolades naïf se fait
// piéger par les accolades contenues dans les chaînes et les commentaires.
// Exigence : ne jamais perdre de code, ne jamais paniquer.
use lm_resizer_core::transforms::source_compressor::SourceCompressor;

fn signatures_conservees(src: &str, signatures: &[&str]) {
    let c = SourceCompressor::default();
    let r = c.compress(src);
    for s in signatures {
        assert!(
            r.compressed.contains(s) || r.compressed == src,
            "signature perdue: {s}\n--- sortie ---\n{}",
            r.compressed
        );
    }
}

#[test]
fn rust_accolade_dans_une_chaine() {
    let src = r##"
pub fn premiere(x: u32) -> String {
    let motif = "}"; // accolade fermante dans une chaine
    let autre = "{{ ceci n'est pas un bloc }}";
    format!("{}{}", motif, autre)
}

pub fn seconde(y: u32) -> u32 {
    y * 2
}
"##;
    signatures_conservees(src, &["pub fn premiere", "pub fn seconde"]);
}

#[test]
fn rust_accolade_dans_un_commentaire() {
    let src = r#"
pub fn alpha(x: u32) -> u32 {
    // ceci ferme } tout
    x
}

pub fn beta(x: u32) -> u32 {
    /* et ceci aussi } */
    x
}
"#;
    signatures_conservees(src, &["pub fn alpha", "pub fn beta"]);
}

#[test]
fn rust_chaine_brute_avec_diese() {
    let src = "pub fn gamma() -> &'static str {\n    r#\"un } et un \" dedans\"#\n}\n\npub fn delta() -> u32 { 7 }\n";
    signatures_conservees(src, &["pub fn gamma", "pub fn delta"]);
}

#[test]
fn python_accolade_et_triple_guillemet() {
    let src = "def un(a):\n    s = \"\"\"un docstring avec def deux(b): dedans\"\"\"\n    return s\n\ndef deux(b):\n    return b + 1\n";
    signatures_conservees(src, &["def un", "def deux"]);
}

#[test]
fn ts_accolade_dans_un_gabarit() {
    let src = "export function un(a: number): string {\n  const t = `valeur ${a} et une } seule`;\n  return t;\n}\n\nexport function deux(b: number): number {\n  return b;\n}\n";
    signatures_conservees(src, &["function un", "function deux"]);
}

#[test]
fn aucune_panique_sur_entrees_hostiles() {
    let c = SourceCompressor::default();
    for src in [
        "pub fn a() { \"",
        "def f(:\n  \"\"\"",
        "function x(){`${",
        "fn a(){}\u{0}fn b(){}",
        "\u{feff}pub fn a() -> u32 { 1 }",
        &"pub fn a() { ".repeat(400),
    ] {
        let _ = c.compress(src);
    }
}

// ---------------------------------------------------------------------------
// Synergie n°1 : le conseiller de plages. On vérifie ici la seule propriété qui
// compte vraiment — un conseil ne fait jamais perdre de code.
// ---------------------------------------------------------------------------
use lm_resizer_core::ccr::{CcrStore, InMemoryCcrStore};
use lm_resizer_core::transforms::retention_advice::{sha256_hex_public, RetentionAdvice};
use lm_resizer_core::transforms::source_compressor::advice_from_symbols;

const SOURCE: &str = "// en-tete jetable\npub fn premiere() -> u32 {\n    // commentaire interne\n    1\n}\n\n\n\n// autre commentaire\npub fn seconde() -> u32 {\n    2\n}\n";

/// Un conseil portant l'empreinte du SOURCE : sans elle il serait refuse, et
/// c'est voulu — des numeros de ligne sans fichier identifie ne veulent rien
/// dire.
fn conseil_sur_source(plages: &str) -> RetentionAdvice {
    let empreinte = sha256_hex_public(SOURCE.as_bytes());
    RetentionAdvice::from_json(&format!(
        r#"{{"source_sha256":"{empreinte}","ranges":{plages}}}"#
    ))
    .unwrap()
}

#[test]
fn une_plage_protegee_survit_meme_si_c_est_un_commentaire() {
    let c = SourceCompressor::default();
    // Ligne 3 : un commentaire que la compression jetterait normalement.
    let advice = conseil_sur_source(r#"[{"start_line":3,"end_line":3}]"#);

    let sans = c.compress_with_advice(SOURCE, &RetentionAdvice::default(), None);
    let avec = c.compress_with_advice(SOURCE, &advice, None);

    assert!(!sans.compressed.contains("commentaire interne"));
    assert!(avec.compressed.contains("commentaire interne"));
}

#[test]
fn un_conseil_vide_se_comporte_comme_l_absence_de_conseil() {
    let c = SourceCompressor::default();
    let vide = c.compress_with_advice(SOURCE, &RetentionAdvice::default(), None);
    let parse = RetentionAdvice::from_json(r#"{"ranges":[]}"#).unwrap();
    assert_eq!(
        vide.compressed,
        c.compress_with_advice(SOURCE, &parse, None).compressed
    );
}

#[test]
fn le_conseil_ne_fait_jamais_perdre_de_code_par_rapport_a_l_absence_de_conseil() {
    // La propriété qui rend le mécanisme sûr : protéger ne peut qu'ajouter.
    let c = SourceCompressor::default();
    let sans = c.compress_with_advice(SOURCE, &RetentionAdvice::default(), None);
    for plage in [
        r#"[{"start_line":1,"end_line":1}]"#,
        r#"[{"start_line":6,"end_line":8}]"#,
        r#"[{"start_line":1,"end_line":12}]"#,
    ] {
        let advice = conseil_sur_source(plage);
        let avec = c.compress_with_advice(SOURCE, &advice, None);
        assert!(
            avec.compressed.len() >= sans.compressed.len(),
            "protéger a fait perdre du texte avec {plage}"
        );
    }
}

#[test]
fn ce_qui_est_retire_reste_recuperable() {
    let c = SourceCompressor::default();
    let store = InMemoryCcrStore::default();
    let advice = conseil_sur_source(r#"[{"start_line":2,"end_line":5}]"#);
    let res = c.compress_with_advice(SOURCE, &advice, Some(&store));
    assert!(res.compressed.len() < SOURCE.len());
    let cle = res
        .ccr_key
        .expect("une compression effective doit deposer une cle");
    assert_eq!(store.get(&cle).as_deref(), Some(SOURCE));
}

#[test]
fn code_explorer_n_est_qu_un_producteur_parmi_d_autres() {
    use lm_resizer_core::transforms::source_compressor::AstSymbol;
    let symboles = vec![
        AstSymbol {
            name: "premiere".into(),
            label: "function".into(),
            start_line: 2,
            end_line: 5,
        },
        // Une plage absurde que le graphe pourrait produire : elle est écartée
        // sans faire tomber les autres.
        AstSymbol {
            name: "bruit".into(),
            label: "function".into(),
            start_line: 9,
            end_line: 4,
        },
    ];
    let advice = advice_from_symbols(&symboles, SOURCE);
    assert_eq!(advice.advisor.as_deref(), Some("code-explorer"));
    assert_eq!(advice.ranges.len(), 1);
    assert_eq!(advice.ranges[0].label.as_deref(), Some("premiere"));
    // Le producteur estampille ce qu'il a lu, sans quoi son conseil ne pourra
    // pas être vérifié plus tard.
    assert!(advice.source_sha256.is_some());

    // Le compresseur ne sait pas d'où vient le conseil : un document écrit à la
    // main, portant la même empreinte, donne exactement le même résultat.
    let c = SourceCompressor::default();
    let empreinte = advice.source_sha256.clone().unwrap();
    let a_la_main = RetentionAdvice::from_json(&format!(
        r#"{{"advisor":"ctags","source_sha256":"{empreinte}","ranges":[{{"start_line":2,"end_line":5,"weight":4.0,"label":"premiere"}}]}}"#
    ))
    .unwrap();
    assert_eq!(
        c.compress_with_advice(SOURCE, &advice, None).compressed,
        c.compress_with_advice(SOURCE, &a_la_main, None).compressed
    );
}

#[test]
fn un_conseil_perime_ne_protege_plus_rien() {
    use lm_resizer_core::transforms::source_compressor::AstSymbol;
    // Le defaut que la contre-verification d'Astra a fait apparaitre : un
    // conseil calcule sur une version anterieure du fichier protegerait les
    // mauvaises lignes, avec le meme aplomb que s'il etait juste.
    let c = SourceCompressor::default();
    let symboles = vec![AstSymbol {
        name: "premiere".into(),
        label: "function".into(),
        start_line: 2,
        end_line: 5,
    }];
    let advice = advice_from_symbols(&symboles, SOURCE);

    // Meme fichier : le conseil s'applique.
    let frais = c.compress_with_advice(SOURCE, &advice, None);

    // Fichier modifie depuis : le conseil est refuse, on retombe sur la
    // compression ligne a ligne, identique a l'absence de conseil.
    let modifie = format!("// une ligne ajoutee en tete\n{SOURCE}");
    let perime = c.compress_with_advice(&modifie, &advice, None);
    let sans = c.compress_with_advice(&modifie, &RetentionAdvice::default(), None);
    assert_eq!(
        perime.compressed, sans.compressed,
        "un conseil perime doit se comporter comme une absence de conseil"
    );
    // Et il ne doit surtout pas ressembler au resultat frais.
    assert_ne!(frais.compressed, perime.compressed);
}
