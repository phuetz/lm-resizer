//! Direct native command adapters must preserve argv, exit codes and raw recovery.
#![cfg(unix)]
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};

#[test]
fn native_adapter_preserves_arguments_and_failure() {
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("phpunit");
    std::fs::write(&program, "#!/bin/sh\nprintf '%s\\n' \"$@\"\nexit 17\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths(
        std::iter::once(dir.path().to_path_buf())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("PATH", path)
        .env("LM_RESIZER_STATE_DIR", dir.path().join("state"))
        .args([
            "phpunit",
            "--filter",
            "name with spaces; literal",
            "--no-coverage",
        ])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(17));
    // Small reductions may omit the visible hint; the archive stays available.
    let key = std::fs::read_dir(dir.path().join("state/tee"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .file_name();
    let recovered = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", dir.path().join("state"))
        .args(["tee", "read"])
        .arg(key)
        .output()
        .unwrap();
    assert_eq!(
        recovered.stdout,
        b"--filter\nname with spaces; literal\n--no-coverage\n"
    );
}

#[test]
fn expand_recovers_a_view_without_accessing_the_original() {
    let dir = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", dir.path())
        .arg("expand")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"LMR-LINES/2\n@\"src/\"\na.rs\nb.rs\n&0\n!1\n[raw: abc123]\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"src/a.rs\nsrc/b.rs\nsrc/a.rs\n");
}

#[test]
fn gain_alias_reports_exact_tracking_fields() {
    let dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", dir.path())
        .args(["gain", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["exec_history"]["commands"], 0);
}

#[test]
fn pipe_filters_without_executing_and_preserves_errors_and_exit_status() {
    let dir = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", dir.path())
        .args(["pipe", "--filter", "pytest", "--exit-code", "2", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let raw =
        "ERROR collecting a.py\nModuleNotFoundError: missing dependency\n457 errors in 5.44s\n";
    child
        .stdin
        .take()
        .unwrap()
        .write_all(raw.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!value["output"]
        .as_str()
        .unwrap()
        .starts_with("[FAIL] Command failed (exit code: 2)\n"));
    assert!(value["output"].as_str().unwrap().contains(raw));
    assert_eq!(value["exit_code"], 2);
}

#[test]
#[cfg(unix)]
fn path_skips_non_executable_program_before_system_env() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let decoy = dir.path().join("env");
    std::fs::write(&decoy, "not an executable\n").unwrap();
    std::fs::set_permissions(&decoy, std::fs::Permissions::from_mode(0o644)).unwrap();
    let path = std::env::join_paths([
        dir.path(),
        std::path::Path::new("/usr/bin"),
        std::path::Path::new("/bin"),
    ])
    .unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("PATH", path)
        .env("LM_RESIZER_STATE_DIR", dir.path().join("state"))
        .args(["exec", "--json", "--", "env", "-i", "LM_SENTINEL=resolved"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["output"], "LM_SENTINEL=resolved\n");
}

#[test]
fn git_history_with_patches_keeps_each_commit_and_its_complete_patch() {
    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .current_dir(dir.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output);
        output.stdout
    };
    git(&["init", "-q"]);
    git(&["config", "user.name", "Fixture User"]);
    git(&["config", "user.email", "fixture@example.test"]);
    for (contents, message) in [
        ("before\n".repeat(30), "first"),
        ("after\n".repeat(30), "second"),
    ] {
        std::fs::write(dir.path().join("source.txt"), contents).unwrap();
        git(&["add", "source.txt"]);
        git(&["commit", "-qm", message]);
    }
    let args = ["log", "-p", "--no-color", "--format=%h %s"];
    let expected = git(&args);
    assert!(String::from_utf8_lossy(&expected).contains("+after"));
    let state = dir.path().join("state");
    for direct in [true, false] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_lm-resizer"));
        command
            .current_dir(dir.path())
            .env("LM_RESIZER_STATE_DIR", &state)
            .env("LM_RESIZER_STORE", dir.path().join("ccr.sqlite"))
            .env("LM_RESIZER_TRACKING", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null");
        if !direct {
            command.args(["exec", "--"]);
        }
        let output = command.arg("git").args(args).output().unwrap();
        assert!(output.status.success(), "{:?}", output);
        assert!(
            output.stdout.len() < expected.len(),
            "patch must actually shrink: direct={direct}"
        );
        let mut decoder = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .arg("expand")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        decoder
            .stdin
            .take()
            .unwrap()
            .write_all(&output.stdout)
            .unwrap();
        let expanded = decoder.wait_with_output().unwrap();
        assert!(expanded.status.success());
        assert_eq!(expanded.stdout, expected, "direct={direct}");
    }
    for file in std::fs::read_dir(state.join("tee")).unwrap() {
        let recalled = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("LM_RESIZER_STATE_DIR", &state)
            .args(["tee", "read"])
            .arg(file.unwrap().file_name())
            .output()
            .unwrap();
        assert!(recalled.status.success());
        assert_eq!(recalled.stdout, expected);
    }
}
