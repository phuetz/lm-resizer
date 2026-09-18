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
        "pub fn a() { \"", "def f(:\n  \"\"\"", "function x(){`${",
        "fn a(){}\u{0}fn b(){}", "\u{feff}pub fn a() -> u32 { 1 }",
        &"pub fn a() { ".repeat(400),
    ] {
        let _ = c.compress(src);
    }
}
