#![cfg(unix)]
use serde_json::Value;
use std::{
    collections::HashSet,
    process::{Command, Stdio},
};
#[test]
fn concurrent_cli_processes_leave_one_complete_record_per_line() {
    let root = tempfile::tempdir().unwrap();
    let mut children = Vec::new();
    for i in 0..32 {
        children.push(
            Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
                .env("HOME", root.path())
                .env("LM_RESIZER_STATE_DIR", root.path())
                .env("LM_RESIZER_TRACKING", "1")
                .args(["proxy", "sh", "-c", "exit 0", "marker"])
                .arg(i.to_string())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
    }
    for child in children {
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let history = std::fs::read_to_string(root.path().join("exec-history.jsonl")).unwrap();
    assert_eq!(history.lines().count(), 32);
    let mut commands = HashSet::new();
    for line in history.lines() {
        let row: Value =
            serde_json::from_str(line).expect("each append must be a complete JSONL frame");
        commands.insert(row["command"].as_str().unwrap().to_string());
    }
    assert_eq!(commands.len(), 32);
}
