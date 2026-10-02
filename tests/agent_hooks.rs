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

macro_rules! policy_test {
    ($name:ident, $agent:literal, $directory:literal, $tool:literal) => {
        #[test]
        fn $name() {
            for content in [
                r#"{"permissions":{"allow":["Bash(git status)"]}}"#,
                r#"{"permissions":{"deny":["Bash(git status)"]}}"#,
                "{invalid",
            ] {
                let root = tempfile::tempdir().unwrap();
                let dir = root.path().join($directory);
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(dir.join("settings.json"), content).unwrap();
                assert_policy_abstention(root.path(), $agent, $tool);
            }
        }
    };
}
policy_test!(
    claude_native_and_preview_constraints,
    "claude",
    ".claude",
    "Bash"
);
policy_test!(
    codex_native_and_preview_constraints,
    "codex",
    ".codex",
    "Bash"
);
policy_test!(
    copilot_native_and_preview_constraints,
    "copilot",
    ".copilot",
    "bash"
);
policy_test!(vibe_native_and_preview_constraints, "vibe", ".vibe", "bash");

fn assert_policy_abstention(root: &std::path::Path, agent: &str, tool: &str) {
    let command = || {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_lm-resizer"));
        cmd.env("HOME", root)
            .env_remove("CODEX_HOME")
            .env_remove("CLAUDE_CONFIG_DIR")
            .env_remove("VIBE_HOME")
            .env_remove("COPILOT_HOME")
            .current_dir(root);
        cmd
    };
    let out = command()
        .args(["hook", "check", "--agent", agent, "git status"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();

    let payload = serde_json::json!({"permission_mode":"default", "hook_event_name":"PreToolUse",
        "tool_name":tool,"tool_input":{"command":"git status","timeout":7}});
    let mut child = command()
        .args(["hook", agent])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.to_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert!(
        out.stdout.is_empty() && value["changed"] == false,
        "{agent}: preview {value}; native {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn vibe_toml_and_copilot_project_config_constraints_are_respected() {
    for (agent, relative, content) in [
        (
            "vibe",
            ".vibe/config.toml",
            "[tools.bash]\nallowlist = ['git status']\n",
        ),
        (
            "vibe",
            ".vibe/config.toml",
            "[tools.bash]\ndenylist = ['git status']\n",
        ),
        ("vibe", ".vibe/config.toml", "broken TOML ["),
        (
            "copilot",
            ".github/settings.json",
            r#"{"permissions":{"allow":["git status"]}}"#,
        ),
        (
            "copilot",
            ".copilot/config.json",
            r#"{"permissions":{"deny":["git status"]}}"#,
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
        assert_policy_abstention(root.path(), agent, "bash");
    }
}
