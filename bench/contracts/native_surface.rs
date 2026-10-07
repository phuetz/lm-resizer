//! User-facing labels must belong to this product, including unchanged views.
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn output_metadata_and_help_do_not_advertise_another_product() {
    let state = tempfile::tempdir().unwrap();
    let binary = env!("CARGO_BIN_EXE_lm-resizer");
    for filter in [
        "git-log",
        "git-diff",
        "git-status",
        "grep",
        "find",
        "pytest",
        "cargo-test",
        "tsc",
    ] {
        let mut child = Command::new(binary)
            .env("LM_RESIZER_STATE_DIR", state.path())
            .args(["pipe", "--filter", filter, "--json"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"diagnostic: retained\n")
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success());
        let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_own_name(report["filter"].as_str().unwrap());
        assert_own_name(&String::from_utf8_lossy(&out.stderr));
        assert_own_name(report["output"].as_str().unwrap());
    }
    let out = Command::new(binary).arg("--help").output().unwrap();
    assert!(out.status.success());
    assert_own_name(&String::from_utf8_lossy(&out.stdout));
    let mut direct = Command::new(binary)
        .env("LM_RESIZER_STATE_DIR", state.path())
        .args(["exec", "--json", "--"])
        .args(echo_stdin_command())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    direct
        .stdin
        .take()
        .unwrap()
        .write_all(b"native capture\n")
        .unwrap();
    let out = direct.wait_with_output().unwrap();
    assert!(out.status.success());
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_own_name(report["filter"].as_str().unwrap());
    // findstr (Windows) rewrites line endings; the content is what matters.
    let output = report["output"].as_str().unwrap().replace("\r\n", "\n");
    assert_eq!(output, "native capture\n");
}

/// A command that copies stdin to stdout. `cat` does not exist on Windows;
/// `findstr "^"` is the stock equivalent and matches every line.
fn echo_stdin_command() -> &'static [&'static str] {
    if cfg!(windows) {
        &["findstr", "^"]
    } else {
        &["cat"]
    }
}

fn assert_own_name(text: &str) {
    for name in ["rtk", "headroom"] {
        assert!(!text.to_lowercase().contains(name), "foreign label: {text}");
    }
}

#[test]
fn structured_views_use_native_names() {
    let state = tempfile::tempdir().unwrap();
    let json = serde_json::to_string(&vec![serde_json::json!({"package": "long_name", "version": "1.0", "description": "ordinary package metadata"}); 50]).unwrap();
    let logs = "INFO request accepted service=worker\n".repeat(500);
    for (raw, expected) in [(json, "lossless:json-table"), (logs, "lossless:log-runs")] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("LM_RESIZER_STATE_DIR", state.path())
            .args(["tool-output", "--json", "--command", "fixture"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(raw.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success());
        let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(report["filter"], expected);
        assert_own_name(report["filter"].as_str().unwrap());
        assert_own_name(report["output"].as_str().unwrap());
    }
}
