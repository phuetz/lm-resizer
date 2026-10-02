//! Observational CLI corrections; only explicit failures followed by success.
use anyhow::{Context, Result};
use clap::Args;
use serde_json::json;
use std::{collections::BTreeMap, io::Write, path::PathBuf};

#[derive(Args, Default)]
#[group(id = "learning_options")]
pub struct Options {
    #[command(flatten)]
    pub selection: crate::session_audit::Options,
    #[arg(short = 'w', long)]
    pub write_rules: bool,
    #[arg(long,default_value_t=0.6,value_parser=confidence)]
    min_confidence: f64,
    #[arg(long, default_value_t = 1)]
    min_occurrences: usize,
}
fn confidence(text: &str) -> std::result::Result<f64, String> {
    let n = text.parse::<f64>().map_err(|e| e.to_string())?;
    if n.is_finite() && (0.0..=1.0).contains(&n) {
        Ok(n)
    } else {
        Err("confidence must be between 0 and 1".into())
    }
}
impl Options {
    pub fn active(&self) -> bool {
        self.selection.active()
            || self.write_rules
            || self.min_confidence != 0.6
            || self.min_occurrences != 1
    }
}

pub fn corrections(calls: &[crate::session_audit::Call]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for pair in calls.windows(2) {
        let (failed, next) = (&pair[0], &pair[1]);
        if failed.failed != Some(true)
            || next.failed != Some(false)
            || failed.command == next.command
        {
            continue;
        }
        let error = failed.output.as_deref().unwrap_or("").to_lowercase();
        if ![
            "unknown option",
            "unrecognized argument",
            "unrecognized option",
            "unexpected argument",
            "invalid option",
            "unknown flag",
            "unknown subcommand",
        ]
        .iter()
        .any(|p| error.contains(p))
        {
            continue;
        }
        let a = crate::split_command_for_filter(&failed.command);
        let b = crate::split_command_for_filter(&next.command);
        if !a.is_empty() && a.first() == b.first() {
            out.push((failed.command.clone(), next.command.clone()));
        }
    }
    out
}

pub fn run(
    paths: &[PathBuf],
    opts: &Options,
    project_dir: Option<PathBuf>,
    json_output: bool,
) -> Result<()> {
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
    let project = if opts.selection.all {
        None
    } else {
        Some(opts.selection.project.as_deref().unwrap_or(&cwd))
    };
    let cutoff = crate::unix_timestamp()
        .saturating_sub(opts.selection.since.unwrap_or(30).saturating_mul(86400));
    let mut counts = BTreeMap::<(String, String), usize>::new();
    let mut attempts = BTreeMap::<String, usize>::new();
    for file in files {
        let content = std::fs::read_to_string(&file)?;
        let (scope, date) = crate::session_audit::metadata(&content);
        if project.is_some_and(|p| !scope.as_deref().is_some_and(|s| s.contains(p))) {
            continue;
        }
        let timestamp = date
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
        if opts.selection.since != Some(0) && timestamp < cutoff {
            continue;
        }
        let calls = crate::session_audit::calls(&content);
        for call in &calls {
            if call.failed == Some(true) {
                *attempts.entry(call.command.clone()).or_default() += 1;
            }
        }
        for correction in corrections(&calls) {
            *counts.entry(correction).or_default() += 1;
        }
    }
    let rows:Vec<_>=counts.into_iter().filter_map(|((from,to),count)|{let confidence=count as f64/attempts[&from] as f64;if count<opts.min_occurrences||confidence<opts.min_confidence{None}else{Some(json!({"failed":from,"corrected":to,"occurrences":count,"confidence":confidence}))}}).collect();
    let report = json!({"corrections":rows,"method":"adjacent calls to the same program; explicit CLI syntax failure followed by explicit success; observations, not universal rules"});
    if opts.write_rules {
        let path = project_dir
            .unwrap_or(std::env::current_dir()?)
            .join(".claude/rules/cli-corrections.md");
        let mut text=String::from("# Observed CLI corrections\n\nThese are observations from sessions, not commands to execute automatically.\n\n");
        for row in &rows {
            text.push_str(&format!(
                "- Failed: {}\n  Successful retry: {}\n  Observations: {}; confidence: {}\n",
                row["failed"], row["corrected"], row["occurrences"], row["confidence"]
            ));
        }
        if path.exists() {
            if std::fs::read_to_string(&path)? != text {
                anyhow::bail!("{} already exists; refusing to overwrite", path.display());
            }
        } else {
            std::fs::create_dir_all(path.parent().unwrap())?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .context("create correction rules")?;
            file.write_all(text.as_bytes())?;
        }
    }
    if json_output || opts.selection.format.as_deref() == Some("json") {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{} observed CLI corrections", rows.len());
        for row in rows {
            println!("{row}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn learns_only_explicit_cli_failures_followed_by_success() {
        let call = |command: &str, failed, output: &str| crate::session_audit::Call {
            command: command.into(),
            failed,
            output: Some(output.into()),
        };
        let pair = [
            call("git status --bad", Some(true), "error: unknown option bad"),
            call("git status", Some(false), "clean"),
        ];
        assert_eq!(
            corrections(&pair),
            vec![("git status --bad".into(), "git status".into())]
        );
        assert!(corrections(&[pair[0].clone(), call("git status", None, "")]).is_empty());
        assert!(corrections(&[
            call("cargo test", Some(true), "test failed"),
            call("cargo test --skip important", Some(false), "ok")
        ])
        .is_empty());
        assert!(confidence("NaN").is_err());
        assert!(confidence("1.1").is_err());
    }
}
