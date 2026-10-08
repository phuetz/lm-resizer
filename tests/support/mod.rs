// Each integration binary uses a different subset of this shared harness.
#![allow(dead_code)]

use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::OnceLock,
};

pub fn sandbox() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(".omx/windows-fix/tests");
    std::fs::create_dir_all(&root).unwrap();
    tempfile::tempdir_in(root).unwrap()
}

pub fn producer() -> &'static Path {
    static PRODUCER: OnceLock<PathBuf> = OnceLock::new();
    PRODUCER.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(".omx/windows-fix/producer");
        std::fs::create_dir_all(&root).unwrap();
        let exe = root.join(format!("producer{}", std::env::consts::EXE_SUFFIX));
        let result = Command::new("rustc")
            .args(["--edition=2021", "tests/fixtures/windows_producer.rs", "-o"])
            .arg(&exe)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .unwrap();
        assert!(result.status.success(), "{:?}", result);
        exe
    })
}

pub fn lm(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lm-resizer"));
    cmd.env("HOME", root)
        .env("USERPROFILE", root)
        .env("CODEX_HOME", root.join("codex"))
        .env("LM_RESIZER_STATE_DIR", root.join("state"))
        .env("LM_RESIZER_TEE", "1");
    cmd
}

pub fn report(out: &Output) -> serde_json::Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}: {out:?}"))
}

pub fn recovered(root: &Path, value: &serde_json::Value) -> Vec<u8> {
    let hint = value["tee_hint"]
        .as_str()
        .expect("original must be recoverable");
    let id = hint
        .strip_prefix("[raw: ")
        .unwrap()
        .strip_suffix(']')
        .unwrap();
    let out = lm(root).args(["tee", "read", id]).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    out.stdout
}
