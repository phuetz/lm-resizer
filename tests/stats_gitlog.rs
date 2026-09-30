use std::process::{Command, Output};

use lm_resizer_core::tokenizer::{TiktokenCounter, Tokenizer};
use serde_json::{json, Value};
use tempfile::TempDir;

fn run(home: &TempDir, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .args(args)
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env_remove("LM_RESIZER_TRACKING")
        .env_remove("LM_RESIZER_TOKENIZER")
        .output()
        .unwrap()
}

fn json_output(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn actual_tokens_are_counted_in_discover() {
    let home = TempDir::new().unwrap();
    let raw = "commit abcdef123456\nAuthor: Fixture <fixture@example.invalid>\nDate: Wed Sep 30 12:00:00 2026 +0000\n\n    Corriger été 東京\n\n src/demo.rs | 100 ++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++----------\n 1 file changed, 90 insertions(+), 10 deletions(-)\n";
    let path = home.path().join("session.jsonl");
    std::fs::write(
        &path,
        json!({"command":"git log --stat", "output": raw}).to_string(),
    )
    .unwrap();
    let report = json_output(run(&home, &["discover", path.to_str().unwrap(), "--json"]));
    let tokens = &report["token_savings"][0];
    let counter = TiktokenCounter::for_model("gpt-4o").unwrap();
    assert_eq!(tokens["original_tokens"], counter.count_text(raw));
    assert_eq!(tokens["tokenizer"], "o200k_base");
    assert_eq!(tokens["estimated"], false);
    assert!(tokens["original_tokens"].as_u64().unwrap() > 0);
    assert_eq!(
        tokens["tokens_saved"].as_i64().unwrap(),
        tokens["original_tokens"].as_i64().unwrap() - tokens["compressed_tokens"].as_i64().unwrap()
    );
    assert_ne!(tokens["tokens_saved"], report["estimated_tokens_saved"]);
    assert!(report.get("estimated_tokens_saved").is_some());
    let custom = json_output(run(
        &home,
        &[
            "--tokenizer",
            "cl100k_base",
            "discover",
            path.to_str().unwrap(),
            "--json",
        ],
    ));
    assert_eq!(custom["token_savings"][0]["tokenizer"], "cl100k_base");
    let estimated = json_output(run(
        &home,
        &[
            "--tokenizer",
            "claude-example",
            "discover",
            path.to_str().unwrap(),
            "--json",
        ],
    ));
    assert_eq!(estimated["token_savings"][0]["estimated"], true);
}

#[test]
fn git_log_stat_preserves_dates_and_all_facts() {
    let home = TempDir::new().unwrap();
    let raw = include_str!("../bench/corpus/git_log_stat.txt");
    let path = home.path().join("capture.txt");
    std::fs::write(&path, raw).unwrap();
    let report = json_output(run(
        &home,
        &[
            "tool-output",
            "--command",
            "git log -40 --stat",
            "--input",
            path.to_str().unwrap(),
            "--json",
        ],
    ));
    assert_eq!(report["filter"], "git_log_stat");
    let output = report["output"].as_str().unwrap();
    for line in raw.lines() {
        let fact = line.trim();
        if fact.starts_with("commit ")
            || fact.starts_with("Author:")
            || fact.starts_with("Date:")
            || fact.starts_with("Maintenance ")
        {
            assert!(output.contains(fact), "missing: {fact}");
        }
        if let Some((file, _)) = fact.split_once(" | ") {
            assert!(output.contains(file), "missing file: {file}");
        }
    }
    assert!(output.len() < raw.len() * 3 / 4);
    assert_eq!(report["token_savings"]["estimated"], false);
    let counter = TiktokenCounter::for_model("gpt-4o").unwrap();
    assert_eq!(
        report["token_savings"]["original_tokens"],
        counter.count_text(raw)
    );
    assert_eq!(
        report["token_savings"]["compressed_tokens"],
        counter.count_text(output)
    );
}

#[test]
fn git_log_stat_keeps_merge_binary_rename_and_unicode_facts() {
    let home = TempDir::new().unwrap();
    let raw = format!("commit abcdef123456\nMerge: 1111111 2222222\nAuthor: Fixture <fixture@example.invalid>\nDate: Wed Sep 30 12:00:00 2026 +0000\n\n    Corriger été 東京\n\n{}\n image.png | Bin 0 -> 2048 bytes\n src/{{old => new}}.rs | 12 ++++++++++--\n \"src/\\303\\251t\\303\\251.rs\" | 3 ++-\n 3 files changed, 12 insertions(+), 3 deletions(-)\n", "    Additional recoverable message detail.\n".repeat(20));
    let path = home.path().join("capture.txt");
    std::fs::write(&path, &raw).unwrap();
    let report = json_output(run(
        &home,
        &[
            "tool-output",
            "--command",
            "git log --stat=80",
            "--input",
            path.to_str().unwrap(),
            "--json",
        ],
    ));
    let output = report["output"].as_str().unwrap();
    for fact in [
        "Merge: 1111111 2222222",
        "Fixture <fixture@example.invalid>",
        "Wed Sep 30 12:00:00 2026 +0000",
        "Corriger été 東京",
        "image.png: Bin 0 -> 2048 bytes",
        "src/{old => new}.rs:",
        "3 files changed, 12 insertions(+), 3 deletions(-)",
        "20 message body lines omitted",
    ] {
        assert!(output.contains(fact), "missing: {fact}");
    }
    assert!(report["token_savings"]["tokens_saved"].as_i64().unwrap() > 0);
    assert!(!report["cache_keys"].as_array().unwrap().is_empty());
}

#[test]
fn exec_persists_counts_and_stats_keeps_the_json_contract() {
    let home = TempDir::new().unwrap();
    let report = json_output(run(&home, &["exec", "--json", "--", "git", "--version"]));
    let counter = TiktokenCounter::for_model("gpt-4o").unwrap();
    let output = report["output"].as_str().unwrap();
    assert_eq!(
        report["token_savings"]["compressed_tokens"],
        counter.count_text(output)
    );
    let stats = json_output(run(&home, &["stats"]));
    let history = &stats["exec_history"];
    for key in [
        "commands",
        "original_bytes",
        "compressed_bytes",
        "bytes_saved",
        "estimated_tokens_saved",
        "by_filter",
        "by_command",
    ] {
        assert!(history.get(key).is_some(), "missing legacy key: {key}");
    }
    assert_eq!(history["commands"], 1);
    assert_eq!(history["token_savings"][0], report["token_savings"]);
    assert_eq!(
        history["by_command"][0]["token_savings"][0],
        report["token_savings"]
    );
    let custom = json_output(run(
        &home,
        &[
            "--tokenizer",
            "cl100k_base",
            "exec",
            "--json",
            "--",
            "git",
            "--version",
        ],
    ));
    assert_eq!(custom["token_savings"]["tokenizer"], "cl100k_base");
    let stats = json_output(run(&home, &["gain", "--json"]));
    assert_eq!(
        stats["exec_history"]["token_savings"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn gain_is_readable_with_denominator() {
    let home = TempDir::new().unwrap();
    let output = run(&home, &["gain"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("commands"));
    assert!(text.contains("tokens saved out of"));
    assert!(text.contains("command output"));
}

#[test]
fn doctor_explains_zero_savings() {
    let home = TempDir::new().unwrap();
    let report = json_output(run(&home, &["doctor", "--json"]));
    let diagnoses = report["savings_diagnostics"].as_array().unwrap();
    assert!(diagnoses
        .iter()
        .any(|v| v.as_str().unwrap().contains("No recorded commands")));
    assert!(diagnoses
        .iter()
        .any(|v| v.as_str().unwrap().contains("lm-resizer exec")));
}
