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
        .env_remove("CODEX_HOME")
        .env_remove("CLAUDE_CONFIG_DIR")
        .current_dir(root.path())
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
        .env("HOME", root.path())
        .env_remove("CODEX_HOME")
        .env_remove("CLAUDE_CONFIG_DIR")
        .current_dir(root.path())
        .args(["hook", "check", "git status"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["changed"], true);
}

#[test]
fn hook_check_defers_to_local_permission_constraints() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".claude")).unwrap();
    std::fs::write(
        root.path().join(".claude/settings.json"),
        r#"{"permissions":{"deny":["Bash(git:*)"]}}"#,
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("HOME", root.path())
        .env_remove("CODEX_HOME")
        .env_remove("CLAUDE_CONFIG_DIR")
        .current_dir(root.path())
        .args(["hook", "check", "--agent", "claude", "git status"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["changed"], false);
}

#[test]
fn copilot_native_payload_preserves_arguments() {
    let root = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("HOME", root.path())
        .env_remove("CODEX_HOME")
        .env_remove("CLAUDE_CONFIG_DIR")
        .current_dir(root.path())
        .args(["hook", "copilot"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            br#"{"toolName":"bash","toolArgs":"{\"command\":\"git status\",\"timeout\":42}"}"#,
        )
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v["modifiedArgs"]["command"],
        "lm-resizer exec -- git status"
    );
    assert_eq!(v["modifiedArgs"]["timeout"], 42);
}

#[test]
fn bad_local_config_and_shell_substitution_leave_hook_input_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("config.toml");
    std::fs::write(&config, "not valid toml").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("HOME", root.path())
        .env("LM_RESIZER_CONFIG", &config)
        .current_dir(root.path())
        .args(["hook", "check", "git status"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    std::fs::write(&config, "").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("HOME", root.path())
        .env("LM_RESIZER_CONFIG", &config)
        .current_dir(root.path())
        .args(["hook", "check", "--agent", "pi", "git show $(danger)"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["changed"], false);
}
