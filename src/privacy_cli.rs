//! Local usage controls. LM Resizer has no remote telemetry transport.
use anyhow::Result;
use clap::{Args, Subcommand};
use serde_json::json;
#[derive(Args)]
pub struct Options {
    #[command(subcommand)]
    action: Option<Action>,
}
#[derive(Subcommand)]
enum Action {
    Status,
    /// Enable local execution counters only; no data is sent remotely.
    Enable,
    /// Disable local execution counters.
    Disable,
    /// Erase local usage counters and disable tracking; preserve recovery data.
    Forget {
        #[arg(long)]
        yes: bool,
    },
}
fn tracking(enabled: bool) -> Result<()> {
    let path = crate::settings_cli::path()?;
    let mut config = crate::settings_cli::load()?;
    config.tracking = enabled;
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(path, toml::to_string_pretty(&config)?)?;
    Ok(())
}
pub fn run(opts: Options) -> Result<()> {
    match opts.action.unwrap_or(Action::Status) {
        Action::Status => println!(
            "{}",
            json!({"remote_telemetry":false,"local_tracking_configured":crate::settings_cli::load()?.tracking,"local_tracking_effective":std::env::var("LM_RESIZER_TRACKING").as_deref()!=Ok("0"),"hook_audit_opt_in":std::env::var("LM_RESIZER_HOOK_AUDIT").as_deref()==Ok("1"),"network_collection":"none"})
        ),
        action @ (Action::Enable | Action::Disable) => {
            let enabled = matches!(action, Action::Enable);
            tracking(enabled)?;
            println!(
                "{}",
                json!({"local_tracking":enabled,"remote_telemetry":false,"note":"local counters only; explicit environment variables take precedence"})
            );
        }
        Action::Forget { yes } => {
            if !yes {
                anyhow::bail!("telemetry forget requires --yes; local metrics will be erased, recovery originals preserved");
            }
            tracking(false)?;
            let dir = crate::default_state_dir()?;
            let mut locations = vec![dir.clone()];
            if dir.exists() {
                for entry in std::fs::read_dir(&dir)? {
                    let entry = entry?;
                    if entry.file_type()?.is_dir()
                        && entry
                            .file_name()
                            .to_string_lossy()
                            .starts_with("statistics-backup-")
                    {
                        locations.push(entry.path());
                    }
                }
            }
            let mut removed = 0;
            for directory in locations {
                for name in [
                    "exec-history.jsonl",
                    "retrieval-feedback.jsonl",
                    "proxy-history.jsonl",
                    "command-errors.jsonl",
                    "hook-audit.jsonl",
                ] {
                    match std::fs::remove_file(directory.join(name)) {
                        Ok(()) => removed += 1,
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => return Err(e.into()),
                    }
                }
            }
            println!(
                "{}",
                json!({"remote_telemetry":false,"local_tracking":false,"removed_metric_files":removed,"recovery_data_preserved":true,"note":"environment opt-ins can re-enable tracking or hook audit"})
            );
        }
    }
    Ok(())
}
