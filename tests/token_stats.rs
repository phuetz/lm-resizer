//! CLI contracts: exact counts persist through JSONL, stats and Markdown.
#![cfg(unix)]
use lm_resizer_core::tokenizer::{TiktokenCounter, Tokenizer};
use serde_json::{json, Value};
use std::process::Command;

fn cli(state: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", state)
        .env("LM_RESIZER_TRACKING", "1")
        .env("LM_RESIZER_TEE", "1")
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn exec_history_and_stats_count_final_output_including_recovery_hint() {
    let state = tempfile::tempdir().unwrap();
    use std::os::unix::fs::PermissionsExt;
    // Une exécution en échec : le code 0 d'un lanceur de tests rendrait la sortie intacte.
    let raw = "test measured_success ... ok\n".repeat(200)
        + "test measured_failure ... FAILED\n\nfailures:\n\n---- measured_failure stdout ----\nassertion failed\n\nfailures:\n    measured_failure\n\ntest result: FAILED. 200 passed; 1 failed\n";
    std::fs::write(state.path().join("raw"), &raw).unwrap();
    let producer = state.path().join("cargo");
    std::fs::write(
        &producer,
        "#!/bin/sh\ncat \"$(dirname \"$0\")/raw\"\nexit 101\n",
    )
    .unwrap();
    std::fs::set_permissions(&producer, std::fs::Permissions::from_mode(0o755)).unwrap();
    let result = cli(
        state.path(),
        &["exec", "--json", "--", producer.to_str().unwrap(), "test"],
    );
    assert_eq!(result.status.code(), Some(101), "{:?}", result.stderr);
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    let tokenizer = TiktokenCounter::for_model("gpt-4o").unwrap();
    let before = tokenizer.count_text(&raw) as i64;
    let after = tokenizer.count_text(report["output"].as_str().unwrap()) as i64;
    assert_eq!(report["original_tokens"], before);
    assert_eq!(report["compressed_tokens"], after);
    assert_eq!(report["tokens_saved"], before - after);
    assert_ne!(
        report["tokens_saved"],
        report["bytes_saved"].as_i64().unwrap() / 4
    );
    assert_eq!(report["tokenizer"], "tiktoken-rs/o200k_base");
    assert_eq!(report["token_count_method"], "exact");
    assert!(report["output"].as_str().unwrap().contains("[tee:"));
    for key in [
        "command",
        "exit_code",
        "filter",
        "original_bytes",
        "filtered_bytes",
        "compressed_bytes",
        "bytes_saved",
        "compression_steps",
        "cache_keys",
        "tee_hint",
        "output",
    ] {
        assert!(report.get(key).is_some(), "lost JSON field {key}");
    }
    let history = std::fs::read_to_string(state.path().join("exec-history.jsonl")).unwrap();
    let record: Value = serde_json::from_str(history.trim()).unwrap();
    assert_eq!(record["tokens_saved"], before - after);
    let stats = cli(state.path(), &["stats"]);
    assert!(stats.status.success());
    let stats: Value = serde_json::from_slice(&stats.stdout).unwrap();
    let summary = &stats["exec_history"];
    assert_eq!(summary["tokens_saved"], before - after);
    assert_eq!(summary["estimated_tokens_saved"], 0);
    assert_eq!(summary["by_filter"][0]["tokens_saved"], before - after);
    assert_eq!(summary["by_command"][0]["tokens_saved"], before - after);
    let markdown = cli(state.path(), &["stats", "--markdown"]);
    let markdown = String::from_utf8(markdown.stdout).unwrap();
    assert!(markdown.contains("# lm-resizer Stats"));
    assert!(markdown.contains("## Top Filters"));
    assert!(markdown.contains("tiktoken-rs/o200k_base; exact text count"));
    assert!(markdown.contains("1 measured / 0 unmeasured commands"));
}

#[test]
fn generic_tool_output_and_gain_record_a_failed_command() {
    let state = tempfile::tempdir().unwrap();
    let input = state.path().join("captured.txt");
    std::fs::write(
        &input,
        "progress\nERROR invoice failed\n42 tests, 1 failed\n",
    )
    .unwrap();
    let result = cli(
        state.path(),
        &[
            "tool-output",
            "--json",
            "--command",
            "unknown-tool",
            "--exit-code",
            "7",
            "--input",
            input.to_str().unwrap(),
        ],
    );
    assert!(result.status.success());
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(report["output"]
        .as_str()
        .unwrap()
        .contains("42 tests, 1 failed"));
    assert!(!report["output"]
        .as_str()
        .unwrap()
        .starts_with("[FAIL] Command failed (exit code: 7)"));
    assert_eq!(report["exit_code"], 7);
    let gain = cli(state.path(), &["gain"]);
    assert!(String::from_utf8_lossy(&gain.stdout).contains("Total commands: 1"));
    let gain = cli(state.path(), &["gain", "--json"]);
    let gain: Value = serde_json::from_slice(&gain.stdout).unwrap();
    assert_eq!(gain["exec_history"]["commands"], 1);
    assert_eq!(gain["exec_history"]["measured_commands"], 1);
    let scoped = cli(state.path(), &["gain", "--history", "--project"]);
    let scoped = String::from_utf8(scoped.stdout).unwrap();
    assert!(scoped.contains("Project: "));
    assert!(scoped.contains("Total commands: 1"));
    assert!(scoped.contains("Recent executions:"));
    assert!(scoped.contains("unknown-tool"));
}

#[test]
fn discover_and_eval_count_unicode_and_keep_existing_json_fields() {
    let state = tempfile::tempdir().unwrap();
    let raw = format!(
        "{}test result: ok. 1 passed; 0 failed\n",
        "test 你好🦀aaaaaaaa ... ok\n".repeat(40)
    );
    let fixture = state.path().join("session.jsonl");
    std::fs::write(
        &fixture,
        format!("{}\n", json!({"command":"cargo test", "output":raw})),
    )
    .unwrap();
    let path = fixture.to_str().unwrap();
    let discover = cli(state.path(), &["discover", "--json", path]);
    assert!(discover.status.success(), "{:?}", discover.stderr);
    let discover: Value = serde_json::from_slice(&discover.stdout).unwrap();
    let before = TiktokenCounter::for_model("gpt-4o")
        .unwrap()
        .count_text(&raw);
    assert_eq!(discover["original_tokens"], before);
    assert_ne!(
        discover["tokens_saved"],
        discover["estimated_bytes_saved"].as_i64().unwrap() / 4
    );
    // Legacy field is an alias for prospective savings, now tokenized.
    assert_eq!(discover["estimated_tokens_saved"], discover["tokens_saved"]);
    assert_eq!(
        discover["candidates"][0]["tokens_saved"],
        discover["tokens_saved"]
    );
    let eval = cli(state.path(), &["eval", "--json", path]);
    assert!(eval.status.success(), "{:?}", eval.stderr);
    let eval: Value = serde_json::from_slice(&eval.stdout).unwrap();
    assert_eq!(eval["tokens_saved"], discover["tokens_saved"]);
    assert_eq!(eval["tokenizer"], "tiktoken-rs/o200k_base");
    assert!(eval["pass"].as_bool().unwrap());
}
