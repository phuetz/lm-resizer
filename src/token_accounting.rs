//! Count command output, never infer provider usage or billing from it.
use std::collections::BTreeMap;
use std::sync::OnceLock;

use lm_resizer_core::tokenizer::{get_tokenizer, Backend, TiktokenCounter, Tokenizer};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

static COUNTER: OnceLock<(String, Box<dyn Tokenizer>)> = OnceLock::new();
static SELECTION: OnceLock<String> = OnceLock::new();

fn counter(name: &str) -> (String, Box<dyn Tokenizer>) {
    let model = match name {
        "o200k_base" => "gpt-4o",
        "cl100k_base" => "gpt-4",
        "p50k_base" => "code-davinci-002",
        "r50k_base" => "davinci",
        model => model,
    };
    if let Ok(counter) = TiktokenCounter::for_model(model) {
        (counter.encoding_name().to_string(), Box::new(counter))
    } else {
        (name.to_string(), get_tokenizer(model))
    }
}

pub fn configure(name: &str) {
    SELECTION.get_or_init(|| name.to_string());
}

fn selected_counter() -> &'static (String, Box<dyn Tokenizer>) {
    COUNTER.get_or_init(|| counter(SELECTION.get().map(String::as_str).unwrap_or("o200k_base")))
}

pub fn smaller_representation(original: String, candidate: String) -> String {
    let (_, tokenizer) = selected_counter();
    if candidate.len() < original.len()
        && tokenizer.count_text(&candidate) < tokenizer.count_text(&original)
    {
        candidate
    } else {
        original
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenSavings {
    pub tokenizer: String,
    pub estimated: bool,
    pub commands: usize,
    pub original_tokens: usize,
    pub compressed_tokens: usize,
    /// Signed: expansion must remain visible.
    pub tokens_saved: i64,
}

pub fn measure(original: &str, emitted: &str) -> TokenSavings {
    let (name, tokenizer) = selected_counter();
    measure_with(name, tokenizer.as_ref(), original, emitted)
}

fn measure_with(
    name: &str,
    tokenizer: &dyn Tokenizer,
    original: &str,
    emitted: &str,
) -> TokenSavings {
    let original_tokens = tokenizer.count_text(original);
    let compressed_tokens = tokenizer.count_text(emitted);
    TokenSavings {
        tokenizer: name.to_string(),
        estimated: tokenizer.backend() == Backend::Estimation,
        commands: 1,
        original_tokens,
        compressed_tokens,
        tokens_saved: original_tokens as i64 - compressed_tokens as i64,
    }
}

pub fn merge(groups: &mut Vec<TokenSavings>, row: TokenSavings) {
    if let Some(group) = groups
        .iter_mut()
        .find(|g| g.tokenizer == row.tokenizer && g.estimated == row.estimated)
    {
        group.commands += row.commands;
        group.original_tokens += row.original_tokens;
        group.compressed_tokens += row.compressed_tokens;
        group.tokens_saved += row.tokens_saved;
    } else {
        groups.push(row);
        groups.sort_by(|a, b| (&a.tokenizer, a.estimated).cmp(&(&b.tokenizer, b.estimated)));
    }
}

pub fn describe(groups: &[TokenSavings]) -> String {
    if groups.is_empty() {
        return "0 tokens saved out of 0 (no observations)".to_string();
    }
    groups
        .iter()
        .map(|g| {
            let percent = if g.original_tokens == 0 {
                0.0
            } else {
                100.0 * g.tokens_saved as f64 / g.original_tokens as f64
            };
            format!(
                "{} tokens saved out of {} ({percent:.1}%; {}, {}; {} commands)",
                g.tokens_saved,
                g.original_tokens,
                g.tokenizer,
                if g.estimated { "estimate" } else { "counted" },
                g.commands
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

pub fn describe_json(value: &Value) -> String {
    describe(&serde_json::from_value::<Vec<TokenSavings>>(value.clone()).unwrap_or_default())
}

#[derive(Default)]
struct Bucket {
    commands: usize,
    original_bytes: usize,
    compressed_bytes: usize,
    bytes_saved: usize,
    tokens: Vec<TokenSavings>,
}

impl Bucket {
    fn add(&mut self, record: &Value, tokens: TokenSavings) {
        self.commands += 1;
        self.original_bytes += number(record, "original_bytes");
        self.compressed_bytes += number(record, "compressed_bytes");
        self.bytes_saved += number(record, "bytes_saved");
        merge(&mut self.tokens, tokens);
    }

    fn json(&self) -> Value {
        json!({"commands": self.commands, "original_bytes": self.original_bytes,
            "compressed_bytes": self.compressed_bytes, "bytes_saved": self.bytes_saved,
            "estimated_tokens_saved": self.bytes_saved / 4,
            "token_savings": self.tokens})
    }
}

fn number(record: &Value, key: &str) -> usize {
    record.get(key).and_then(Value::as_u64).unwrap_or(0) as usize
}

pub fn summarize(content: &str) -> Value {
    let mut total = Bucket::default();
    let mut by_filter = BTreeMap::<String, Bucket>::new();
    let mut by_command = BTreeMap::<String, Bucket>::new();
    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if !record.is_object() {
            continue;
        }
        let tokens = record
            .get("token_savings")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_else(|| {
                let original = number(&record, "original_bytes") / 4;
                let compressed = number(&record, "compressed_bytes") / 4;
                TokenSavings {
                    tokenizer: "legacy_bytes_div_4".to_string(),
                    estimated: true,
                    commands: 1,
                    original_tokens: original,
                    compressed_tokens: compressed,
                    tokens_saved: original as i64 - compressed as i64,
                }
            });
        total.add(&record, tokens.clone());
        let filter = record
            .get("filter")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let command = record
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" ");
        by_filter
            .entry(filter.to_string())
            .or_default()
            .add(&record, tokens.clone());
        by_command.entry(command).or_default().add(&record, tokens);
    }
    let mut result = total.json();
    for (key, buckets) in [("by_filter", by_filter), ("by_command", by_command)] {
        let mut rows = buckets
            .into_iter()
            .map(|(name, bucket)| {
                let mut row = bucket.json();
                row["name"] = Value::String(name);
                row
            })
            .collect::<Vec<_>>();
        rows.sort_by_key(|row| std::cmp::Reverse(number(row, "bytes_saved")));
        rows.truncate(20);
        result[key] = json!(rows);
    }
    result
}

pub fn diagnostics(history: &Value, tracking: bool) -> Vec<String> {
    let mut messages = Vec::new();
    if !tracking {
        messages.push(
            "Tracking disabled: LM_RESIZER_TRACKING=0; unset it to record command savings."
                .to_string(),
        );
    }
    if number(history, "commands") == 0 {
        messages.push("No recorded commands in this state directory. Run lm-resizer exec -- <command>, then lm-resizer gain. Check HOME/XDG_DATA_HOME if another session has statistics.".to_string());
    } else if number(history, "bytes_saved") == 0 {
        messages.push("Recorded commands saved zero bytes. Small or already compact output, raw-on-failure, and unsupported filters may explain this; inspect exec --json and rewrite <command>.".to_string());
    } else {
        messages.push(format!(
            "Recorded command output: {}",
            describe_json(&history["token_savings"])
        ));
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_counts_match_bpe_and_expansion_stays_negative() {
        let (name, t) = counter("o200k_base");
        let text = "été 東京 🦀";
        let row = measure_with(&name, t.as_ref(), text, &text.repeat(10));
        assert_eq!(row.original_tokens, t.count_text(text));
        assert!(row.tokens_saved < 0);
        assert!(!row.estimated);
        assert!(describe(&[row]).contains("counted"));
    }

    #[test]
    fn legacy_and_different_encodings_stay_separate_in_every_bucket() {
        let content = [
            json!({"command":"git log", "filter":"git_log", "original_bytes":100, "compressed_bytes":40, "bytes_saved":60}),
            json!({"command":"git log", "filter":"git_log", "original_bytes":100, "compressed_bytes":40, "bytes_saved":60,
                "token_savings": {"tokenizer":"o200k_base", "estimated":false, "commands":1, "original_tokens":40, "compressed_tokens":5, "tokens_saved":35}}),
            json!({"command":"git log", "filter":"git_log", "original_bytes":100, "compressed_bytes":40, "bytes_saved":60,
                "token_savings": {"tokenizer":"cl100k_base", "estimated":false, "commands":1, "original_tokens":42, "compressed_tokens":7, "tokens_saved":35}}),
        ].map(|r| r.to_string()).join("\n");
        let result = summarize(&(content + "\ninvalid\nnull\n"));
        assert_eq!(result["commands"], 3);
        assert_eq!(result["estimated_tokens_saved"], 45);
        for group in [&result, &result["by_filter"][0], &result["by_command"][0]] {
            assert_eq!(group["token_savings"].as_array().unwrap().len(), 3);
            assert!(describe_json(&group["token_savings"]).contains("estimate"));
        }
    }

    #[test]
    fn zero_savings_and_disabled_tracking_have_actionable_diagnostics() {
        let history = summarize(
            r#"{"commands":1,"original_bytes":20,"compressed_bytes":20,"bytes_saved":0}"#,
        );
        assert!(diagnostics(&history, true)[0].contains("zero bytes"));
        assert!(diagnostics(&history, false)[0].contains("Tracking disabled"));
    }
}
