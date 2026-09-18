//! Output token reduction primitives: verbosity steering and effort routing.
//!
//! While context compressors optimize what *enters* the LLM, output token reduction
//! optimizes what *exits* the model. Output tokens cost 4x to 5x more than input
//! tokens, making output reduction essential for controlling total cost.
//!
//! # Core Capabilities
//!
//! 1. **Verbosity Steering**: Injects concision guidance strictly into the live zone
//!    (latest un-frozen user turn), guaranteeing byte-for-byte fidelity of the
//!    provider prompt-cache frozen prefix. Idempotent and disableable.
//! 2. **Effort Routing**: Dynamically reduces reasoning effort on routine turns
//!    (file reads, passing test logs, data listings, clean command outputs) while
//!    preserving full effort on non-routine turns (questions, errors, unexpected output).
//!    Derived strictly from repository signal detectors ([`crate::signals::KeywordDetector`]
//!    and [`crate::transforms::detect_content_type`]). Refuses unsupported providers explicitly.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cache_control::compute_frozen_count;
use crate::signals::keyword_detector::KeywordDetector;
use crate::transforms::content_detector::{detect_content_type, ContentType};

/// Standard concision prompt injected during verbosity steering.
pub const CONCISION_PROMPT: &str =
    "Respond concisely. Provide direct answers without unnecessary preamble, boilerplate, or repetition.";

/// Classification of a conversation turn for reasoning effort decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnClassification {
    /// Routine turn (file read, passing test, listing, clean command). Effort can be safely reduced.
    Routine,
    /// Non-routine turn (real question, failure/error, unexpected content, or ambiguous). Full effort kept.
    NonRoutine,
}

impl TurnClassification {
    pub fn as_str(&self) -> &'static str {
        match self {
            TurnClassification::Routine => "routine",
            TurnClassification::NonRoutine => "non_routine",
        }
    }
}

/// Error returned when effort routing is attempted on a provider with no standard effort parameter.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EffortRoutingError {
    #[error(
        "effort routing is not supported for provider '{0}': refusing to inject invented parameter"
    )]
    UnsupportedProvider(String),
}

/// Errors occurring during output shaping.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OutputShapingError {
    #[error("effort routing error: {0}")]
    Effort(#[from] EffortRoutingError),
    #[error("invalid payload shape: {0}")]
    InvalidShape(String),
}

/// Check if text contains real error indicators, discounting zero-failure passing test summaries.
fn contains_real_error(text: &str) -> bool {
    let detector = KeywordDetector::default();
    if !detector.contains_error_indicator(text) {
        return false;
    }
    for line in text.lines() {
        let trimmed = line.trim();
        // Check for common test passing lines that mention "0 failed" or "0 errors"
        if (trimmed.contains("0 failed")
            || trimmed.contains("0 failure")
            || trimmed.contains("failed: 0")
            || trimmed.contains("failures: 0")
            || trimmed.contains("0 errors"))
            && (trimmed.contains("ok")
                || trimmed.contains("passed")
                || trimmed.contains("test result: ok")
                || trimmed.contains("Success"))
        {
            continue;
        }
        if detector.contains_error_indicator(line) {
            return true;
        }
    }
    false
}

/// Classify a single text block/turn based on repository signals.
///
/// If `is_tool_output` is true, the text originated from a tool execution (file read, command, test).
/// If false, it originated from direct user prompt input.
///
/// In case of doubt or missing signals, returns [`TurnClassification::NonRoutine`].
pub fn classify_turn(text: &str, is_tool_output: bool) -> TurnClassification {
    // 1. Error detection using the canonical KeywordDetector (with 0-failed passing summary awareness)
    if contains_real_error(text) {
        return TurnClassification::NonRoutine;
    }

    // 2. Content classification using detect_content_type
    let detection = detect_content_type(text);

    match detection.content_type {
        ContentType::SourceCode => {
            // Reading source code is routine
            TurnClassification::Routine
        }
        ContentType::JsonArray => {
            // Structured data / listing is routine
            TurnClassification::Routine
        }
        ContentType::SearchResults => {
            // Grep / search result listing is routine
            TurnClassification::Routine
        }
        ContentType::GitDiff => {
            // Diff without errors is routine
            TurnClassification::Routine
        }
        ContentType::BuildOutput => {
            // Build / test output with NO error indicators -> passing test / clean build
            TurnClassification::Routine
        }
        ContentType::PlainText => {
            if is_tool_output {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    // Clean silent command (e.g. mkdir, touch, quiet pass)
                    TurnClassification::Routine
                } else if is_routine_command_output(trimmed) {
                    TurnClassification::Routine
                } else {
                    // Doubt costs tokens, not accuracy
                    TurnClassification::NonRoutine
                }
            } else {
                // Direct user text: questions, new instructions
                TurnClassification::NonRoutine
            }
        }
        ContentType::Html => {
            if is_tool_output {
                TurnClassification::Routine
            } else {
                TurnClassification::NonRoutine
            }
        }
    }
}

/// Heuristic check for routine plain text tool output (listings, clean messages).
fn is_routine_command_output(text: &str) -> bool {
    // If it asks a question or has conversational query indicators, it's not a routine command output
    if text.contains('?') {
        return false;
    }
    let lower = text.to_ascii_lowercase();
    if lower.starts_with("why ")
        || lower.starts_with("how ")
        || lower.starts_with("what ")
        || lower.starts_with("explain ")
        || lower.starts_with("please ")
    {
        return false;
    }

    // Typical routine command output patterns: filenames, listing, status ok
    true
}

/// Classify the active live-zone turn in a full request body.
pub fn classify_request_turn(provider_name: &str, path: &str, body: &Value) -> TurnClassification {
    let provider_norm = provider_name.to_ascii_lowercase();

    if provider_norm == "anthropic" || provider_norm == "claude" || path.ends_with("/v1/messages") {
        let Some(messages) = body.get("messages").and_then(Value::as_array) else {
            return TurnClassification::NonRoutine;
        };
        let Some(last_msg) = messages.last() else {
            return TurnClassification::NonRoutine;
        };

        match last_msg.get("content") {
            Some(Value::String(s)) => {
                let is_tool = last_msg.get("role").and_then(Value::as_str) == Some("tool");
                classify_turn(s, is_tool)
            }
            Some(Value::Array(blocks)) => {
                if blocks.is_empty() {
                    return TurnClassification::NonRoutine;
                }
                for block in blocks {
                    let b_type = block.get("type").and_then(Value::as_str).unwrap_or("");
                    let is_tool = b_type == "tool_result";
                    let content_text = if is_tool {
                        extract_block_text(block.get("content"))
                    } else {
                        extract_block_text(block.get("text"))
                    };
                    if classify_turn(&content_text, is_tool) == TurnClassification::NonRoutine {
                        return TurnClassification::NonRoutine;
                    }
                }
                TurnClassification::Routine
            }
            _ => TurnClassification::NonRoutine,
        }
    } else if path.ends_with("/v1/responses") {
        let Some(input) = body.get("input").and_then(Value::as_array) else {
            return TurnClassification::NonRoutine;
        };
        let Some(last_item) = input.last() else {
            return TurnClassification::NonRoutine;
        };
        let i_type = last_item.get("type").and_then(Value::as_str).unwrap_or("");
        let is_tool = i_type == "function_call_output";
        let content_text =
            extract_block_text(last_item.get("output").or_else(|| last_item.get("content")));
        classify_turn(&content_text, is_tool)
    } else {
        // OpenAI chat completions or generic chat
        let Some(messages) = body.get("messages").and_then(Value::as_array) else {
            return TurnClassification::NonRoutine;
        };
        let Some(last_msg) = messages.last() else {
            return TurnClassification::NonRoutine;
        };
        let role = last_msg.get("role").and_then(Value::as_str).unwrap_or("");
        let is_tool = role == "tool";
        let content_text = extract_block_text(last_msg.get("content"));
        classify_turn(&content_text, is_tool)
    }
}

fn extract_block_text(val: Option<&Value>) -> String {
    match val {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Check if a concision prompt is already present in the request body.
pub fn has_concision_instruction(body: &Value) -> bool {
    let s = body.to_string();
    s.contains(CONCISION_PROMPT)
        || s.contains("Respond concisely")
        || s.contains("concise and terse")
        || s.contains("[lm-resizer:concise]")
}

/// Apply verbosity steering to a request body in place.
///
/// Ensures the frozen prompt prefix remains 100% identical byte-for-byte.
/// Returns `Ok(true)` if concision was injected, `Ok(false)` if skipped (disabled,
/// already present, or unmanaged route), or an error.
pub fn steer_verbosity(
    provider_name: &str,
    path: &str,
    body: &mut Value,
    enabled: bool,
) -> Result<bool, OutputShapingError> {
    if !enabled {
        return Ok(false);
    }
    if has_concision_instruction(body) {
        return Ok(false);
    }

    let provider_norm = provider_name.to_ascii_lowercase();
    match provider_norm.as_str() {
        "anthropic" | "claude" => {
            let frozen_count = compute_frozen_count(body);
            let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) else {
                return Ok(false);
            };
            if messages.is_empty() {
                return Ok(false);
            }

            // Find latest user message index
            let latest_user_idx = messages
                .iter()
                .rposition(|m| m.get("role").and_then(Value::as_str) == Some("user"));
            let Some(target_idx) = latest_user_idx else {
                return Ok(false);
            };

            // Invariant: The frozen prefix (indices < frozen_count) must NEVER be modified!
            // Modifying a message within the frozen prefix breaks prompt caching.
            if target_idx < frozen_count {
                return Ok(false);
            }

            // Append to the target user message in the live zone
            append_concision_to_message(&mut messages[target_idx])
        }
        "openai" | "openai-compatible" | "chatgpt" => {
            if path.ends_with("/v1/responses") {
                let Some(input) = body.get_mut("input").and_then(Value::as_array_mut) else {
                    return Ok(false);
                };
                if input.is_empty() {
                    return Ok(false);
                }
                // Append to latest user item, or trailing item in live zone
                let target_idx = input
                    .iter()
                    .rposition(|item| item.get("role").and_then(Value::as_str) == Some("user"))
                    .unwrap_or(input.len().saturating_sub(1));
                append_concision_to_message(&mut input[target_idx])
            } else {
                // OpenAI chat completions
                let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) else {
                    return Ok(false);
                };
                if messages.is_empty() {
                    return Ok(false);
                }
                // Locate latest user message in live zone. Never modify messages[0] or system!
                let target_idx = messages
                    .iter()
                    .rposition(|m| m.get("role").and_then(Value::as_str) == Some("user"))
                    .unwrap_or(messages.len().saturating_sub(1));
                append_concision_to_message(&mut messages[target_idx])
            }
        }
        _ => {
            // Unmanaged routes / providers: do not inject
            Ok(false)
        }
    }
}

fn append_concision_to_message(message: &mut Value) -> Result<bool, OutputShapingError> {
    if let Some(content) = message.get_mut("content") {
        match content {
            Value::String(text) => {
                text.push_str("\n\n");
                text.push_str(CONCISION_PROMPT);
                Ok(true)
            }
            Value::Array(blocks) => {
                blocks.push(serde_json::json!({
                    "type": "text",
                    "text": CONCISION_PROMPT,
                }));
                Ok(true)
            }
            _ => Ok(false),
        }
    } else {
        Ok(false)
    }
}

/// Whether we may add an `output_config` object that the caller did not send.
///
/// Off by default: see the comment in [`route_effort`]. Set
/// `LM_RESIZER_ANTHROPIC_EFFORT=1` to opt in once the field has been confirmed
/// against the target API.
fn anthropic_effort_injection_allowed() -> bool {
    matches!(
        std::env::var("LM_RESIZER_ANTHROPIC_EFFORT").ok().as_deref(),
        Some("1") | Some("true")
    )
}

/// Route reasoning effort for the current turn.
///
/// On routine turns, dials reasoning effort down to "low". On non-routine turns, leaves
/// full effort unchanged.
///
/// Explicitly rejects unsupported providers (Bedrock, Vertex, unknown) with [`EffortRoutingError`].
pub fn route_effort(
    provider_name: &str,
    path: &str,
    body: &mut Value,
    classification: TurnClassification,
) -> Result<bool, EffortRoutingError> {
    let provider_norm = provider_name.to_ascii_lowercase();
    match provider_norm.as_str() {
        "openai" | "openai-compatible" | "chatgpt" => {
            if classification == TurnClassification::Routine {
                if path.ends_with("/v1/responses") {
                    body["reasoning"] = serde_json::json!({ "effort": "low" });
                } else {
                    body["reasoning_effort"] = serde_json::json!("low");
                }
                Ok(true)
            } else {
                Ok(false)
            }
        }
        "anthropic" | "claude" => {
            if classification != TurnClassification::Routine {
                return Ok(false);
            }
            // Conservative by construction: we narrow a field the caller already
            // sends, we do not invent one. An unknown top-level field makes the
            // Anthropic API reject the whole request, which would break every
            // routine turn rather than merely failing to save tokens. Inventing
            // the field is therefore opt-in, for a caller who has checked it
            // against the API they actually target.
            if let Some(cfg) = body.get_mut("output_config").and_then(Value::as_object_mut) {
                cfg.insert("effort".to_string(), serde_json::json!("low"));
                Ok(true)
            } else if anthropic_effort_injection_allowed() {
                body["output_config"] = serde_json::json!({ "effort": "low" });
                Ok(true)
            } else {
                Ok(false)
            }
        }
        "bedrock" | "aws-bedrock" | "vertex" | "vertexai" | "vertex-ai" | "google-vertex" => Err(
            EffortRoutingError::UnsupportedProvider(provider_name.to_string()),
        ),
        other => Err(EffortRoutingError::UnsupportedProvider(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn routine_vs_non_routine_classification() {
        // Routine: file read (source code)
        assert_eq!(
            classify_turn("fn add(a: i32, b: i32) -> i32 { a + b }", true),
            TurnClassification::Routine
        );

        // Routine: passing tests
        let test_pass = "running 10 tests\ntest test_one ... ok\ntest test_two ... ok\ntest result: ok. 10 passed; 0 failed";
        assert_eq!(classify_turn(test_pass, true), TurnClassification::Routine);

        // Routine: listing
        let listing = "[{\"id\": 1, \"name\": \"alpha\"}, {\"id\": 2, \"name\": \"beta\"}]";
        assert_eq!(classify_turn(listing, true), TurnClassification::Routine);

        // Routine: clean command output
        assert_eq!(
            classify_turn("crates\nsrc\nCargo.toml\nREADME.md", true),
            TurnClassification::Routine
        );

        // Non-routine: error / failure
        let test_fail =
            "running 2 tests\ntest test_one ... FAILED\nfailures:\nerror: assertion failed";
        assert_eq!(
            classify_turn(test_fail, true),
            TurnClassification::NonRoutine
        );

        // Non-routine: real question from user
        assert_eq!(
            classify_turn(
                "How can I fix the bug in the authentication handler?",
                false
            ),
            TurnClassification::NonRoutine
        );

        // Non-routine: unexpected / ambiguous content
        assert_eq!(
            classify_turn("Why did this crash with a panic?", false),
            TurnClassification::NonRoutine
        );
    }

    #[test]
    fn verbosity_steering_applied_once_idempotent() {
        let mut body = json!({
            "model": "gpt-4o",
            "messages": [
                {"role": "user", "content": "list the files"}
            ]
        });

        // First application
        let applied1 = steer_verbosity("openai", "/v1/chat/completions", &mut body, true).unwrap();
        assert!(applied1);
        let content1 = body["messages"][0]["content"].as_str().unwrap().to_string();
        assert!(content1.contains(CONCISION_PROMPT));

        // Second application should be a no-op
        let applied2 = steer_verbosity("openai", "/v1/chat/completions", &mut body, true).unwrap();
        assert!(!applied2);
        let content2 = body["messages"][0]["content"].as_str().unwrap();
        assert_eq!(&content1, content2);
    }

    #[test]
    fn verbosity_steering_can_be_disabled() {
        let mut body = json!({
            "model": "gpt-4o",
            "messages": [
                {"role": "user", "content": "list the files"}
            ]
        });

        let applied = steer_verbosity("openai", "/v1/chat/completions", &mut body, false).unwrap();
        assert!(!applied);
        assert!(!has_concision_instruction(&body));
    }

    #[test]
    fn effort_routing_reduces_routine_and_keeps_non_routine() {
        let mut body_openai = json!({
            "model": "o3-mini",
            "messages": [{"role": "user", "content": "files"}]
        });

        // Routine -> reduces effort
        let routed = route_effort(
            "openai",
            "/v1/chat/completions",
            &mut body_openai,
            TurnClassification::Routine,
        )
        .unwrap();
        assert!(routed);
        assert_eq!(body_openai["reasoning_effort"], "low");

        // Non-routine -> leaves full effort
        let mut body_openai_non_routine = json!({
            "model": "o3-mini",
            "messages": [{"role": "user", "content": "solve P vs NP"}]
        });
        let routed2 = route_effort(
            "openai",
            "/v1/chat/completions",
            &mut body_openai_non_routine,
            TurnClassification::NonRoutine,
        )
        .unwrap();
        assert!(!routed2);
        assert!(body_openai_non_routine.get("reasoning_effort").is_none());
    }

    #[test]
    fn effort_routing_anthropic_narrows_a_field_the_caller_already_sends() {
        let mut body_claude = json!({
            "model": "claude-3-7-sonnet-20250219",
            "messages": [{"role": "user", "content": "status"}],
            "output_config": {"max_tokens": 1024}
        });

        let routed = route_effort(
            "anthropic",
            "/v1/messages",
            &mut body_claude,
            TurnClassification::Routine,
        )
        .unwrap();
        assert!(routed);
        assert_eq!(body_claude["output_config"]["effort"], "low");
        // the caller's own keys survive
        assert_eq!(body_claude["output_config"]["max_tokens"], 1024);
    }

    #[test]
    fn effort_routing_anthropic_does_not_invent_the_field_by_default() {
        let mut body_claude = json!({
            "model": "claude-3-7-sonnet-20250219",
            "messages": [{"role": "user", "content": "status"}]
        });

        let routed = route_effort(
            "anthropic",
            "/v1/messages",
            &mut body_claude,
            TurnClassification::Routine,
        )
        .unwrap();
        // Not saving tokens is acceptable; making the API reject the request is not.
        assert!(!routed);
        assert!(body_claude.get("output_config").is_none());
    }

    #[test]
    fn effort_routing_explicitly_refuses_unsupported_providers() {
        let mut body = json!({"messages": []});

        // Bedrock must be rejected
        let err_bedrock = route_effort(
            "bedrock",
            "/model/invoke",
            &mut body,
            TurnClassification::Routine,
        )
        .unwrap_err();
        assert_eq!(
            err_bedrock,
            EffortRoutingError::UnsupportedProvider("bedrock".to_string())
        );

        // Vertex must be rejected
        let err_vertex = route_effort(
            "vertex",
            "/v1beta/models/gemini:generateContent",
            &mut body,
            TurnClassification::Routine,
        )
        .unwrap_err();
        assert_eq!(
            err_vertex,
            EffortRoutingError::UnsupportedProvider("vertex".to_string())
        );
    }
}
