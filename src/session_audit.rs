//! Read-only session auditing with tool-call correlation, scoped by project/time.
use anyhow::Result;
use clap::Args;
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Args, Default)]
pub struct Options {
    /// Project path substring; defaults to the current project.
    #[arg(short, long, conflicts_with = "all")]
    pub project: Option<String>,
    #[arg(short, long)]
    pub all: bool,
    /// Session lookback in days (default: 30; 0: all dates).
    #[arg(short, long)]
    pub since: Option<u64>,
    #[arg(short, long, default_value_t = 15)]
    pub limit: usize,
    #[arg(short, long, value_parser = ["text", "json"])]
    pub format: Option<String>,
}
impl Options {
    pub fn active(&self) -> bool {
        self.project.is_some()
            || self.all
            || self.since.is_some()
            || self.format.is_some()
            || self.limit != 15
    }
}

#[derive(Clone)]
pub struct Call {
    pub command: String,
    pub output: Option<String>,
    pub failed: Option<bool>,
}

/// Both Claude tool_use/tool_result and Codex function_call/function_call_output.
/// Results are joined by ID rather than attached to the most recent command.
pub fn calls(content: &str) -> Vec<Call> {
    fn visit(
        v: &Value,
        pending: &mut BTreeMap<String, Call>,
        order: &mut Vec<String>,
        results: &mut BTreeMap<String, (String, Option<bool>)>,
    ) {
        if let Some(obj) = v.as_object() {
            let kind = v["type"].as_str().unwrap_or("");
            if matches!(kind, "tool_use" | "function_call") {
                let name = v["name"].as_str().unwrap_or("");
                if matches!(
                    name,
                    "Bash"
                        | "bash"
                        | "shell"
                        | "shell_command"
                        | "exec_command"
                        | "run_shell_command"
                ) {
                    let args = if let Some(s) = v["arguments"].as_str() {
                        serde_json::from_str::<Value>(s).unwrap_or_default()
                    } else {
                        v["input"].clone()
                    };
                    if let Some(cmd) = args["command"].as_str().or_else(|| args["cmd"].as_str()) {
                        let id = v["call_id"]
                            .as_str()
                            .or_else(|| v["id"].as_str())
                            .map(str::to_owned)
                            .unwrap_or_else(|| format!("anonymous-{}", order.len()));
                        if !pending.contains_key(&id) {
                            order.push(id.clone());
                        }
                        pending.insert(
                            id,
                            Call {
                                command: cmd.into(),
                                output: None,
                                failed: None,
                            },
                        );
                    }
                }
            } else if matches!(kind, "tool_result" | "function_call_output") {
                if let Some(id) = v["tool_use_id"].as_str().or_else(|| v["call_id"].as_str()) {
                    let output = if kind == "tool_result" {
                        &v["content"]
                    } else {
                        &v["output"]
                    };
                    let text = match output {
                        Value::String(s) => s.clone(),
                        Value::Array(items) => items
                            .iter()
                            .filter_map(|i| i["text"].as_str())
                            .collect::<Vec<_>>()
                            .join("\n"),
                        _ => String::new(),
                    };
                    let failed = v["is_error"]
                        .as_bool()
                        .or_else(|| v["exit_code"].as_i64().map(|code| code != 0))
                        .or_else(|| {
                            text.lines().find_map(|line| {
                                line.strip_prefix("Process exited with code ")
                                    .or_else(|| line.strip_prefix("Exit code: "))
                                    .and_then(|code| code.trim().parse::<i64>().ok())
                                    .map(|code| code != 0)
                            })
                        });
                    results.insert(id.into(), (text, failed));
                }
            }
            for child in obj.values().filter(|v| v.is_object() || v.is_array()) {
                visit(child, pending, order, results);
            }
        } else if let Some(items) = v.as_array() {
            for child in items {
                visit(child, pending, order, results);
            }
        }
    }
    let mut pending = BTreeMap::new();
    let mut order = Vec::new();
    let mut results = BTreeMap::new();
    for line in content.lines() {
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            visit(&v, &mut pending, &mut order, &mut results);
        }
    }
    order
        .into_iter()
        .filter_map(|id| {
            pending.remove(&id).map(|mut c| {
                if let Some((text, failed)) = results.remove(&id) {
                    c.output = Some(text);
                    c.failed = failed;
                }
                c
            })
        })
        .collect()
}

pub fn metadata(content: &str) -> (Option<String>, Option<u64>) {
    let mut project = None;
    let mut latest = None;
    for line in content.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if project.is_none() {
            project = v["cwd"]
                .as_str()
                .or_else(|| v.pointer("/payload/cwd").and_then(Value::as_str))
                .map(str::to_owned);
        }
        let stamp = v["timestamp"]
            .as_str()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .and_then(|d| u64::try_from(d.timestamp()).ok())
            .or_else(|| v["timestamp_unix"].as_u64());
        latest = latest.max(stamp);
    }
    (project, latest)
}

pub fn scan(paths: &[PathBuf], opts: &Options) -> Result<Value> {
    let paths = if paths.is_empty() {
        crate::agent_session_candidates(crate::AgentSessionKind::All)
            .into_iter()
            .filter(|p| p.exists())
            .collect::<Vec<_>>()
    } else {
        paths.to_vec()
    };
    let files = crate::collect_discover_files(&paths, true)?;
    let cwd = std::env::current_dir()?.to_string_lossy().into_owned();
    let project = if opts.all {
        None
    } else {
        Some(opts.project.as_deref().unwrap_or(&cwd))
    };
    let cutoff =
        crate::unix_timestamp().saturating_sub(opts.since.unwrap_or(30).saturating_mul(86400));
    let mut sessions = Vec::new();
    let mut opportunities = Vec::new();
    let mut unsupported = BTreeMap::<String, usize>::new();
    let mut unreadable = 0;
    let mut unscoped = 0;
    for file in files {
        let content = match std::fs::read_to_string(&file) {
            Ok(s) => s,
            Err(_) => {
                unreadable += 1;
                continue;
            }
        };
        let (scope, date) = metadata(&content);
        if scope.is_none() {
            unscoped += 1;
        }
        if project.is_some_and(|p| !scope.as_deref().is_some_and(|s| s.contains(p))) {
            continue;
        }
        let stamp = date
            .or_else(|| {
                std::fs::metadata(&file)
                    .ok()?
                    .modified()
                    .ok()?
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_secs())
            })
            .unwrap_or(0);
        if opts.since != Some(0) && stamp < cutoff {
            continue;
        }
        let calls = calls(&content);
        if calls.is_empty() {
            continue;
        }
        let mut wrapped = 0;
        let mut supported = 0;
        for call in &calls {
            let words = crate::split_command_for_filter(&call.command);
            if words
                .first()
                .is_some_and(|w| crate::command_basename(w) == "lm-resizer")
            {
                wrapped += 1;
                continue;
            }
            if crate::command_has_specific_filter(&call.command) {
                supported += 1;
                let mut candidate = json!({"command":call.command,"source":file,"project":scope,"output_available":call.output.is_some()});
                if let Some(raw) = &call.output {
                    let (_, filtered) = crate::filter_command_output(&words, raw);
                    candidate["prospective_tokens"] =
                        serde_json::to_value(crate::TokenCounts::measure(raw, &filtered))?;
                }
                opportunities.push(candidate);
            } else {
                *unsupported
                    .entry(words.first().cloned().unwrap_or_default())
                    .or_default() += 1;
            }
        }
        sessions.push(json!({"source":file,"project":scope,"commands":calls.len(),"wrapped":wrapped,"missed_supported":supported,"adoption_percent":100.0 * wrapped as f64 / calls.len() as f64}));
    }
    opportunities.sort_by_key(|r| {
        std::cmp::Reverse(
            r.pointer("/prospective_tokens/tokens_saved")
                .and_then(Value::as_i64)
                .unwrap_or(0),
        )
    });
    let total = opportunities.len();
    opportunities.truncate(opts.limit);
    Ok(
        json!({"sessions":sessions,"opportunities":opportunities,"opportunities_total":total,"unsupported":unsupported,"unreadable_files":unreadable,"unscoped_files":unscoped,"method":"prospective exact text savings; tool results correlated by call ID; UTC; timestamp fallback to file modification time"}),
    )
}

pub fn print_report(report: &Value, json_output: bool) -> Result<()> {
    if json_output {
        println!("{}", serde_json::to_string_pretty(report)?);
    } else {
        println!(
            "Sessions: {} | Missed supported commands: {}",
            report["sessions"].as_array().map_or(0, Vec::len),
            report["opportunities_total"]
        );
        for key in ["sessions", "opportunities"] {
            if let Some(rows) = report[key].as_array() {
                for row in rows {
                    println!("{row}");
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interleaved_results_are_correlated_by_id() {
        let content = [json!({"message":{"content":[{"type":"tool_use","id":"a","name":"Bash","input":{"command":"git status"}},{"type":"tool_use","id":"b","name":"Bash","input":{"command":"cargo test"}}]}}),json!({"message":{"content":[{"type":"tool_result","tool_use_id":"b","content":"test result"},{"type":"tool_result","tool_use_id":"a","content":"git result"}]}})].iter().map(Value::to_string).collect::<Vec<_>>().join("\n");
        let c = calls(&content);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].output.as_deref(), Some("git result"));
        assert_eq!(c[1].output.as_deref(), Some("test result"));
    }
    #[test]
    fn codex_calls_and_unrelated_tools() {
        let content = "{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"exec_command\",\"call_id\":\"1\",\"arguments\":\"{\\\"cmd\\\":\\\"git status\\\"}\"}}\n{\"type\":\"function_call_output\",\"call_id\":\"1\",\"output\":\"ok\"}";
        assert_eq!(calls(content)[0].output.as_deref(), Some("ok"));
        assert!(calls(
            "{\"type\":\"tool_use\",\"name\":\"Read\",\"input\":{\"command\":\"git status\"}}"
        )
        .is_empty());
    }
}
