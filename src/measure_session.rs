//! Session token totals come from Anthropic `message.usage`, never from the
//! serialized transcript. Replaying a Bash result is a separate diagnostic.

use super::*;
use anyhow::bail;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MessageUsage {
    input: u64,
    cache_creation: u64,
    cache_read: u64,
    output: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ToolUse {
    name: String,
    command: Option<String>,
}

fn counter(usage: &Value, field: &str, required: bool, line: usize) -> Result<u64> {
    match usage.get(field) {
        Some(value) => value
            .as_u64()
            .with_context(|| format!("line {line}: {field} must be a non-negative integer")),
        None if !required => Ok(0),
        None => bail!("line {line}: missing {field} in provider usage"),
    }
}

fn parse_usage(usage: &Value, line: usize) -> Result<MessageUsage> {
    if !usage.is_object() {
        bail!("line {line}: provider usage must be an object");
    }
    Ok(MessageUsage {
        input: counter(usage, "input_tokens", true, line)?,
        cache_creation: counter(usage, "cache_creation_input_tokens", false, line)?,
        cache_read: counter(usage, "cache_read_input_tokens", false, line)?,
        output: counter(usage, "output_tokens", true, line)?,
    })
}

fn checked_sum(a: u64, b: u64, field: &str) -> Result<u64> {
    a.checked_add(b)
        .with_context(|| format!("provider token total overflow: {field}"))
}

fn provider_usage_totals(content: &str) -> Result<SessionUsageTotals> {
    let mut messages: HashMap<String, MessageUsage> = HashMap::new();
    for (index, line) in content.lines().enumerate() {
        let line_no = index + 1;
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line)
            .with_context(|| format!("line {line_no}: invalid session JSON"))?;
        let message = value.get("message").unwrap_or(&value);
        let role = message.get("role").and_then(Value::as_str);
        if role != Some("assistant") {
            continue;
        }
        let id = message
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .with_context(|| format!("line {line_no}: assistant message id is missing"))?;
        let usage = message
            .get("usage")
            .with_context(|| format!("line {line_no}: assistant provider usage is missing"))?;
        let parsed = parse_usage(usage, line_no)?;
        match messages.get_mut(id) {
            Some(previous) => {
                if (previous.input, previous.cache_creation, previous.cache_read)
                    != (parsed.input, parsed.cache_creation, parsed.cache_read)
                {
                    bail!("line {line_no}: inconsistent usage for assistant message {id}");
                }
                // A streamed message can appear several times with cumulative
                // output usage. Its final snapshot is the largest one.
                previous.output = previous.output.max(parsed.output);
            }
            None => {
                messages.insert(id.to_string(), parsed);
            }
        }
    }
    if messages.is_empty() {
        bail!("no assistant messages with provider usage; complete session total unavailable");
    }
    let mut totals = SessionUsageTotals {
        requests: messages.len(),
        ..Default::default()
    };
    for usage in messages.values() {
        totals.input_tokens = checked_sum(totals.input_tokens, usage.input, "input")?;
        totals.cache_creation_input_tokens = checked_sum(
            totals.cache_creation_input_tokens,
            usage.cache_creation,
            "cache creation",
        )?;
        totals.cache_read_input_tokens = checked_sum(
            totals.cache_read_input_tokens,
            usage.cache_read,
            "cache read",
        )?;
        totals.output_tokens = checked_sum(totals.output_tokens, usage.output, "output")?;
    }
    totals.total_tokens = checked_sum(
        totals.input_tokens,
        totals.cache_creation_input_tokens,
        "total",
    )?;
    totals.total_tokens =
        checked_sum(totals.total_tokens, totals.cache_read_input_tokens, "total")?;
    totals.total_tokens = checked_sum(totals.total_tokens, totals.output_tokens, "total")?;
    Ok(totals)
}

fn tool_result_text(content: &Value) -> Option<String> {
    if let Some(text) = content.as_str() {
        return Some(text.to_string());
    }
    let blocks = content.as_array()?;
    let mut text = String::new();
    for block in blocks {
        if block.get("type").and_then(Value::as_str) != Some("text") {
            return None;
        }
        text.push_str(block.get("text")?.as_str()?);
    }
    Some(text)
}

fn replay_tool_results(content: &str) -> Result<Vec<ReplayedToolResult>> {
    let tokenizer = lm_resizer_core::tokenizer::get_tokenizer("o200k_base");
    let store = InMemoryCcrStore::new();
    let pipeline = build_pipeline();
    let mut uses: HashMap<String, ToolUse> = HashMap::new();
    let mut seen_results: HashSet<String> = HashSet::new();
    let mut results = Vec::new();
    for (index, line) in content.lines().enumerate() {
        let line_no = index + 1;
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line)
            .with_context(|| format!("line {line_no}: invalid session JSON"))?;
        let mut stack = vec![&value];
        while let Some(node) = stack.pop() {
            match node {
                Value::Object(obj) => {
                    match obj.get("type").and_then(Value::as_str) {
                        Some("tool_use") => {
                            let id = obj.get("id").and_then(Value::as_str).with_context(|| {
                                format!("line {line_no}: tool_use id is missing")
                            })?;
                            let name =
                                obj.get("name").and_then(Value::as_str).with_context(|| {
                                    format!("line {line_no}: tool_use name is missing")
                                })?;
                            let command = if name == "Bash" {
                                Some(
                                    obj.get("input")
                                        .and_then(|v| v.get("command"))
                                        .and_then(Value::as_str)
                                        .with_context(|| {
                                            format!("line {line_no}: Bash command is missing")
                                        })?
                                        .to_string(),
                                )
                            } else {
                                None
                            };
                            let current = ToolUse {
                                name: name.to_string(),
                                command,
                            };
                            if let Some(previous) = uses.insert(id.to_string(), current.clone()) {
                                if previous != current {
                                    bail!("line {line_no}: conflicting tool_use id {id}");
                                }
                            }
                            continue;
                        }
                        Some("tool_result") => {
                            let id = obj
                                .get("tool_use_id")
                                .and_then(Value::as_str)
                                .with_context(|| {
                                    format!("line {line_no}: tool_result id is missing")
                                })?;
                            let tool = uses.get(id).with_context(|| {
                                format!("line {line_no}: tool_result {id} has no earlier tool_use")
                            })?;
                            if !seen_results.insert(id.to_string()) {
                                bail!("line {line_no}: duplicate tool_result {id}");
                            }
                            if tool.name == "Bash" {
                                let command =
                                    tool.command.as_deref().expect("Bash command validated");
                                let parts = split_shell_words(command)
                                    .filter(|parts| !parts.is_empty())
                                    .with_context(|| {
                                        format!("line {line_no}: invalid Bash command for {id}")
                                    })?;
                                let raw =
                                    obj.get("content").and_then(tool_result_text).with_context(
                                        || format!("line {line_no}: non-text Bash result {id}"),
                                    )?;
                                let (filter, filtered) = filter_command_output(&parts, &raw);
                                let compressed = compress_text_with_pipeline_gate(
                                    &filtered, "", &store, &pipeline, None, false,
                                )
                                .with_context(|| {
                                    format!("line {line_no}: replay failed for {id}")
                                })?;
                                results.push(ReplayedToolResult {
                                    tool_use_id: id.to_string(),
                                    filter,
                                    original_tokens: tokenizer.count_text(&raw),
                                    filtered_tokens: tokenizer.count_text(&compressed.output),
                                });
                            }
                            continue;
                        }
                        _ => {}
                    }
                    for child in obj.values().rev() {
                        stack.push(child);
                    }
                }
                Value::Array(items) => {
                    for child in items.iter().rev() {
                        stack.push(child);
                    }
                }
                _ => {}
            }
        }
    }
    Ok(results)
}

fn savings_percent(baseline: u64, optimized: u64) -> Result<String> {
    if baseline == 0 {
        bail!("baseline provider token total is zero; savings percentage is undefined");
    }
    let difference = i128::from(baseline) - i128::from(optimized);
    let denominator = i128::from(baseline);
    let numerator = difference
        .checked_mul(10_000)
        .context("savings percentage overflow")?;
    let rounded = if numerator >= 0 {
        (numerator + denominator / 2) / denominator
    } else {
        (numerator - denominator / 2) / denominator
    };
    let sign = if rounded < 0 { "-" } else { "" };
    let absolute = rounded.abs();
    Ok(format!("{sign}{}.{:02}", absolute / 100, absolute % 100))
}

pub(super) async fn run_measure_session(
    input: &Path,
    optimized: Option<&Path>,
) -> Result<MeasureSessionReport> {
    let content = tokio::fs::read_to_string(input)
        .await
        .with_context(|| format!("could not read {}", input.display()))?;
    let baseline_usage = provider_usage_totals(&content)?;
    let replayed_tools = replay_tool_results(&content)?;
    let original_tool_tokens = replayed_tools.iter().try_fold(0usize, |sum, result| {
        sum.checked_add(result.original_tokens)
            .context("replay token total overflow")
    })?;
    let filtered_tool_tokens = replayed_tools.iter().try_fold(0usize, |sum, result| {
        sum.checked_add(result.filtered_tokens)
            .context("replay token total overflow")
    })?;
    let optimized_usage = match optimized {
        Some(path) => {
            let optimized_content = tokio::fs::read_to_string(path)
                .await
                .with_context(|| format!("could not read {}", path.display()))?;
            // Validate tool-use/result linkage on both sides of the comparison.
            replay_tool_results(&optimized_content)?;
            Some(provider_usage_totals(&optimized_content)?)
        }
        None => None,
    };
    let session_token_difference = optimized_usage
        .as_ref()
        .map(|usage| i128::from(baseline_usage.total_tokens) - i128::from(usage.total_tokens));
    let session_token_savings_percent = optimized_usage
        .as_ref()
        .map(|usage| savings_percent(baseline_usage.total_tokens, usage.total_tokens))
        .transpose()?;
    Ok(MeasureSessionReport {
        file_path: input.display().to_string(),
        optimized_file_path: optimized.map(|path| path.display().to_string()),
        baseline_usage,
        optimized_usage,
        session_token_difference,
        session_token_savings_percent,
        tool_results_count: replayed_tools.len(),
        original_tool_tokens,
        filtered_tool_tokens,
        total_session_tokens: baseline_usage.total_tokens,
        replayed_tools,
        caveats: vec![
            "Le pourcentage A/B compare les compteurs de jetons déclarés par le fournisseur pour deux captures complètes ; il ne prouve pas à lui seul une qualité équivalente ni une causalité.".to_string(),
            "Le rejeu local des sorties Bash utilise o200k_base et reste distinct des compteurs du fournisseur ; il ne sert jamais au pourcentage de session.".to_string(),
            "Le coût dépend du modèle, du tarif du cache et d'éventuels outils facturés séparément.".to_string(),
        ],
    })
}

pub(super) fn format_measure_session_report(report: &MeasureSessionReport) -> String {
    let mut out = format!(
        "Session de référence: {}\nRequêtes fournisseur: {}\nEntrée non cachée: {}\nÉcriture cache: {}\nLecture cache: {}\nSortie: {}\nJetons totaux de la session (usage fournisseur): {}\n",
        report.file_path,
        report.baseline_usage.requests,
        report.baseline_usage.input_tokens,
        report.baseline_usage.cache_creation_input_tokens,
        report.baseline_usage.cache_read_input_tokens,
        report.baseline_usage.output_tokens,
        report.total_session_tokens,
    );
    if let (Some(path), Some(usage), Some(difference), Some(percent)) = (
        &report.optimized_file_path,
        &report.optimized_usage,
        report.session_token_difference,
        &report.session_token_savings_percent,
    ) {
        out.push_str(&format!(
            "Session optimisée: {path}\nJetons totaux optimisés (usage fournisseur): {}\nDifférence de jetons A/B: {difference}\nÉconomie de jetons de session A/B: {percent}%\n",
            usage.total_tokens,
        ));
    } else {
        out.push_str("Économie de jetons de session A/B: indisponible sans --optimized\n");
    }
    out.push_str(&format!(
        "Rejeu local Bash: {} résultat(s), {} jetons avant, {} après (o200k_base)\n",
        report.tool_results_count, report.original_tool_tokens, report.filtered_tool_tokens,
    ));
    for result in &report.replayed_tools {
        out.push_str(&format!(
            "- {}: filtre {}, {} → {} jetons\n",
            result.tool_use_id, result.filter, result.original_tokens, result.filtered_tokens,
        ));
    }
    out.push_str("Limites :\n");
    for caveat in &report.caveats {
        out.push_str(&format!("- {caveat}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_usage_and_malformed_json_instead_of_a_partial_total() {
        let missing =
            r#"{"type":"assistant","message":{"id":"m1","role":"assistant","content":[]}}"#;
        assert!(provider_usage_totals(missing)
            .unwrap_err()
            .to_string()
            .contains("usage is missing"));
        let valid = r#"{"type":"assistant","message":{"id":"m1","role":"assistant","usage":{"input_tokens":1,"output_tokens":1}}}"#;
        let malformed = format!("{valid}\n{{broken");
        assert!(provider_usage_totals(&malformed)
            .unwrap_err()
            .to_string()
            .contains("line 2"));
    }

    #[test]
    fn deduplicates_stream_snapshots_and_includes_cache_tokens() {
        let first = r#"{"type":"assistant","message":{"id":"m1","role":"assistant","usage":{"input_tokens":3,"cache_creation_input_tokens":5,"cache_read_input_tokens":7,"output_tokens":2}}}"#;
        let final_snapshot = first.replace("\"output_tokens\":2", "\"output_tokens\":4");
        let totals = provider_usage_totals(&format!("{first}\n{final_snapshot}")).unwrap();
        assert_eq!(totals.requests, 1);
        assert_eq!(totals.total_tokens, 19);
        let invalid = first.replace("\"input_tokens\":3", "\"input_tokens\":4");
        assert!(provider_usage_totals(&format!("{first}\n{invalid}")).is_err());
    }

    #[test]
    fn rejects_bad_usage_counters_and_overflow() {
        for value in ["-1", "1.0", "\"5\"", "null"] {
            let line = format!(
                r#"{{"type":"assistant","message":{{"id":"m1","role":"assistant","usage":{{"input_tokens":{value},"output_tokens":1}}}}}}"#
            );
            assert!(provider_usage_totals(&line).is_err(), "accepted {value}");
        }
        let overflow = format!(
            r#"{{"type":"assistant","message":{{"id":"m1","role":"assistant","usage":{{"input_tokens":{},"output_tokens":1}}}}}}"#,
            u64::MAX
        );
        assert!(provider_usage_totals(&overflow)
            .unwrap_err()
            .to_string()
            .contains("overflow"));
    }

    #[test]
    fn tool_result_must_match_a_prior_tool_use() {
        let fixture = std::fs::read_to_string("fixtures/exec/synthetic_session.jsonl").unwrap();
        let broken = fixture.replace(
            "\"tool_use_id\": \"call_1\"",
            "\"tool_use_id\": \"wrong_id\"",
        );
        assert!(replay_tool_results(&broken)
            .unwrap_err()
            .to_string()
            .contains("no earlier tool_use"));
        let replay = replay_tool_results(&fixture).unwrap();
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].filter, "cargo_test");
        assert_eq!(
            (replay[0].original_tokens, replay[0].filtered_tokens),
            (110, 8)
        );
    }

    #[test]
    fn percentage_rounding_is_reproducible() {
        assert_eq!(savings_percent(390, 300).unwrap(), "23.08");
        assert_eq!(savings_percent(300, 390).unwrap(), "-30.00");
        assert!(savings_percent(0, 1).is_err());
    }
}
