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

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn private_state(dir: &Path) -> std::path::PathBuf {
    let state = dir.join("state");
    fs::create_dir_all(&state).unwrap();
    fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap();
    state
}

/// Décision du 10 octobre : un fichier d'état à soi, ordinaire, à un seul lien, dont seul le mode
/// est trop ouvert (un historique laissé en 0664 par une version antérieure) est resserré à 0600
/// sur le descripteur déjà contrôlé, puis écrit ; il n'est plus refusé.
#[test]
fn an_own_history_with_a_loose_mode_is_tightened_and_written() {
    let dir = tempfile::tempdir().unwrap();
    let state = private_state(dir.path());
    for name in ["exec-history.jsonl", "hook-audit.jsonl"] {
        let path = state.join(name);
        fs::write(&path, "").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o664)).unwrap();
    }
    let out = lm(&state)
        .args(["exec", "--", "echo", "bonjour"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("refusé"), "{stderr}");
    let history = state.join("exec-history.jsonl");
    assert!(fs::read_to_string(&history)
        .unwrap()
        .contains("echo bonjour"));
    assert_eq!(mode(&history), 0o600);
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
    child.wait_with_output().unwrap();
    let audit = state.join("hook-audit.jsonl");
    assert!(fs::read_to_string(&audit).unwrap().contains("PreToolUse"));
    assert_eq!(mode(&audit), 0o600);
}

/// Même décision pour la base CCR : contre-audit Grok n° 2, une base déjà en 0644 le restait et
/// `exec` y écrivait quand même.
#[test]
fn a_ccr_store_with_a_loose_mode_is_tightened() {
    let dir = tempfile::tempdir().unwrap();
    let state = private_state(dir.path());
    let out = lm(&state)
        .args(["exec", "--", "echo", "un"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let base = state.join("ccr.sqlite3");
    fs::set_permissions(&base, fs::Permissions::from_mode(0o644)).unwrap();
    let out = lm(&state)
        .args(["exec", "--", "echo", "deux"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(mode(&base), 0o600);
}

/// Les fichiers `-wal` et `-shm` d'une base passent les mêmes contrôles : un `-wal` déjà présent
/// sous forme de lien dur vers un fichier extérieur ne reçoit rien (la base est refusée, `exec`
/// continue sans elle).
#[test]
fn a_hard_linked_wal_beside_the_ccr_store_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let state = private_state(dir.path());
    let out = lm(&state)
        .args(["exec", "--", "echo", "un"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let outside = dir.path().join("outside-wal");
    fs::write(&outside, "").unwrap();
    fs::set_permissions(&outside, fs::Permissions::from_mode(0o600)).unwrap();
    let wal = state.join("ccr.sqlite3-wal");
    let _ = fs::remove_file(&wal);
    fs::hard_link(&outside, &wal).unwrap();
    // Un échec `cargo test` réduit : le brut est rangé dans la base, donc écrit dans son `-wal`.
    let raw = format!(
        "\nrunning 201 tests\n{}test b ... FAILED\n\nfailures:\n\n---- b stdout ----\nboom\n\nfailures:\n    b\n\ntest result: FAILED. 200 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n",
        "test a ... ok\n".repeat(200)
    );
    let mut child = lm(&state)
        .args([
            "tool-output",
            "--command",
            "cargo test",
            "--exit-code",
            "101",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(raw.as_bytes())
        .unwrap();
    let _ = child.wait_with_output().unwrap();
    assert_eq!(
        fs::metadata(&outside).unwrap().len(),
        0,
        "le -wal extérieur a été écrit"
    );
}

/// Contre-audit Grok n° 2 : un lien dur vers une base vide à soi était suivi, l'inode devenait une
/// base SQLite. Un lien dur reste refusé, comme un lien symbolique.
#[test]
fn a_hard_link_in_place_of_the_ccr_store_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let state = private_state(dir.path());
    let outside = dir.path().join("outside-store");
    fs::write(&outside, "").unwrap();
    fs::set_permissions(&outside, fs::Permissions::from_mode(0o600)).unwrap();
    fs::hard_link(&outside, state.join("ccr.sqlite3")).unwrap();
    let _ = lm(&state)
        .args(["exec", "--", "echo", "secret"])
        .output()
        .unwrap();
    assert_eq!(
        fs::metadata(&outside).unwrap().len(),
        0,
        "la base a été écrite à travers le lien dur"
    );
}
