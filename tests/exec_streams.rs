//! Child stream provenance, failure output and Unix signal contracts.
#![cfg(unix)]
use serde_json::Value;
use std::process::Command;

#[test]
fn mixed_streams_remain_identified_and_exit_code_survives() {
    for code in [0, 37] {
        let state = tempfile::tempdir().unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("LM_RESIZER_STATE_DIR", state.path())
            .args(["exec", "--json", "--", "sh", "-c"])
            .arg(format!(
                "printf 'ok\\n'; printf 'diagnostic\\n' >&2; exit {code}"
            ))
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(code));
        let report: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(report["exit_code"], code);
        assert_eq!(report["streams"]["stdout_bytes"], 3);
        assert_eq!(report["streams"]["stderr_bytes"], 11);
        let text = report["output"].as_str().unwrap();
        assert!(text.contains("ok\n"));
        assert!(text.contains("[stderr]\ndiagnostic"));
    }
}

#[test]
fn raw_failure_keeps_whitespace_and_stderr_only_is_labeled() {
    for (script, expected) in [
        (
            "printf '  \\n'; printf 'err\\n' >&2; exit 8",
            "  \n\n[stderr]\nerr\n",
        ),
        ("printf 'err\\n' >&2; exit 8", "[stderr]\nerr\n"),
    ] {
        let state = tempfile::tempdir().unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("LM_RESIZER_STATE_DIR", state.path())
            .args([
                "exec",
                "--json",
                "--raw-on-failure",
                "--",
                "sh",
                "-c",
                script,
            ])
            .output()
            .unwrap();
        let report: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(out.status.code(), Some(8));
        assert_eq!(report["output"], expected);
    }
}

#[test]
fn child_signal_uses_shell_exit_convention() {
    let state = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", state.path())
        .args(["exec", "--json", "--", "sh", "-c", "kill -TERM $$"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(143));
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["exit_code"], 143);
}
