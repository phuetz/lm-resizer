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
