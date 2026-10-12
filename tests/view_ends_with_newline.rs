//! Une vue raccourcie se termine par un retour à la ligne : l'invite ne se colle pas à sa dernière
//! ligne (D13 de la recette). Une sortie rendue à l'identique garde ses octets, même sans.
#![cfg(unix)]
use std::process::Command;

fn run(dir: &std::path::Path, args: &[&str]) -> Vec<u8> {
    let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .current_dir(dir)
        .env("LM_RESIZER_STATE_DIR", dir.join("state"))
        .env("LM_RESIZER_TRACKING", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "{args:?}: {out:?}");
    out.stdout
}

fn git(dir: &std::path::Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(out.status.success(), "{args:?}: {out:?}");
}

#[test]
fn shortened_git_views_end_with_a_newline_and_identical_output_keeps_its_bytes() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.name", "T"]);
    git(dir.path(), &["config", "user.email", "t@example.test"]);
    std::fs::write(dir.path().join("f"), "x\n").unwrap();
    git(dir.path(), &["add", "f"]);
    git(dir.path(), &["commit", "-qm", "m"]);
    let status = run(dir.path(), &["exec", "--", "git", "status"]);
    assert!(
        status.ends_with(b"\n"),
        "{:?}",
        String::from_utf8_lossy(&status)
    );
    // Une sortie sans retour à la ligne final, rendue telle quelle, reste octet pour octet.
    let bare = run(dir.path(), &["exec", "--", "printf", "sans-fin"]);
    assert_eq!(bare, b"sans-fin");
}
