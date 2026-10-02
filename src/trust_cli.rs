//! Administrative aliases over the existing trust registry; no filter changes.
use anyhow::Result;
use clap::Args;
use std::{
    io::{IsTerminal, Write},
    path::PathBuf,
};
#[derive(Args)]
pub struct Options {
    #[arg(long)]
    list: bool,
    #[arg(short, long)]
    yes: bool,
    #[arg(long, default_value = ".lm-resizer/filters.toml")]
    path: PathBuf,
}
pub fn run(opts: Options) -> Result<()> {
    if opts.list {
        println!(
            "{}",
            serde_json::to_string_pretty(&crate::load_trusted_filter_records()?)?
        );
        return Ok(());
    }
    if !opts.yes {
        if !std::io::stdin().is_terminal() {
            anyhow::bail!("trust requires --yes without an interactive terminal; inspect with audit-filters first");
        }
        eprint!("Trust {} after verification? [y/N] ", opts.path.display());
        std::io::stderr().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !matches!(input.trim(), "y" | "Y" | "yes") {
            anyhow::bail!("trust cancelled");
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&crate::trust_filter_file(&opts.path)?)?
    );
    Ok(())
}

/// Trust requires successful fixtures for every declared filter. Diagnostics
/// include absent coverage and duplicate definitions; neither is a clean audit.
pub fn require_verified(report: &crate::VerifyFiltersReport) -> Result<()> {
    if report.failed > 0 || report.tests == 0 || !report.diagnostics.is_empty() {
        anyhow::bail!("filter trust requires passing fixtures for every filter and no diagnostics: {} failed; {}", report.failed, report.diagnostics.join("; "));
    }
    Ok(())
}
