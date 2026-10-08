//! Tout `[tee:<id>]` affiché dit en une ligne comment relire l'original (D7).
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

#[test]
fn every_displayed_tee_hint_says_how_to_read_the_original() {
    let dir = tempfile::tempdir().unwrap();
    let script = "i=0; while [ $i -lt 3000 ]; do echo \"tout va bien dans cette etape\"; i=$((i+1)); done; echo 'error: boom'";
    let view = exec(dir.path(), script);
    let hint = view
        .lines()
        .find(|row| row.starts_with("[tee:"))
        .unwrap_or_else(|| panic!("pas de [tee:] dans\n{view}"));
    let id = hint
        .strip_prefix("[tee:")
        .and_then(|rest| rest.split_once(']'))
        .map(|(id, _)| id)
        .unwrap();
    assert!(
        hint.contains(&format!("lm-resizer tee read {id}")),
        "le rappel n'explique pas la relecture : {hint}"
    );
}
