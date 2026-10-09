//! L'archive tee ne suit pas un lien symbolique et refuse un fichier existant qui ne serait pas
//! privé. Audit du 9 octobre 2026 : un lien posé à la place de `<empreinte>.log` faisait écrire la
//! sortie brute, secret compris, dans le fichier de l'attaquant ; un fichier existant en 0666 gardait
//! son mode.
#![cfg(unix)]
use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

const SECRET: &str = "SECRET-TOKEN-ABC";

/// SHA-256 de `SECRET\n`, nom de son archive.
fn archive_name() -> String {
    let out = Command::new("sha256sum")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(format!("{SECRET}\n").as_bytes())?;
            child.wait_with_output()
        })
        .unwrap();
    let digest = String::from_utf8(out.stdout).unwrap();
    format!("{}.log", &digest[..64])
}

fn exec_echo(state: &Path) -> (Value, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", state)
        .args(["exec", "--json", "--", "echo", SECRET])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    (
        serde_json::from_slice(&out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

/// Dossier tee ouvert à tous, comme celui d'une ancienne version ou d'un compte partagé.
fn open_tee_dir(state: &Path) -> std::path::PathBuf {
    let tee = state.join("tee");
    fs::create_dir_all(&tee).unwrap();
    fs::set_permissions(state, fs::Permissions::from_mode(0o700)).unwrap();
    fs::set_permissions(&tee, fs::Permissions::from_mode(0o777)).unwrap();
    tee
}

#[test]
fn a_symlink_in_place_of_the_archive_is_not_followed() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let tee = open_tee_dir(&state);
    let target = dir.path().join("attacker.txt");
    fs::write(&target, "KEEP-ME\n").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o666)).unwrap();
    std::os::unix::fs::symlink(&target, tee.join(archive_name())).unwrap();

    let (report, stderr) = exec_echo(&state);
    assert_eq!(report["output"], format!("{SECRET}\n"));
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "KEEP-ME\n",
        "{stderr}"
    );
    assert!(report["tee_hint"].is_null(), "{report}");
    assert!(stderr.contains("tee"), "{stderr}");
}

#[test]
fn an_existing_archive_readable_by_others_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let tee = open_tee_dir(&state);
    let archive = tee.join(archive_name());
    fs::write(&archive, format!("{SECRET}\n")).unwrap();
    fs::set_permissions(&archive, fs::Permissions::from_mode(0o666)).unwrap();

    let (report, stderr) = exec_echo(&state);
    assert!(report["tee_hint"].is_null(), "{report}");
    assert!(stderr.contains("chmod"), "{stderr}");
}

#[test]
fn a_new_archive_is_private_and_a_private_one_is_reused() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let (first, _) = exec_echo(&state);
    assert!(first["tee_hint"].is_string(), "{first}");
    let archive = state.join("tee").join(archive_name());
    assert_eq!(
        fs::metadata(&archive).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let (second, stderr) = exec_echo(&state);
    assert_eq!(second["tee_hint"], first["tee_hint"], "{stderr}");
    assert_eq!(fs::read_to_string(&archive).unwrap(), format!("{SECRET}\n"));
}
