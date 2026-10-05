#[cfg(unix)]
#[test]
fn closed_stdout_pipe_exits_quietly_with_sigpipe_status() {
    use std::process::Command;

    let home = tempfile::tempdir().unwrap();
    let result = Command::new("bash")
        .arg("-o")
        .arg("pipefail")
        .arg("-c")
        .arg("\"$LMR_TEST_BIN\" run --command 'seq 100000' | head -1")
        .env("LMR_TEST_BIN", env!("CARGO_BIN_EXE_lm-resizer"))
        .env("HOME", home.path())
        .env("LM_RESIZER_STATE_DIR", home.path().join("state"))
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(141), "{result:?}");
    assert_eq!(result.stdout, b"1\n");
    assert!(result.stderr.is_empty(), "{result:?}");
}

#[cfg(unix)]
#[test]
fn tee_list_closed_pipe_exits_quietly() {
    use std::process::Command;

    let home = tempfile::tempdir().unwrap();
    let tee = home.path().join("state/tee");
    std::fs::create_dir_all(&tee).unwrap();
    for i in 0..400_u32 {
        std::fs::write(tee.join(format!("{i:064x}.log")), b"raw").unwrap();
    }
    let result = Command::new("bash")
        .arg("-o")
        .arg("pipefail")
        .arg("-c")
        .arg("\"$LMR_TEST_BIN\" tee list | head -1")
        .env("LMR_TEST_BIN", env!("CARGO_BIN_EXE_lm-resizer"))
        .env("HOME", home.path())
        .env("LM_RESIZER_STATE_DIR", home.path().join("state"))
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(141), "{result:?}");
    assert!(!result.stdout.is_empty());
    assert!(result.stderr.is_empty(), "{result:?}");
}
