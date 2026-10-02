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
    assert_eq!(
        String::from_utf8(result.stdout).unwrap(),
        "--filter\nname with spaces; literal\n--no-coverage\n"
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
        .arg("gain")
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["exec_history"]["commands"], 0);
}
