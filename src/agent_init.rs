//! Protocol reference: rtk-ai/rtk v0.50.0, commit
//! 1d87b8e719ce0a50c223cd93ca64dd16921f9aec, src/hooks/{init,hook_cmd}.rs.
//! Independent implementation; see THIRD-PARTY-NOTICES.
//! Reversible agent integration plans. Paths follow RTK v0.50.0's documented
//! host conventions; implementation is independent, with no copied RTK code.
use anyhow::{bail, Context, Result};
use clap::Args;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const START: &str = "\n<!-- lm-resizer integration -->\n";
const END: &str = "\n<!-- /lm-resizer integration -->\n";
const GUIDANCE: &str = "Use `lm-resizer exec --raw-on-failure -- <command>` for verbose command output. Recover complete output with `lm-resizer tee read <reference>` or `lm-resizer retrieve <key>`. Inspect savings with `lm-resizer gain --history`. Preserve the command's exit status and never hide failures. Use `lm-resizer rewrite-shell '<command>'` to preview a rewrite without execution.";

#[derive(Args, Default, Clone)]
pub struct Options {
    #[arg(long, default_value = "claude", value_parser = ["claude", "codex", "gemini", "cursor", "trae", "copilot", "droid", "vibe", "opencode", "pi", "omp", "hermes", "windsurf", "cline", "roo", "kilocode", "antigravity", "kimi"])]
    pub agent: String,
    #[arg(short, long)]
    pub global: bool,
    #[arg(long, conflicts_with_all = ["gemini", "copilot", "opencode"])]
    pub codex: bool,
    #[arg(long, conflicts_with_all = ["codex", "copilot", "opencode"])]
    pub gemini: bool,
    #[arg(long, conflicts_with_all = ["codex", "gemini", "opencode"])]
    pub copilot: bool,
    #[arg(long, conflicts_with_all = ["codex", "gemini", "copilot"])]
    pub opencode: bool,
    #[arg(long, conflicts_with = "claude_md")]
    pub hook_only: bool,
    #[arg(long, conflicts_with_all = ["no_trust_filters", "uninstall"])]
    pub trust_filters: bool,
    #[arg(long)]
    pub no_trust_filters: bool,
    #[arg(long)]
    pub claude_md: bool,
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
    pub delete: bool,
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

pub fn run(mut opts: Options) -> Result<()> {
    for (selected, agent) in [
        (opts.codex, "codex"),
        (opts.gemini, "gemini"),
        (opts.copilot, "copilot"),
    ] {
        if selected {
            opts.agent = agent.into();
        }
    }
    let project = opts.project_dir.clone().unwrap_or(std::env::current_dir()?);
    let home = crate::user_home_dir().context("cannot determine home directory")?;
    let mut edits = plan(&opts, &project, &home)?;
    if opts.opencode && opts.agent != "opencode" {
        let plugin_opts = Options {
            agent: "opencode".into(),
            ..opts.clone()
        };
        edits.extend(plan(&plugin_opts, &project, &home)?);
    }
    let preview = opts.dry_run || opts.show || opts.no_patch;
    let report: Vec<Value> = edits
        .iter()
        .map(
            |edit| json!({"path":edit.path,"changed":edit.before!=edit.after,"content":if opts.show { &edit.before } else { &edit.after }}),
        )
        .collect();
    if !preview {
        let filter_path = project.join(".lm-resizer/filters.toml");
        if opts.trust_filters && filter_path.exists() {
            let verification = crate::verify_filter_file(&filter_path)?;
            if verification.failed > 0 {
                bail!("project filter verification failed; no installation changes applied");
            }
        }
        apply(&edits)?;
        if opts.trust_filters && filter_path.exists() {
            crate::trust_filter_file(&filter_path)?;
        }
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
                println!("{}", if opts.show { edit.before } else { edit.after });
            }
        }
    }
    Ok(())
}

pub fn plan(opts: &Options, project: &Path, home: &Path) -> Result<Vec<Edit>> {
    if [
        "claude", "codex", "gemini", "cursor", "trae", "copilot", "droid", "vibe",
    ]
    .contains(&opts.agent.as_str())
    {
        return native_plan(opts, project, home);
    }
    if ["opencode", "pi", "omp", "hermes"].contains(&opts.agent.as_str()) {
        return plugin_plan(opts, project, home);
    }
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
        delete: false,
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
        if edit.delete {
            std::fs::remove_file(&edit.path)?;
            continue;
        }
        // Do not delete user-owned configuration or instruction files.
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

fn native_plan(opts: &Options, project: &Path, home: &Path) -> Result<Vec<Edit>> {
    let agent = opts.agent.as_str();
    if opts.claude_md {
        if agent != "claude" {
            bail!("--claude-md requires --agent claude");
        }
        let path = if opts.global {
            home.join(".claude/CLAUDE.md")
        } else {
            project.join("CLAUDE.md")
        };
        let before = read(&path)?;
        let after = replace_block(&before, opts.uninstall)?;
        return Ok(vec![Edit {
            path,
            before,
            after,
            delete: false,
        }]);
    }
    if matches!(agent, "cursor" | "vibe") && !opts.global {
        bail!("{agent} native hooks require --global");
    }
    let base = if opts.global { home } else { project };
    let (relative, event, matcher, rules) = match agent {
        "claude" => (
            ".claude/settings.json",
            "PreToolUse",
            "Bash",
            if opts.global {
                ".claude/CLAUDE.md"
            } else {
                "CLAUDE.md"
            },
        ),
        "codex" => (
            ".codex/hooks.json",
            "PreToolUse",
            "Bash",
            if opts.global {
                ".codex/AGENTS.md"
            } else {
                "AGENTS.md"
            },
        ),
        "gemini" => (
            ".gemini/settings.json",
            "BeforeTool",
            "run_shell_command",
            if opts.global {
                ".gemini/GEMINI.md"
            } else {
                "GEMINI.md"
            },
        ),
        "cursor" => (
            ".cursor/hooks.json",
            "preToolUse",
            "Shell",
            ".cursor/rules/lm-resizer.mdc",
        ),
        "trae" => (
            ".trae/hooks.json",
            "PreToolUse",
            "RunCommand",
            ".trae/rules/lm-resizer.md",
        ),
        "copilot" => (
            if opts.global {
                ".copilot/hooks/lm-resizer.json"
            } else {
                ".github/hooks/lm-resizer.json"
            },
            "PreToolUse",
            "Bash",
            if opts.global {
                ".copilot/copilot-instructions.md"
            } else {
                ".github/copilot-instructions.md"
            },
        ),
        "droid" => (
            ".factory/hooks.json",
            "PreToolUse",
            "Execute",
            ".factory/AGENTS.md",
        ),
        "vibe" => (
            ".vibe/hooks.toml",
            "pre_tool",
            "bash",
            ".vibe/prompts/lm-resizer.md",
        ),
        _ => unreachable!(),
    };
    let path = native_path(base, relative, agent, opts.global);
    let before = read(&path)?;
    let command = format!("lm-resizer hook {agent}");
    let after = if agent == "vibe" {
        patch_vibe(&before, opts.uninstall)?
    } else {
        patch_json(
            &before,
            event,
            matcher,
            &command,
            matches!(agent, "cursor" | "copilot"),
            opts.uninstall,
        )?
    };
    let mut edits = vec![Edit {
        path,
        before,
        after,
        delete: false,
    }];
    if agent == "trae" && opts.global {
        let path = home.join(".trae-cn/hooks.json");
        let before = read(&path)?;
        let after = patch_json(&before, event, matcher, &command, false, opts.uninstall)?;
        edits.push(Edit {
            path,
            before,
            after,
            delete: false,
        });
    }
    if !opts.hook_only {
        let path = native_path(base, rules, agent, opts.global);
        let before = read(&path)?;
        let after = replace_block(&before, opts.uninstall)?;
        edits.push(Edit {
            path,
            before,
            after,
            delete: false,
        });
    }
    Ok(edits)
}

fn patch_json(
    text: &str,
    event: &str,
    matcher: &str,
    command: &str,
    flat: bool,
    uninstall: bool,
) -> Result<String> {
    if text.trim().is_empty() && uninstall {
        return Ok(text.into());
    }
    let mut root: Value = if text.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(text.trim_start_matches('\u{feff}'))
            .context("invalid agent JSON; left untouched")?
    };
    let original = root.clone();
    let object = root
        .as_object_mut()
        .context("agent config must be an object")?;
    let hooks = object
        .entry("hooks")
        .or_insert(json!({}))
        .as_object_mut()
        .context("hooks must be an object")?;
    let entries = hooks
        .entry(event)
        .or_insert(json!([]))
        .as_array_mut()
        .context("hook event must be an array")?;
    let mut found = false;
    for entry in entries.iter_mut() {
        if flat {
            if entry["command"] == command {
                found = true;
            }
        } else if let Some(items) = entry.get_mut("hooks").and_then(Value::as_array_mut) {
            if items.iter().any(|h| h["command"] == command) {
                found = true;
            }
            if uninstall {
                items.retain(|h| h["command"] != command);
            }
        }
    }
    if uninstall {
        if flat {
            entries.retain(|e| e["command"] != command);
        } else {
            entries.retain(|e| {
                !e.get("hooks")
                    .and_then(Value::as_array)
                    .is_some_and(Vec::is_empty)
            });
        }
    } else if !found {
        entries.push(if flat {
            json!({"type":"command","command":command,"matcher":matcher})
        } else {
            json!({"matcher":matcher,"hooks":[{"type":"command","command":command}]})
        });
    }
    if flat {
        object.entry("version").or_insert(json!(1));
    }
    if root == original {
        Ok(text.into())
    } else {
        Ok(serde_json::to_string_pretty(&root)? + "\n")
    }
}

fn patch_vibe(text: &str, uninstall: bool) -> Result<String> {
    // Validate TOML, but splice only the owned block to preserve comments.
    let _: toml::Value = toml::from_str(text).context("invalid Vibe TOML; left untouched")?;
    const BLOCK:&str="\n# lm-resizer hook begin\n[[hooks]]\nname = \"lm-resizer-rewrite\"\ntype = \"pre_tool\"\nmatch = \"bash\"\ncommand = \"lm-resizer hook vibe\"\n# lm-resizer hook end\n";
    if text.contains(BLOCK) {
        Ok(if uninstall {
            text.replacen(BLOCK, "", 1)
        } else {
            text.into()
        })
    } else if text.contains("# lm-resizer hook begin") {
        bail!("modified lm-resizer Vibe hook; left untouched")
    } else if uninstall {
        Ok(text.into())
    } else {
        Ok(format!("{text}{BLOCK}"))
    }
}

fn owned_file(path: PathBuf, content: &str, uninstall: bool) -> Result<Vec<Edit>> {
    let before = read(&path)?;
    let fingerprint_path = path.with_file_name(format!(
        "{}.lm-resizer.sha256",
        path.file_name()
            .context("plugin filename")?
            .to_string_lossy()
    ));
    let fingerprint_before = read(&fingerprint_path)?;
    if !before.is_empty()
        && before != content
        && fingerprint_before.trim() != crate::sha256_hex(before.as_bytes())
    {
        bail!(
            "modified or unowned plugin {}; left untouched",
            path.display()
        );
    }
    let after = if uninstall {
        String::new()
    } else {
        content.into()
    };
    let fingerprint_after = if uninstall {
        String::new()
    } else {
        crate::sha256_hex(content.as_bytes()) + "\n"
    };
    Ok(vec![
        Edit {
            path,
            before,
            after,
            delete: uninstall,
        },
        Edit {
            path: fingerprint_path,
            before: fingerprint_before,
            after: fingerprint_after,
            delete: uninstall,
        },
    ])
}

fn plugin_plan(opts: &Options, project: &Path, home: &Path) -> Result<Vec<Edit>> {
    let agent = opts.agent.as_str();
    let base = if opts.global { home } else { project };
    let mut edits = Vec::new();
    match agent {
        "opencode" => {
            let path = if opts.global {
                std::env::var_os("XDG_CONFIG_HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join(".config"))
                    .join("opencode/plugins/lm-resizer.ts")
            } else {
                project.join(".opencode/plugins/lm-resizer.ts")
            };
            edits.extend(owned_file(
                path,
                include_str!("../integrations/opencode.ts"),
                opts.uninstall,
            )?);
        }
        "pi" | "omp" => {
            let relative = if opts.global {
                format!(".{agent}/agent/extensions/lm-resizer.ts")
            } else {
                format!(".{agent}/extensions/lm-resizer.ts")
            };
            edits.extend(owned_file(
                base.join(relative),
                include_str!("../integrations/pi.ts"),
                opts.uninstall,
            )?);
        }
        "hermes" => {
            if !opts.global {
                bail!("Hermes plugins require --global");
            }
            let dir = std::env::var_os("HERMES_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".hermes"));
            edits.extend(owned_file(
                dir.join("plugins/lm-resizer-rewrite/__init__.py"),
                include_str!("../integrations/hermes.py"),
                opts.uninstall,
            )?);
            edits.extend(owned_file(dir.join("plugins/lm-resizer-rewrite/plugin.yaml"),"name: lm-resizer-rewrite\nversion: '0.1.0'\ndescription: LM Resizer command rewriting\nhooks: [pre_tool_call]\nprovides_hooks: [pre_tool_call]\n",opts.uninstall)?);
            let path = dir.join("config.yaml");
            let before = read(&path)?;
            let mut root: serde_yaml::Value = if before.trim().is_empty() {
                serde_yaml::from_str("plugins: {enabled: []}")?
            } else {
                serde_yaml::from_str(&before)?
            };
            let mapping = root
                .as_mapping_mut()
                .context("Hermes config must be a YAML mapping")?;
            let plugins = mapping
                .entry(serde_yaml::Value::String("plugins".into()))
                .or_insert(serde_yaml::from_str("enabled: []")?)
                .as_mapping_mut()
                .context("Hermes plugins must be a mapping")?;
            let enabled = plugins
                .entry(serde_yaml::Value::String("enabled".into()))
                .or_insert(serde_yaml::Value::Sequence(vec![]))
                .as_sequence_mut()
                .context("Hermes plugins.enabled must be a list")?;
            let name = serde_yaml::Value::String("lm-resizer-rewrite".into());
            if opts.uninstall {
                enabled.retain(|e| e != &name);
            } else if !enabled.contains(&name) {
                enabled.push(name);
            }
            let after = serde_yaml::to_string(&root)?;
            edits.push(Edit {
                path,
                before,
                after,
                delete: false,
            });
        }
        _ => unreachable!(),
    }
    Ok(edits)
}

fn native_path(base: &Path, relative: &str, agent: &str, global: bool) -> PathBuf {
    if global {
        let variable = match agent {
            "codex" => Some("CODEX_HOME"),
            "claude" => Some("CLAUDE_CONFIG_DIR"),
            _ => None,
        };
        if let Some(directory) = variable.and_then(std::env::var_os) {
            return PathBuf::from(directory)
                .join(relative.split_once('/').map_or(relative, |(_, tail)| tail));
        }
    }
    base.join(relative)
}
