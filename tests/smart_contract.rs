use std::path::Path;
use std::process::{Command, Output};

fn run(path: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .arg("smart")
        .arg(path)
        .args(args)
        .env(
            "LM_RESIZER_STORE",
            path.parent().unwrap().join("store.sqlite"),
        )
        .env(
            "LM_RESIZER_CODE_EXPLORER_BIN",
            "absent-smart-ast-test-producer",
        )
        .env("LM_RESIZER_NO_CODE_EXPLORER", "1")
        .output()
        .unwrap()
}

#[test]
fn legacy_text_and_json_remain_byte_identical_to_baseline() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.rs");
    std::fs::write(&path, include_bytes!("fixtures/smart/legacy.rs")).unwrap();
    let text = run(&path, &[]);
    assert!(text.status.success());
    assert_eq!(
        text.stdout,
        include_bytes!("fixtures/smart/legacy.expected.txt")
    );
    let json = run(&path, &["--json"]);
    assert!(json.status.success());
    assert_eq!(
        json.stdout,
        include_bytes!("fixtures/smart/legacy.expected.json")
    );
    for option in ["--model", "--ultra-compact"] {
        assert!(!run(&path, &[option]).status.success());
    }
}

#[test]
fn ast_json_reports_approximate_tokens_and_needs_no_store() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.rs");
    let source = include_str!("fixtures/smart/legacy.rs");
    std::fs::write(&path, source).unwrap();
    let output = run(
        &path,
        &["--ast", "--json", "--store", path.to_str().unwrap()],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let summary = report["output"].as_str().unwrap();
    assert!(summary.contains("pub fn total(values: &[u64]) -> u64"));
    assert!(!summary.contains("total +="));
    assert_eq!(report["original_bytes"], source.len());
    assert_eq!(report["compressed_bytes"], summary.len());
    assert_eq!(report["original_tokens"], source.len() as f64 / 4.0);
    assert_eq!(report["compressed_tokens"], summary.len() as f64 / 4.0);
    assert_eq!(report["token_count_method"], "approximate");
    assert_eq!(report["tokenizer"], "bytes/4");
    assert!(summary.len() < source.len() / 4);
    assert!(!dir.path().join("store.sqlite").exists());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
}
