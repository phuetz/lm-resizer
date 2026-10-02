use serde_json::Value;
use std::process::Command;
#[test]
fn cli_errors_are_recorded_and_reported_separately_from_raw_fallbacks() {
    let root = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("LM_RESIZER_STATE_DIR", root.path())
            .env("LM_RESIZER_TRACKING", "1")
            .args(args)
            .output()
            .unwrap()
    };
    assert!(!run(&["compress", "--input", "nonexistent-file-for-test"])
        .status
        .success());
    let out = run(&["gain", "--failures", "--format", "json"]);
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["failures"]["errors"][0]["reason"], "cli_error");
    assert!(v["failures"]["raw_fallbacks"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(!run(&["gain", "--project", "--reset", "--yes"])
        .status
        .success());
}
