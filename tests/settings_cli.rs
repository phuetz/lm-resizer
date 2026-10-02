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
