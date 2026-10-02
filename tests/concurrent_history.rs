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

#[test]
fn journal_waits_for_another_writer_before_appending_large_frames() {
    use fs2::FileExt;
    use std::io::{BufRead, BufReader};
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("exec-history.jsonl");
    let guard = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .unwrap();
    guard.lock_exclusive().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("HOME", root.path())
        .env("LM_RESIZER_STATE_DIR", root.path())
        .env("LM_RESIZER_TRACKING", "1")
        .args(["proxy", "sh", "-c", "echo ready", "marker"])
        .arg("x".repeat(60000))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert_eq!(line, "ready\n");
    // The child has completed its command and now records it. Keep the lock
    // until checking the journal, then always release and reap before asserting.
    std::thread::sleep(std::time::Duration::from_millis(300));
    let blocked = child.try_wait().unwrap().is_none();
    let before = std::fs::metadata(&path).unwrap().len();
    FileExt::unlock(&guard).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(blocked, "journal must wait for the existing writer's lock");
    assert_eq!(before, 0, "no bytes may interleave with the locked writer");
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text.lines().count(), 1);
    let row: Value = serde_json::from_str(text.trim()).unwrap();
    assert!(row["command"].as_str().unwrap().len() >= 60000);
}
