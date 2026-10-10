//! `tool-output --exit-code N` sort avec N, comme `exec` et `pipe`. Audit du 9 octobre 2026 : le
//! texte disait `[FAIL] Command failed (exit code: 7)` et le processus sortait 0 ; un script qui ne
//! lit que `$?` croyait la commande réussie.
use std::io::Write;
use std::process::{Command, Stdio};

fn tool_output(extra: &[&str], code: &str) -> std::process::Output {
    let state = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", state.path())
        .args([
            "tool-output",
            "--command",
            "unknown-tool",
            "--exit-code",
            code,
        ])
        .args(extra)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"Permission denied: secret\n")
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn tool_output_exits_with_the_given_code() {
    let mut wrong = Vec::new();
    for (extra, code, expected) in [
        (&[][..], "7", 7),
        (&["--json"][..], "7", 7),
        (&["--raw-on-failure"][..], "3", 3),
        (&[][..], "0", 0),
        (&[][..], "101", 101),
    ] {
        let out = tool_output(extra, code);
        if out.status.code() != Some(expected) {
            wrong.push(format!(
                "{extra:?} --exit-code {code}: processus {:?}, texte {:?}",
                out.status.code(),
                String::from_utf8_lossy(&out.stdout)
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Revue indépendante du 9 octobre : sous Unix seul l'octet bas d'un code de sortie survit, donc
/// `--exit-code 256` sortait 0, un succès. Un code non nul ne devient jamais 0 : son octet bas s'il
/// n'est pas nul, 1 sinon. Même règle pour `pipe`.
#[cfg(unix)]
#[test]
fn an_exit_code_out_of_range_never_becomes_a_success() {
    let mut wrong = Vec::new();
    for (code, expected) in [
        ("256", 1),
        ("512", 1),
        ("-256", 1),
        ("-1", 255),
        ("255", 255),
        ("257", 1),
    ] {
        let flag = format!("--exit-code={code}");
        let out = tool_output(&[], "0");
        assert_eq!(out.status.code(), Some(0));
        for subcommand in ["tool-output", "pipe"] {
            let state = tempfile::tempdir().unwrap();
            let mut command = Command::new(env!("CARGO_BIN_EXE_lm-resizer"));
            command.env("LM_RESIZER_STATE_DIR", state.path());
            if subcommand == "tool-output" {
                command.args(["tool-output", "--command", "unknown-tool", &flag]);
            } else {
                command.args(["pipe", "--filter", "cargo-test", &flag]);
            }
            let mut child = command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(b"Permission denied: secret\n")
                .unwrap();
            let out = child.wait_with_output().unwrap();
            if out.status.code() != Some(expected) {
                wrong.push(format!(
                    "{subcommand} {flag}: processus {:?}, attendu {expected}",
                    out.status.code()
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
