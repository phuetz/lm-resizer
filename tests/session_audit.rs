use serde_json::{json, Value};
use std::process::Command;
#[test]
fn discover_scopes_projects_dates_and_limits_and_session_measures_adoption() {
    let root = tempfile::tempdir().unwrap();
    for (name, cwd, date) in [
        ("a", "/wanted/project", "2026-10-02T12:00:00Z"),
        ("b", "/other", "2026-10-02T12:00:00Z"),
        ("c", "/wanted/project", "2020-01-01T00:00:00Z"),
    ] {
        let rows = [
            json!({"cwd":cwd,"timestamp":date}),
            json!({"type":"tool_use","id":"a","name":"Bash","input":{"command":"git status"}}),
            json!({"type":"tool_result","tool_use_id":"a","content":"On branch main\n"}),
            json!({"type":"tool_use","id":"b","name":"Bash","input":{"command":"lm-resizer exec -- git status"}}),
        ];
        std::fs::write(
            root.path().join(format!("{name}.jsonl")),
            rows.iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .unwrap();
    }
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("HOME", root.path())
            .env("LM_RESIZER_STATE_DIR", root.path().join("state"))
            .args(args)
            .arg(root.path())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice::<Value>(&out.stdout).unwrap()
    };
    let report = run(&[
        "discover", "-p", "wanted", "--since", "0", "--limit", "1", "--json",
    ]);
    assert_eq!(report["sessions"].as_array().unwrap().len(), 2);
    assert_eq!(report["opportunities_total"], 2);
    assert_eq!(report["opportunities"].as_array().unwrap().len(), 1);
    assert_eq!(report["sessions"][0]["adoption_percent"], 50.0);
    let report = run(&["session", "--all", "--since", "0", "-f", "json"]);
    assert_eq!(report["sessions"].as_array().unwrap().len(), 3);
}
