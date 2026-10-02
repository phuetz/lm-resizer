use serde_json::Value;
use std::{
    io::Write,
    process::{Command, Stdio},
};
#[test]
fn hook_protocol_and_audit_never_execute_input() {
    let root = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("HOME", root.path())
        .env("LM_RESIZER_STATE_DIR", root.path())
        .env("LM_RESIZER_HOOK_AUDIT", "1")
        .args(["hook", "gemini"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(br#"{"tool_name":"run_shell_command","tool_input":{"command":"git status","cwd":"/project"}}"#).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["decision"], "ask_user");
    assert_eq!(v["hookSpecificOutput"]["tool_input"]["cwd"], "/project");
    let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", root.path())
        .args(["hook-audit", "--since", "0"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["rewritten"], 1);
    let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .args(["hook", "check", "git status"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["changed"], true);
}
