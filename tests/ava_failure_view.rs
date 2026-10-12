//! Un échec AVA garde son assertion, la valeur obtenue et la ligne fautive (D3 de la recette).
#![cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[test]
fn exec_of_a_failing_ava_run_keeps_the_assertion() {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let npx = bin.join("npx");
    std::fs::write(
        &npx,
        format!(
            "#!/bin/sh\ncat '{}/tests/fixtures/ava_failure.txt'\nexit 1\n",
            env!("CARGO_MANIFEST_DIR")
        ),
    )
    .unwrap();
    std::fs::set_permissions(&npx, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths(
        std::iter::once(bin).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .current_dir(dir.path())
        .env("PATH", path)
        .env("LM_RESIZER_STATE_DIR", dir.path().join("state"))
        .env("LM_RESIZER_TRACKING", "0")
        .args(["exec", "--", "npx", "--no-install", "ava"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let view = String::from_utf8(out.stdout).unwrap();
    for fact in [
        "Value is not `false`:",
        "t.false(isPlainObject({}));",
        "1 test failed",
    ] {
        assert!(view.contains(fact), "manque {fact:?} dans\n{view}");
    }
}
