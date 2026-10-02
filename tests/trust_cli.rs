use std::io::Write;
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
    std::fs::OpenOptions::new().append(true).open(root.path().join(".lm-resizer/filters.toml")).unwrap()
        .write_all(b"\n[[tests]]\nfilter = 'sample'\nname = 'keeps error'\ninput = \"error\\n\"\nexpected = \"error\\n\"\n").unwrap();
    assert!(run(&["trust", "--yes"]).status.success());
    assert!(String::from_utf8_lossy(&run(&["trust", "--list"]).stdout).contains("filters.toml"));
    assert!(run(&["untrust"]).status.success());
    assert_eq!(
        String::from_utf8_lossy(&run(&["trust", "--list"]).stdout).trim(),
        "[]"
    );
}

#[test]
fn trust_refuses_missing_fixture_coverage_before_registration_or_hook_installation() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".lm-resizer")).unwrap();
    let path = root.path().join(".lm-resizer/filters.toml");
    let filter = "[[filters]]\nname = 'sample'\nmatch_command = '^sample'\nkeep_lines_matching = ['error']\n";
    std::fs::write(&path, filter).unwrap();
    for args in [
        vec!["trust", "--yes"],
        vec!["trust-filters"],
        vec!["init", "--agent", "claude", "--trust-filters"],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("HOME", root.path())
            .env("LM_RESIZER_STATE_DIR", root.path().join("state"))
            .current_dir(root.path())
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(!out.status.success(), "missing fixtures must prevent trust");
        assert!(!root.path().join(".claude/settings.json").exists());
        let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("HOME", root.path())
            .env("LM_RESIZER_STATE_DIR", root.path().join("state"))
            .current_dir(root.path())
            .args(["trust", "--list"])
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "[]");
    }
}
