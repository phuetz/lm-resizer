//! Release checks and explicit installation through bundled checksum installers.
use anyhow::{bail, Context, Result};
use clap::Args;
use serde_json::json;
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

#[derive(Args)]
pub struct Options {
    /// Exact version; omitted to query the latest GitHub release.
    #[arg(long)]
    version: Option<String>,
    /// Actually install; otherwise only report the planned operation.
    #[arg(long, conflicts_with = "dry_run")]
    apply: bool,
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    install_dir: Option<PathBuf>,
    /// Alternate release asset location, useful for offline verified releases.
    #[arg(long, requires = "version")]
    release_base_url: Option<String>,
}
fn valid_version(version: &str) -> bool {
    !version.is_empty()
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".+-".contains(&b))
}
pub async fn run(opts: Options) -> Result<()> {
    let version = match opts.version {
        Some(v) => v,
        None => {
            let response = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .build()?
                .get("https://api.github.com/repos/phuetz/lm-resizer/releases/latest")
                .header("User-Agent", "lm-resizer-update")
                .send()
                .await?
                .error_for_status()?
                .json::<serde_json::Value>()
                .await?;
            response["tag_name"]
                .as_str()
                .context("release has no tag_name")?
                .trim_start_matches('v')
                .to_owned()
        }
    };
    if !valid_version(&version) {
        bail!("invalid release version");
    }
    let directory = opts
        .install_dir
        .or_else(|| std::env::var_os("LM_RESIZER_INSTALL_DIR").map(PathBuf::from))
        .unwrap_or(
            crate::user_home_dir()
                .context("cannot determine installation directory")?
                .join(".local/bin"),
        );
    let base = opts.release_base_url.unwrap_or_else(|| {
        format!("https://github.com/phuetz/lm-resizer/releases/download/v{version}")
    });
    println!(
        "{}",
        json!({"current_version":env!("CARGO_PKG_VERSION"),"requested_version":version,"install_dir":directory,"release_base_url":base,"apply":opts.apply,"checksum":"SHA-256; archive filename and binary version verified by bundled installer"})
    );
    if !opts.apply {
        return Ok(());
    }
    let mut command = if cfg!(windows) {
        let mut c = Command::new("powershell");
        c.args(["-NoProfile", "-NonInteractive", "-Command", "-"]);
        c
    } else {
        let mut c = Command::new("sh");
        c.arg("-s");
        c
    };
    let script = if cfg!(windows) {
        include_str!("../install.ps1")
    } else {
        include_str!("../install.sh")
    };
    let mut child = command
        .env("LM_RESIZER_VERSION", &version)
        .env("LM_RESIZER_INSTALL_DIR", directory)
        .env("LM_RESIZER_RELEASE_BASE_URL", base)
        .env("LM_RESIZER_SKIP_PATH_UPDATE", "1")
        .stdin(Stdio::piped())
        .spawn()
        .context("start bundled installer")?;
    child
        .stdin
        .take()
        .context("installer stdin")?
        .write_all(script.as_bytes())?;
    let status = child.wait()?;
    if !status.success() {
        bail!("installer failed with exit {}; existing binary may be unchanged; inspect installer diagnostics",crate::child_exit_code(status));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn versions_cannot_inject_shell_or_url_paths() {
        for v in ["0.2.4", "1.0.0-rc.1"] {
            assert!(valid_version(v));
        }
        for v in ["", "../file", "$(id)", "1;echo bad", "v/1"] {
            assert!(!valid_version(v));
        }
    }
}
