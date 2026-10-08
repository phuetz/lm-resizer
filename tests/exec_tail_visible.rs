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
        .args(["exec", "--", "sh", "-c", script])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    String::from_utf8(out.stdout).unwrap()
}

/// Sortie de script longue, de plusieurs formes, dont la dernière ligne porte le verdict.
fn long_script(kind: usize) -> String {
    let body = match kind {
        0 => "i=0; while [ $i -lt 3000 ]; do echo \"== étape $i: compilation de crate-$i\"; echo \"warning: unused variable x$i\"; echo \"  --> src/f$i.rs:$i:5\"; i=$((i+1)); done",
        1 => "i=0; while [ $i -lt 2500 ]; do echo \"downloading chunk $i of 2500\"; echo \"test module::case_$i ... ok\"; i=$((i+1)); done",
        _ => "i=0; while [ $i -lt 800 ]; do echo \"ok $i\"; i=$((i+1)); done; echo 'error: une erreur au milieu'; i=0; while [ $i -lt 800 ]; do echo \"ok $i\"; i=$((i+1)); done",
    };
    format!("{body}; echo 'Packaged dist/x.tar.gz'; echo 'Vérification README install : OK'; echo 'release check passed'")
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
