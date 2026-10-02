#![cfg(unix)]
use serde_json::Value;
use std::process::Command;
#[test]
fn proxy_preserves_binary_streams_arguments_and_exit_code() {
    let root = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", root.path())
        .args([
            "proxy",
            "sh",
            "-c",
            "printf '%s' \"$1\"; printf '\\377'; printf 'err' >&2; exit 17",
            "sh",
            "a b;$literal",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(17));
    assert_eq!(out.stdout, b"a b;$literal\xff");
    assert_eq!(out.stderr, b"err");
    let row: Value = serde_json::from_str(
        std::fs::read_to_string(root.path().join("exec-history.jsonl"))
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(row["filter"], "proxy");
    assert_eq!(row["bytes_saved"], 0);
    assert_eq!(row["token_count_method"], "unmeasured binary output");
    assert_eq!(row["exit_code"], 17);
}
