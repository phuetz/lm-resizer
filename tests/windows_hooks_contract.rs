mod support;
use support::*;

#[test]
fn forced_hook_update_preserves_surrounding_instructions_at_original_position() {
    let dir = sandbox();
    let agents = dir.path().join("AGENTS.md");
    let install = || {
        lm(dir.path())
            .args(["install-hooks", "--client", "codex", "--project-dir"])
            .arg(dir.path())
            .arg("--force")
            .output()
            .unwrap()
    };
    assert!(install().status.success());
    let block = std::fs::read_to_string(&agents).unwrap();
    let original =
        format!("# Priorité avant\r\n\r\n{block}\r\n# Priorité après\r\ntexte final\r\n");
    std::fs::write(&agents, &original).unwrap();
    assert!(install().status.success());
    assert_eq!(std::fs::read_to_string(&agents).unwrap(), original);
}
