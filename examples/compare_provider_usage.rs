//! Offline A/B comparison of provider usage counters already captured in JSONL.
//!
//! Reads no network and calls no model. Token totals come only from the usage
//! object (`prompt_tokens`/`completion_tokens` or `input_tokens`/`output_tokens`).
//! Prompt and response bodies are dropped on parse and never enter the report.

use anyhow::{bail, Context, Result};
use serde_json::{Number, Value};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::process::ExitCode;

const ORACLE_NOTE: &str = "quality_pass is an oracle supplied with the capture. It does not prove semantic equivalence, factual accuracy, task success, user satisfaction, or a lower provider bill.";

const MEASUREMENT_NOTE: &str = "Counters are the provider usage fields supplied in the JSONL (prompt_tokens/completion_tokens or input_tokens/output_tokens). They are not derived from text length and they are not the planning constants of 75 concision tokens or 800 effort tokens. A synthetic model id such as \"TEST TECHNIQUE\" is a technical fixture, not evidence of provider savings.";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("compare_provider_usage: {err:#}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<()> {
    let mut args = env::args().skip(1);
    let path = args.next().context(
        "missing JSONL path. usage: cargo run --example compare_provider_usage -- <file.jsonl>",
    )?;
    if args.next().is_some() {
        bail!("unexpected extra argument");
    }
    let report = {
        let text = fs::read_to_string(&path).with_context(|| format!("read {path}"))?;
        compare_jsonl(&text)?
    };
    let stdout = io::stdout();
    let mut locked = stdout.lock();
    serde_json::to_writer_pretty(&mut locked, &report)?;
    locked.write_all(b"\n")?;
    Ok(())
}

#[derive(Clone, Copy)]
enum Variant {
    Baseline,
    Optimized,
}

impl Variant {
    fn as_str(self) -> &'static str {
        match self {
            Variant::Baseline => "baseline",
            Variant::Optimized => "optimized",
        }
    }
}

struct Side {
    model: String,
    input_tokens: u64,
    output_tokens: u64,
    quality_pass: bool,
}

#[derive(Default)]
struct PartialPair {
    baseline: Option<Side>,
    optimized: Option<Side>,
}

struct Record {
    case_id: String,
    variant: Variant,
    side: Side,
}

fn compare_jsonl(text: &str) -> Result<Value> {
    let mut pairs: BTreeMap<String, PartialPair> = BTreeMap::new();
    for (idx, line) in text.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(trimmed)
            .with_context(|| format!("line {line_no}: invalid JSON"))?;
        let record = parse_record(value).with_context(|| format!("line {line_no}"))?;
        insert_record(&mut pairs, record)?;
    }
    finalize(pairs)
}

fn parse_record(value: Value) -> Result<Record> {
    let obj = value.as_object().context("record must be a JSON object")?;
    let case_id = require_string(obj, "case_id")?;
    if case_id.is_empty() {
        bail!("case_id is empty");
    }
    let variant = parse_variant(obj.get("variant"), &case_id)?;
    let model = require_string(obj, "model")?;
    if model.is_empty() {
        bail!("case '{case_id}': model is empty");
    }
    let quality_pass = match obj.get("quality_pass") {
        Some(Value::Bool(pass)) => *pass,
        Some(_) => {
            bail!("case '{case_id}': quality_pass has an invalid type; quality is not comparable")
        }
        None => bail!("case '{case_id}': quality_pass is missing; quality is not comparable"),
    };
    let usage = obj
        .get("usage")
        .with_context(|| format!("case '{case_id}': usage is missing"))?;
    let (input_tokens, output_tokens) = parse_usage(usage, &case_id)?;
    // `value` owns any prompt/response text and is dropped with this function.
    Ok(Record {
        case_id,
        variant,
        side: Side {
            model,
            input_tokens,
            output_tokens,
            quality_pass,
        },
    })
}

fn require_string(obj: &serde_json::Map<String, Value>, field: &str) -> Result<String> {
    match obj.get(field) {
        Some(Value::String(text)) => Ok(text.clone()),
        Some(_) => bail!("{field} has an invalid type; expected a string"),
        None => bail!("{field} is missing"),
    }
}

fn parse_variant(value: Option<&Value>, case_id: &str) -> Result<Variant> {
    match value {
        Some(Value::String(text)) if text == "baseline" => Ok(Variant::Baseline),
        Some(Value::String(text)) if text == "optimized" => Ok(Variant::Optimized),
        Some(Value::String(text)) => {
            bail!("case '{case_id}': invalid variant '{text}'; expected baseline or optimized")
        }
        Some(_) => bail!("case '{case_id}': variant has an invalid type"),
        None => bail!("case '{case_id}': variant is missing"),
    }
}

fn parse_usage(value: &Value, case_id: &str) -> Result<(u64, u64)> {
    let obj = value
        .as_object()
        .with_context(|| format!("case '{case_id}': usage has an invalid type"))?;
    let chat = paired_counters(obj, "prompt_tokens", "completion_tokens", case_id)?;
    let responses = paired_counters(obj, "input_tokens", "output_tokens", case_id)?;
    match (chat, responses) {
        (Some(chat), Some(responses)) if chat == responses => Ok(chat),
        (Some(_), Some(_)) => bail!(
            "case '{case_id}': usage counters disagree between prompt_tokens/completion_tokens and input_tokens/output_tokens"
        ),
        (Some(chat), None) => Ok(chat),
        (None, Some(responses)) => Ok(responses),
        (None, None) => bail!(
            "case '{case_id}': usage is missing provider counters (prompt_tokens/completion_tokens or input_tokens/output_tokens)"
        ),
    }
}

fn paired_counters(
    obj: &serde_json::Map<String, Value>,
    input_field: &str,
    output_field: &str,
    case_id: &str,
) -> Result<Option<(u64, u64)>> {
    match (obj.get(input_field), obj.get(output_field)) {
        (None, None) => Ok(None),
        (Some(input), Some(output)) => Ok(Some((
            non_negative_integer(input, input_field, case_id)?,
            non_negative_integer(output, output_field, case_id)?,
        ))),
        _ => bail!(
            "case '{case_id}': incomplete usage; {input_field} and {output_field} are required together"
        ),
    }
}

fn non_negative_integer(value: &Value, field: &str, case_id: &str) -> Result<u64> {
    let Value::Number(number) = value else {
        bail!("case '{case_id}': {field} has an invalid type; expected a non-negative integer");
    };
    if let Some(parsed) = number.as_u64() {
        return Ok(parsed);
    }
    let rendered = number.to_string();
    if rendered.starts_with('-') {
        bail!("case '{case_id}': {field} is negative");
    }
    if rendered.contains('.') || rendered.contains('e') || rendered.contains('E') {
        bail!("case '{case_id}': {field} is not an integer");
    }
    bail!("case '{case_id}': {field} is outside u64");
}

fn insert_record(pairs: &mut BTreeMap<String, PartialPair>, record: Record) -> Result<()> {
    let slot = pairs.entry(record.case_id.clone()).or_default();
    let destination = match record.variant {
        Variant::Baseline => &mut slot.baseline,
        Variant::Optimized => &mut slot.optimized,
    };
    if destination.is_some() {
        bail!(
            "duplicate {} record for case '{}'",
            record.variant.as_str(),
            record.case_id
        );
    }
    *destination = Some(record.side);
    Ok(())
}

fn finalize(pairs: BTreeMap<String, PartialPair>) -> Result<Value> {
    if pairs.is_empty() {
        bail!("no baseline/optimized pairs in input");
    }

    let mut degraded = Vec::new();
    let mut shared_model: Option<&str> = None;
    for (case_id, pair) in &pairs {
        let (baseline, optimized) = complete_pair(case_id, pair)?;
        if baseline.model != optimized.model {
            bail!(
                "case '{case_id}': model mismatch between baseline ({}) and optimized ({})",
                baseline.model,
                optimized.model
            );
        }
        match shared_model {
            None => shared_model = Some(baseline.model.as_str()),
            Some(model) if model != baseline.model => {
                bail!(
                    "heterogeneous models in file ('{model}' and '{}'); one report covers one model",
                    baseline.model
                );
            }
            Some(_) => {}
        }
        if baseline.quality_pass && !optimized.quality_pass {
            degraded.push(case_id.clone());
        }
    }
    if !degraded.is_empty() {
        bail!(
            "quality degraded for case(s) {}: baseline quality_pass is true and optimized quality_pass is false; refusing to report token savings",
            degraded.join(", ")
        );
    }

    let mut input_baseline = 0u64;
    let mut input_optimized = 0u64;
    let mut output_baseline = 0u64;
    let mut output_optimized = 0u64;
    let mut both_pass = 0u64;
    let mut both_fail = 0u64;
    let mut optimized_recovered = 0u64;
    for pair in pairs.values() {
        let baseline = pair.baseline.as_ref().expect("pair checked");
        let optimized = pair.optimized.as_ref().expect("pair checked");
        input_baseline = checked_tokens(input_baseline, baseline.input_tokens, "input baseline")?;
        input_optimized =
            checked_tokens(input_optimized, optimized.input_tokens, "input optimized")?;
        output_baseline =
            checked_tokens(output_baseline, baseline.output_tokens, "output baseline")?;
        output_optimized = checked_tokens(
            output_optimized,
            optimized.output_tokens,
            "output optimized",
        )?;
        match (baseline.quality_pass, optimized.quality_pass) {
            (true, true) => both_pass += 1,
            (false, false) => both_fail += 1,
            (false, true) => optimized_recovered += 1,
            (true, false) => unreachable!("degraded pairs already refused"),
        }
    }

    let input_total_baseline = checked_tokens(input_baseline, output_baseline, "total baseline")?;
    let input_total_optimized =
        checked_tokens(input_optimized, output_optimized, "total optimized")?;
    let model = shared_model.context("model missing after pair checks")?;
    Ok(serde_json::json!({
        "sample_size": pairs.len(),
        "model": model,
        "input": counter_block(input_baseline, input_optimized)?,
        "output": counter_block(output_baseline, output_optimized)?,
        "total": counter_block(input_total_baseline, input_total_optimized)?,
        "quality_pass_counts": {
            "both_pass": both_pass,
            "both_fail": both_fail,
            "optimized_recovered": optimized_recovered,
        },
        "quality_pass_oracle": ORACLE_NOTE,
        "measurement": MEASUREMENT_NOTE,
    }))
}

fn complete_pair<'a>(case_id: &str, pair: &'a PartialPair) -> Result<(&'a Side, &'a Side)> {
    match (&pair.baseline, &pair.optimized) {
        (Some(baseline), Some(optimized)) => Ok((baseline, optimized)),
        (Some(_), None) => {
            bail!("missing pair for case '{case_id}': optimized record absent")
        }
        (None, Some(_)) => {
            bail!("missing pair for case '{case_id}': baseline record absent")
        }
        (None, None) => bail!("missing pair for case '{case_id}'"),
    }
}

fn checked_tokens(sum: u64, addend: u64, label: &str) -> Result<u64> {
    sum.checked_add(addend)
        .with_context(|| format!("{label} sum overflow"))
}

fn counter_block(baseline: u64, optimized: u64) -> Result<Value> {
    let difference = i128::from(baseline) - i128::from(optimized);
    let savings_percent = if baseline == 0 {
        Value::Null
    } else {
        Value::Number(percent_number(difference, baseline)?)
    };
    Ok(serde_json::json!({
        "baseline_tokens": baseline,
        "optimized_tokens": optimized,
        "absolute_difference": json_i128(difference)?,
        "savings_percent": savings_percent,
    }))
}

fn percent_number(difference: i128, baseline: u64) -> Result<Number> {
    let denom = i128::from(baseline);
    let numer = difference
        .checked_mul(10_000)
        .context("savings percent overflow")?;
    let mut hundredths = numer / denom;
    let remainder = numer % denom;
    let remainder_abs = remainder
        .checked_abs()
        .context("savings percent remainder")?;
    if remainder_abs
        .checked_mul(2)
        .context("savings percent rounding")?
        >= denom
    {
        hundredths += if numer >= 0 { 1 } else { -1 };
    }
    let negative = hundredths < 0;
    let abs = hundredths.unsigned_abs();
    let whole = abs / 100;
    let frac = abs % 100;
    let rendered = if negative {
        format!("-{whole}.{frac:02}")
    } else {
        format!("{whole}.{frac:02}")
    };
    serde_json::from_str::<Number>(&rendered).context("serialize savings percent")
}

fn json_i128(value: i128) -> Result<Value> {
    let number: Number =
        serde_json::from_str(&value.to_string()).context("serialize absolute difference")?;
    Ok(Value::Number(number))
}

#[cfg(test)]
mod tests {
    use super::compare_jsonl;
    use serde_json::Value;

    const MODEL: &str = "TEST TECHNIQUE";

    fn report(jsonl: &str) -> Value {
        compare_jsonl(jsonl).unwrap_or_else(|err| panic!("expected a report, got {err:#}"))
    }

    fn error_text(jsonl: &str) -> String {
        match compare_jsonl(jsonl) {
            Ok(value) => panic!("expected a refusal, got {value}"),
            Err(err) => format!("{err:#}"),
        }
    }

    fn line(case_id: &str, variant: &str, usage: &str, quality_pass: bool, extra: &str) -> String {
        format!(
            r#"{{"case_id":"{case_id}","variant":"{variant}","model":"{MODEL}","usage":{usage},"quality_pass":{quality_pass}{extra}}}"#
        )
    }

    #[test]
    fn nominal_uses_supplied_counters_not_text_or_planning_constants() {
        let jsonl = format!(
            "{}\n{}\n{}\n{}\n",
            line(
                "c1",
                "baseline",
                r#"{"prompt_tokens":200,"completion_tokens":100,"total_tokens":99999}"#,
                true,
                r#","prompt":"SENTINEL_PROMPT","response":"xxxxxxxxxxxxxxxx""#
            ),
            line(
                "c1",
                "optimized",
                r#"{"prompt_tokens":100,"completion_tokens":50}"#,
                true,
                ""
            ),
            line(
                "c2",
                "baseline",
                r#"{"input_tokens":100,"output_tokens":40}"#,
                true,
                ""
            ),
            line(
                "c2",
                "optimized",
                r#"{"input_tokens":50,"output_tokens":20}"#,
                true,
                ""
            ),
        );
        let value = report(&jsonl);
        let raw = serde_json::to_string(&value).unwrap();
        assert!(!raw.contains("SENTINEL_PROMPT"));
        assert!(!raw.contains("xxxxxxxx"));
        assert!(!raw.contains("99999"));
        assert_eq!(value["sample_size"], 2);
        assert_eq!(value["model"], MODEL);
        assert_eq!(value["input"]["baseline_tokens"], 300);
        assert_eq!(value["input"]["optimized_tokens"], 150);
        assert_eq!(value["input"]["absolute_difference"], 150);
        assert_eq!(value["input"]["savings_percent"].to_string(), "50.00");
        assert_eq!(value["output"]["baseline_tokens"], 140);
        assert_eq!(value["output"]["optimized_tokens"], 70);
        assert_eq!(value["output"]["absolute_difference"], 70);
        assert_eq!(value["output"]["savings_percent"].to_string(), "50.00");
        assert_eq!(value["total"]["baseline_tokens"], 440);
        assert_eq!(value["total"]["optimized_tokens"], 220);
        assert_eq!(value["total"]["absolute_difference"], 220);
        assert_eq!(value["total"]["savings_percent"].to_string(), "50.00");
        assert_eq!(value["quality_pass_counts"]["both_pass"], 2);
        assert!(value["quality_pass_oracle"]
            .as_str()
            .unwrap()
            .contains("oracle"));
        assert!(value["measurement"]
            .as_str()
            .unwrap()
            .contains("TEST TECHNIQUE"));
        assert!(!value["output"]["absolute_difference"]
            .to_string()
            .contains("75"));
        assert_ne!(value["output"]["absolute_difference"], 800);
    }

    #[test]
    fn rejects_bad_usage() {
        let cases = [
            (
                line(
                    "bad",
                    "baseline",
                    r#"{"prompt_tokens":1.0,"completion_tokens":1}"#,
                    true,
                    "",
                ),
                "not an integer",
            ),
            (
                line(
                    "bad",
                    "baseline",
                    r#"{"prompt_tokens":1,"completion_tokens":-3}"#,
                    true,
                    "",
                ),
                "negative",
            ),
            (
                line(
                    "bad",
                    "baseline",
                    r#"{"prompt_tokens":1.5,"completion_tokens":1}"#,
                    true,
                    "",
                ),
                "not an integer",
            ),
            (
                line(
                    "bad",
                    "baseline",
                    r#"{"prompt_tokens":"10","completion_tokens":1}"#,
                    true,
                    "",
                ),
                "invalid type",
            ),
            (
                line("bad", "baseline", r#"{}"#, true, ""),
                "missing provider counters",
            ),
            (
                line("bad", "baseline", r#"{"prompt_tokens":10}"#, true, ""),
                "incomplete usage",
            ),
            (
                line(
                    "bad",
                    "baseline",
                    r#"{"prompt_tokens":4,"completion_tokens":2,"input_tokens":4,"output_tokens":1}"#,
                    true,
                    "",
                ),
                "disagree",
            ),
            (
                format!(
                    r#"{{"case_id":"bad","variant":"baseline","model":"{MODEL}","usage":{{"prompt_tokens":1,"completion_tokens":1}},"quality_pass":"yes"}}"#
                ),
                "not comparable",
            ),
            (
                format!(
                    r#"{{"case_id":"bad","variant":"baseline","model":"{MODEL}","usage":{{"prompt_tokens":1,"completion_tokens":1}}}}"#
                ),
                "not comparable",
            ),
        ];
        for (jsonl, needle) in cases {
            let err = error_text(&jsonl);
            assert!(
                err.contains(needle),
                "error `{err}` did not contain `{needle}`"
            );
        }
    }

    #[test]
    fn rejects_duplicates() {
        let jsonl = format!(
            "{}\n{}\n",
            line(
                "dup",
                "baseline",
                r#"{"prompt_tokens":3,"completion_tokens":3}"#,
                true,
                ""
            ),
            line(
                "dup",
                "baseline",
                r#"{"prompt_tokens":1,"completion_tokens":1}"#,
                true,
                ""
            ),
        );
        let err = error_text(&jsonl);
        assert!(err.contains("duplicate"));
        assert!(err.contains("dup"));
        assert!(err.contains("baseline"));
    }

    #[test]
    fn rejects_missing_pair() {
        let jsonl = line(
            "solo",
            "optimized",
            r#"{"input_tokens":8,"output_tokens":2}"#,
            true,
            "",
        );
        let err = error_text(&jsonl);
        assert!(err.contains("missing pair"));
        assert!(err.contains("solo"));
        assert!(err.contains("baseline record absent"));
    }

    #[test]
    fn rejects_degraded_quality_without_a_savings_report() {
        let jsonl = format!(
            "{}\n{}\n",
            line(
                "drop",
                "baseline",
                r#"{"prompt_tokens":100,"completion_tokens":80}"#,
                true,
                ""
            ),
            line(
                "drop",
                "optimized",
                r#"{"prompt_tokens":10,"completion_tokens":1}"#,
                false,
                ""
            ),
        );
        let err = error_text(&jsonl);
        assert!(err.contains("quality degraded"));
        assert!(err.contains("drop"));
        assert!(err.contains("refusing to report token savings"));
        assert!(!err.contains("savings_percent"));
    }

    #[test]
    fn zero_baseline_output_sets_percent_null() {
        let huge_reply = "y".repeat(5_000);
        let jsonl = format!(
            "{}\n{}\n",
            line(
                "zero",
                "baseline",
                r#"{"prompt_tokens":40,"completion_tokens":0}"#,
                true,
                &format!(r#","response":"{huge_reply}""#)
            ),
            line(
                "zero",
                "optimized",
                r#"{"prompt_tokens":20,"completion_tokens":5}"#,
                true,
                "",
            ),
        );
        let value = report(&jsonl);
        let raw = serde_json::to_string(&value).unwrap();
        assert!(!raw.contains(&huge_reply));
        assert_eq!(value["output"]["baseline_tokens"], 0);
        assert_eq!(value["output"]["optimized_tokens"], 5);
        assert_eq!(value["output"]["absolute_difference"], -5);
        assert!(value["output"]["savings_percent"].is_null());
        assert_eq!(value["input"]["savings_percent"].to_string(), "50.00");
        assert_eq!(value["total"]["baseline_tokens"], 40);
        assert_eq!(value["total"]["optimized_tokens"], 25);
        assert_eq!(value["total"]["absolute_difference"], 15);
        assert_eq!(value["total"]["savings_percent"].to_string(), "37.50");
    }

    #[test]
    fn rejects_model_mismatch_and_token_overflow() {
        let mismatch = format!(
            r#"{{"case_id":"m","variant":"baseline","model":"A","usage":{{"prompt_tokens":1,"completion_tokens":1}},"quality_pass":true}}
{{"case_id":"m","variant":"optimized","model":"B","usage":{{"prompt_tokens":1,"completion_tokens":1}},"quality_pass":true}}"#
        );
        let err = error_text(&mismatch);
        assert!(err.contains("model mismatch"));
        assert!(err.contains("m"));

        let overflow = format!(
            "{}\n{}\n{}\n{}\n",
            line(
                "a",
                "baseline",
                r#"{"prompt_tokens":18446744073709551615,"completion_tokens":0}"#,
                true,
                ""
            ),
            line(
                "a",
                "optimized",
                r#"{"prompt_tokens":0,"completion_tokens":0}"#,
                true,
                ""
            ),
            line(
                "b",
                "baseline",
                r#"{"prompt_tokens":1,"completion_tokens":0}"#,
                true,
                ""
            ),
            line(
                "b",
                "optimized",
                r#"{"prompt_tokens":0,"completion_tokens":0}"#,
                true,
                ""
            ),
        );
        let err = error_text(&overflow);
        assert!(err.contains("overflow"));
    }
}
