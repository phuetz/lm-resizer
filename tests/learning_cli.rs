use serde_json::{json, Value};
use std::process::Command;
#[test]
fn learn_filters_observations_and_writes_non_destructive_rules() {
    let root = tempfile::tempdir().unwrap();
    let cwd = root.path().to_str().unwrap();
    let rows = [
        json!({"cwd":cwd}),
        json!({"type":"tool_use","id":"a","name":"Bash","input":{"command":"git status --bad"}}),
        json!({"type":"tool_result","tool_use_id":"a","content":"error: unknown option bad","is_error":true}),
        json!({"type":"tool_use","id":"b","name":"Bash","input":{"command":"git status"}}),
        json!({"type":"tool_result","tool_use_id":"b","content":"clean","is_error":false}),
    ];
    let path = root.path().join("session.jsonl");
    std::fs::write(
        &path,
        rows.iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    let run = |extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("HOME", root.path())
            .current_dir(root.path())
            .args(["learn", "--since", "0", "--json"])
            .args(extra)
            .arg(&path)
            .output()
            .unwrap()
    };
    let out = run(&["--write-rules"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["corrections"][0]["corrected"], "git status");
    let rules = root.path().join(".claude/rules/cli-corrections.md");
    assert!(rules.exists());
    std::fs::write(&rules, "personal rules").unwrap();
    assert!(!run(&["--write-rules"]).status.success());
    assert_eq!(std::fs::read_to_string(&rules).unwrap(), "personal rules");
    let out = run(&["--min-occurrences", "2"]);
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["corrections"].as_array().unwrap().is_empty());
}
