use serde_json::{json, Value};
use std::process::Command;
#[test]
fn gain_exports_calendar_views_and_history_csv() {
    let state = tempfile::tempdir().unwrap();
    let record = json!({"timestamp_unix":1709251200,"cwd":"/project","command":"git status","tokenizer":"tiktoken-rs/o200k_base","token_count_method":"exact","original_tokens":10,"compressed_tokens":14});
    std::fs::write(state.path().join("exec-history.jsonl"), record.to_string()).unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("LM_RESIZER_STATE_DIR", state.path())
            .args(args)
            .output()
            .unwrap()
    };
    let out = run(&["gain", "--all", "--graph", "--format", "json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["monthly"][0]["period"], "2024-03");
    assert_eq!(report["weekly"][0]["period"], "2024-02-26");
    assert_eq!(report["daily"][0]["tokens_saved"], -4);
    assert!(report["graph"].as_str().unwrap().contains("-####"));
    let out = run(&["gain", "-H", "-f", "csv"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("\"git status\""));
    assert!(!run(&["gain", "--tier", "invalid"]).status.success());
}
