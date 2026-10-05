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
    let producer = dir.path().join("lm-resizer");
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
fn grep_via_bash_resume_et_preserve_toutes_les_lignes_dans_tee() {
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
    assert!(output.contains("1601 matches"));
    // Search hits are regrouped, not sampled: the first 200 hits of the file
    // stay visible and the exact remainder is counted.
    assert!(output.contains("fonction_0199"));
    assert!(output.contains("+1401"));
    let key = report["tee_hint"]
        .as_str()
        .unwrap()
        .strip_prefix("[raw: ")
        .unwrap()
        .strip_suffix(']')
        .unwrap();
    let recovered = exec_at(&dir, &["tee", "read", key]);
    assert!(recovered.status.success());
    assert!(
        recovered.stdout == raw.stdout,
        "tee must preserve every byte"
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
    assert_eq!(output, log);
    for line in [
        "ERROR comparaison interrompue",
        "Collection: [alpha, beta]",
        "Not found: gamma",
    ] {
        assert!(output.contains(line), "diagnostic perdu : {line}");
    }
}
