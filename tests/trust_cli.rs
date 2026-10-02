use std::process::{Command, Stdio};
#[test]
fn trust_aliases_require_consent_and_register_only_verified_content() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".lm-resizer")).unwrap();
    std::fs::write(root.path().join(".lm-resizer/filters.toml"),"[[filters]]\nname = \"sample\"\nmatch_command = \"^sample\"\nkeep_lines_matching = [\"error\"]\n").unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("HOME", root.path())
            .env("LM_RESIZER_STATE_DIR", root.path().join("state"))
            .current_dir(root.path())
            .stdin(Stdio::null())
            .args(args)
            .output()
            .unwrap()
    };
    assert!(!run(&["trust"]).status.success());
    assert!(run(&["trust", "--yes"]).status.success());
    assert!(String::from_utf8_lossy(&run(&["trust", "--list"]).stdout).contains("filters.toml"));
    assert!(run(&["untrust"]).status.success());
    assert_eq!(
        String::from_utf8_lossy(&run(&["trust", "--list"]).stdout).trim(),
        "[]"
    );
}
