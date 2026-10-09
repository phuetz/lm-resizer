//! Child stream provenance, failure output and Unix signal contracts.
#![cfg(unix)]
mod support;
use serde_json::Value;
use std::process::Command;
use support::is_interleaving;

#[test]
fn mixed_streams_keep_execution_order_and_exit_code_survives() {
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
        assert!(report["streams"].is_null());
        assert_eq!(report["original_bytes"], 14);
        let text = report["output"].as_str().unwrap();
        assert!(text.contains("ok\n"));
        if code == 0 {
            assert_eq!(text, "ok\ndiagnostic\n");
        } else {
            assert_eq!(
                text,
                "[FAIL] Command failed (exit code: 37)\nok\ndiagnostic\n"
            );
        }
    }
}

#[test]
fn raw_failure_keeps_whitespace_and_stderr_only_is_labeled() {
    for (script, expected) in [
        (
            "printf '  \\n'; printf 'err\\n' >&2; exit 8",
            "  \n\n[stderr]\nerr\n[capture: stdout and stderr captured separately; displayed order is not chronological]\n",
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
        assert_eq!(
            report["output"],
            format!("[FAIL] Command failed (exit code: 8)\n{expected}")
        );
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

#[test]
fn legacy_filter_cannot_discard_the_stderr_boundary() {
    use std::os::unix::fs::PermissionsExt;
    let state = tempfile::tempdir().unwrap();
    let npm = state.path().join("npm");
    std::fs::write(&npm, "#!/bin/sh\nprintf 'warning: preserve this diagnostic\\ncontext 1\\ncontext 2\\ncontext 3\\nstdout detail\\n'\nprintf 'separate stream detail\\n' >&2\n").unwrap();
    std::fs::set_permissions(&npm, std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", state.path().join("state"))
        .env("LM_RESIZER_NO_TOML_FILTERS", "1")
        .args(["exec", "--raw-on-failure", "--json", "--"])
        .arg(npm)
        .args(["run", "build"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["filter"], "native:packages");
    assert!(report["output"]
        .as_str()
        .unwrap()
        .contains("[stderr]\nseparate stream detail"));
}

#[test]
fn non_utf8_bytes_survive_tee_in_captured_and_streamed_execution() {
    for stream in [false, true] {
        let state = tempfile::tempdir().unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_lm-resizer"));
        command
            .env("LM_RESIZER_STATE_DIR", state.path())
            .args(["exec", "--json"]);
        if stream {
            command.arg("--stream");
        }
        let out = command
            .args([
                "--",
                "sh",
                "-c",
                "printf 'caf\\351_budget.txt\\n'; printf 'err\\377\\n' >&2; exit 9",
            ])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(9));
        // Streaming stdout contains the original bytes followed by the JSON report.
        let start = if stream {
            out.stdout.iter().position(|b| *b == b'{').unwrap()
        } else {
            0
        };
        let report: Value = serde_json::from_slice(&out.stdout[start..]).unwrap();
        let view = report["output"].as_str().unwrap();
        assert!(view.contains("non-UTF-8 capture"));
        assert!(view.contains("caf\\xE9_budget.txt"));
        assert!(!view.contains('\u{fffd}'));
        if stream {
            assert!(view.contains("[stderr]\nerr\\xFF\n"));
            assert_eq!(report["streams"]["stdout_bytes"], 16);
            assert_eq!(report["streams"]["stderr_bytes"], 5);
            assert_eq!(&out.stdout[..start], b"caf\xe9_budget.txt\n");
            assert_eq!(out.stderr, b"err\xff\n");
        } else {
            assert!(report["streams"].is_null());
        }
        let hint = report["tee_hint"].as_str().unwrap();
        let hash = hint
            .strip_prefix("[raw: ")
            .unwrap()
            .strip_suffix(']')
            .unwrap();
        // No paid trailer on a small/non-compressing view; JSON keeps the key.
        assert!(!view.contains("[tee:"));
        let recovered = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("LM_RESIZER_STATE_DIR", state.path())
            .args(["tee", "read", hash])
            .output()
            .unwrap();
        assert!(recovered.status.success());
        if stream {
            // Separate pipes: exact streams, drain order between them.
            assert!(
                is_interleaving(&recovered.stdout, b"caf\xe9_budget.txt\n", b"err\xff\n"),
                "{:?}",
                recovered.stdout
            );
        } else {
            assert_eq!(recovered.stdout, b"caf\xe9_budget.txt\nerr\xff\n");
        }
    }
}

#[test]
fn find_non_utf8_filename_is_recoverable_without_replacement() {
    use std::os::unix::ffi::OsStringExt;
    let dir = tempfile::tempdir().unwrap();
    let filename = std::ffi::OsString::from_vec(b"caf\xe9_budget.txt".to_vec());
    if let Err(err) = std::fs::write(dir.path().join(filename), b"invoice") {
        // APFS stores names as UTF-8 and answers EILSEQ (92): such a file cannot
        // exist there, so `find` can never print one. Every other error still fails.
        if cfg!(target_os = "macos") && err.raw_os_error() == Some(92) {
            eprintln!("skipped: this filesystem rejects non-UTF-8 file names ({err})");
            return;
        }
        panic!("cannot create the non-UTF-8 file name: {err}");
    }
    let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .current_dir(dir.path())
        .env("LM_RESIZER_STATE_DIR", dir.path().join("state"))
        .args([
            "exec",
            "--json",
            "--",
            "find",
            ".",
            "-maxdepth",
            "1",
            "-type",
            "f",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    // reference's view decodes the non-UTF-8 name; only the tee is a byte-exact
    // filename oracle. Never use the displayed spelling to recover this file.
    assert!(report["output"].as_str().unwrap().contains("budget.txt"));
    let hash = report["tee_hint"]
        .as_str()
        .unwrap()
        .strip_prefix("[raw: ")
        .unwrap()
        .strip_suffix(']')
        .unwrap();
    let recovered = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", dir.path().join("state"))
        .args(["tee", "read", hash])
        .output()
        .unwrap();
    assert!(recovered.status.success());
    assert_eq!(recovered.stdout, b"./caf\xe9_budget.txt\n");
}
