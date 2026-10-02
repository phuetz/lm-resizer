//! Installation verification without executing agent binaries or writing configs.
use anyhow::{Context, Result};
use clap::Args;
use serde_json::{json, Value};
use std::path::PathBuf;

pub const CLIENTS: &[&str] = &[
    "claude",
    "codex",
    "gemini",
    "cursor",
    "trae",
    "copilot",
    "droid",
    "vibe",
    "opencode",
    "pi",
    "omp",
    "hermes",
    "windsurf",
    "cline",
    "roo",
    "kilocode",
    "antigravity",
    "kimi",
];
#[derive(Args)]
pub struct Options {
    #[arg(long, default_value = "claude")]
    agent: String,
    #[arg(short, long)]
    global: bool,
    #[arg(long)]
    project_dir: Option<PathBuf>,
    #[arg(long)]
    json: bool,
}

pub fn inspect(
    agent: &str,
    global: bool,
    project: &std::path::Path,
    home: &std::path::Path,
) -> Value {
    let options = crate::agent_init::Options {
        agent: agent.into(),
        global,
        ..Default::default()
    };
    match crate::agent_init::plan(&options, project, home) {
        Ok(edits) => {
            let valid = edits.iter().all(|e| e.before == e.after);
            json!({"agent":agent,"scope":if global{"global"}else{"project"},"valid":valid,"files":edits.iter().map(|e|json!({"path":e.path,"installed":!e.before.is_empty(),"matches_expected":e.before==e.after})).collect::<Vec<_>>()})
        }
        Err(e) => json!({"agent":agent,"valid":false,"error":e.to_string()}),
    }
}
pub fn run(opts: Options) -> Result<()> {
    let home = crate::user_home_dir().context("cannot determine home")?;
    let project = opts.project_dir.unwrap_or(std::env::current_dir()?);
    let clients = if opts.agent == "all" {
        CLIENTS.to_vec()
    } else {
        if !CLIENTS.contains(&opts.agent.as_str()) {
            anyhow::bail!("unknown agent {}", opts.agent);
        }
        vec![opts.agent.as_str()]
    };
    let reports: Vec<_> = clients
        .iter()
        .map(|agent| inspect(agent, opts.global, &project, &home))
        .collect();
    let valid = reports.iter().all(|r| r["valid"] == true);
    if opts.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({"valid":valid,"integrations":reports}))?
        );
    } else {
        for report in &reports {
            println!(
                "{}: {}",
                report["agent"].as_str().unwrap(),
                if report["valid"] == true {
                    "ok"
                } else {
                    "missing or modified"
                }
            );
            if let Some(e) = report["error"].as_str() {
                println!("  {e}");
            }
        }
    }
    if !valid {
        anyhow::bail!("integration verification failed; inspect with init --show or preview repair with init --dry-run");
    }
    Ok(())
}
