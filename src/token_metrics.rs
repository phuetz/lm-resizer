//! Reproducible text-only metrics; a reference encoding, not provider billing.
use std::collections::{BTreeMap, VecDeque};
use std::sync::{LazyLock, Mutex};

use lm_resizer_core::tokenizer::{TiktokenCounter, Tokenizer};
use serde::Serialize;
use serde_json::{json, Value};

pub const TOKENIZER: &str = "tiktoken-rs/o200k_base";
static COUNTER: LazyLock<TiktokenCounter> =
    LazyLock::new(|| TiktokenCounter::for_model("gpt-4o").expect("reference tokenizer"));

// `exec` first chooses a lossless view, then counts the final view with its
// recovery marker. Keep at most three texts, <=16 MiB total, for raw, filtered and final views. Equality is checked on the full text, never a hash alone.
static COUNT_CACHE: LazyLock<Mutex<VecDeque<(String, usize)>>> =
    LazyLock::new(|| Mutex::new(VecDeque::new()));

fn count_cached(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    let reuse = {
        let cache = COUNT_CACHE.lock().expect("token count cache");
        let mut prefix_count = None;
        for (known, count) in cache.iter().rev() {
            if known == text {
                return *count;
            }
            // o200k has a proven pre-token boundary after LF and before '[':
            // punctuation's trailing [\r\n/]* cannot absorb '['. Restrict
            // reuse to our short recovery trailer, not arbitrary extensions.
            if known.ends_with('\n') && text.starts_with(known) {
                let suffix = &text[known.len()..];
                if suffix.len() <= 100
                    && (suffix.starts_with("[raw: ") || suffix.starts_with("[tee:"))
                    && suffix.ends_with('\n')
                    && !suffix[..suffix.len() - 1].contains('\n')
                {
                    prefix_count = Some((*count, known.len()));
                }
            }
        }
        prefix_count
    };
    let count = match reuse {
        Some((prefix, offset)) => prefix + COUNTER.count_text(&text[offset..]),
        None => COUNTER.count_text(text),
    };
    if text.len() <= 16 * 1024 * 1024 {
        let mut cache = COUNT_CACHE.lock().expect("token count cache");
        while cache.len() >= 3
            || cache.iter().map(|(s, _)| s.len()).sum::<usize>() + text.len() > 16 * 1024 * 1024
        {
            cache.pop_front();
        }
        cache.push_back((text.to_string(), count));
    }
    count
}

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
        if original.is_empty() && compressed.is_empty() {
            return Self::default();
        }
        let started = std::time::Instant::now();
        let _counter = &*COUNTER;
        crate::perf_stage("tokenizer_init", started.elapsed());
        let started = std::time::Instant::now();
        let original_tokens = count_cached(original);
        crate::perf_stage("original_token_count", started.elapsed());
        let compressed_tokens = if original == compressed {
            original_tokens
        } else {
            count_cached(compressed)
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

/// Project filtering never assigns legacy entries without cwd to a project.
/// Signed exact savings and legacy estimates keep their existing distinction.
pub fn select_history(
    content: &str,
    project: Option<&str>,
    limit: usize,
) -> (Value, Vec<Value>, usize) {
    let records: Vec<Value> = content
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(Value::is_object)
        .collect();
    let unscoped = records
        .iter()
        .filter(|row| row["cwd"].as_str().is_none())
        .count();
    let mut selected: Vec<Value> = records
        .into_iter()
        .filter(|row| project.is_none_or(|path| row["cwd"].as_str() == Some(path)))
        .collect();
    let summary = summarize_history(
        &selected
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    );
    selected.reverse();
    selected.sort_by_key(|row| std::cmp::Reverse(row["timestamp_unix"].as_u64().unwrap_or(0)));
    selected.truncate(limit);
    (summary, selected, unscoped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_counts_and_recovery_boundary_match_uncached_bpe() {
        for prefix in [
            "",
            "plain\n",
            "\r\n",
            "//\n",
            "!1\n",
            "with spaces  \n\n",
            "é漢字\n",
            "<|endoftext|>\n",
            "a\t\n",
            "[raw: literal]\n",
        ] {
            for repeats in [1, 17, 1000] {
                let raw = prefix.repeat(repeats);
                let trailer = "[tee:abcdef012345] lm-resizer tee read abcdef012345\n";
                assert_eq!(count_cached(&raw), COUNTER.count_text(&raw));
                let combined = raw + trailer;
                assert_eq!(count_cached(&combined), COUNTER.count_text(&combined));
                assert_eq!(count_cached(&combined), COUNTER.count_text(&combined));
            }
        }
    }

    #[test]
    fn project_history_retains_negative_savings_and_excludes_unscoped_rows() {
        let mut a = json!({"cwd":"/project/a","timestamp_unix":2,"command":"cargo test","tokenizer":TOKENIZER,"token_count_method":"exact","original_tokens":10,"compressed_tokens":12});
        let legacy = json!({"timestamp_unix":1,"command":"legacy","bytes_saved":40});
        let first = a.to_string();
        a["cwd"] = json!("/project/b");
        let content = [first, a.to_string(), legacy.to_string()].join("\n");
        let (summary, recent, unscoped) = select_history(&content, Some("/project/a"), 1);
        assert_eq!(summary["commands"], 1);
        assert_eq!(summary["tokens_saved"], -2);
        assert_eq!(recent.len(), 1);
        assert_eq!(unscoped, 1);
        assert_eq!(select_history(&content, None, 2).0["commands"], 3);
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
