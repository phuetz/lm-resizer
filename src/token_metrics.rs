//! Reproducible text-only metrics; a reference encoding, not provider billing.
use std::collections::BTreeMap;
use std::sync::LazyLock;

use lm_resizer_core::tokenizer::{TiktokenCounter, Tokenizer};
use serde::Serialize;
use serde_json::{json, Value};

pub const TOKENIZER: &str = "tiktoken-rs/o200k_base";
static COUNTER: LazyLock<TiktokenCounter> =
    LazyLock::new(|| TiktokenCounter::for_model("gpt-4o").expect("reference tokenizer"));

#[derive(Debug, Clone, Serialize)]
pub struct TokenCounts {
    pub tokenizer: &'static str,
    pub token_count_method: &'static str,
    pub original_tokens: usize,
    pub compressed_tokens: usize,
    /// Signed: fewer bytes can still require MORE tokens.
    pub tokens_saved: i64,
}

impl Default for TokenCounts {
    fn default() -> Self {
        Self {
            tokenizer: TOKENIZER,
            token_count_method: "exact",
            original_tokens: 0,
            compressed_tokens: 0,
            tokens_saved: 0,
        }
    }
}

impl TokenCounts {
    pub fn measure(original: &str, compressed: &str) -> Self {
        Self::measure_if(true, original, compressed)
    }

    pub fn measure_if(enabled: bool, original: &str, compressed: &str) -> Self {
        Self::measure_with(enabled, original, compressed, |text| {
            let started = std::time::Instant::now();
            let counter = &*COUNTER;
            crate::perf_stage("tokenizer_init", started.elapsed());
            let started = std::time::Instant::now();
            let tokens = counter.count_text(text);
            crate::perf_stage("token_count", started.elapsed());
            tokens
        })
    }

    fn measure_with(
        enabled: bool,
        original: &str,
        compressed: &str,
        mut count: impl FnMut(&str) -> usize,
    ) -> Self {
        if !enabled || (original.is_empty() && compressed.is_empty()) {
            return Self::default();
        }
        let original_tokens = count(original);
        let compressed_tokens = if original == compressed {
            original_tokens
        } else {
            count(compressed)
        };
        Self {
            original_tokens,
            compressed_tokens,
            tokens_saved: original_tokens as i64 - compressed_tokens as i64,
            ..Self::default()
        }
    }

    pub fn add(&mut self, other: &Self) {
        self.original_tokens += other.original_tokens;
        self.compressed_tokens += other.compressed_tokens;
        self.tokens_saved += other.tokens_saved;
    }
}

#[derive(Default, Serialize)]
struct HistoryTotals {
    commands: usize,
    original_bytes: u64,
    compressed_bytes: u64,
    bytes_saved: u64,
    #[serde(flatten)]
    tokens: TokenCounts,
    measured_commands: usize,
    unmeasured_commands: usize,
    /// Compatibility field: ONLY unmeasured historical records, never added
    /// to the exact total. Their original text is no longer available.
    estimated_tokens_saved: u64,
    estimation_method: &'static str,
}

impl HistoryTotals {
    fn add_record(&mut self, record: &Value) {
        self.commands += 1;
        self.original_bytes += number(record, "original_bytes");
        self.compressed_bytes += number(record, "compressed_bytes");
        self.bytes_saved += number(record, "bytes_saved");
        self.estimation_method = "legacy bytes_saved / 4; unmeasured records only";
        match (
            record.get("original_tokens").and_then(Value::as_u64),
            record.get("compressed_tokens").and_then(Value::as_u64),
        ) {
            (Some(before), Some(after))
                if record["tokenizer"] == TOKENIZER && record["token_count_method"] == "exact" =>
            {
                self.measured_commands += 1;
                self.tokens.original_tokens += before as usize;
                self.tokens.compressed_tokens += after as usize;
                self.tokens.tokens_saved += before as i64 - after as i64;
            }
            _ => {
                self.unmeasured_commands += 1;
                self.estimated_tokens_saved += number(record, "bytes_saved") / 4;
            }
        }
    }
}

fn number(record: &Value, key: &str) -> u64 {
    record.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn buckets_json(buckets: BTreeMap<String, HistoryTotals>) -> Vec<Value> {
    let mut rows: Vec<_> = buckets
        .into_iter()
        .map(|(name, totals)| {
            let mut row = serde_json::to_value(totals).expect("history totals");
            row["name"] = json!(name);
            row
        })
        .collect();
    rows.sort_by_key(|row| std::cmp::Reverse(number(row, "bytes_saved")));
    rows.truncate(20);
    rows
}

pub fn summarize_history(content: &str) -> Value {
    let mut totals = HistoryTotals {
        estimation_method: "legacy bytes_saved / 4; unmeasured records only",
        ..HistoryTotals::default()
    };
    let mut by_filter = BTreeMap::<String, HistoryTotals>::new();
    let mut by_command = BTreeMap::<String, HistoryTotals>::new();
    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if !record.is_object() {
            continue;
        }
        totals.add_record(&record);
        let filter = record["filter"].as_str().unwrap_or("unknown").to_string();
        let command = record["command"]
            .as_str()
            .unwrap_or("unknown")
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" ");
        by_filter.entry(filter).or_default().add_record(&record);
        by_command.entry(command).or_default().add_record(&record);
    }
    let mut report = serde_json::to_value(totals).expect("history totals");
    report["by_filter"] = json!(buckets_json(by_filter));
    report["by_command"] = json!(buckets_json(by_command));
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_text_is_counted_once_and_disabled_metrics_never_count() {
        let mut calls = Vec::new();
        let same = TokenCounts::measure_with(true, "héllo", "héllo", |text| {
            calls.push(text.to_string());
            7
        });
        assert_eq!(calls, ["héllo"]);
        assert_eq!(same.original_tokens, 7);
        assert_eq!(same.compressed_tokens, 7);
        assert_eq!(same.tokens_saved, 0);
        for (enabled, original, compressed) in [(false, "before", "after"), (true, "", "")] {
            let counts = TokenCounts::measure_with(enabled, original, compressed, |_| {
                panic!("unneeded tokenizer call")
            });
            assert_eq!(counts.original_tokens, 0);
            assert_eq!(counts.compressed_tokens, 0);
        }
        let mut calls = 0;
        let different = TokenCounts::measure_with(true, "before", "after", |_| {
            calls += 1;
            calls
        });
        assert_eq!(calls, 2);
        assert_eq!(different.tokens_saved, -1);
    }

    #[test]
    fn real_counts_are_not_bytes_divided_by_four() {
        // o200k_base assigns one token to eight repeated ASCII 'a's.
        let counts = TokenCounts::measure("aaaaaaaa", "");
        assert_eq!(counts.original_tokens, 1);
        assert_eq!(counts.tokens_saved, 1);
        assert_ne!(counts.tokens_saved, 8 / 4);
        assert_eq!(TokenCounts::measure("", "aaaaaaaa").tokens_saved, -1);
    }

    #[test]
    fn mixed_history_separates_measurements_from_legacy_estimates() {
        let mut measured = serde_json::to_value(TokenCounts::measure("aaaaaaaa", "")).unwrap();
        measured["command"] = json!("cargo test --workspace");
        measured["filter"] = json!("cargo_test");
        measured["original_bytes"] = json!(8);
        measured["compressed_bytes"] = json!(0);
        measured["bytes_saved"] = json!(8);
        let legacy = json!({"command":"cargo test", "filter":"cargo_test",
            "original_bytes":80,"compressed_bytes":40,"bytes_saved":40});
        let content = format!("{measured}\n{legacy}\nmalformed\nnull\n");
        let summary = summarize_history(&content);
        assert_eq!(summary["commands"], 2);
        assert_eq!(summary["measured_commands"], 1);
        assert_eq!(summary["unmeasured_commands"], 1);
        assert_eq!(summary["original_tokens"], 1);
        assert_eq!(summary["compressed_tokens"], 0);
        assert_eq!(summary["tokens_saved"], 1);
        assert_eq!(summary["estimated_tokens_saved"], 10);
        assert_eq!(summary["by_filter"][0]["tokens_saved"], 1);
        assert_eq!(summary["by_command"][0]["estimated_tokens_saved"], 10);
        assert_eq!(summary["tokenizer"], TOKENIZER);
        // Existing byte fields and grouping shape remain available.
        assert_eq!(summary["bytes_saved"], 48);
        assert_eq!(summary["by_filter"][0]["name"], "cargo_test");
        assert_eq!(summarize_history("")["tokens_saved"], 0);
    }

    #[test]
    fn unknown_encoding_is_never_aggregated_as_reference_measurement() {
        let record = json!({"tokenizer":"another encoding","token_count_method":"exact",
            "original_tokens":100,"compressed_tokens":1,"bytes_saved":8});
        let report = summarize_history(&record.to_string());
        assert_eq!(report["tokens_saved"], 0);
        assert_eq!(report["unmeasured_commands"], 1);
    }
}
