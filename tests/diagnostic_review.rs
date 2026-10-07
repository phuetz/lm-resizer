//! Grok3: preserve values, panic bodies and locations, not just the verdict.
use std::process::Command;

#[test]
fn generic_compression_keeps_real_diagnostic_forms_in_the_middle() {
    let dir = tempfile::tempdir().unwrap();
    let raw = format!("{}\n================ FAILURES ================\ntests/test_orders.py:42: in test_order_total\n    assert total == 300\nE   assert 301 == 300\n\nthread 'main' panicked at src/main.rs:10:5:\ncalled `Option::unwrap()` on a `None` value\n\nAssertionError: mismatch\n      at tests/orders.test.ts:42:9\n\nnpm ERR! code ELIFECYCLE\nCANCELED\nThe command '/bin/sh -c npm test' returned a non-zero code: 1\n{}\n1 failed, 19 passed\n", "INFO successful iteration\n".repeat(200), "INFO captured stdout\n".repeat(200));
    let input = dir.path().join("diagnostics.log");
    std::fs::write(&input, &raw).unwrap();
    let binary = std::env::var_os("LM_RESIZER_TEST_BINARY")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_lm-resizer").into());
    let output = Command::new(binary)
        .env("LM_RESIZER_STATE_DIR", dir.path().join("state"))
        .args(["compress", "--json", "--input"])
        .arg(input)
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let view = report["output"].as_str().unwrap();
    for fact in [
        "E   assert 301 == 300",
        "assert total == 300",
        "tests/test_orders.py:42: in test_order_total",
        "called `Option::unwrap()` on a `None` value",
        "at tests/orders.test.ts:42:9",
        "npm ERR! code ELIFECYCLE",
        "CANCELED",
        "returned a non-zero code: 1",
    ] {
        assert!(view.contains(fact), "lost diagnostic: {fact}");
    }
}

#[test]
fn git_log_shown_authors_are_correct_and_all_commits_recoverable() {
    let dir = tempfile::tempdir().unwrap();
    let raw = (0..80).map(|n| format!("commit {n:040x}\nAuthor: {} <bench@example.invalid>\nDate: 2026-10-02\n\n    Same complete body, commit {n}\n\n", ["Alice", "Bob", "Carol"][n % 3])).collect::<String>();
    let input = dir.path().join("log");
    std::fs::write(&input, &raw).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", dir.path().join("state"))
        .args(["tool-output", "--json", "--command", "git log", "--input"])
        .arg(input)
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let view = report["output"].as_str().unwrap();
    let mut commit = None;
    for line in view.lines() {
        if let Some(hash) = line.strip_prefix("commit ") {
            commit = Some(usize::from_str_radix(hash, 16).unwrap());
        }
        if line.trim_start().starts_with("Author:") {
            let n = commit.expect("an author must have a commit");
            assert!(line.contains(["Alice", "Bob", "Carol"][n % 3]));
        }
    }
    assert!(commit.is_some());
    assert_recovered(&report, dir.path(), &raw);
}

#[test]
fn direct_grep_and_rg_keep_source_matches_even_when_named_error_or_panic() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("matches");
    let raw = (1..400)
        .map(|n| format!("src/module.rs:{n}:fn error_does_not_panic_{n}() {{}}\n"))
        .collect::<String>();
    std::fs::write(&input, &raw).unwrap();
    let location = regex::Regex::new(r"^\s+(\d+): (.*)$").unwrap();
    for command in ["grep -rn fn src", "rg --line-number fn src"] {
        let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("LM_RESIZER_STATE_DIR", dir.path().join("state"))
            .args(["tool-output", "--json", "--command", command, "--input"])
            .arg(&input)
            .output()
            .unwrap();
        assert!(out.status.success());
        let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let view = report["output"].as_str().unwrap();
        assert!(view.contains("399 matches"));
        let mut shown = 0;
        for line in view.lines() {
            if let Some(caps) = location.captures(line) {
                let n: usize = caps[1].parse().unwrap();
                assert_eq!(&caps[2], format!("fn error_does_not_panic_{n}() {{}}"));
                shown += 1;
            }
        }
        assert!(shown > 0);
        assert_recovered(&report, dir.path(), &raw);
    }
}

fn assert_recovered(report: &serde_json::Value, dir: &std::path::Path, raw: &str) {
    let key = report["tee_hint"]
        .as_str()
        .unwrap()
        .strip_prefix("[raw: ")
        .unwrap()
        .strip_suffix(']')
        .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", dir.join("state"))
        .args(["tee", "read", key])
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(result.stdout, raw.as_bytes());
}
