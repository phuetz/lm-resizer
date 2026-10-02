//! Reversible agent integration plans. Paths follow RTK v0.50.0's documented
//! host conventions; implementation is independent, with no copied RTK code.
use anyhow::{bail, Context, Result};
use clap::Args;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const START: &str = "\n<!-- lm-resizer integration -->\n";
const END: &str = "\n<!-- /lm-resizer integration -->\n";
const GUIDANCE: &str = "Use `lm-resizer exec --raw-on-failure -- <command>` for verbose command output. Recover complete output with `lm-resizer tee read <reference>` or `lm-resizer retrieve <key>`. Inspect savings with `lm-resizer gain --history`. Preserve the command's exit status and never hide failures. Use `lm-resizer rewrite-shell '<command>'` to preview a rewrite without execution.";

#[derive(Args)]
pub struct Options {
    #[arg(long, default_value = "claude", value_parser = ["claude", "codex", "gemini", "cursor", "trae", "copilot", "droid", "vibe", "opencode", "pi", "omp", "hermes", "windsurf", "cline", "roo", "kilocode", "antigravity", "kimi"])]
    pub agent: String,
    #[arg(short, long)]
    pub global: bool,
    #[arg(long)]
    pub project_dir: Option<PathBuf>,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub show: bool,
    #[arg(long)]
    pub uninstall: bool,
    #[arg(long, conflicts_with = "no_patch")]
    pub auto_patch: bool,
    #[arg(long)]
    pub no_patch: bool,
    #[arg(long)]
    pub json: bool,
}

pub struct Edit {
    pub path: PathBuf,
    pub before: String,
    pub after: String,
}

fn read(path: &Path) -> Result<String> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e).with_context(|| format!("read {}", path.display())),
    }
}

fn replace_block(text: &str, uninstall: bool) -> Result<String> {
    let block = format!("{START}{GUIDANCE}{END}");
    if let Some(start) = text.find(START) {
        let end = text[start..]
            .find(END)
            .context("unterminated lm-resizer integration block; refusing to modify")?
            + start
            + END.len();
        // A locally edited managed block belongs to the user until resolved.
        if text[start..end] != block {
            bail!("modified lm-resizer integration block; refusing to overwrite or remove");
        }
        Ok(format!(
            "{}{}{}",
            &text[..start],
            if uninstall { "" } else { &block },
            &text[end..]
        ))
    } else if uninstall {
        Ok(text.into())
    } else {
        Ok(format!("{text}{block}"))
    }
}

pub fn run(opts: Options) -> Result<()> {
    let project = opts.project_dir.clone().unwrap_or(std::env::current_dir()?);
    let home = crate::user_home_dir().context("cannot determine home directory")?;
    let edits = plan(&opts, &project, &home)?;
    let preview = opts.dry_run || opts.show || opts.no_patch;
    let report: Vec<Value> = edits
        .iter()
        .map(
            |edit| json!({"path":edit.path,"changed":edit.before!=edit.after,"content":edit.after}),
        )
        .collect();
    if !preview {
        apply(&edits)?;
    }
    if opts.json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &json!({"preview":preview,"agent":opts.agent,"files":report})
            )?
        );
    } else {
        for edit in edits {
            println!(
                "{} {}",
                if preview {
                    "Preview"
                } else if edit.before == edit.after {
                    "Unchanged"
                } else {
                    "Updated"
                },
                edit.path.display()
            );
            if preview {
                println!("{}", edit.after);
            }
        }
    }
    Ok(())
}

pub fn plan(opts: &Options, project: &Path, _home: &Path) -> Result<Vec<Edit>> {
    let relative = match opts.agent.as_str() {
        "windsurf" => ".windsurfrules",
        "cline" => ".clinerules",
        "roo" => ".roo/rules/lm-resizer.md",
        "kilocode" => ".kilocode/rules/lm-resizer.md",
        "antigravity" => ".agents/rules/lm-resizer.md",
        "kimi" => "AGENTS.md",
        other => bail!("native integration for {other} is not available in this build"),
    };
    if opts.global {
        bail!("{} uses project-scoped rules; omit --global", opts.agent);
    }
    let mut path = project.join(relative);
    if opts.agent == "cline" && path.is_dir() {
        path = path.join("lm-resizer.md");
    }
    let before = read(&path)?;
    let after = replace_block(&before, opts.uninstall)?;
    Ok(vec![Edit {
        path,
        before,
        after,
    }])
}

pub fn apply(edits: &[Edit]) -> Result<()> {
    // Validate the entire plan before the first mutation.
    for edit in edits {
        if read(&edit.path)? != edit.before {
            bail!(
                "configuration changed since planning: {}",
                edit.path.display()
            );
        }
        if std::fs::symlink_metadata(&edit.path).is_ok_and(|m| m.file_type().is_symlink()) {
            bail!("refusing to replace symlink {}", edit.path.display());
        }
    }
    for edit in edits {
        if edit.before == edit.after {
            continue;
        }
        if let Some(parent) = edit.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Do not delete the file: an empty pre-existing file is still user-owned.
        std::fs::write(&edit.path, &edit.after)
            .with_context(|| format!("write {}", edit.path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rules_preserve_user_bytes_idempotently_and_refuse_edited_blocks() {
        for user in ["", "personal rules", "rules\r\n", "other\n\n"] {
            let installed = replace_block(user, false).unwrap();
            assert_eq!(replace_block(&installed, false).unwrap(), installed);
            assert_eq!(replace_block(&installed, true).unwrap(), user);
            assert!(replace_block(&installed.replace("verbose", "changed"), true).is_err());
        }
        assert!(replace_block(START, false).is_err());
    }
}
