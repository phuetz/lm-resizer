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
    out.stdout
        .iter()
        .enumerate()
        .filter(|(_, byte)| **byte == b'{')
        .find_map(|(start, _)| serde_json::from_slice(&out.stdout[start..]).ok())
        .unwrap_or_else(|| panic!("no JSON report in output: {out:?}"))
}

/// Whether `merged` holds exactly the bytes of `stdout` and `stderr`, each in
/// its own order, interleaved in any way. Separate-stream modes archive chunks
/// in the order two drain workers deliver them: per-stream bytes are a
/// contract, the order between the two streams is not.
pub fn is_interleaving(merged: &[u8], stdout: &[u8], stderr: &[u8]) -> bool {
    if merged.len() != stdout.len() + stderr.len() {
        return false;
    }
    // reachable[j]: merged[..i + j] is stdout[..i] interleaved with stderr[..j].
    let mut reachable = vec![false; stderr.len() + 1];
    for i in 0..=stdout.len() {
        for j in 0..=stderr.len() {
            reachable[j] = (i == 0 && j == 0)
                || (i > 0 && reachable[j] && stdout[i - 1] == merged[i + j - 1])
                || (j > 0 && reachable[j - 1] && stderr[j - 1] == merged[i + j - 1]);
        }
    }
    reachable[stderr.len()]
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
