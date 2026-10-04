use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::tempdir;

#[test]
#[cfg(unix)]
fn test_exec_stdin_normal() {
    let temp_dir = tempdir().unwrap();
    let state_dir = temp_dir.path().join("state");
    let store_path = temp_dir.path().join("ccr.sqlite3");

    let mut child = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", &state_dir)
        .arg("exec")
        .arg("--store")
        .arg(&store_path)
        .arg("--")
        .arg("cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn lm-resizer");

    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        stdin
            .write_all(b"hello\n")
            .expect("Failed to write to stdin");
    }

    let output = child.wait_with_output().expect("Failed to read stdout");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("hello"),
        "stdout did not contain 'hello', got: {}",
        stdout
    );
}

#[test]
#[cfg(unix)]
fn test_exec_stdin_stream() {
    let temp_dir = tempdir().unwrap();
    let state_dir = temp_dir.path().join("state");
    let store_path = temp_dir.path().join("ccr.sqlite3");

    let mut child = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", &state_dir)
        .arg("exec")
        .arg("--stream")
        .arg("--store")
        .arg(&store_path)
        .arg("--")
        .arg("cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn lm-resizer");

    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        stdin
            .write_all(b"hello\n")
            .expect("Failed to write to stdin");
    }

    let output = child.wait_with_output().expect("Failed to read stdout");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("hello"),
        "stdout did not contain 'hello', got: {}",
        stdout
    );
}

#[test]
#[cfg(unix)]
fn test_exec_stdin_null() {
    let temp_dir = tempdir().unwrap();
    let state_dir = temp_dir.path().join("state");
    let store_path = temp_dir.path().join("ccr.sqlite3");

    let child = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", &state_dir)
        .arg("exec")
        .arg("--store")
        .arg(&store_path)
        .arg("--")
        .arg("cat")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn lm-resizer");

    let output = child.wait_with_output().expect("Failed to read stdout");
    assert!(output.status.success());
}
