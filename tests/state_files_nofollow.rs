//! Les fichiers d'état ne suivent pas un lien symbolique. Contre-audit Grok du 9 octobre :
//! dans un dossier d'état ouvert en écriture, `exec-history.jsonl` remplacé par un lien vers un
//! fichier 0666 recevait la ligne de commande (`echo sk-live-SECRET`) ; `hook-audit.jsonl` et
//! `ccr.sqlite3` suivaient aussi le lien.
#![cfg(unix)]
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

fn planted(dir: &Path, state: &Path, name: &str, content: &str) -> std::path::PathBuf {
    fs::create_dir_all(state).unwrap();
    fs::set_permissions(state, fs::Permissions::from_mode(0o777)).unwrap();
    let target = dir.join(format!("attacker-{name}"));
    fs::write(&target, content).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o666)).unwrap();
    std::os::unix::fs::symlink(&target, state.join(name)).unwrap();
    target
}

fn lm(state: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_lm-resizer"));
    command
        .env("LM_RESIZER_STATE_DIR", state)
        .env_remove("LM_RESIZER_STORE")
        .env("LM_RESIZER_TRACKING", "1");
    command
}

#[test]
fn the_exec_history_does_not_follow_a_symbolic_link() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let target = planted(dir.path(), &state, "exec-history.jsonl", "KEEP-ME\n");
    let out = lm(&state)
        .args(["exec", "--", "echo", "sk-live-SECRET"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "sk-live-SECRET\n");
    assert_eq!(fs::read_to_string(&target).unwrap(), "KEEP-ME\n");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("lien symbolique"), "{stderr}");
}

#[test]
fn the_hook_audit_does_not_follow_a_symbolic_link() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let target = planted(dir.path(), &state, "hook-audit.jsonl", "KEEP-ME\n");
    let mut child = lm(&state)
        .args(["hook", "--client", "claude", "--event", "PreToolUse"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"tool_name":"Bash","tool_input":{"command":"git status"}}"#)
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(!out.stdout.is_empty(), "le crochet réécrit toujours");
    assert_eq!(fs::read_to_string(&target).unwrap(), "KEEP-ME\n");
}

#[test]
fn the_ccr_store_does_not_follow_a_symbolic_link() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    // Fichier vide : SQLite l'initialiserait en base à travers le lien.
    let target = planted(dir.path(), &state, "ccr.sqlite3", "");
    let mut child = lm(&state)
        .args(["tool-output", "--command", "cargo test", "--exit-code", "1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"CVE-2024-99999: token sk-live-SECRET\n")
        .unwrap();
    let _ = child.wait_with_output().unwrap();
    assert_eq!(
        fs::metadata(&target).unwrap().len(),
        0,
        "la cible du lien a été écrite"
    );
}
