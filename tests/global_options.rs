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

#[test]
fn skip_env_after_native_command_respects_literal_separator() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let git = root.path().join("git");
    std::fs::write(
        &git,
        "#!/bin/sh\nprintf '%s|%s\\n' \"$SKIP_ENV_VALIDATION\" \"$*\"\n",
    )
    .unwrap();
    std::fs::set_permissions(git, std::fs::Permissions::from_mode(0o755)).unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("PATH", root.path())
            .env("LM_RESIZER_STATE_DIR", root.path().join("state"))
            .env_remove("SKIP_ENV_VALIDATION")
            .args(args)
            .output()
            .unwrap()
    };
    let out = run(&["git", "--skip-env", "status"]);
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "1|status\n");
    let out = run(&["git", "--", "--skip-env"]);
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "|-- --skip-env\n");
}
