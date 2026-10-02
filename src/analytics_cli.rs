//! Calendar views over existing exact execution metrics. No output filters.
use anyhow::Result;
use chrono::{DateTime, Datelike, Duration, Utc};
use clap::{Args, ValueEnum};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Clone, Copy, ValueEnum)]
pub enum Format {
    Text,
    Json,
    Csv,
}

#[derive(Args, Default)]
pub struct Views {
    /// Show observable CLI errors and explicit raw fallbacks.
    #[arg(short = 'F', long)]
    failures: bool,
    /// Archive statistics and start fresh; recovery originals remain available.
    #[arg(long, conflicts_with = "project")]
    reset: bool,
    #[arg(long, requires = "reset")]
    yes: bool,
    /// Correlate recoveries to recorded output transformations.
    #[arg(long)]
    recalls: bool,
    #[arg(short = 'g', long)]
    graph: bool,
    #[arg(short = 'd', long)]
    daily: bool,
    #[arg(short = 'w', long)]
    weekly: bool,
    #[arg(short = 'm', long)]
    monthly: bool,
    #[arg(short = 'a', long)]
    all: bool,
    #[arg(short = 'f', long, value_enum)]
    format: Option<Format>,
    /// Illustrative quota estimate, never provider billing or an actual quota.
    #[arg(short = 'q', long)]
    quota: bool,
    #[arg(short = 't', long, default_value = "20x", value_parser = ["pro", "5x", "20x"])]
    tier: String,
}

pub fn periods(content: &str, project: Option<&str>, kind: &str) -> Vec<Value> {
    let mut groups = BTreeMap::<String, Vec<String>>::new();
    for line in content.lines() {
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if project.is_some_and(|p| row["cwd"].as_str() != Some(p)) {
            continue;
        }
        let Some(date) = row["timestamp_unix"]
            .as_i64()
            .and_then(|t| DateTime::<Utc>::from_timestamp(t, 0))
        else {
            continue;
        };
        let key = match kind {
            "weekly" => (date - Duration::days(date.weekday().num_days_from_monday() as i64))
                .format("%Y-%m-%d")
                .to_string(),
            "monthly" => date.format("%Y-%m").to_string(),
            _ => date.format("%Y-%m-%d").to_string(),
        };
        groups.entry(key).or_default().push(line.to_owned());
    }
    groups
        .into_iter()
        .map(|(period, lines)| {
            let mut summary = crate::token_metrics::summarize_history(&lines.join("\n"));
            summary["period"] = json!(period);
            summary
        })
        .collect()
}

impl Views {
    pub fn reset_if_requested(&self) -> Result<bool> {
        if !self.reset {
            return Ok(false);
        }
        println!(
            "{}",
            crate::history_admin::reset(&crate::default_state_dir()?, self.yes)?
        );
        Ok(true)
    }

    pub fn enrich(&self, report: &mut Value, project: bool) -> Result<()> {
        if self.failures {
            let cwd = std::env::current_dir()?.to_string_lossy().into_owned();
            report["failures"] = crate::failure_log::report(project.then_some(cwd.as_str()))?;
        }
        if self.recalls {
            let dir = crate::default_state_dir()?;
            let history = crate::history_admin::read(&dir, "exec-history.jsonl")?;
            let history = if project {
                let cwd = std::env::current_dir()?.to_string_lossy().into_owned();
                history
                    .lines()
                    .filter(|line| {
                        serde_json::from_str::<Value>(line).is_ok_and(|r| r["cwd"] == cwd)
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                history
            };
            report["recalls"] = crate::history_admin::recalls(
                &history,
                &crate::history_admin::read(&dir, "retrieval-feedback.jsonl")?,
            );
        }

        if !(self.daily || self.weekly || self.monthly || self.all || self.graph || self.quota) {
            return Ok(());
        }
        let path = crate::default_state_dir()?.join("exec-history.jsonl");
        let content = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e.into()),
        };
        let cwd = std::env::current_dir()?.to_string_lossy().into_owned();
        for (kind, enabled) in [
            ("daily", self.daily || self.all || self.graph),
            ("weekly", self.weekly || self.all),
            ("monthly", self.monthly || self.all || self.quota),
        ] {
            if enabled {
                report[kind] = json!(periods(&content, project.then_some(cwd.as_str()), kind));
            }
        }
        report["calendar_timezone"] = json!("UTC; weeks start Monday");
        if self.quota {
            let month = DateTime::<Utc>::from_timestamp(crate::unix_timestamp() as i64, 0)
                .unwrap()
                .format("%Y-%m")
                .to_string();
            let saved = report["monthly"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["period"] == month)
                .and_then(|r| r["tokens_saved"].as_i64())
                .unwrap_or(0);
            let quota = 6_000_000_i64
                * match self.tier.as_str() {
                    "5x" => 5,
                    "20x" => 20,
                    _ => 1,
                };
            report["quota"] = json!({"tier":self.tier,"period":month,"estimated_monthly_tokens":quota,"tokens_saved":saved,"percent": saved as f64 * 100.0 / quota as f64,"method":"illustrative 6M tokens/month times tier; not an actual provider quota or billing measurement"});
        }
        if self.graph {
            let rows = report["daily"].as_array().unwrap();
            let max = rows
                .iter()
                .filter_map(|r| r["tokens_saved"].as_i64())
                .map(i64::unsigned_abs)
                .max()
                .unwrap_or(1)
                .max(1);
            report["graph"] = json!(rows
                .iter()
                .map(|r| {
                    let n = r["tokens_saved"].as_i64().unwrap_or(0);
                    format!(
                        "{} {:>12} {}{}",
                        r["period"].as_str().unwrap(),
                        n,
                        if n < 0 { "-" } else { "" },
                        "#".repeat((n.unsigned_abs() as f64 / max as f64 * 40.0).round() as usize)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"));
        }
        Ok(())
    }

    pub fn render(&self, report: &Value) -> Result<bool> {
        match self.format {
            Some(Format::Json) => println!("{}", serde_json::to_string_pretty(report)?),
            Some(Format::Csv) => print!("{}", csv(report)),
            Some(Format::Text) => {
                print!("{}", crate::format_stats_markdown(report));
                for kind in ["daily", "weekly", "monthly", "history"] {
                    if let Some(rows) = report[kind].as_array() {
                        println!("\n{kind}:");
                        for row in rows {
                            println!("{}", row);
                        }
                    }
                }
                if let Some(graph) = report["graph"].as_str() {
                    println!("\n{graph}");
                }
                if let Some(failures) = report.get("failures") {
                    println!("\nFailures: {failures}");
                }
                if let Some(recalls) = report.get("recalls") {
                    println!("\nRecalls: {recalls}");
                }
                if let Some(quota) = report.get("quota") {
                    println!("\nQuota estimate: {quota}");
                }
            }
            None => return Ok(false),
        }
        Ok(true)
    }
}

fn csv(report: &Value) -> String {
    fn cell(v: &Value) -> String {
        let s = v.as_str().map(str::to_owned).unwrap_or_else(|| {
            if v.is_null() {
                String::new()
            } else {
                v.to_string()
            }
        });
        format!("\"{}\"", s.replace('"', "\"\""))
    }
    let columns = [
        "period",
        "command",
        "commands",
        "original_tokens",
        "compressed_tokens",
        "tokens_saved",
        "measured_commands",
        "unmeasured_commands",
        "estimated_tokens_saved",
        "exit_code",
        "duration_ms",
    ];
    let mut out = format!("view,{}\r\n", columns.join(","));
    for kind in ["exec_history", "daily", "weekly", "monthly", "history"] {
        let rows = match report.get(kind) {
            Some(Value::Array(rows)) => rows.clone(),
            Some(row) => vec![row.clone()],
            None => continue,
        };
        for row in rows {
            out.push_str(&format!(
                "{kind},{}\r\n",
                columns
                    .iter()
                    .map(|key| cell(&row[*key]))
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calendar_boundaries_legacy_and_negative_counts() {
        let row = |date: &str, after| {
            json!({"timestamp_unix":DateTime::parse_from_rfc3339(date).unwrap().timestamp(),"cwd":"/project","original_tokens":10,"compressed_tokens":after,"tokenizer":crate::token_metrics::TOKENIZER,"token_count_method":"exact"}).to_string()
        };
        let content = [
            row("2024-02-29T23:59:59Z", 20),
            row("2024-03-01T00:00:00Z", 3),
            "{\"timestamp_unix\":1709251200,\"bytes_saved\":40}".into(),
            "invalid".into(),
        ]
        .join("\n");
        let daily = periods(&content, Some("/project"), "daily");
        assert_eq!(daily.len(), 2);
        assert_eq!(daily[0]["tokens_saved"], -10);
        assert_eq!(periods(&content, None, "weekly")[0]["period"], "2024-02-26");
        assert_eq!(periods(&content, None, "weekly")[0]["tokens_saved"], -3);
        assert_eq!(
            periods(&content, None, "monthly")[1]["unmeasured_commands"],
            1
        );
        assert!(periods(&content, Some("/other"), "daily").is_empty());
    }
    #[test]
    fn csv_quotes_commands_and_preserves_negative_savings() {
        let s = csv(&json!({"history":[{"command":"printf \"x,y\"\nnext","tokens_saved":-7}]}));
        assert!(s.contains("\"printf \"\"x,y\"\"\nnext\""));
        assert!(s.contains("\"-7\""));
    }
}
