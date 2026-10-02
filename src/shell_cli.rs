//! Explicit shell escape hatch, without compression or usage tracking.
use anyhow::{Context, Result};
use clap::Args;
use std::process::{Command, Stdio};

#[derive(Args)]
pub struct Options {
    #[arg(short = 'c', long, conflicts_with = "args")]
    command: Option<String>,
    #[arg(
        required_unless_present = "command",
        trailing_var_arg = true,
        allow_hyphen_values = true
    )]
    args: Vec<String>,
}
pub fn run(opts: Options) -> Result<i32> {
    let text = opts.command.unwrap_or_else(|| {
        if opts.args.len() == 1 {
            opts.args[0].clone()
        } else {
            crate::shell_join(&opts.args)
        }
    });
    let mut command = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/D", "/S", "/C"]);
        c
    } else {
        let mut c = Command::new("sh");
        c.arg("-c");
        c
    };
    let status = command
        .arg(text)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("execute shell command")?;
    Ok(crate::child_exit_code(status))
}
