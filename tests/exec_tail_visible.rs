//! La fin d'une sortie ne disparaît jamais sans `[tee:<id>]` (D20).
#![cfg(unix)]
use std::process::Command;

fn exec(dir: &std::path::Path, script: &str) -> String {
    let state = dir.join("state");
    let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .current_dir(dir)
        .env("LM_RESIZER_STATE_DIR", &state)
        .env("LM_RESIZER_STORE", state.join("ccr.sqlite"))
        .env("LM_RESIZER_TRACKING", "0")
        // `awk` : un programme inconnu (le résumé générique s'applique), pas un shell composé (brut).
        .args(["exec", "--", "awk", script])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    String::from_utf8(out.stdout).unwrap()
}

/// Sortie longue de plusieurs formes (programme `awk`), dont la dernière ligne porte le verdict.
fn long_script(kind: usize) -> String {
    let body = match kind {
        0 => {
            r#"for (i = 0; i < 3000; i++) { print "== étape " i ": compilation de crate-" i; print "warning: unused variable x" i; print "  --> src/f" i ".rs:" i ":5" }"#
        }
        1 => {
            r#"for (i = 0; i < 2500; i++) { print "downloading chunk " i " of 2500"; print "test module::case_" i " ... ok" }"#
        }
        _ => {
            r#"for (i = 0; i < 800; i++) print "ok " i; print "error: une erreur au milieu"; for (i = 0; i < 800; i++) print "ok " i"#
        }
    };
    format!(
        "BEGIN {{ {body}; print \"Packaged dist/x.tar.gz\"; print \"Vérification README install : OK\"; print \"release check passed\" }}"
    )
}

#[test]
fn the_last_line_of_a_long_script_never_vanishes_without_a_tee_hint() {
    let dir = tempfile::tempdir().unwrap();
    for kind in 0..3 {
        let view = exec(dir.path(), &long_script(kind));
        let last_line_kept = view
            .lines()
            .any(|row| row.trim_end() == "release check passed");
        assert!(
            last_line_kept || view.contains("[tee:"),
            "forme {kind} : fin coupée sans [tee:] ; fin de la vue :\n{}",
            view.lines().rev().take(4).collect::<Vec<_>>().join("\n")
        );
        // La ligne de verdict est gardée, pas seulement signalée.
        assert!(last_line_kept, "forme {kind}");
    }
}
