use std::process::Command;
#[test]
fn recall_unique_prefix_list_and_windows() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("tee")).unwrap();
    let key = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
    std::fs::write(root.path().join(format!("tee/{key}.log")), "a\r\nb\r\nc").unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("LM_RESIZER_STATE_DIR", root.path())
            .args(args)
            .output()
            .unwrap()
    };
    let out = run(&["recall", "abcdef", "--from", "2", "--lines", "1"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout, b"b\r\n");
    assert!(String::from_utf8_lossy(&run(&["recall", "--list"]).stdout).contains(key));
    std::fs::write(root.path().join("tee/abcdef999999.log"), "other").unwrap();
    assert!(!run(&["recall", "abcdef"]).status.success());
    assert!(!run(&["recall", "../secret"]).status.success());
    assert!(!run(&["recall", key, "--from", "0"]).status.success());
}
