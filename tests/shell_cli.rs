#![cfg(unix)]
use std::process::Command;
#[test]
fn run_preserves_shell_semantics_streams_and_exit_without_tracking() {
    let root = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", root.path())
        .args(["run", "-c", "printf 'a'; printf 'b' >&2; exit 23"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(23));
    assert_eq!(out.stdout, b"a");
    assert_eq!(out.stderr, b"b");
    assert!(!root.path().join("exec-history.jsonl").exists());
}
