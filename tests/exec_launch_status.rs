//! Launch failures must have the same status across all exec capture modes.
use std::process::{Command, Output};
use tempfile::tempdir;

fn capture(options: &[&str], program: &str) -> Output {
    let state = tempdir().unwrap();
    Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", state.path())
        .env("LM_RESIZER_TRACKING", "0")
        .env("PATH", state.path().join("empty-path"))
        .arg("exec")
        .args(options)
        .arg("--")
        .arg(program)
        .output()
        .unwrap()
}

const MODES: &[&[&str]] = &[
    &[],
    &["--raw-on-failure"],
    &["--stream"],
    &["--stream", "--raw-on-failure"],
];

#[test]
fn missing_pytest_returns_127_in_every_capture_mode() {
    for options in MODES {
        let output = capture(options, "pytest");
        assert_eq!(output.status.code(), Some(127), "mode {options:?}");
        let diagnostic = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(diagnostic.contains("pytest"), "{diagnostic}");
    }
}

#[test]
#[cfg(unix)]
fn denied_executable_returns_126_in_every_capture_mode() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = tempdir().unwrap();
    let program = fixture.path().join("not-executable");
    std::fs::write(&program, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o600)).unwrap();
    for options in MODES {
        let output = capture(options, program.to_str().unwrap());
        assert_eq!(output.status.code(), Some(126), "mode {options:?}");
    }
}
