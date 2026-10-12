//! La clé d'API du proxy ne doit jamais figurer dans la ligne de commande d'un processus
//! lancé par `wrap` : `/proc/<pid>/cmdline` est lisible par tous les comptes locaux
//! (audit du 08/10/2026). Linux seulement : le test lit `/proc`.
#![cfg(target_os = "linux")]

use std::fs;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

const KEY: &str = "sk-test-LMR-HYGIENE-0123456789";

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// « Agent » factice : relève les lignes de commande de tous les processus pendant que le
/// proxy tourne. Écrit `serve` puis la ligne si un `serve` contient la clé, `clean` sinon.
fn agent_script(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("agent.sh");
    fs::write(
        &path,
        format!(
            r#"#!/bin/sh
seen_serve=no
leak=
for f in /proc/[0-9]*/cmdline; do
  line=$(tr '\000' ' ' < "$f" 2>/dev/null) || continue
  case "$line" in
    "$LMR_BIN serve "*)
      seen_serve=yes
      case "$line" in *{KEY}*) leak="$line" ;; esac ;;
  esac
done
printf '%s\n%s\n' "$seen_serve" "$leak" > "$LMR_RESULT"
"#
        ),
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn wrap(dir: &Path, extra: &[&str], envs: &[(&str, &str)]) -> (std::process::Output, String) {
    let agent = agent_script(dir);
    let result = dir.join("result.txt");
    let mut command = Command::new(env!("CARGO_BIN_EXE_lm-resizer"));
    command
        .arg("wrap")
        .arg(&agent)
        .arg("--bind")
        .arg(format!("127.0.0.1:{}", free_port()))
        .arg("--upstream")
        .arg("http://127.0.0.1:9")
        .args(extra)
        .env_remove("LM_RESIZER_API_KEY")
        .env_remove("LM_RESIZER_API_KEY_FILE")
        .env("HOME", dir)
        .env("LM_RESIZER_STATE_DIR", dir.join("state"))
        .env("LMR_BIN", env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LMR_RESULT", &result);
    for (name, value) in envs {
        command.env(name, value);
    }
    let output = command.output().unwrap();
    let text = fs::read_to_string(&result).unwrap_or_default();
    (output, text)
}

#[test]
fn key_from_environment_is_not_forwarded_in_argv() {
    let dir = tempfile::tempdir().unwrap();
    let (output, result) = wrap(dir.path(), &[], &[("LM_RESIZER_API_KEY", KEY)]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        result, "yes\n\n",
        "la clé figure dans la ligne de commande du proxy"
    );
}

#[test]
fn key_from_flag_is_not_forwarded_to_the_proxy_argv() {
    let dir = tempfile::tempdir().unwrap();
    let (output, result) = wrap(dir.path(), &["--api-key", KEY], &[]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        result, "yes\n\n",
        "la clé figure dans la ligne de commande du proxy"
    );
    // La ligne de commande du parent, elle, porte la clé : l'avertir.
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--api-key est visible"),
        "{output:?}"
    );
}

#[test]
fn key_file_must_be_private_and_is_not_forwarded_in_argv() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("key");
    fs::write(&file, format!("{KEY}\n")).unwrap();

    fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
    let (output, result) = wrap(dir.path(), &["--api-key-file", file.to_str().unwrap()], &[]);
    assert!(!output.status.success(), "un fichier 0644 doit être refusé");
    assert!(result.is_empty(), "l'agent ne doit pas être lancé");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("chmod 600"), "{stderr}");
    assert!(!stderr.contains(KEY), "{stderr}");

    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    let (output, result) = wrap(dir.path(), &["--api-key-file", file.to_str().unwrap()], &[]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(result, "yes\n\n");
}
