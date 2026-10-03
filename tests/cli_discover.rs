use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_discover_tilde_expansion() {
    let temp = tempdir().unwrap();
    let home_dir = temp.path();

    // Create a mock `.codex` dir and file
    let codex_dir = home_dir.join(".codex");
    fs::create_dir(&codex_dir).unwrap();
    let mock_file = codex_dir.join("x.jsonl");
    fs::write(&mock_file, "{}").unwrap();

    let exe = env!("CARGO_BIN_EXE_lm-resizer");

    let output = Command::new(exe)
        .env_clear()
        .env("HOME", home_dir)
        .env("USERPROFILE", home_dir)
        .env("LM_RESIZER_STATE_DIR", home_dir.join(".lm-resizer"))
        .arg("discover")
        .arg("~/.codex")
        .arg("--recursive")
        .arg("--json")
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "Command failed: {:?}", output);

    let stdout = String::from_utf8_lossy(&output.stdout);

    // We expect the JSON to be generated and files_scanned to be at least 1.
    // The exact JSON structure contains `files_scanned`.
    assert!(
        stdout.contains("\"files_scanned\": 1") || stdout.contains("\"files_scanned\":1"),
        "Output should show 1 file scanned: {}",
        stdout
    );
}
