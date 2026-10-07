//! Wire adapters for supported agent hook schemas. Rewriting preserves the
//! original tool arguments and delegates command eligibility to the shared rule.
use serde_json::{json, Value};

pub fn rewrite(value: &Value, exe: &str, event: &str, client: &str) -> Option<Value> {
    let mut normalized = value.clone();
    if let Some(args) = value.get("toolArgs") {
        normalized["tool_input"] = if let Some(s) = args.as_str() {
            serde_json::from_str(s).ok()?
        } else {
            args.clone()
        };
    }
    let tool = value
        .get("tool_name")
        .or(value.get("toolName"))
        .and_then(Value::as_str);
    if matches!(client, "gemini" | "copilot" | "cursor")
        && !matches!(tool, Some("run_shell_command" | "bash" | "Bash" | "Shell"))
    {
        return None;
    }
    let result = crate::pretooluse_rewrite_json(&normalized, exe, event)?;
    let input = result.pointer("/hookSpecificOutput/updatedInput")?.clone();
    Some(match client {
        "gemini" => json!({"hookSpecificOutput":{"hookEventName":"BeforeTool","tool_input":input}}),
        "copilot" => json!({"modifiedArgs":input}),
        "cursor" => json!({"permission":"allow","updated_input":input}),
        _ => result,
    })
}

pub fn config(exe: &str, client: &str) -> Option<Value> {
    let command = |event: &str| format!("\"{exe}\" hook --client {client} --event {event}");
    // Only pre-execution rewriting is installed: the wrapped exec already
    // records measured savings, so a post hook would count the same run twice.
    Some(match client {
        "gemini" => {
            json!({"hooks":{"BeforeTool":[{"matcher":"^run_shell_command$","hooks":[{"type":"command","command":command("BeforeTool"),"timeout":30000}]}]}})
        }
        "copilot" => {
            json!({"version":1,"hooks":{"preToolUse":[{"type":"command","bash":command("preToolUse"),"powershell":format!("& {}",command("preToolUse")),"timeoutSec":30}]}})
        }
        "cursor" => {
            json!({"version":1,"hooks":{"preToolUse":[{"command":command("preToolUse"),"matcher":"^Shell$","timeout":30}]}})
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adapters_preserve_tool_arguments_and_do_not_rewrite_other_tools() {
        for (client, tool, path) in [
            (
                "gemini",
                "run_shell_command",
                "/hookSpecificOutput/tool_input",
            ),
            ("copilot", "bash", "/modifiedArgs"),
            ("cursor", "Shell", "/updated_input"),
        ] {
            let mut value = json!({"tool_name":tool,"tool_input":{"command":"git status","directory":"folder with spaces","timeout":9}});
            if client == "copilot" {
                value = json!({"toolName":tool,"toolArgs":value["tool_input"].to_string()});
            }
            let out = rewrite(&value, "/opt/lm-resizer", "BeforeTool", client).unwrap();
            let args = out.pointer(path).unwrap();
            assert_eq!(args["directory"], "folder with spaces");
            assert_eq!(args["timeout"], 9);
            assert!(args["command"]
                .as_str()
                .unwrap()
                .contains("exec -- git status"));
            assert!(rewrite(
                &json!({"tool_name":"read_file","tool_input":{"command":"git status"}}),
                "lm-resizer",
                "preToolUse",
                client
            )
            .is_none());
        }
    }
    #[test]
    fn configurations_have_agent_specific_envelopes() {
        assert_eq!(
            config("lm-resizer", "gemini").unwrap()["hooks"]["BeforeTool"][0]["hooks"][0]
                ["timeout"],
            30000
        );
        assert_eq!(config("lm-resizer", "copilot").unwrap()["version"], 1);
        assert_eq!(
            config("lm-resizer", "cursor").unwrap()["hooks"]["preToolUse"][0]["matcher"],
            "^Shell$"
        );
    }
}
