//! Reproducible offline A/B corpus: raw control against the same captured
//! output after LM Resizer. No provider billing or answer-quality claim.
//!
//! Run: cargo build && cargo run --example parity_bench -- target/debug/lm-resizer

use lm_resizer_core::tokenizer::get_tokenizer;
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Deserialize)]
struct Case {
    name: String,
    kind: String,
    path: String,
    command: Option<String>,
    #[serde(default)]
    exit_code: i32,
    #[serde(default)]
    raw_equal: bool,
    oracle: Vec<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let binary = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/debug/lm-resizer"));
    let binary = binary.canonicalize()?;
    let cases: Vec<Case> =
        serde_json::from_slice(&std::fs::read(root.join("fixtures/parity/manifest.json"))?)?;
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let temp =
        std::env::temp_dir().join(format!("lm-resizer-parity-{}-{nonce}", std::process::id()));
    let source = temp.join("source");
    std::fs::create_dir_all(&source)?;
    for case in cases
        .iter()
        .filter(|c| c.kind == "source" || c.kind == "smart")
    {
        let source_file = root.join(&case.path);
        std::fs::copy(&source_file, source.join(source_file.file_name().unwrap()))?;
    }

    let mut ce = Command::new("code-explorer");
    ce.args(["analyze", "--skip-git", "--force"])
        .arg(&source)
        .env("CODE_EXPLORER_HOME", &temp);
    let ce_ready = ce.output().is_ok_and(|o| o.status.success());
    if !ce_ready {
        std::fs::remove_dir_all(&temp)?;
        return Err("Code Explorer did not create the required source index".into());
    }
    let tokenizer = get_tokenizer("gpt-4o");
    let store = temp.join("ccr.sqlite3");
    let state = temp.join("state");

    println!("Corpus: fixtures/parity/manifest.json | tokenizer: gpt-4o | Code Explorer indexed: {ce_ready}");
    println!("Control: raw captured bytes. Treatment: LM Resizer CLI, same bytes and exit code.");
    println!("Latency: elapsed CLI wall time (includes process startup), median of 3 runs; raw control adds 0 ms transformation.");
    println!("| Case | Raw tokens | After tokens | Saved | Oracle | CCR | Median ms | Mode |");
    println!("|---|---:|---:|---:|---|---|---:|---|");
    let mut total_before = 0usize;
    let mut total_after = 0usize;
    let mut failed = false;
    for case in &cases {
        let fixture = root.join(&case.path);
        let raw = std::fs::read_to_string(&fixture)?;
        let input = if case.kind == "source" || case.kind == "smart" {
            source.join(fixture.file_name().unwrap())
        } else {
            fixture
        };
        let mut timings = Vec::new();
        let mut response = Value::Null;
        for _ in 0..3 {
            let mut cmd = Command::new(&binary);
            if case.kind == "smart" {
                cmd.arg("smart").arg(&input);
            } else if case.kind != "tool" {
                cmd.arg("compress").arg("--input").arg(&input);
            } else {
                cmd.arg("tool-output")
                    .arg("--input")
                    .arg(&input)
                    .arg("--command")
                    .arg(case.command.as_deref().unwrap_or(""))
                    .arg("--exit-code")
                    .arg(case.exit_code.to_string());
            }
            cmd.arg("--json")
                .env("LM_RESIZER_STORE", &store)
                .env("LM_RESIZER_STATE_DIR", &state)
                .env("LM_RESIZER_TRACKING", "0")
                .env("CODE_EXPLORER_HOME", &temp);
            let start = Instant::now();
            let result = cmd.output()?;
            timings.push(start.elapsed().as_secs_f64() * 1000.0);
            if !result.status.success() {
                return Err(
                    format!("{}: {}", case.name, String::from_utf8_lossy(&result.stderr)).into(),
                );
            }
            response = serde_json::from_slice(&result.stdout)?;
        }
        timings.sort_by(f64::total_cmp);
        let after = response["output"].as_str().ok_or("missing output")?;
        let oracle_ok = case.oracle.iter().all(|fact| after.contains(fact))
            && (!case.raw_equal || after == raw);
        let keys = response["cache_keys"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let ccr_ok = if after == raw {
            true
        } else {
            let mut recovered = false;
            for key in keys.iter().filter_map(Value::as_str) {
                let output = Command::new(&binary)
                    .arg("retrieve")
                    .arg(key)
                    .arg("--store")
                    .arg(&store)
                    .env("LM_RESIZER_TRACKING", "0")
                    .output()?;
                if output.status.success() && output.stdout == raw.as_bytes() {
                    recovered = true;
                    break;
                }
            }
            recovered
        };
        let mode = if case.kind == "source" || case.kind == "smart" {
            response["advice"]["status"].as_str().unwrap_or("fallback")
        } else if case.kind == "text" {
            "prose"
        } else {
            response["filter"].as_str().unwrap_or("unknown")
        };
        let before_tokens = tokenizer.count_text(&raw);
        let after_tokens = tokenizer.count_text(after);
        total_before += before_tokens;
        total_after += after_tokens;
        if !oracle_ok
            || !ccr_ok
            || (ce_ready && (case.kind == "source" || case.kind == "smart") && mode != "applied")
        {
            failed = true;
        }
        println!(
            "| {} | {} | {} | {} | {} | {} | {:.2} | {} |",
            case.name,
            before_tokens,
            after_tokens,
            before_tokens as i64 - after_tokens as i64,
            if oracle_ok { "OK" } else { "FAIL" },
            if ccr_ok { "OK" } else { "FAIL" },
            timings[1],
            mode
        );
    }
    println!(
        "Total tokens: {total_before} -> {total_after} (saved {})",
        total_before as i64 - total_after as i64
    );
    std::fs::remove_dir_all(&temp)?;
    if failed {
        return Err("oracle, CCR or structural-mode check failed".into());
    }
    Ok(())
}
