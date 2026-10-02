use std::process::Command;
#[test]
fn config_creation_is_non_destructive_and_tracking_is_effective() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("settings.toml");
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("HOME", root.path())
            .env("LM_RESIZER_CONFIG", &path)
            .env("LM_RESIZER_STATE_DIR", root.path().join("state"))
            .env_remove("LM_RESIZER_TRACKING")
            .args(args)
            .output()
            .unwrap()
    };
    assert!(run(&["config", "--create"]).status.success());
    let original = std::fs::read(&path).unwrap();
    assert!(!run(&["config", "--create"]).status.success());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert!(run(&["config", "set", "tracking", "false"])
        .status
        .success());
    let out = run(&["proxy", "echo", "hello"]);
    assert!(out.status.success());
    assert!(!root.path().join("state/exec-history.jsonl").exists());
    assert!(!run(&["config", "set", "tracking", "invalid"])
        .status
        .success());
    assert!(run(&["config", "unset", "tracking"]).status.success());
    assert!(run(&["proxy", "echo", "hello"]).status.success());
    assert!(root.path().join("state/exec-history.jsonl").exists());
}

#[test]
fn sqlite_snapshot_mode_produces_a_recoverable_reference() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("settings.toml");
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("LM_RESIZER_CONFIG", &path)
            .env("LM_RESIZER_STATE_DIR", root.path().join("state"))
            .env_remove("LM_RESIZER_RECALL")
            .env("LM_RESIZER_TEE", "1")
            .args(args)
            .output()
            .unwrap()
    };
    assert!(run(&["config", "recall", "sqlite"]).status.success());
    let raw = "line repeated\n".repeat(200);
    let out = run(&["exec", "--json", "--", "echo", &raw]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let key = v["tee_hint"]
        .as_str()
        .unwrap()
        .trim_start_matches("[raw: ")
        .trim_end_matches(']');
    let recalled = run(&["recall", key]);
    assert!(recalled.status.success());
    assert_eq!(recalled.stdout, format!("{raw}\n").as_bytes());
    assert!(!root.path().join("state/tee").exists());
    assert!(run(&["config", "recall", "disabled"]).status.success());
    let out = run(&["exec", "--json", "--", "echo", &raw]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["tee_hint"].is_null());
}
