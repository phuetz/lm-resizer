//! Persistent local CLI settings; explicit environment variables take precedence.
use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use std::{io::Write, path::PathBuf};

#[derive(Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub tracking: bool,
    pub tee: bool,
    pub store: Option<PathBuf>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            tracking: true,
            tee: true,
            store: None,
        }
    }
}

#[derive(Args)]
pub struct Options {
    #[arg(long)]
    create: bool,
    #[arg(long)]
    json: bool,
    #[command(subcommand)]
    action: Option<Action>,
}
#[derive(Subcommand)]
enum Action {
    /// Set a local setting: tracking, tee, or store.
    Set { key: String, value: String },
    /// Remove an override and return to its default.
    Unset { key: String },
    /// Print the configuration file path.
    Path,
}

pub fn path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("LM_RESIZER_CONFIG") {
        return Ok(path.into());
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| crate::user_home_dir().map(|h| h.join(".config")))
        .context("cannot determine config directory")?;
    Ok(base.join("lm-resizer/config.toml"))
}
pub fn load() -> Result<Settings> {
    match std::fs::read_to_string(path()?) {
        Ok(s) => toml::from_str(&s).context("invalid LM Resizer config"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(e) => Err(e.into()),
    }
}
pub fn apply() -> Result<()> {
    let settings = load()?;
    for (key, value) in [
        (
            "LM_RESIZER_TRACKING",
            Some(if settings.tracking { "1" } else { "0" }.to_owned()),
        ),
        (
            "LM_RESIZER_TEE",
            Some(if settings.tee { "1" } else { "0" }.to_owned()),
        ),
        (
            "LM_RESIZER_STORE",
            settings.store.map(|p| p.to_string_lossy().into_owned()),
        ),
    ] {
        if std::env::var_os(key).is_none() {
            if let Some(value) = value {
                std::env::set_var(key, value);
            }
        }
    }
    Ok(())
}
pub fn run(opts: Options) -> Result<()> {
    let path = path()?;
    if matches!(opts.action, Some(Action::Path)) {
        println!("{}", path.display());
        return Ok(());
    }
    if opts.create {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .context("config already exists or cannot be created; left untouched")?;
        file.write_all(toml::to_string_pretty(&Settings::default())?.as_bytes())?;
    }
    let mut settings = load()?;
    if let Some(action) = opts.action {
        let (key, value) = match action {
            Action::Set { key, value } => (key, Some(value)),
            Action::Unset { key } => (key, None),
            Action::Path => unreachable!(),
        };
        match key.as_str() {
            "tracking" => settings.tracking = boolean(value.as_deref(), true)?,
            "tee" => settings.tee = boolean(value.as_deref(), true)?,
            "store" => settings.store = value.filter(|s| !s.is_empty()).map(PathBuf::from),
            _ => bail!("unknown setting {key}; expected tracking, tee or store"),
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, toml::to_string_pretty(&settings)?)?;
    }
    if opts.json {
        println!("{}", serde_json::to_string_pretty(&settings)?);
    } else {
        println!(
            "# {}\n{}",
            path.display(),
            toml::to_string_pretty(&settings)?
        );
    }
    Ok(())
}
fn boolean(value: Option<&str>, default: bool) -> Result<bool> {
    match value {
        None => Ok(default),
        Some("true" | "1") => Ok(true),
        Some("false" | "0") => Ok(false),
        _ => bail!("expected true or false"),
    }
}
