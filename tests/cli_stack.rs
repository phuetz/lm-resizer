//! Exercise the Windows-sized main-thread stack on Unix too.
#![cfg(unix)]
use std::process::Command;

#[test]
fn cli_parses_and_compresses_with_a_one_mib_main_stack() {
    let state = tempfile::tempdir().unwrap();
    let input = state.path().join("input.json");
    std::fs::write(&input, "{\"message\":\"hello\"}\n").unwrap();
    for args in [
        vec!["--help"],
        vec!["compress", "--input", input.to_str().unwrap(), "--json"],
    ] {
        let output = Command::new("sh")
            .args(["-c", "ulimit -s 1024; exec \"$@\"", "cli-stack"])
            .arg(env!("CARGO_BIN_EXE_lm-resizer"))
            .args(args)
            .env("LM_RESIZER_STATE_DIR", state.path())
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(!output.stdout.is_empty());
    }
}
