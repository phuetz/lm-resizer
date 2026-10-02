use serde_json::Value;
use std::process::Command;
#[test]
fn local_telemetry_controls_erase_metrics_without_recovery_loss() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("state");
    std::fs::create_dir(&state).unwrap();
    std::fs::write(state.join("exec-history.jsonl"), "{}\n").unwrap();
    std::fs::write(state.join("raw.log"), "original").unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("HOME", root.path())
            .env("LM_RESIZER_CONFIG", root.path().join("config.toml"))
            .env("LM_RESIZER_STATE_DIR", &state)
            .env_remove("LM_RESIZER_TRACKING")
            .args(args)
            .output()
            .unwrap()
    };
    let out = run(&["telemetry", "status"]);
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["remote_telemetry"], false);
    assert!(!run(&["telemetry", "forget"]).status.success());
    assert!(state.join("exec-history.jsonl").exists());
    assert!(run(&["telemetry", "forget", "--yes"]).status.success());
    assert!(!state.join("exec-history.jsonl").exists());
    assert!(state.join("raw.log").exists());
    let out = run(&["telemetry", "status"]);
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["local_tracking_effective"], false);
    assert!(run(&["telemetry", "enable"]).status.success());
}
