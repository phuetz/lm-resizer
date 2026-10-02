use serde_json::Value;
use std::process::{Command, Stdio};
#[test]
fn reset_requires_confirmation_and_keeps_recovery_data() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("exec-history.jsonl"), "{}\n").unwrap();
    std::fs::write(root.path().join("raw.log"), "original").unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("LM_RESIZER_STATE_DIR", root.path())
            .stdin(Stdio::null())
            .args(args)
            .output()
            .unwrap()
    };
    assert!(!run(&["gain", "--reset"]).status.success());
    assert!(root.path().join("exec-history.jsonl").exists());
    let out = run(&["gain", "--reset", "--yes"]);
    assert!(out.status.success());
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["recovery_data_preserved"], true);
    assert!(root.path().join("raw.log").exists());
    assert!(!root.path().join("exec-history.jsonl").exists());
}
#[test]
fn tee_recall_is_attributed_to_the_recorded_filter() {
    let root = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("LM_RESIZER_STATE_DIR", root.path())
            .env("LM_RESIZER_TRACKING", "1")
            .env("LM_RESIZER_TEE", "1")
            .args(args)
            .output()
            .unwrap()
    };
    let text = "repeated line\n".repeat(200);
    let out = run(&["exec", "--json", "--", "echo", &text]);
    assert!(out.status.success());
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    let hint = report["tee_hint"]
        .as_str()
        .unwrap()
        .trim_start_matches("[raw: ")
        .trim_end_matches(']');
    assert!(run(&["tee", "read", hint]).status.success());
    let out = run(&["gain", "--recalls"]);
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["recalls"]["unattributed_recalls"], 0);
    assert_eq!(report["recalls"]["by_filter"][0]["recalls"], 1);
}
