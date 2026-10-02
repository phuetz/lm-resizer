//! Protocol reference: rtk-ai/rtk v0.50.0, commit
//! 1d87b8e719ce0a50c223cd93ca64dd16921f9aec, src/hooks/{init,hook_cmd}.rs.
//! Independent implementation; see THIRD-PARTY-NOTICES.
//! Host-specific wire formats; execution and final approval remain with the host.
use anyhow::Result;
use serde_json::{json, Value};
use std::io::Read;

pub fn response(agent: &str, value: &Value, exe: &str) -> Option<Value> {
    if agent == "copilot" && value.get("tool_name").is_none() {
        let tool = value["toolName"].as_str()?;
        if !matches!(tool, "bash" | "powershell") {
            return None;
        }
        let args: Value = serde_json::from_str(value["toolArgs"].as_str()?).ok()?;
        let normalized = json!({"tool_name":"Bash","tool_input":args});
        let output = response(agent, &normalized, exe)?;
        return Some(json!({"modifiedArgs":output["hookSpecificOutput"]["updatedInput"]}));
    }
    if agent == "codex"
        && (value["hook_event_name"] != "PreToolUse"
            || !matches!(
                value["permission_mode"].as_str(),
                Some("default" | "acceptEdits" | "plan" | "dontAsk" | "bypassPermissions")
            ))
    {
        return None;
    }
    let name = value["tool_name"].as_str()?;
    let matches = match agent {
        "claude" | "codex" => matches!(name, "Bash" | "bash"),
        "copilot" => matches!(
            name,
            "Bash" | "bash" | "runTerminalCommand" | "run_in_terminal"
        ),
        "gemini" => name == "run_shell_command",
        "cursor" => matches!(name, "Shell" | "shell"),
        "trae" => name == "RunCommand",
        "droid" => name == "Execute",
        "vibe" => name == "bash",
        _ => false,
    };
    if !matches {
        return None;
    }
    let mut input = value["tool_input"].as_object()?.clone();
    let command = input.get("command")?.as_str()?;
    // Shell expansion and compound commands remain under the host's own rules.
    if command.contains("$(") || command.contains('`') || command.contains('\n') {
        return None;
    }
    let rewritten = crate::rewrite_command_for_hook(command, exe)?;
    input.insert("command".into(), json!(rewritten));
    let input = Value::Object(input);
    Some(match agent {
        // Codex requires protocol-level allow to accept updatedInput. Its
        // native approval/sandbox checks still run on the replacement. Local
        // rule configurations are conservatively deferred by run().
        "codex" => {
            json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow","updatedInput":input}})
        }
        "gemini" => json!({"decision":"ask_user","hookSpecificOutput":{"tool_input":input}}),
        "cursor" => json!({"continue":true,"permission":"ask","updated_input":input}),
        "vibe" => json!({"hook_specific_output":{"tool_input":input}}),
        _ => json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","updatedInput":input}}),
    })
}

pub fn run(agent: &str, command: &[String], check_agent: &str) -> Result<()> {
    if agent == "check" {
        let raw = command.join(" ");
        let client = if check_agent == "unknown" {
            "claude"
        } else {
            check_agent
        };
        if !crate::integration_doctor::CLIENTS.contains(&client) {
            anyhow::bail!("unknown agent {client}");
        }
        let rewritten = if host_has_constraints(client) {
            None
        } else {
            crate::rewrite_command_for_hook(&raw, "lm-resizer")
        };
        println!(
            "{}",
            json!({"command":raw,"rewritten":rewritten,"changed":rewritten.is_some()})
        );
        return Ok(());
    }
    if ![
        "claude", "codex", "gemini", "cursor", "trae", "droid", "copilot", "vibe",
    ]
    .contains(&agent)
    {
        anyhow::bail!("unknown hook client: {agent}");
    }
    let mut input = String::new();
    if std::io::stdin()
        .take(1024 * 1024 + 1)
        .read_to_string(&mut input)
        .is_err()
        || input.len() > 1024 * 1024
    {
        return Ok(());
    }
    let value = serde_json::from_str::<Value>(input.trim_start_matches('\u{feff}')).ok();
    // Installed hook commands already resolve lm-resizer on PATH. A bare name
    // is valid in both POSIX shells and PowerShell, unlike a quoted path.
    let exe = "lm-resizer";
    let output = if host_has_constraints(agent) {
        None
    } else {
        value.as_ref().and_then(|v| response(agent, v, exe))
    };
    if let Some(ref output) = output {
        println!("{output}");
    }
    // Opt-in, local-only audit; a logging error must not affect the hook.
    if std::env::var("LM_RESIZER_HOOK_AUDIT").as_deref() == Ok("1") {
        let _ = audit(agent, output.is_some(), value.as_ref());
    }
    Ok(())
}

fn audit(agent: &str, changed: bool, value: Option<&Value>) -> Result<()> {
    let path = crate::default_state_dir()?.join("hook-audit.jsonl");
    std::fs::create_dir_all(path.parent().unwrap())?;
    crate::journal::append(
        &path,
        &json!({"timestamp_unix":crate::unix_timestamp(),"agent":agent,"changed":changed,"session_id":value.and_then(|v|v.get("session_id")),"tool_use_id":value.and_then(|v|v.get("tool_use_id"))}),
    )?;
    Ok(())
}

pub fn audit_report(since: u64) -> Result<()> {
    let path = crate::default_state_dir()?.join("hook-audit.jsonl");
    let text = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    let cutoff = crate::unix_timestamp().saturating_sub(since.saturating_mul(86400));
    let rows: Vec<Value> = text
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|r| since == 0 || r["timestamp_unix"].as_u64().unwrap_or(0) >= cutoff)
        .collect();
    let changed = rows.iter().filter(|r| r["changed"] == true).count();
    println!(
        "{}",
        json!({"events":rows.len(),"rewritten":changed,"skipped":rows.len()-changed,"entries":rows,"enabled":std::env::var("LM_RESIZER_HOOK_AUDIT").as_deref()==Ok("1")})
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn host_contracts_preserve_fields_and_codex_protocol_acknowledgement() {
        for (agent, tool, pointer) in [
            ("claude", "Bash", "/hookSpecificOutput/updatedInput"),
            ("codex", "Bash", "/hookSpecificOutput/updatedInput"),
            ("trae", "RunCommand", "/hookSpecificOutput/updatedInput"),
            ("droid", "Execute", "/hookSpecificOutput/updatedInput"),
            ("copilot", "Bash", "/hookSpecificOutput/updatedInput"),
            (
                "gemini",
                "run_shell_command",
                "/hookSpecificOutput/tool_input",
            ),
            ("cursor", "Shell", "/updated_input"),
            ("vibe", "bash", "/hook_specific_output/tool_input"),
        ] {
            let v = json!({"permission_mode":"default","hook_event_name":"PreToolUse","tool_name":tool,"tool_input":{"command":"git status","timeout":42}});
            let out = response(agent, &v, "lm-resizer").unwrap();
            assert_eq!(out.pointer(pointer).unwrap()["timeout"], 42);
            assert!(out.pointer(pointer).unwrap()["command"]
                .as_str()
                .unwrap()
                .ends_with("exec -- git status"));
            assert_eq!(out.to_string().contains("\"allow\""), agent == "codex");
            assert!(response(
                agent,
                &json!({"tool_name":"Read","tool_input":{"command":"git status"}}),
                "lm-resizer"
            )
            .is_none());
        }
    }
    #[test]
    fn unsupported_compound_and_recursive_commands_pass_through() {
        for cmd in [
            "git status && git diff",
            "lm-resizer exec -- git status",
            "git diff > output",
            "git show $(danger)",
            "unknown",
            "",
        ] {
            assert!(
                response(
                    "claude",
                    &json!({"tool_name":"Bash","tool_input":{"command":cmd}}),
                    "lm-resizer"
                )
                .is_none(),
                "{cmd}"
            );
        }
    }
}

// A wrapper changes the command the host checks. Until every host's permission
// language can be evaluated exactly, defer if local deny/ask policies exist.
fn host_has_constraints(agent: &str) -> bool {
    fn restricted(v: &Value) -> bool {
        match v {
            Value::Object(map) => map.iter().any(|(key, value)| {
                (matches!(
                    key.as_str(),
                    "deny" | "ask" | "exclude" | "commandDenylist" | "commandAllowlist"
                ) && match value {
                    Value::Array(a) => !a.is_empty(),
                    Value::Object(o) => !o.is_empty(),
                    Value::Null => false,
                    _ => true,
                }) || restricted(value)
            }),
            Value::Array(items) => items.iter().any(restricted),
            _ => false,
        }
    }
    let mut roots = vec![];
    if let Some(home) = crate::user_home_dir() {
        roots.push(home);
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.extend(cwd.ancestors().map(std::path::Path::to_path_buf));
    }
    let directory = match agent {
        "claude" => ".claude",
        "codex" => ".codex",
        "gemini" => ".gemini",
        "cursor" => ".cursor",
        "droid" => ".factory",
        "trae" => ".trae",
        _ => return false,
    };
    let mut directories: Vec<_> = roots.into_iter().map(|root| root.join(directory)).collect();
    let variable = match agent {
        "codex" => Some("CODEX_HOME"),
        "claude" => Some("CLAUDE_CONFIG_DIR"),
        _ => None,
    };
    if let Some(path) = variable.and_then(std::env::var_os) {
        directories.push(path.into());
    }
    for dir in directories {
        if agent == "codex" && dir.join("rules").is_dir() {
            match std::fs::read_dir(dir.join("rules")) {
                Ok(mut entries) => {
                    if entries.next().is_some() {
                        return true;
                    }
                }
                Err(_) => return true,
            }
        }
        for name in ["settings.json", "settings.local.json", "cli-config.json"] {
            match std::fs::read_to_string(dir.join(name)) {
                Ok(text) => {
                    match serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) {
                        Ok(value) => {
                            if restricted(&value) {
                                return true;
                            }
                        }
                        Err(_) => return true,
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return true,
            }
        }
    }
    false
}
