use std::process::Command;
#[test]
fn verify_distinguishes_missing_installed_and_modified_plugins() {
    let root = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("HOME", root.path())
            .current_dir(root.path())
            .args(args)
            .output()
            .unwrap()
    };
    assert!(!run(&["verify", "--agent", "pi"]).status.success());
    assert!(!root.path().join(".pi").exists());
    assert!(run(&["init", "--agent", "pi"]).status.success());
    assert!(run(&["verify", "--agent", "pi", "--json"]).status.success());
    std::fs::write(root.path().join(".pi/extensions/lm-resizer.ts"), "modified").unwrap();
    assert!(!run(&["verify", "--agent", "pi"]).status.success());
    assert_eq!(
        std::fs::read_to_string(root.path().join(".pi/extensions/lm-resizer.ts")).unwrap(),
        "modified"
    );
}
