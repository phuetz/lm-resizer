#![cfg(unix)]
use std::process::Command;
#[test]
fn skip_env_reaches_child_and_verbose_stays_on_stderr() {
    let state = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", state.path())
        .args([
            "-vv",
            "exec",
            "--skip-env",
            "--",
            "sh",
            "-c",
            "printf '%s' \"$SKIP_ENV_VALIDATION\"",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "1");
    assert!(String::from_utf8_lossy(&out.stderr).contains("verbosity 2"));
}
