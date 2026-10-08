//! L'état de lm-resizer contient les commandes lancées et les sorties brutes (tee) : il doit
//! rester privé quel que soit le umask de l'appelant (audit du 08/10/2026).
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

fn run_with_umask_zero(home: &Path, args: &[&str]) {
    // `umask 000` : le pire cas, tout serait lisible par tous sans correction.
    let status = Command::new("/bin/sh")
        .arg("-c")
        .arg("umask 000; exec \"$@\"")
        .arg("sh")
        .arg(env!("CARGO_BIN_EXE_lm-resizer"))
        .args(args)
        .env("HOME", home)
        .env_remove("LM_RESIZER_STATE_DIR")
        .env_remove("LM_RESIZER_STORE")
        .env_remove("XDG_STATE_HOME")
        .env_remove("LOCALAPPDATA")
        .env_remove("LM_RESIZER_TEE")
        .env_remove("LM_RESIZER_TRACKING")
        .status()
        .unwrap();
    assert!(status.success(), "{args:?}");
}

#[test]
fn state_directory_and_files_are_private_whatever_the_umask() {
    let home = tempfile::tempdir().unwrap();
    run_with_umask_zero(
        home.path(),
        &["exec", "--", "printf", "%s", "Bearer sk-hist-SECRET"],
    );
    let state = home.path().join("lm-resizer");
    assert_eq!(mode(&state), 0o700, "dossier d'état");
    assert_eq!(mode(&state.join("tee")), 0o700, "dossier tee");
    assert_eq!(mode(&state.join("exec-history.jsonl")), 0o600, "historique");
    let archives: Vec<_> = fs::read_dir(state.join("tee"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert!(!archives.is_empty(), "aucune archive tee écrite");
    for archive in archives {
        assert_eq!(mode(&archive), 0o600, "{}", archive.display());
    }
}

#[test]
fn ccr_database_is_private_whatever_the_umask() {
    let home = tempfile::tempdir().unwrap();
    let input = home.path().join("input.txt");
    fs::write(&input, "alpha\n".repeat(400)).unwrap();
    run_with_umask_zero(
        home.path(),
        &["compress", "--input", input.to_str().unwrap()],
    );
    let database = home.path().join("lm-resizer").join("ccr.sqlite3");
    assert!(database.exists(), "le store CCR n'a pas été créé");
    assert_eq!(mode(&database), 0o600, "base CCR");
    assert_eq!(mode(database.parent().unwrap()), 0o700, "dossier d'état");
}
