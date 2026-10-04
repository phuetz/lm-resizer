#![cfg(unix)]
use std::process::Command;
fn cli(dir: &std::path::Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lm-resizer"));
    cmd.env("LM_RESIZER_STATE_DIR", dir.join("state"))
        .env("LM_RESIZER_STORE", dir.join("store.sqlite"));
    cmd
}
#[test]
fn arbitrary_wrapper_has_exact_interleaved_tee_and_producer_status() {
    let dir = tempfile::tempdir().unwrap();
    for mode in ["err", "test", "summary", "proxy"] {
        let output = cli(dir.path())
            .args([
                mode,
                "--json",
                "--",
                "sh",
                "-c",
                "printf 'out\\n'; printf 'ERROR invoice\\n' >&2; printf 'last\\n'; exit 7",
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(7));
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let hint = report["tee_hint"].as_str().unwrap();
        let key = hint.trim_start_matches("[raw: ").trim_end_matches(']');
        let restored = cli(dir.path()).args(["tee", "read", key]).output().unwrap();
        assert!(restored.status.success());
        assert_eq!(restored.stdout, b"out\nERROR invoice\nlast\n");
    }
    let listed = cli(dir.path()).args(["recall", "--list"]).output().unwrap();
    assert!(listed.status.success());
    assert!(!listed.stdout.is_empty());
}

#[test]
fn missing_generic_program_reports_real_launch_status_first() {
    let dir = tempfile::tempdir().unwrap();
    let out = cli(dir.path())
        .args(["err", "--", "lm-resizer-certainly-missing-command"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(127));
    let view = String::from_utf8(out.stdout).unwrap();
    assert!(view.starts_with("[FAIL] Command failed (exit code: 127)\n"));
    assert!(view.contains("command not found"));
}
#[test]
fn dependencies_and_file_read_are_public_commands() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"dependencies":{"example":"^2.0"},"devDependencies":{"test-kit":"1"}}"#,
    )
    .unwrap();
    let out = cli(dir.path())
        .arg("deps")
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("example"));
    assert!(text.contains("test-kit"));
    let file = dir.path().join("source.txt");
    std::fs::write(&file, "one\ntwo\nthree\n").unwrap();
    let out = cli(dir.path())
        .arg("read")
        .arg(&file)
        .args(["--head-lines", "1"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(out.stdout.starts_with(b"one\n"));
    assert!(!out.stdout.windows(3).any(|w| w == b"two"));
    let out = cli(dir.path())
        .args(["rewrite", "head", "-n", "1"])
        .arg(&file)
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("lm-resizer read"));
}

#[test]
fn new_agent_hooks_install_and_rewrite_without_losing_arguments() {
    use std::io::Write;
    use std::process::Stdio;
    let dir = tempfile::tempdir().unwrap();
    for (client, tool, file, pointer, event) in [
        (
            "gemini",
            "run_shell_command",
            ".gemini/settings.json",
            "/hookSpecificOutput/tool_input",
            "BeforeTool",
        ),
        (
            "copilot",
            "bash",
            ".github/hooks/lm-resizer.json",
            "/modifiedArgs",
            "preToolUse",
        ),
        (
            "cursor",
            "Shell",
            ".cursor/hooks.json",
            "/updated_input",
            "preToolUse",
        ),
    ] {
        let result = cli(dir.path())
            .args(["init", "--client", client, "--project-dir"])
            .arg(dir.path())
            .output()
            .unwrap();
        assert!(result.status.success(), "{:?}", result.stderr);
        let config = std::fs::read_to_string(dir.path().join(file)).unwrap();
        assert!(config.contains("hook --client"));
        let original = config.clone();
        let again = cli(dir.path())
            .args(["init", "--client", client, "--project-dir"])
            .arg(dir.path())
            .output()
            .unwrap();
        assert!(!again.status.success());
        assert_eq!(
            std::fs::read_to_string(dir.path().join(file)).unwrap(),
            original
        );
        let input = if client == "copilot" {
            serde_json::json!({"toolName":tool,"toolArgs":{"command":"git status","description":"keep this"}})
        } else {
            serde_json::json!({"tool_name":tool,"tool_input":{"command":"git status","description":"keep this"}})
        };
        let mut child = cli(dir.path())
            .args(["hook", "--client", client, "--event", event])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.to_string().as_bytes())
            .unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(result.status.success());
        let output: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        let args = output.pointer(pointer).unwrap();
        assert_eq!(args["description"], "keep this");
        assert!(args["command"]
            .as_str()
            .unwrap()
            .ends_with("exec -- git status"));
        let audit = std::fs::read_to_string(dir.path().join("state/hook-audit.jsonl")).unwrap();
        assert!(!audit.contains("git status"));
        assert!(!audit.contains("keep this"));
    }
}

#[test]
fn literal_shell_and_hook_audit_keep_local_contracts() {
    let dir = tempfile::tempdir().unwrap();
    let out = cli(dir.path())
        .args([
            "run",
            "-c",
            "printf 'literal\\n'; printf 'diagnostic\\n' >&2; exit 9",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(9));
    assert_eq!(out.stdout, b"literal\ndiagnostic\n");
    std::fs::create_dir_all(dir.path().join("state")).unwrap();
    std::fs::write(
        dir.path().join("state/hook-audit.jsonl"),
        "{\"client\":\"gemini\"}\ninvalid\n{\"client\":\"gemini\"}\n{\"client\":\"cursor\"}\n",
    )
    .unwrap();
    let out = cli(dir.path()).args(["hook-audit"]).output().unwrap();
    assert!(out.status.success());
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["rewrites"], 3);
    assert_eq!(report["clients"]["gemini"], 2);
}

#[test]
fn inspection_schema_environment_and_formatter_have_recoverable_contracts() {
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let input = "{\"invoices\":[{\"amount\":42,\"paid\":false}],\"note\":null}";
    let file = dir.path().join("data.json");
    std::fs::write(&file, input).unwrap();
    let out = cli(dir.path()).arg("json").arg(file).output().unwrap();
    assert!(out.status.success());
    for key in ["invoices", "amount", "paid", "note"] {
        assert!(String::from_utf8_lossy(&out.stdout).contains(key));
    }
    let key = format!("{:x}", Sha256::digest(input.as_bytes()));
    assert_eq!(
        cli(dir.path())
            .args(["tee", "read", &key])
            .output()
            .unwrap()
            .stdout,
        input.as_bytes()
    );
    let out = cli(dir.path())
        .env("INSPECTION_PASS", "never-print")
        .args(["env", "INSPECTION_PASS"])
        .output()
        .unwrap();
    assert_eq!(out.stdout, b"INSPECTION_PASS=[redacted]\n");
    let out = cli(dir.path()).arg("config").output().unwrap();
    let config: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(config["tokenizer"], "o200k_base");
    let shim = dir.path().join("ruff");
    std::fs::write(
        &shim,
        "#!/bin/sh\nprintf '%s\\n' \"$@\"\nprintf 'ERROR invalid syntax\\n' >&2\nexit 7\n",
    )
    .unwrap();
    std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = cli(dir.path())
        .env("PATH", dir.path())
        .args(["format", "ruff", "--check", "a b.py"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(7));
    let raw = b"format\n--check\na b.py\nERROR invalid syntax\n";
    let mut expected = b"[FAIL] Command failed (exit code: 7)\n".to_vec();
    expected.extend_from_slice(raw);
    assert_eq!(out.stdout, expected);
    let key = format!("{:x}", Sha256::digest(raw));
    assert_eq!(
        cli(dir.path())
            .args(["tee", "read", &key])
            .output()
            .unwrap()
            .stdout,
        raw
    );
}

#[test]
fn environment_masks_passphrase_variants_and_conservative_pass_components() {
    let dir = tempfile::tempdir().unwrap();
    let out = cli(dir.path())
        .env_clear()
        .env("LM_RESIZER_STATE_DIR", dir.path().join("state"))
        .env("INSPECTION_PASSPHRASE_FILE", "/keys/private-location")
        .env("INSPECTION_PASS_COUNT", "42")
        .env("INSPECTION_LANGUAGE", "Rust")
        .args(["env", "INSPECTION_"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(out.stdout, b"INSPECTION_LANGUAGE=Rust\nINSPECTION_PASSPHRASE_FILE=[redacted]\nINSPECTION_PASS_COUNT=[redacted]\n");
}
