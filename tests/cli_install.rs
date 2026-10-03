use std::process::Command;

#[test]
fn test_cli_install_nonexistent_project_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let absent = tmp.path().join("absent");

    let output = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env_clear()
        .env("HOME", tmp.path())
        .env("USERPROFILE", tmp.path())
        .arg("install")
        .arg("--client")
        .arg("cursor")
        .arg("--scope")
        .arg("project")
        .arg("--project-dir")
        .arg(&absent)
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        output.status.code(),
        Some(1),
        "Expected non-zero exit code, stderr: {}",
        stderr
    );
    assert!(
        stderr.contains("does not exist"),
        "Expected 'does not exist' in stderr, got: {}",
        stderr
    );
    assert!(
        stderr.contains("absent"),
        "Expected 'absent' in stderr, got: {}",
        stderr
    );
    assert!(!absent.exists(), "Expected project dir to not be created");
}
