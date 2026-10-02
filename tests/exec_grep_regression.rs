#![cfg(unix)]

use std::process::{Command, Output};
use tempfile::{tempdir, TempDir};

fn exec_at(dir: &TempDir, args: &[&str]) -> Output {
    let binary = std::env::var_os("LM_RESIZER_TEST_BINARY")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_lm-resizer").into());
    Command::new(binary)
        .current_dir(dir.path())
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir.path().join("home"))
        .env("LC_ALL", "C")
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn tee_couvre_les_omissions_de_la_pipeline_seule() {
    let dir = tempdir().unwrap();
    std::fs::create_dir(dir.path().join("home")).unwrap();
    let rows = (0..80)
        .map(|index| serde_json::json!({"id": index, "status": "ok", "score": 100}).to_string())
        .collect::<Vec<_>>()
        .join(",\n");
    // Moins de 240 lignes, toutes distinctes : seul le compactage JSON intervient.
    let log = format!("{{\"rows\":[\n{rows}\n]}}\n");
    std::fs::write(dir.path().join("rows.json"), &log).unwrap();
    // A pre-filtered producer uses only the subsequent generic pipeline.
    // This isolates the tee fix from the new lossless cat/shell routing.
    use std::os::unix::fs::PermissionsExt;
    let producer = dir.path().join("rtk");
    std::fs::write(&producer, "#!/bin/sh\ncat rows.json\n").unwrap();
    std::fs::set_permissions(&producer, std::fs::Permissions::from_mode(0o755)).unwrap();
    let result = exec_at(&dir, &["exec", "--json", "--", producer.to_str().unwrap()]);
    assert!(result.status.success(), "{:?}", result.stderr);
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["filtered_bytes"].as_u64().unwrap(), log.len() as u64);
    assert!(report["compressed_bytes"].as_u64().unwrap() < log.len() as u64);
    let hint = report["tee_hint"]
        .as_str()
        .expect("référence tee absente après compression");
    let reference = hint
        .strip_prefix("[raw: ")
        .unwrap()
        .strip_suffix(']')
        .unwrap();
    let recovered = exec_at(&dir, &["tee", "read", reference]);
    assert!(recovered.status.success(), "{:?}", recovered.stderr);
    assert_eq!(recovered.stdout, log.as_bytes());
}

#[test]
fn grep_via_bash_compresse_et_recupere_toutes_les_lignes() {
    let dir = tempdir().unwrap();
    std::fs::create_dir(dir.path().join("home")).unwrap();
    let transforms = dir.path().join("crates/lm-resizer-core/src/transforms");
    std::fs::create_dir_all(&transforms).unwrap();
    let mut source = (0..1600)
        .map(|index| format!("pub fn fonction_{index:04}() -> usize {{ {index} }}\n"))
        .collect::<String>();
    // Un signal situé après le budget doit rester visible, même s'il provient de source.
    source.push_str("pub fn error() -> usize { 1600 }\n");
    std::fs::write(transforms.join("regression.rs"), source).unwrap();
    let shell = "grep -rn fn crates/lm-resizer-core/src/transforms";
    let raw = Command::new("bash")
        .current_dir(dir.path())
        .args(["-c", shell])
        .output()
        .unwrap();
    assert!(raw.status.success());
    let compressed = exec_at(&dir, &["exec", "--json", "--", "bash", "-c", shell]);
    assert!(compressed.status.success(), "{:?}", compressed.stderr);
    let report: serde_json::Value = serde_json::from_slice(&compressed.stdout).unwrap();
    let output = report["output"].as_str().unwrap();
    assert_eq!(
        report["original_bytes"].as_u64().unwrap(),
        raw.stdout.len() as u64
    );
    assert!(
        output.len() * 2 < raw.stdout.len(),
        "régression : {} -> {} octets",
        raw.stdout.len(),
        output.len()
    );
    assert!(output.contains("pub fn error() -> usize { 1600 }"));
    assert_eq!(decode_view(output).as_bytes(), raw.stdout);
    // Utiliser la référence réellement annoncée au lecteur, pas un chemin connu du test.
    let hint = report["tee_hint"].as_str().expect("référence tee absente");
    assert!(output.contains(hint));
    let reference = hint
        .strip_prefix("[raw: ")
        .unwrap()
        .strip_suffix(']')
        .unwrap();
    let recovered = exec_at(&dir, &["tee", "read", reference]);
    assert!(recovered.status.success(), "{:?}", recovered.stderr);
    // Égalité octet par octet : toutes les lignes, leur ordre et leur multiplicité.
    assert_eq!(recovered.stdout, raw.stdout);
    assert_eq!(
        recovered.stdout.split(|byte| *byte == b'\n').count() - 1,
        1601
    );
}

#[test]
fn compactage_generique_conserve_chaque_ligne_et_chaque_diagnostic() {
    let dir = tempdir().unwrap();
    std::fs::create_dir(dir.path().join("home")).unwrap();
    let mut log = (0..1000)
        .map(|index| format!("étape {index:04} terminée\n"))
        .collect::<String>();
    // Hors du début et de la fin du budget, avec des faits sans mot d'échec.
    log.push_str("ERROR comparaison interrompue\nCollection: [alpha, beta]\nNot found: gamma\n");
    log.extend((1000..2000).map(|index| format!("étape {index:04} terminée\n")));
    std::fs::write(dir.path().join("diagnostics.log"), &log).unwrap();
    let result = exec_at(
        &dir,
        &["exec", "--json", "--", "bash", "-c", "cat diagnostics.log"],
    );
    assert!(result.status.success(), "{:?}", result.stderr);
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    let output = report["output"].as_str().unwrap();
    assert!(
        output.len() < log.len(),
        "{} -> {}",
        log.len(),
        output.len()
    );
    assert_eq!(decode_view(output), log);
    for line in [
        "ERROR comparaison interrompue",
        "Collection: [alpha, beta]",
        "Not found: gamma",
    ] {
        assert!(output.contains(line), "diagnostic perdu : {line}");
    }
}

// Independent decoder: verify the visible representation, not just the tee.
fn decode_view(view: &str) -> String {
    let text = view.rsplit_once("[raw: ").map_or(view, |(body, _)| body);
    if !text.starts_with("LMR-LINES/2\n") {
        return text.to_string();
    }
    let mut prefix = String::new();
    let mut rows: Vec<String> = Vec::new();
    for row in text.split_terminator('\n').skip(1) {
        if let Some(value) = row.strip_prefix('@') {
            prefix = serde_json::from_str(value).unwrap();
        } else if let Some(value) = row.strip_prefix('&') {
            rows.push(rows[value.parse::<usize>().unwrap()].clone());
        } else if let Some(value) = row.strip_prefix('=') {
            for _ in 0..value.parse::<usize>().unwrap() {
                rows.push(rows.last().unwrap().clone());
            }
        } else if row == "!0" || row == "!1" {
            return rows.join("\n") + if row == "!1" { "\n" } else { "" };
        } else {
            rows.push(prefix.clone() + row.strip_prefix('\\').unwrap_or(row));
        }
    }
    panic!("unterminated lossless view")
}
