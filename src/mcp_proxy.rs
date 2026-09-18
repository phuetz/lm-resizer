use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use lm_resizer_core::ccr::{compute_key, CcrStore};
use lm_resizer_core::transforms::CompressionPipeline;
use serde_json::{json, Value};

use crate::{
    build_pipeline, compress_text_with_pipeline, open_store, record_exec_history,
    resolve_command_path, ExecReport,
};

/// Tracks pending JSON-RPC requests issued by the client/agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRequest {
    pub method: String,
    pub tool_name: String,
    pub query: String,
}

/// Extract search query or pattern from tool arguments if present.
pub fn extract_query_from_params(params: Option<&Value>) -> String {
    let Some(params) = params else {
        return String::new();
    };
    let Some(args) = params.get("arguments") else {
        return String::new();
    };
    for key in ["query", "q", "pattern", "prompt"] {
        if let Some(q) = args.get(key).and_then(Value::as_str) {
            return q.to_string();
        }
    }
    String::new()
}

/// Record client requests with both "id" and "method".
pub fn process_agent_line(line: &str, pending: &Mutex<HashMap<Value, PendingRequest>>) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }
    let Ok(req) = serde_json::from_str::<Value>(trimmed) else {
        return;
    };
    if let (Some(id), Some(method)) = (req.get("id"), req.get("method").and_then(Value::as_str)) {
        let query = extract_query_from_params(req.get("params"));
        let tool_name = req
            .get("params")
            .and_then(|p| p.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        pending.lock().unwrap().insert(
            id.clone(),
            PendingRequest {
                method: method.to_string(),
                tool_name,
                query,
            },
        );
    }
}

/// Outcome of processing an upstream MCP message line.
#[derive(Debug, PartialEq, Eq)]
pub enum ProcessOutcome {
    Unmodified,
    Modified(String),
    Ignore,
}

/// Process a single upstream line and compress tool call text content when profitable.
pub fn process_upstream_line(
    line: &str,
    pending: &Mutex<HashMap<Value, PendingRequest>>,
    store: &dyn CcrStore,
    pipeline: &CompressionPipeline,
) -> ProcessOutcome {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return ProcessOutcome::Ignore;
    }

    let mut val: Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => return ProcessOutcome::Unmodified,
    };

    // Upstream notifications or server-to-client requests have a "method" field
    if val.get("method").is_some() {
        return ProcessOutcome::Unmodified;
    }

    let Some(id) = val.get("id").cloned() else {
        return ProcessOutcome::Unmodified;
    };

    let pending_req = {
        let mut lock = pending.lock().unwrap();
        lock.remove(&id)
    };

    let Some(req) = pending_req else {
        return ProcessOutcome::Unmodified;
    };

    // Point 2: tools/list, initialize, resources, etc. are NOT compressed
    if req.method != "tools/call" {
        return ProcessOutcome::Unmodified;
    }

    // Point 1: Protocol errors pass through unchanged
    if val.get("error").is_some() {
        return ProcessOutcome::Unmodified;
    }

    let modified = compress_tool_call_result(&mut val, store, pipeline, &req.query, &req.tool_name);
    if modified {
        match serde_json::to_string(&val) {
            Ok(s) => ProcessOutcome::Modified(s),
            Err(_) => ProcessOutcome::Unmodified,
        }
    } else {
        ProcessOutcome::Unmodified
    }
}

/// Compress text blocks within an MCP `tools/call` response result.
pub fn compress_tool_call_result(
    response: &mut Value,
    store: &dyn CcrStore,
    pipeline: &CompressionPipeline,
    query: &str,
    tool_name: &str,
) -> bool {
    let Some(result) = response.get_mut("result") else {
        return false;
    };
    let Some(content) = result.get_mut("content").and_then(Value::as_array_mut) else {
        return false;
    };

    let mut modified = false;

    for item in content.iter_mut() {
        let is_text = item.get("type").and_then(Value::as_str) == Some("text");
        if !is_text {
            // Point 1: Non-text content (e.g. image, resource) passes completely untouched
            continue;
        }

        let Some(text_val) = item.get_mut("text") else {
            continue;
        };
        let Some(text) = text_val.as_str() else {
            continue;
        };

        // Point 4: Do not compress what is already compact (< 512 bytes)
        if text.len() < 512 {
            continue;
        }

        let Ok(report) = compress_text_with_pipeline(text, query, store, pipeline, None) else {
            continue;
        };

        if report.compressed_bytes >= report.original_bytes || report.bytes_saved == 0 {
            // No savings achieved
            continue;
        }

        // Point 3: Store full original in CCR backend and verify write
        let hash = compute_key(text.as_bytes());
        store.put(&hash, text);
        if store.get(&hash).is_none() {
            continue;
        }

        let mut candidate = report.output;
        let hint = format!("[full output: <<ccr:{hash}>>]");
        if !candidate.contains(&hint) && !candidate.contains(&format!("<<ccr:{hash}")) {
            if !candidate.ends_with('\n') && !candidate.is_empty() {
                candidate.push('\n');
            }
            candidate.push_str(&hint);
        }

        // Point 4: Strict no-growth acceptance gate
        if candidate.len() < text.len() {
            let original_bytes = text.len();
            let compressed_bytes = candidate.len();
            let bytes_saved = original_bytes.saturating_sub(compressed_bytes);

            *text_val = Value::String(candidate);
            modified = true;

            let exec_report = ExecReport {
                command: format!("mcp:{tool_name}"),
                exit_code: 0,
                filter: "mcp_proxy".to_string(),
                original_bytes,
                filtered_bytes: original_bytes,
                compressed_bytes,
                bytes_saved,
                compression_steps: report.steps_applied,
                cache_keys: vec![hash],
                tee_hint: None,
                output: String::new(),
            };
            let _ = record_exec_history(&exec_report, Duration::ZERO);
        }
    }

    modified
}

/// Helper to write raw line with trailing newline.
pub fn write_raw_line<W: Write>(out: &mut W, line: &str) -> io::Result<()> {
    out.write_all(line.as_bytes())?;
    if !line.ends_with('\n') {
        out.write_all(b"\n")?;
    }
    out.flush()
}

/// Helper to serialize and write JSON value.
pub fn write_json_value<W: Write>(out: &mut W, val: &Value) -> io::Result<()> {
    let mut bytes =
        serde_json::to_vec(val).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    bytes.push(b'\n');
    out.write_all(&bytes)?;
    out.flush()
}

/// Run the transparent MCP stdio proxy.
pub fn run_mcp_proxy(command: Vec<String>, store_path: Option<PathBuf>) -> Result<()> {
    if command.is_empty() {
        anyhow::bail!("missing command for mcp-proxy");
    }
    let (program, args) = command
        .split_first()
        .context("missing command for mcp-proxy")?;
    let resolved_program = resolve_command_path(program).unwrap_or_else(|| PathBuf::from(program));
    let store = open_store(store_path)?;
    let pipeline = build_pipeline();

    let mut child = match Command::new(&resolved_program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(c) => c,
        Err(err) => {
            anyhow::bail!(
                "failed to start upstream MCP server '{}': {err}",
                command.join(" ")
            );
        }
    };

    let mut child_in = child
        .stdin
        .take()
        .context("failed to capture child stdin")?;
    let child_out = child
        .stdout
        .take()
        .context("failed to capture child stdout")?;

    let pending: Arc<Mutex<HashMap<Value, PendingRequest>>> = Arc::new(Mutex::new(HashMap::new()));
    let pending_in = Arc::clone(&pending);

    let child_alive = Arc::new(AtomicBool::new(true));
    let child_alive_in = Arc::clone(&child_alive);

    // Stdin forwarding thread: Agent stdin -> Child stdin
    let _stdin_thread = std::thread::spawn(move || {
        let stdin = io::stdin();
        let mut reader = stdin.lock();
        let mut line = String::new();

        while child_alive_in.load(Ordering::Relaxed) {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    process_agent_line(&line, &pending_in);
                    if child_in.write_all(line.as_bytes()).is_err() || child_in.flush().is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        drop(child_in);
    });

    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();
    let reader = BufReader::new(child_out);

    for line_res in reader.lines() {
        let line = match line_res {
            Ok(l) => l,
            Err(_) => break,
        };

        match process_upstream_line(&line, &pending, store.as_ref(), &pipeline) {
            ProcessOutcome::Modified(mod_line) => {
                let _ = write_raw_line(&mut stdout_lock, &mod_line);
            }
            ProcessOutcome::Unmodified => {
                let _ = write_raw_line(&mut stdout_lock, &line);
            }
            ProcessOutcome::Ignore => {}
        }
    }

    child_alive.store(false, Ordering::SeqCst);

    // Point 5: Wait for child exit status
    let status = child.wait()?;

    // Check if any requests were pending when child terminated
    let leftover: Vec<(Value, PendingRequest)> = {
        let mut p = pending.lock().unwrap();
        p.drain().collect()
    };

    if !leftover.is_empty() {
        for (id, _) in leftover {
            let error_resp = json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32000,
                    "message": format!("Upstream MCP server terminated unexpectedly ({status})")
                }
            });
            let _ = write_json_value(&mut stdout_lock, &error_resp);
        }
    }

    Ok(())
}

/// Internal mock MCP server used for testing proxy behaviors without network.
pub fn run_mock_mcp_server(scenario: &str) -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();

    let mut lines_iter = stdin.lock().lines();

    if scenario == "concurrent" {
        let mut reqs = Vec::new();
        for line in lines_iter.by_ref() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(req) = serde_json::from_str::<Value>(&line) {
                reqs.push(req);
                if reqs.len() >= 3 {
                    break;
                }
            }
        }
        // Respond to reqs in reverse order: index 1, index 2, index 0
        for req in reqs.into_iter().rev() {
            let id = req.get("id").cloned().unwrap_or(Value::Null);
            let method = req.get("method").and_then(Value::as_str).unwrap_or("");
            let resp = match method {
                "tools/list" => json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "tools": [{"name": "tool1"}] }
                }),
                "tools/call" => {
                    let tool_name = req
                        .get("params")
                        .and_then(|p| p.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if tool_name == "large_tool" {
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "content": [{ "type": "text", "text": generate_mock_large_text() }]
                            }
                        })
                    } else {
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "content": [{ "type": "text", "text": "compact result" }]
                            }
                        })
                    }
                }
                _ => json!({"jsonrpc":"2.0","id":id,"result":{}}),
            };
            write_json_value(&mut stdout_lock, &resp)?;
        }
        return Ok(());
    }

    for line in lines_iter {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req.get("method").and_then(Value::as_str).unwrap_or("");

        match method {
            "initialize" => {
                let resp = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": { "tools": {} },
                        "serverInfo": { "name": "mock-mcp-server", "version": "1.0.0" }
                    }
                });
                write_json_value(&mut stdout_lock, &resp)?;
            }
            "tools/list" => {
                let resp = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "tools": [
                            {
                                "name": "large_tool",
                                "description": "Returns a large compressible response",
                                "inputSchema": { "type": "object" }
                            },
                            {
                                "name": "compact_tool",
                                "description": "Returns a compact response",
                                "inputSchema": { "type": "object" }
                            },
                            {
                                "name": "image_tool",
                                "description": "Returns an image",
                                "inputSchema": { "type": "object" }
                            },
                            {
                                "name": "crash_tool",
                                "description": "Crashes the server immediately",
                                "inputSchema": { "type": "object" }
                            }
                        ]
                    }
                });
                write_json_value(&mut stdout_lock, &resp)?;
            }
            "tools/call" => {
                let name = req
                    .get("params")
                    .and_then(|p| p.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if scenario == "die" || name == "crash_tool" {
                    std::process::exit(1);
                }
                let resp = match name {
                    "large_tool" => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [{
                                "type": "text",
                                "text": generate_mock_large_text()
                            }]
                        }
                    }),
                    "compact_tool" => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [{
                                "type": "text",
                                "text": "compact result below threshold"
                            }]
                        }
                    }),
                    "image_tool" => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [{
                                "type": "image",
                                "data": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
                                "mimeType": "image/png"
                            }]
                        }
                    }),
                    _ => {
                        if scenario == "large" {
                            json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": {
                                    "content": [{
                                        "type": "text",
                                        "text": generate_mock_large_text()
                                    }]
                                }
                            })
                        } else if scenario == "compact" {
                            json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": {
                                    "content": [{
                                        "type": "text",
                                        "text": "compact result below threshold"
                                    }]
                                }
                            })
                        } else if scenario == "image" {
                            json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": {
                                    "content": [{
                                        "type": "image",
                                        "data": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
                                        "mimeType": "image/png"
                                    }]
                                }
                            })
                        } else {
                            json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": {
                                    "content": [{
                                        "type": "text",
                                        "text": "default response"
                                    }]
                                }
                            })
                        }
                    }
                };
                write_json_value(&mut stdout_lock, &resp)?;
            }
            _ => {
                let resp = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32601, "message": "method not found" }
                });
                write_json_value(&mut stdout_lock, &resp)?;
            }
        }
    }

    Ok(())
}

fn generate_mock_large_text() -> String {
    let mut items = Vec::new();
    for i in 0..50 {
        items.push(json!({
            "index": i,
            "status": "PASS",
            "message": "Task executed successfully with repetitive log output for testing context compression",
            "timestamp": "2026-09-18T06:00:00Z",
            "tags": ["test", "ci", "build", "logging"]
        }));
    }
    serde_json::to_string_pretty(&items).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_resizer_core::ccr::InMemoryCcrStore;

    // ----------------------------------------------------------------
    // Contre-vérification : la propriété qui rend le proxy sûr est que la
    // compression marche par liste d'AUTORISATION. Seul tools/call est touché.
    // Tout le reste passe intact, y compris ce qu'on n'a pas su interpréter.
    // Ces tests fixent cette propriété pour qu'on ne l'inverse pas un jour par
    // commodité.
    // ----------------------------------------------------------------

    #[test]
    fn une_reponse_sans_requete_connue_passe_intacte() {
        // Cas réel : le proxy a redémarré, ou l'agent avait envoyé sa requête
        // avant lui. La méthode est alors inconnue — et une réponse dont on
        // ignore la nature ne doit surtout pas être compressée : elle pourrait
        // être un tools/list, dont la troncature casserait l'agent.
        let store = InMemoryCcrStore::new();
        let pipeline = build_pipeline();
        let pending = Mutex::new(HashMap::new());

        let enorme = "x".repeat(200_000);
        let resp = json!({
            "jsonrpc": "2.0",
            "id": 4242,
            "result": {"content": [{"type": "text", "text": enorme}]}
        })
        .to_string();

        // Aucune requête enregistrée pour cet identifiant.
        let outcome = process_upstream_line(&resp, &pending, &store, &pipeline);
        assert_eq!(
            outcome,
            ProcessOutcome::Unmodified,
            "une reponse dont on ignore la methode doit passer intacte, meme enorme"
        );
    }

    #[test]
    fn un_tools_list_non_enregistre_passe_intact() {
        let store = InMemoryCcrStore::new();
        let pipeline = build_pipeline();
        let pending = Mutex::new(HashMap::new());

        let outils: Vec<Value> = (0..300)
            .map(|n| {
                json!({
                    "name": format!("outil_{n}"),
                    "description": "description deliberement longue ".repeat(20),
                    "inputSchema": {"type": "object"}
                })
            })
            .collect();
        let resp = json!({"jsonrpc":"2.0","id":7,"result":{"tools": outils}}).to_string();

        assert_eq!(
            process_upstream_line(&resp, &pending, &store, &pipeline),
            ProcessOutcome::Unmodified
        );
    }

    #[test]
    fn un_identifiant_de_type_different_n_est_pas_confondu() {
        // L'identifiant 1 (nombre) et "1" (chaine) sont deux requetes
        // distinctes selon JSON-RPC. Les confondre reviendrait a appliquer a
        // une reponse la methode d'une autre.
        let store = InMemoryCcrStore::new();
        let pipeline = build_pipeline();
        let pending = Mutex::new(HashMap::new());

        let req = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
                         "params":{"name":"un_outil","arguments":{}}})
        .to_string();
        process_agent_line(&req, &pending);

        let resp = json!({
            "jsonrpc":"2.0","id":"1",
            "result":{"content":[{"type":"text","text":"y".repeat(200_000)}]}
        })
        .to_string();

        assert_eq!(
            process_upstream_line(&resp, &pending, &store, &pipeline),
            ProcessOutcome::Unmodified,
            "un identifiant chaine ne doit pas consommer la requete numerique"
        );
    }

    #[test]
    fn une_ligne_illisible_passe_au_lieu_d_etre_perdue() {
        // Un proxy qui avale ce qu'il ne comprend pas fait disparaitre des
        // reponses. Passer sans toucher est toujours le bon choix.
        let store = InMemoryCcrStore::new();
        let pipeline = build_pipeline();
        let pending = Mutex::new(HashMap::new());

        for ligne in [
            "{ ceci n'est pas du json",
            "null",
            "[]",
            "\u{feff}{\"a\":1}",
        ] {
            let outcome = process_upstream_line(ligne, &pending, &store, &pipeline);
            assert!(
                matches!(outcome, ProcessOutcome::Unmodified | ProcessOutcome::Ignore),
                "ligne avalee : {ligne}"
            );
        }
    }

    #[test]
    fn une_erreur_sur_tools_call_passe_intacte() {
        let store = InMemoryCcrStore::new();
        let pipeline = build_pipeline();
        let pending = Mutex::new(HashMap::new());

        let req = json!({"jsonrpc":"2.0","id":9,"method":"tools/call",
                         "params":{"name":"un_outil","arguments":{}}})
        .to_string();
        process_agent_line(&req, &pending);

        let resp = json!({
            "jsonrpc":"2.0","id":9,
            "error":{"code":-32000,"message":"echec cote serveur","data":"z".repeat(100_000)}
        })
        .to_string();

        assert_eq!(
            process_upstream_line(&resp, &pending, &store, &pipeline),
            ProcessOutcome::Unmodified,
            "une erreur doit rester lisible telle quelle"
        );
    }

    #[test]
    fn test_tools_list_response_intact() {
        let store = InMemoryCcrStore::new();
        let pipeline = build_pipeline();
        let pending = Mutex::new(HashMap::new());

        let req = json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}).to_string();
        process_agent_line(&req, &pending);

        let resp = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "tools": [
                    {
                        "name": "any_tool",
                        "description": "Any tool description",
                        "inputSchema": { "type": "object" }
                    }
                ]
            }
        })
        .to_string();

        let outcome = process_upstream_line(&resp, &pending, &store, &pipeline);
        assert_eq!(outcome, ProcessOutcome::Unmodified);
    }

    #[test]
    fn test_initialize_response_intact() {
        let store = InMemoryCcrStore::new();
        let pipeline = build_pipeline();
        let pending = Mutex::new(HashMap::new());

        let req =
            json!({"jsonrpc":"2.0","id":"init-1","method":"initialize","params":{}}).to_string();
        process_agent_line(&req, &pending);

        let resp = json!({
            "jsonrpc": "2.0",
            "id": "init-1",
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "server", "version": "1.0" }
            }
        })
        .to_string();

        let outcome = process_upstream_line(&resp, &pending, &store, &pipeline);
        assert_eq!(outcome, ProcessOutcome::Unmodified);
    }

    #[test]
    fn test_large_tool_call_compressed_and_recoverable() {
        let store = InMemoryCcrStore::new();
        let pipeline = build_pipeline();
        let pending = Mutex::new(HashMap::new());

        let req = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "method": "tools/call",
            "params": {
                "name": "large_tool",
                "arguments": { "query": "test" }
            }
        })
        .to_string();
        process_agent_line(&req, &pending);

        let original_text = generate_mock_large_text();
        assert!(original_text.len() > 2000);

        let resp = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "content": [
                    {
                        "type": "text",
                        "text": original_text
                    }
                ],
                "isError": false
            }
        })
        .to_string();

        let outcome = process_upstream_line(&resp, &pending, &store, &pipeline);
        let ProcessOutcome::Modified(mod_str) = outcome else {
            panic!("expected ProcessOutcome::Modified for large tool response");
        };

        let parsed: Value =
            serde_json::from_str(&mod_str).expect("modified response must be valid JSON");
        assert_eq!(parsed["id"], 42);
        assert_eq!(parsed["result"]["isError"], false);

        let compressed_text = parsed["result"]["content"][0]["text"]
            .as_str()
            .expect("text field must be string");

        // Must be strictly smaller
        assert!(
            compressed_text.len() < original_text.len(),
            "compressed length {} must be < original length {}",
            compressed_text.len(),
            original_text.len()
        );

        // Point 3: Must include retrieval hint and marker
        assert!(
            compressed_text.contains("<<ccr:"),
            "compressed text must contain <<ccr: marker"
        );
        assert!(
            compressed_text.contains("[full output: <<ccr:"),
            "compressed text must contain [full output: <<ccr: hint"
        );

        // Point 3: Full original must be recoverable from CCR store
        let hash = compute_key(original_text.as_bytes());
        let retrieved = store
            .get(&hash)
            .expect("original must be recoverable from CCR store");
        assert_eq!(retrieved, original_text);
    }

    #[test]
    fn test_compact_response_unchanged_and_no_growth() {
        let store = InMemoryCcrStore::new();
        let pipeline = build_pipeline();
        let pending = Mutex::new(HashMap::new());

        let req = json!({
            "jsonrpc": "2.0",
            "id": 100,
            "method": "tools/call",
            "params": {
                "name": "compact_tool",
                "arguments": {}
            }
        })
        .to_string();
        process_agent_line(&req, &pending);

        // Compact response (< 512 bytes)
        let resp = json!({
            "jsonrpc": "2.0",
            "id": 100,
            "result": {
                "content": [
                    {
                        "type": "text",
                        "text": "Status: ok. Everything healthy."
                    }
                ]
            }
        })
        .to_string();

        let outcome = process_upstream_line(&resp, &pending, &store, &pipeline);
        assert_eq!(outcome, ProcessOutcome::Unmodified);

        // 1051-byte already-compact / uncompressible response (Astra's measurement)
        let req2 = json!({
            "jsonrpc": "2.0",
            "id": 101,
            "method": "tools/call",
            "params": {
                "name": "context",
                "arguments": { "name": "symbol" }
            }
        })
        .to_string();
        process_agent_line(&req2, &pending);

        let text_1051 = "x".repeat(1051);
        let resp2 = json!({
            "jsonrpc": "2.0",
            "id": 101,
            "result": {
                "content": [
                    {
                        "type": "text",
                        "text": text_1051
                    }
                ]
            }
        })
        .to_string();

        let outcome2 = process_upstream_line(&resp2, &pending, &store, &pipeline);
        assert_eq!(outcome2, ProcessOutcome::Unmodified);
    }

    #[test]
    fn test_non_text_content_intact() {
        let store = InMemoryCcrStore::new();
        let pipeline = build_pipeline();
        let pending = Mutex::new(HashMap::new());

        let req = json!({
            "jsonrpc": "2.0",
            "id": 200,
            "method": "tools/call",
            "params": {
                "name": "image_tool",
                "arguments": {}
            }
        })
        .to_string();
        process_agent_line(&req, &pending);

        let resp = json!({
            "jsonrpc": "2.0",
            "id": 200,
            "result": {
                "content": [
                    {
                        "type": "image",
                        "data": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
                        "mimeType": "image/png"
                    }
                ]
            }
        })
        .to_string();

        let outcome = process_upstream_line(&resp, &pending, &store, &pipeline);
        assert_eq!(outcome, ProcessOutcome::Unmodified);
    }

    #[test]
    fn test_concurrent_requests_preserved_and_not_swapped() {
        let store = InMemoryCcrStore::new();
        let pipeline = build_pipeline();
        let pending = Mutex::new(HashMap::new());

        // Agent sends 3 requests: req-A, req-B, req-C
        let req_a = json!({"jsonrpc":"2.0","id":"req-A","method":"tools/call","params":{"name":"large_tool"}}).to_string();
        let req_b = json!({"jsonrpc":"2.0","id":"req-B","method":"tools/call","params":{"name":"compact_tool"}}).to_string();
        let req_c =
            json!({"jsonrpc":"2.0","id":"req-C","method":"tools/list","params":{}}).to_string();

        process_agent_line(&req_a, &pending);
        process_agent_line(&req_b, &pending);
        process_agent_line(&req_c, &pending);

        // Server responds in reverse order: B, then C, then A
        let resp_b = json!({
            "jsonrpc": "2.0",
            "id": "req-B",
            "result": { "content": [{ "type": "text", "text": "compact result" }] }
        })
        .to_string();

        let resp_c = json!({
            "jsonrpc": "2.0",
            "id": "req-C",
            "result": { "tools": [{ "name": "some_tool" }] }
        })
        .to_string();

        let resp_a = json!({
            "jsonrpc": "2.0",
            "id": "req-A",
            "result": { "content": [{ "type": "text", "text": generate_mock_large_text() }] }
        })
        .to_string();

        let out_b = process_upstream_line(&resp_b, &pending, &store, &pipeline);
        assert_eq!(out_b, ProcessOutcome::Unmodified);

        let out_c = process_upstream_line(&resp_c, &pending, &store, &pipeline);
        assert_eq!(out_c, ProcessOutcome::Unmodified);

        let out_a = process_upstream_line(&resp_a, &pending, &store, &pipeline);
        match out_a {
            ProcessOutcome::Modified(a_str) => {
                let parsed: Value = serde_json::from_str(&a_str).unwrap();
                assert_eq!(parsed["id"], "req-A");
                let text = parsed["result"]["content"][0]["text"].as_str().unwrap();
                assert!(text.contains("[full output: <<ccr:"));
            }
            _ => panic!("req-A must be modified (compressed)"),
        }
    }

    #[test]
    fn test_server_error_response_passes_unchanged() {
        let store = InMemoryCcrStore::new();
        let pipeline = build_pipeline();
        let pending = Mutex::new(HashMap::new());

        let req =
            json!({"jsonrpc":"2.0","id":999,"method":"tools/call","params":{"name":"err_tool"}})
                .to_string();
        process_agent_line(&req, &pending);

        let err_resp = json!({
            "jsonrpc": "2.0",
            "id": 999,
            "error": {
                "code": -32602,
                "message": "Invalid parameters"
            }
        })
        .to_string();

        let outcome = process_upstream_line(&err_resp, &pending, &store, &pipeline);
        assert_eq!(outcome, ProcessOutcome::Unmodified);
    }

    #[test]
    fn test_subprocess_mock_server_crash_in_middle_of_exchange() {
        let exe = match option_env!("CARGO_BIN_EXE_lm-resizer") {
            Some(path) => PathBuf::from(path),
            None => {
                // Fallback to target/debug or target/release
                let mut p = std::env::current_exe().unwrap();
                p.pop();
                if p.ends_with("deps") {
                    p.pop();
                }
                p.join("lm-resizer")
            }
        };
        if !exe.exists() {
            return;
        }

        let mut proxy = Command::new(&exe)
            .args([
                "mcp-proxy",
                "--",
                exe.to_str().unwrap(),
                "mock-mcp-server",
                "--scenario",
                "die",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("must spawn mcp-proxy");

        let mut stdin = proxy.stdin.take().unwrap();
        let stdout = proxy.stdout.take().unwrap();
        let mut reader = BufReader::new(stdout);

        // 1. Initialize succeeds
        let init_req =
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}).to_string();
        stdin.write_all(init_req.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();

        let mut init_res = String::new();
        reader.read_line(&mut init_res).unwrap();
        let parsed_init: Value = serde_json::from_str(&init_res).unwrap();
        assert_eq!(parsed_init["id"], 1);

        // 2. tools/call causes upstream to die
        let call_req =
            json!({"jsonrpc":"2.0","id":99,"method":"tools/call","params":{"name":"any"}})
                .to_string();
        stdin.write_all(call_req.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();

        // Proxy must return a clean error with id=99 instead of hanging
        let mut call_res = String::new();
        reader.read_line(&mut call_res).unwrap();
        let parsed_call: Value =
            serde_json::from_str(&call_res).expect("proxy must emit valid JSON error response");
        assert_eq!(parsed_call["id"], 99);
        assert!(
            parsed_call.get("error").is_some(),
            "response must have error field"
        );
        assert_eq!(parsed_call["error"]["code"], -32000);

        let status = proxy.wait().expect("proxy must exit without hanging");
        assert!(status.success());
    }

    #[test]
    fn test_subprocess_mock_server_all_scenarios() {
        let exe = match option_env!("CARGO_BIN_EXE_lm-resizer") {
            Some(path) => PathBuf::from(path),
            None => {
                let mut p = std::env::current_exe().unwrap();
                p.pop();
                if p.ends_with("deps") {
                    p.pop();
                }
                p.join("lm-resizer")
            }
        };
        if !exe.exists() {
            return;
        }

        let mut proxy = Command::new(&exe)
            .args([
                "mcp-proxy",
                "--",
                exe.to_str().unwrap(),
                "mock-mcp-server",
                "--scenario",
                "all",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("must spawn mcp-proxy");

        let mut stdin = proxy.stdin.take().unwrap();
        let stdout = proxy.stdout.take().unwrap();
        let mut reader = BufReader::new(stdout);

        // 1. tools/list intact
        let req1 = json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}).to_string();
        stdin.write_all(req1.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();

        let mut line1 = String::new();
        reader.read_line(&mut line1).unwrap();
        let res1: Value = serde_json::from_str(&line1).unwrap();
        assert_eq!(res1["id"], 1);
        assert!(res1["result"]["tools"].as_array().unwrap().len() >= 4);

        // 2. large_tool compressed + CCR
        let req2 =
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"large_tool"}})
                .to_string();
        stdin.write_all(req2.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();

        let mut line2 = String::new();
        reader.read_line(&mut line2).unwrap();
        let res2: Value = serde_json::from_str(&line2).unwrap();
        assert_eq!(res2["id"], 2);
        let text2 = res2["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text2.contains("<<ccr:"));
        assert!(text2.contains("[full output: <<ccr:"));

        // 3. compact_tool unchanged
        let req3 =
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"compact_tool"}})
                .to_string();
        stdin.write_all(req3.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();

        let mut line3 = String::new();
        reader.read_line(&mut line3).unwrap();
        let res3: Value = serde_json::from_str(&line3).unwrap();
        assert_eq!(res3["id"], 3);
        assert_eq!(
            res3["result"]["content"][0]["text"],
            "compact result below threshold"
        );

        // 4. image_tool unchanged
        let req4 =
            json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"image_tool"}})
                .to_string();
        stdin.write_all(req4.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();

        let mut line4 = String::new();
        reader.read_line(&mut line4).unwrap();
        let res4: Value = serde_json::from_str(&line4).unwrap();
        assert_eq!(res4["id"], 4);
        assert_eq!(res4["result"]["content"][0]["type"], "image");

        drop(stdin);
        let status = proxy.wait().unwrap();
        assert!(status.success());
    }

    #[test]
    fn test_subprocess_mock_server_concurrent_requests() {
        let exe = match option_env!("CARGO_BIN_EXE_lm-resizer") {
            Some(path) => PathBuf::from(path),
            None => {
                let mut p = std::env::current_exe().unwrap();
                p.pop();
                if p.ends_with("deps") {
                    p.pop();
                }
                p.join("lm-resizer")
            }
        };
        if !exe.exists() {
            return;
        }

        let mut proxy = Command::new(&exe)
            .args([
                "mcp-proxy",
                "--",
                exe.to_str().unwrap(),
                "mock-mcp-server",
                "--scenario",
                "concurrent",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("must spawn mcp-proxy");

        let mut stdin = proxy.stdin.take().unwrap();
        let stdout = proxy.stdout.take().unwrap();
        let mut reader = BufReader::new(stdout);

        let req1 = json!({"jsonrpc":"2.0","id":"req-1","method":"tools/call","params":{"name":"large_tool"}}).to_string();
        let req2 = json!({"jsonrpc":"2.0","id":"req-2","method":"tools/call","params":{"name":"compact_tool"}}).to_string();
        let req3 =
            json!({"jsonrpc":"2.0","id":"req-3","method":"tools/list","params":{}}).to_string();

        // Write all 3 concurrently before reading responses
        stdin.write_all(req1.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.write_all(req2.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.write_all(req3.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();

        // Expect responses for req-3, req-2, req-1 (in reverse order)
        let mut l1 = String::new();
        reader.read_line(&mut l1).unwrap();
        let p1: Value = serde_json::from_str(&l1).unwrap();
        assert_eq!(p1["id"], "req-3");
        assert!(p1["result"].get("tools").is_some());

        let mut l2 = String::new();
        reader.read_line(&mut l2).unwrap();
        let p2: Value = serde_json::from_str(&l2).unwrap();
        assert_eq!(p2["id"], "req-2");
        assert_eq!(p2["result"]["content"][0]["text"], "compact result");

        let mut l3 = String::new();
        reader.read_line(&mut l3).unwrap();
        let p3: Value = serde_json::from_str(&l3).unwrap();
        assert_eq!(p3["id"], "req-1");
        let t3 = p3["result"]["content"][0]["text"].as_str().unwrap();
        assert!(t3.contains("[full output: <<ccr:"));

        drop(stdin);
        let status = proxy.wait().unwrap();
        assert!(status.success());
    }
}
