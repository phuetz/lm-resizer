//! Exact whole-block references in an explicitly supplied message window.
//! Never rewrites an earlier message or a cache-protected message.
use anyhow::{Context, Result};
use serde_json::{json, Value};

pub fn fold(raw: &str) -> Result<String> {
    let mut messages: Vec<Value> =
        serde_json::from_str(raw).context("expected an array of chat messages")?;
    // A pre-existing object can resemble the v1 codec's reference syntax.
    // Preserve such windows literally rather than interpreting user content.
    if messages
        .iter()
        .any(|m| m["content"].get("same_as_message").is_some())
    {
        return Ok(raw.into());
    }
    let mut earlier = std::collections::HashMap::<String, usize>::new();
    let mut changed = false;
    for (index, message) in messages.iter_mut().enumerate() {
        if message["role"] != "tool" || message.get("cache_control").is_some() {
            continue;
        }
        let Some(text) = message["content"].as_str() else {
            continue;
        };
        if text.lines().count() < 3 || text.chars().count() < 40 {
            continue;
        }
        if let Some(&source) = earlier.get(text) {
            message["content"] = json!({"same_as_message":source});
            changed = true;
        } else {
            earlier.insert(text.into(), index);
        }
    }
    if !changed {
        return Ok(raw.into());
    }
    let out = serde_json::to_string(&json!({"format":"conversation-fold-v1","messages":messages}))?;
    let restored: Value = serde_json::from_str(&expand(&out)?)?;
    anyhow::ensure!(
        restored == serde_json::from_str::<Value>(raw)?,
        "conversation inverse mismatch"
    );
    if crate::token_metrics::TokenCounts::measure(raw, &out).tokens_saved > 0 {
        Ok(out)
    } else {
        Ok(raw.into())
    }
}

pub fn expand(view: &str) -> Result<String> {
    let mut envelope: Value = serde_json::from_str(view)?;
    anyhow::ensure!(
        envelope["format"] == "conversation-fold-v1",
        "unknown conversation format"
    );
    let messages = envelope["messages"]
        .as_array_mut()
        .context("missing messages")?;
    for i in 0..messages.len() {
        if let Some(source) = messages[i]["content"]["same_as_message"].as_u64() {
            let source = usize::try_from(source)?;
            anyhow::ensure!(
                source < i,
                "reference must point to an earlier message in this window"
            );
            let text = messages[source]["content"]
                .as_str()
                .context("reference target is not text")?
                .to_owned();
            messages[i]["content"] = Value::String(text);
        }
    }
    Ok(serde_json::to_string(messages)?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_shaped_user_content_remains_literal() {
        let text = "invoice processed successfully\n".repeat(80);
        let raw = json!([
            {"role":"tool","content":text},
            {"role":"tool","content":{"same_as_message":0,"payload":"keep"}},
            {"role":"tool","content":text}
        ])
        .to_string();
        assert_eq!(fold(&raw).unwrap(), raw);
    }
    #[test]
    fn repeats_reference_only_earlier_visible_unprotected_tools() {
        let log = "08:00:01 invoice processed successfully\n".repeat(80);
        let changed = log.replacen("invoice", "another", 1);
        let raw=json!([{"role":"tool","content":log},{"role":"tool","content":log},{"role":"tool","content":changed},{"role":"tool","content":log,"cache_control":{"type":"ephemeral"}}]).to_string();
        let view = fold(&raw).unwrap();
        let v: Value = serde_json::from_str(&view).unwrap();
        assert_eq!(v["messages"][0]["content"], log);
        assert_eq!(v["messages"][1]["content"]["same_as_message"], 0);
        assert_eq!(v["messages"][2]["content"], changed);
        assert_eq!(v["messages"][3]["content"], log);
        assert_eq!(
            serde_json::from_str::<Value>(&expand(&view).unwrap()).unwrap(),
            serde_json::from_str::<Value>(&raw).unwrap()
        );
        assert_eq!(
            fold(&json!([{"role":"tool","content":log}]).to_string()).unwrap(),
            json!([{"role":"tool","content":log}]).to_string()
        );
    }
}
