//! Host-specific wire formats; never execute commands or grant permission here.
use anyhow::Result;
use serde_json::{json, Value};
use std::io::{Read, Write};

pub fn response(agent: &str, value: &Value, exe: &str) -> Option<Value> {
    let name = value["tool_name"].as_str()?;
    let matches = match agent {
        "claude" | "codex" | "copilot" => matches!(name, "Bash" | "bash"),
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
        "gemini" => json!({"decision":"ask_user","hookSpecificOutput":{"tool_input":input}}),
        "cursor" => json!({"continue":true,"permission":"ask","updated_input":input}),
        "vibe" => json!({"hook_specific_output":{"tool_input":input}}),
        _ => json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","updatedInput":input}}),
    })
}

pub fn run(agent: &str, command: &[String]) -> Result<()> {
    if agent == "check" {
        let raw = command.join(" ");
        let rewritten = crate::rewrite_command_for_hook(&raw, "lm-resizer");
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
    let exe = std::env::current_exe()?.to_string_lossy().into_owned();
    let output = value.as_ref().and_then(|v| response(agent, v, &exe));
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
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(
        file,
        "{}",
        json!({"timestamp_unix":crate::unix_timestamp(),"agent":agent,"changed":changed,"session_id":value.and_then(|v|v.get("session_id")),"tool_use_id":value.and_then(|v|v.get("tool_use_id"))})
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
    fn host_contracts_preserve_fields_without_granting_permission() {
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
            let v = json!({"tool_name":tool,"tool_input":{"command":"git status","timeout":42}});
            let out = response(agent, &v, "lm-resizer").unwrap();
            assert_eq!(out.pointer(pointer).unwrap()["timeout"], 42);
            assert!(out.pointer(pointer).unwrap()["command"]
                .as_str()
                .unwrap()
                .ends_with("exec -- git status"));
            assert!(!out.to_string().contains("\"allow\""));
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
