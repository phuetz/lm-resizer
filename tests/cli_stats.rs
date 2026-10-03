use std::{fs, process::Command};

#[test]
fn stats_handles_invalid_history_lines() {
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("state");
    fs::create_dir(&state).unwrap();
    fs::write(
        state.join("exec-history.jsonl"),
        "{\"command\":\"cargo test\",\"original_bytes\":100,\"compressed_bytes\":50,\"bytes_saved\":50}\n{\"orig\n\n",
    )
    .unwrap();

    let exe = env!("CARGO_BIN_EXE_lm-resizer");
    let json_output = Command::new(exe)
        .env("LM_RESIZER_STATE_DIR", &state)
        .arg("stats")
        .output()
        .unwrap();
    assert!(json_output.status.success(), "{json_output:?}");
    let report: serde_json::Value = serde_json::from_slice(&json_output.stdout).unwrap();
    let history = &report["exec_history"];
    assert_eq!(history["commands"], 1);
    assert_eq!(history["exec_history_invalid_lines"], 1);
    assert_eq!(history["bytes_saved"], 50);

    let markdown_output = Command::new(exe)
        .env("LM_RESIZER_STATE_DIR", &state)
        .arg("stats")
        .arg("--markdown")
        .output()
        .unwrap();
    assert!(markdown_output.status.success(), "{markdown_output:?}");
    let markdown = String::from_utf8(markdown_output.stdout).unwrap();
    assert!(markdown.contains("Warning: 1 invalid line(s) ignored in exec-history.jsonl"));
}
