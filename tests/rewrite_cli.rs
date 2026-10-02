#![cfg(unix)]
use std::process::Command;
#[test]
fn rewrite_accepts_full_shell_line_and_escapes_literal_argv() {
    let binary = env!("CARGO_BIN_EXE_lm-resizer");
    let out = Command::new(binary)
        .args(["rewrite", "git status && git diff"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("lm-resizer exec -- git status"));
    assert!(text.contains("&&"));
    let root = tempfile::tempdir().unwrap();
    let marker = root.path().join("should-not-exist");
    let argument = format!("$(touch {})", marker.display());
    let out = Command::new(binary)
        .args(["rewrite", "git", "show", &argument])
        .output()
        .unwrap();
    assert!(out.status.success());
    let rewritten = String::from_utf8(out.stdout).unwrap();
    assert!(rewritten.contains("\\$"));
    // Replace the wrapper prefix with printf to inspect reparsed argv without running git.
    let inspect = rewritten.replacen("lm-resizer exec -- git show", "printf '%s'", 1);
    let out = Command::new("sh").args(["-c", &inspect]).output().unwrap();
    assert_eq!(String::from_utf8(out.stdout).unwrap(), argument);
    assert!(!marker.exists());
}

#[test]
fn shell_string_rewrite_preserves_quotes_and_expansions_verbatim() {
    let command = r#"git show "$REV" 'literal $value' "a\|b""#;
    let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .args(["rewrite", command])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap().trim(),
        format!("lm-resizer exec -- {command}")
    );
}
