use serde_json::Value;
use std::process::Command;

#[test]
fn skill_grok_claims_are_true() {
    let skill_path = format!(
        "{}/skills/grok/lm-resizer/SKILL.md",
        env!("CARGO_MANIFEST_DIR")
    );
    let skill_text = std::fs::read_to_string(skill_path).unwrap();

    let temp_dir = tempfile::tempdir().unwrap();

    // (1) `--version` réussit, et le texte ne contient pas « no --version » ni « no `--version` »
    let version_output = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .arg("--version")
        .env("LM_RESIZER_STATE_DIR", temp_dir.path())
        .env("HOME", temp_dir.path())
        .output()
        .unwrap();

    assert!(
        version_output.status.success(),
        "lm-resizer --version should succeed"
    );

    let text_lower = skill_text.to_lowercase();
    assert!(
        !text_lower.contains("no --version"),
        "Text should not claim there is no --version"
    );
    assert!(
        !text_lower.contains("no `--version`"),
        "Text should not claim there is no `--version`"
    );

    // (2) pour chaque nom de `doctor --json` -> `mcp_tools`, le texte de SKILL.md le contient
    let doctor_output = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .arg("doctor")
        .arg("--json")
        .env("LM_RESIZER_STATE_DIR", temp_dir.path())
        .env("HOME", temp_dir.path())
        .output()
        .unwrap();

    assert!(
        doctor_output.status.success(),
        "lm-resizer doctor --json should succeed"
    );

    let doctor_json: Value = serde_json::from_slice(&doctor_output.stdout).unwrap();
    let mcp_tools = doctor_json["mcp_tools"].as_array().unwrap();

    for tool in mcp_tools {
        let tool_name = tool.as_str().unwrap();
        assert!(
            skill_text.contains(tool_name),
            "SKILL.md should contain tool name: {}",
            tool_name
        );
    }
}
