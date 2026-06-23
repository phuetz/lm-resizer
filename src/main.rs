use std::io::{self, BufRead, Read, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use axum::body::{Body, Bytes};
use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{OriginalUri, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use clap::{Parser, Subcommand, ValueEnum};
use flate2::read::{GzDecoder, ZlibDecoder};
use futures_util::{SinkExt, StreamExt};
use lm_resizer_core::ccr::{from_config, CcrBackendConfig, CcrStore, InMemoryCcrStore};
use lm_resizer_core::compute_frozen_count;
use lm_resizer_core::default_pipeline;
use lm_resizer_core::transforms::{
    compress_anthropic_live_zone_with_ccr, compress_openai_chat_live_zone,
    compress_openai_responses_live_zone, detect_content_type, AuthMode, CompressionContext,
    CompressionManifest, CompressionPipeline, LiveZoneOutcome,
};
use rayon::prelude::*;
use regex::{Regex, RegexSet};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::Client;
use ring::rand::SystemRandom;
use ring::signature::{RsaKeyPair, RSA_PKCS1_SHA256};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message as TungsteniteMessage;
use walkdir::WalkDir;

#[derive(Parser)]
#[command(name = "lm-resizer")]
#[command(about = "Rust-native context compression for LLM agents")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compress stdin or a file and persist originals for CCR retrieval.
    Compress {
        /// Input file. Reads stdin when omitted.
        #[arg(short, long)]
        input: Option<PathBuf>,
        /// User query used by relevance-aware compressors.
        #[arg(short, long, default_value = "")]
        query: String,
        /// Token budget: force lossy row-dropping so the output fits ~N tokens,
        /// keeping the rows most relevant to --query. Omit for lossless-first.
        #[arg(long)]
        token_budget: Option<usize>,
        /// Emit JSON metadata instead of raw compressed text.
        #[arg(long)]
        json: bool,
        /// CCR SQLite database path.
        #[arg(long)]
        store: Option<PathBuf>,
    },
    /// Compress many files in parallel.
    Batch {
        /// Files or directories to process.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Recurse into directories.
        #[arg(short, long)]
        recursive: bool,
        /// Limit worker threads. Defaults to Rayon's global pool.
        #[arg(short, long)]
        jobs: Option<usize>,
        /// Comma-separated extension allowlist, for example: log,json,diff,txt.
        #[arg(long, value_delimiter = ',')]
        ext: Vec<String>,
        /// User query used by relevance-aware compressors.
        #[arg(short, long, default_value = "")]
        query: String,
        /// Write compressed outputs into this directory.
        #[arg(long)]
        write_dir: Option<PathBuf>,
        /// Emit JSON summary.
        #[arg(long)]
        json: bool,
        /// CCR SQLite database path.
        #[arg(long)]
        store: Option<PathBuf>,
    },
    /// Execute a command, apply RTK-style output filtering, then run the compression pipeline.
    Exec {
        /// User query used by relevance-aware compressors.
        #[arg(short, long, default_value = "")]
        query: String,
        /// Emit JSON metadata instead of raw compressed text.
        #[arg(long)]
        json: bool,
        /// CCR SQLite database path.
        #[arg(long)]
        store: Option<PathBuf>,
        /// Print raw command output when the child exits non-zero.
        #[arg(long)]
        raw_on_failure: bool,
        /// Stream child output live, then emit the filtered/compressed result after exit.
        #[arg(long)]
        stream: bool,
        /// Command and arguments to execute. Use `--` before commands with flags.
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Show how a shell command would be routed through `lm-resizer exec`.
    Rewrite {
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
        /// Command and arguments to inspect. Use `--` before commands with flags.
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Rewrite a full shell command line without executing it.
    RewriteShell {
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
        /// Shell command line to inspect.
        command: String,
    },
    /// Retrieve an original payload by CCR hash.
    Retrieve {
        hash: String,
        /// CCR SQLite database path.
        #[arg(long)]
        store: Option<PathBuf>,
    },
    /// Show CCR store statistics.
    Stats {
        /// CCR SQLite database path.
        #[arg(long)]
        store: Option<PathBuf>,
        /// Emit a Markdown stats summary.
        #[arg(long)]
        markdown: bool,
    },
    /// Inspect image payload size and dimensions for context-budget decisions.
    Image {
        /// Image file to inspect.
        input: PathBuf,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Analyze or clean voice transcript filler words.
    Voice {
        /// Transcript file. Reads stdin when omitted.
        #[arg(short, long)]
        input: Option<PathBuf>,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
        /// Print cleaned transcript instead of a human summary.
        #[arg(long)]
        clean: bool,
    },
    /// Report optional ML classifier/model configuration.
    MlStatus {
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Manage raw output recovery files created by exec.
    Tee {
        #[command(subcommand)]
        command: TeeCommand,
    },
    /// Trust a project-local `.lm-resizer/filters.toml` file.
    TrustFilters {
        /// Filter file to trust.
        #[arg(long, default_value = ".lm-resizer/filters.toml")]
        path: PathBuf,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// List trusted project filter files.
    ListTrustedFilters {
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Remove a project filter file from the trust registry.
    UntrustFilters {
        /// Filter file to untrust.
        #[arg(long, default_value = ".lm-resizer/filters.toml")]
        path: PathBuf,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Show a readable audit of a TOML filter file before trusting it.
    AuditFilters {
        /// Filter file to audit.
        #[arg(long, default_value = ".lm-resizer/filters.toml")]
        path: PathBuf,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
        /// Emit a review-ready Markdown report.
        #[arg(long)]
        review: bool,
    },
    /// Validate a TOML filter file and run its inline tests.
    VerifyFilters {
        /// Filter file to verify.
        #[arg(long, default_value = ".lm-resizer/filters.toml")]
        path: PathBuf,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Create a starter project filter file with inline tests.
    InitFilters {
        /// Filter file to create.
        #[arg(long, default_value = ".lm-resizer/filters.toml")]
        path: PathBuf,
        /// Starter profile to write.
        #[arg(long, value_enum, default_value_t = FilterProfile::Generic)]
        profile: FilterProfile,
        /// Overwrite an existing file.
        #[arg(long)]
        force: bool,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Sanitize a real provider payload into a shareable fixture JSON file.
    SanitizeProviderFixture {
        /// Provider kind: openai, anthropic, bedrock, or vertex.
        #[arg(long)]
        provider: ProviderKind,
        /// Input JSON payload.
        #[arg(long)]
        input: PathBuf,
        /// Output JSON fixture path.
        #[arg(long)]
        output: PathBuf,
        /// Replace strings at or above this byte length with a placeholder.
        #[arg(long, default_value_t = 256)]
        max_string: usize,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Analyze logs/session files for commands that lm-resizer exec can reduce.
    Discover {
        /// Files or directories to scan.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Recurse into directories.
        #[arg(short, long)]
        recursive: bool,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
        /// Emit a Markdown audit summary.
        #[arg(long)]
        markdown: bool,
    },
    /// Discover compressible command output in known Claude/Codex session stores.
    DiscoverSessions {
        /// Agent session store to scan.
        #[arg(long, value_enum, default_value_t = AgentSessionKind::All)]
        agent: AgentSessionKind,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
        /// Emit a Markdown audit summary.
        #[arg(long)]
        markdown: bool,
    },
    /// Run a lightweight evaluation harness over session/log fixtures.
    Eval {
        /// Files or directories to evaluate.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Recurse into directories.
        #[arg(short, long)]
        recursive: bool,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
        /// Emit Markdown report.
        #[arg(long)]
        markdown: bool,
    },
    /// Mine sessions/history and propose durable AGENTS.md / CLAUDE.md guidance.
    Learn {
        /// Files or directories to scan.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Recurse into directories.
        #[arg(short, long)]
        recursive: bool,
        /// Project directory where `.lm-resizer/learning` is written.
        #[arg(long)]
        project_dir: Option<PathBuf>,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
        /// Emit Markdown guidance only.
        #[arg(long)]
        markdown: bool,
        /// Write recommendations into `.lm-resizer/learning`.
        #[arg(long)]
        write: bool,
        /// Also install a reversible learning block into AGENTS.md / CLAUDE.md.
        #[arg(long)]
        install: bool,
        /// Agent file to update when --install is used: codex, claude, or all.
        #[arg(long, default_value = "all")]
        client: String,
    },
    /// Generate local hook helper scripts for agent command rewriting.
    InitHooks {
        /// Project directory where `.lm-resizer/hooks` will be written.
        #[arg(long)]
        project_dir: Option<PathBuf>,
        /// Overwrite existing hook helper files.
        #[arg(long)]
        force: bool,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Install native Codex/Claude hook config that calls `lm-resizer hook`.
    InitNativeHooks {
        /// Agent to configure: codex, claude, or all.
        #[arg(long, default_value = "all")]
        client: String,
        /// Project directory where native hook config will be written.
        #[arg(long)]
        project_dir: Option<PathBuf>,
        /// Overwrite existing native hook config files.
        #[arg(long)]
        force: bool,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Native Codex/Claude hook handler. Reads event JSON from stdin and never blocks.
    Hook {
        /// Agent client name.
        #[arg(long, default_value = "unknown")]
        client: String,
        /// Hook event name.
        #[arg(long, default_value = "unknown")]
        event: String,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Generate opt-in PATH shims that automatically route known commands through exec.
    InitShims {
        /// Project directory where `.lm-resizer/shims` will be written.
        #[arg(long)]
        project_dir: Option<PathBuf>,
        /// Overwrite existing shim files.
        #[arg(long)]
        force: bool,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Install reversible project agent instructions for hook helpers.
    InstallHooks {
        /// Agent to configure: codex, claude, or all.
        #[arg(long, default_value = "codex")]
        client: String,
        /// Project directory containing AGENTS.md / CLAUDE.md.
        #[arg(long)]
        project_dir: Option<PathBuf>,
        /// Overwrite existing generated helper files.
        #[arg(long)]
        force: bool,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Remove generated lm-resizer hook instructions from project agent files.
    UninstallHooks {
        /// Agent to unconfigure: codex, claude, or all.
        #[arg(long, default_value = "codex")]
        client: String,
        /// Project directory containing AGENTS.md / CLAUDE.md.
        #[arg(long)]
        project_dir: Option<PathBuf>,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Diagnose local lm-resizer setup.
    Doctor {
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
        /// CCR SQLite database path.
        #[arg(long)]
        store: Option<PathBuf>,
    },
    /// Run a minimal MCP stdio server.
    Mcp {
        /// CCR SQLite database path.
        #[arg(long)]
        store: Option<PathBuf>,
    },
    /// Install lm-resizer as an MCP server for common agent clients.
    Install {
        /// Client to configure: claude, codex, cursor, vscode, all.
        #[arg(long, default_value = "claude")]
        client: String,
        /// Installation scope: project or global.
        #[arg(long, default_value = "project")]
        scope: String,
        /// Project directory for project-scoped config files.
        #[arg(long)]
        project_dir: Option<PathBuf>,
        /// CCR SQLite database path passed to the MCP server.
        #[arg(long)]
        store: Option<PathBuf>,
    },
    /// Run a small HTTP API.
    Serve {
        #[arg(long, default_value = "127.0.0.1:8787")]
        bind: SocketAddr,
        /// Optional OpenAI-compatible upstream base URL.
        #[arg(long, env = "LM_RESIZER_UPSTREAM")]
        upstream: Option<String>,
        /// Optional bearer token for the upstream provider.
        #[arg(long, env = "LM_RESIZER_API_KEY")]
        api_key: Option<String>,
        /// Upstream provider header mode: openai, anthropic, bedrock, or vertex.
        #[arg(long, env = "LM_RESIZER_PROVIDER", default_value = "openai")]
        provider: String,
        /// CCR SQLite database path.
        #[arg(long)]
        store: Option<PathBuf>,
        /// Enable local HTML dashboard at /dashboard.
        #[arg(long)]
        dashboard: bool,
    },
    /// Start the local proxy, then launch an agent through it.
    Wrap {
        /// Agent command to launch: claude, codex, cursor, opencode, openclaw, aider, copilot, or a custom binary.
        agent: String,
        /// Arguments passed after the agent command.
        #[arg(last = true)]
        args: Vec<String>,
        /// Proxy bind address.
        #[arg(long, default_value = "127.0.0.1:8787")]
        bind: SocketAddr,
        /// Upstream provider base URL used by the proxy.
        #[arg(long, env = "LM_RESIZER_UPSTREAM")]
        upstream: Option<String>,
        /// Optional bearer token for the upstream provider.
        #[arg(long, env = "LM_RESIZER_API_KEY")]
        api_key: Option<String>,
        /// Upstream provider header mode: openai, anthropic, bedrock, or vertex.
        #[arg(long, env = "LM_RESIZER_PROVIDER", default_value = "openai")]
        provider: String,
        /// CCR SQLite database path.
        #[arg(long)]
        store: Option<PathBuf>,
        /// Kill the wrapped agent after this many seconds. Omit for no timeout.
        #[arg(long)]
        timeout_sec: Option<u64>,
    },
}

#[derive(Subcommand)]
enum TeeCommand {
    /// List raw output recovery files.
    List {
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Print one raw output recovery file by filename or path.
    Read {
        /// Tee file name or path.
        file: String,
    },
    /// Delete raw output recovery files.
    Purge {
        /// Delete all tee files.
        #[arg(long)]
        all: bool,
        /// Delete one tee file by filename or path.
        #[arg(long)]
        file: Option<String>,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum AgentSessionKind {
    All,
    Codex,
    Claude,
}

impl AgentSessionKind {
    fn as_str(self) -> &'static str {
        match self {
            AgentSessionKind::All => "all",
            AgentSessionKind::Codex => "codex",
            AgentSessionKind::Claude => "claude",
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum FilterProfile {
    Generic,
    Rust,
    Node,
    Python,
    Infra,
}

#[derive(Debug, Serialize)]
struct CompressReport {
    content_type: String,
    original_bytes: usize,
    compressed_bytes: usize,
    bytes_saved: usize,
    steps_applied: Vec<String>,
    cache_keys: Vec<String>,
    output: String,
}

#[derive(Debug, Serialize)]
struct BatchReport {
    files: usize,
    ok: usize,
    failed: usize,
    original_bytes: usize,
    compressed_bytes: usize,
    bytes_saved: usize,
    items: Vec<BatchItemReport>,
}

#[derive(Debug, Serialize)]
struct BatchItemReport {
    path: String,
    ok: bool,
    content_type: Option<String>,
    original_bytes: Option<usize>,
    compressed_bytes: Option<usize>,
    bytes_saved: Option<usize>,
    steps_applied: Vec<String>,
    cache_keys: Vec<String>,
    output_path: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct ExecReport {
    command: String,
    exit_code: i32,
    filter: String,
    original_bytes: usize,
    filtered_bytes: usize,
    compressed_bytes: usize,
    bytes_saved: usize,
    compression_steps: Vec<String>,
    cache_keys: Vec<String>,
    tee_hint: Option<String>,
    output: String,
}

#[derive(Debug, Serialize)]
struct RewriteReport {
    command: String,
    supported: bool,
    filter: String,
    rewritten: Option<String>,
    argv: Vec<String>,
}

#[derive(Debug, Serialize)]
struct RewriteShellReport {
    command: String,
    changed: bool,
    rewritten: String,
    rewrites: Vec<RewriteShellSegment>,
}

#[derive(Debug, Serialize)]
struct RewriteShellSegment {
    original: String,
    rewritten: String,
    filter: String,
}

#[derive(Debug, Serialize)]
struct ExecHistoryRecord {
    timestamp_unix: u64,
    command: String,
    exit_code: i32,
    filter: String,
    original_bytes: usize,
    filtered_bytes: usize,
    compressed_bytes: usize,
    bytes_saved: usize,
    duration_ms: u128,
}

#[derive(Debug, Serialize)]
struct TeeListReport {
    directory: String,
    files: Vec<TeeFileReport>,
}

#[derive(Debug, Serialize)]
struct TeeFileReport {
    name: String,
    path: String,
    bytes: u64,
}

#[derive(Debug, Serialize)]
struct TeePurgeReport {
    deleted: usize,
    files: Vec<String>,
}

#[derive(Debug, Serialize)]
struct TrustFilterReport {
    path: String,
    hash: String,
    trusted: bool,
}

#[derive(Debug, Serialize)]
struct ListTrustedFiltersReport {
    entries: Vec<TrustedFilterRecord>,
}

#[derive(Debug, Serialize)]
struct UntrustFilterReport {
    path: String,
    removed: bool,
}

#[derive(Debug, Serialize)]
struct AuditFiltersReport {
    path: String,
    hash: String,
    trusted_hash: Option<String>,
    trust_status: String,
    filters: Vec<AuditFilterItem>,
    verification: VerifyFiltersReport,
}

#[derive(Debug, Serialize)]
struct AuditFilterItem {
    name: String,
    match_command: String,
    actions: Vec<String>,
}

#[derive(Debug, Serialize)]
struct VerifyFiltersReport {
    path: String,
    filters: usize,
    tests: usize,
    passed: usize,
    failed: usize,
    diagnostics: Vec<String>,
    outcomes: Vec<FilterTestOutcome>,
}

#[derive(Debug, Serialize)]
struct InitFiltersReport {
    path: String,
    written: bool,
    next_steps: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SanitizedProviderFixtureReport {
    provider: String,
    input: String,
    output: String,
    redacted_fields: usize,
    placeholder_strings: usize,
}

#[derive(Debug, Serialize)]
struct FilterTestOutcome {
    filter: String,
    name: String,
    passed: bool,
    expected: String,
    actual: String,
}

#[derive(Debug, Serialize, Default)]
struct DiscoverReport {
    files_scanned: usize,
    command_outputs: usize,
    rewritable_commands: usize,
    original_bytes: usize,
    filtered_bytes: usize,
    estimated_bytes_saved: usize,
    estimated_tokens_saved: usize,
    candidates: Vec<DiscoverCandidate>,
}

#[derive(Debug, Serialize)]
struct DiscoverCandidate {
    command: String,
    filter: String,
    original_bytes: usize,
    filtered_bytes: usize,
    estimated_bytes_saved: usize,
    source: String,
}

#[derive(Debug, Serialize)]
struct DiscoverSessionsReport {
    agent: String,
    paths: Vec<String>,
    missing: Vec<String>,
    discover: DiscoverReport,
}

#[derive(Debug, Serialize)]
struct ImageReport {
    path: String,
    bytes: u64,
    format: String,
    width: Option<u32>,
    height: Option<u32>,
    recommendation: String,
}

#[derive(Debug, Serialize)]
struct VoiceReport {
    original_chars: usize,
    cleaned_chars: usize,
    filler_count: usize,
    cleaned: String,
}

#[derive(Debug, Serialize)]
struct MlStatusReport {
    magika_enabled: bool,
    magika_model: Option<String>,
    onnx_runtime: String,
    hot_path: String,
}

#[derive(Debug, Serialize)]
struct EvalReport {
    files_scanned: usize,
    command_outputs: usize,
    candidates: usize,
    estimated_bytes_saved: usize,
    estimated_tokens_saved: usize,
    pass: bool,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct LearnReport {
    project_dir: String,
    files_scanned: usize,
    command_outputs: usize,
    recommendations: Vec<LearnRecommendation>,
    memory_file: Option<String>,
    instruction_files: Vec<String>,
    markdown: String,
}

#[derive(Debug, Serialize, Clone)]
struct LearnRecommendation {
    title: String,
    reason: String,
    instruction: String,
    evidence: Vec<String>,
}

#[derive(Debug, Serialize)]
struct InitHooksReport {
    directory: String,
    files: Vec<String>,
}

#[derive(Debug, Serialize)]
struct NativeHooksReport {
    project_dir: String,
    files: Vec<String>,
}

#[derive(Debug, Serialize)]
struct NativeHookRunReport {
    client: String,
    event: String,
    command_found: bool,
    output_found: bool,
    recorded: bool,
    filter: Option<String>,
    bytes_saved: usize,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct AgentHooksReport {
    helper_directory: String,
    instruction_files: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ShimReport {
    directory: String,
    files: Vec<String>,
    skipped: Vec<String>,
    path_hint: String,
}

#[derive(Debug, Serialize)]
struct UninstallHooksReport {
    instruction_files: Vec<String>,
    removed: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct TrustedFilterRecord {
    path: String,
    hash: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TomlFilterFile {
    #[serde(default)]
    filters: Vec<TomlFilterDef>,
    #[serde(default)]
    tests: Vec<TomlFilterTestDef>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TomlFilterDef {
    name: String,
    match_command: String,
    #[serde(default)]
    strip_ansi: bool,
    #[serde(default)]
    strip_lines_matching: Vec<String>,
    #[serde(default)]
    keep_lines_matching: Vec<String>,
    #[serde(default)]
    replace: Vec<TomlReplaceRule>,
    truncate_lines_at: Option<usize>,
    head_lines: Option<usize>,
    tail_lines: Option<usize>,
    max_lines: Option<usize>,
    on_empty: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TomlReplaceRule {
    pattern: String,
    replacement: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TomlFilterTestDef {
    filter: String,
    name: String,
    input: String,
    expected: String,
}

#[derive(Debug)]
struct CompiledTomlFilter {
    name: String,
    match_command: Regex,
    strip_ansi: bool,
    strip_lines_matching: Option<RegexSet>,
    keep_lines_matching: Option<RegexSet>,
    replace: Vec<(Regex, String)>,
    truncate_lines_at: Option<usize>,
    head_lines: Option<usize>,
    tail_lines: Option<usize>,
    max_lines: Option<usize>,
    on_empty: Option<String>,
}

struct BatchOptions {
    paths: Vec<PathBuf>,
    recursive: bool,
    jobs: Option<usize>,
    extensions: Vec<String>,
    query: String,
    write_dir: Option<PathBuf>,
    store: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    binary: String,
    store_path: String,
    store_ok: bool,
    mcp_tools: Vec<String>,
    clients: Vec<ClientCheck>,
}

#[derive(Debug, Serialize)]
struct ClientCheck {
    name: String,
    command: String,
    available: bool,
    version: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CompressRequest {
    content: String,
    #[serde(default)]
    query: String,
}

#[derive(Clone)]
struct AppState {
    store_path: PathBuf,
    upstream: Option<String>,
    api_key: Option<String>,
    provider: ProviderKind,
    client: Client,
    dashboard_enabled: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ProviderKind {
    #[value(name = "openai", alias = "openai-compatible", alias = "chatgpt")]
    OpenAi,
    #[value(alias = "claude", alias = "anthropic-compatible")]
    Anthropic,
    #[value(alias = "aws-bedrock")]
    Bedrock,
    #[value(alias = "vertex-ai", alias = "google-vertex")]
    Vertex,
}

impl std::str::FromStr for ProviderKind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "openai" | "openai-compatible" | "chatgpt" => Ok(Self::OpenAi),
            "anthropic" | "claude" => Ok(Self::Anthropic),
            "bedrock" | "aws-bedrock" => Ok(Self::Bedrock),
            "vertex" | "vertexai" | "vertex-ai" | "google-vertex" => Ok(Self::Vertex),
            other => anyhow::bail!(
                "unsupported provider '{other}'. Use openai, anthropic, bedrock, or vertex"
            ),
        }
    }
}

fn provider_label(provider: ProviderKind) -> &'static str {
    match provider {
        ProviderKind::OpenAi => "openai",
        ProviderKind::Anthropic => "anthropic",
        ProviderKind::Bedrock => "bedrock",
        ProviderKind::Vertex => "vertex",
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Compress {
            input,
            query,
            token_budget,
            json,
            store,
        } => {
            let input_text = read_input(input.as_deref()).await?;
            let store = open_store(store)?;
            let pipeline = build_pipeline();
            let report = compress_text_with_pipeline(
                &input_text,
                &query,
                store.as_ref(),
                &pipeline,
                token_budget,
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", report.output);
            }
        }
        Commands::Batch {
            paths,
            recursive,
            jobs,
            ext,
            query,
            write_dir,
            json,
            store,
        } => {
            let report = compress_batch(BatchOptions {
                paths,
                recursive,
                jobs,
                extensions: ext,
                query,
                write_dir,
                store,
            })?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "Processed {} files: {} ok, {} failed, {} bytes saved",
                    report.files, report.ok, report.failed, report.bytes_saved
                );
                for item in &report.items {
                    if item.ok {
                        println!(
                            "  OK {}: {} -> {} bytes ({})",
                            item.path,
                            item.original_bytes.unwrap_or_default(),
                            item.compressed_bytes.unwrap_or_default(),
                            item.steps_applied.join(",")
                        );
                    } else {
                        println!(
                            "  ERR {}: {}",
                            item.path,
                            item.error.as_deref().unwrap_or("unknown error")
                        );
                    }
                }
            }
        }
        Commands::Exec {
            query,
            json,
            store,
            raw_on_failure,
            stream,
            command,
        } => {
            let store = open_store(store)?;
            let report =
                run_exec_command(&command, &query, raw_on_failure, stream, store.as_ref())?;
            let exit_code = report.exit_code;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else if !stream {
                print!("{}", report.output);
            } else if !report.output.is_empty() {
                eprintln!("\n[lm-resizer filtered output]\n{}", report.output);
            }
            if exit_code != 0 {
                std::process::exit(exit_code);
            }
        }
        Commands::Rewrite { json, command } => {
            let report = rewrite_command_report(&command);
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else if let Some(rewritten) = report.rewritten {
                println!("{rewritten}");
            } else {
                println!("{}", report.command);
            }
        }
        Commands::RewriteShell { json, command } => {
            let report = rewrite_shell_report(&command);
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("{}", report.rewritten);
            }
        }
        Commands::Retrieve { hash, store } => {
            let store = open_store(store)?;
            let payload = store
                .get(&hash)
                .with_context(|| format!("CCR entry not found: {hash}"))?;
            let _ = record_retrieval_feedback(&hash, payload.len(), "cli");
            print!("{payload}");
        }
        Commands::Stats { store, markdown } => {
            let store = open_store(store)?;
            let exec_history = summarize_exec_history().unwrap_or_default();
            let retrieval_feedback = summarize_retrieval_feedback().unwrap_or_default();
            let report = json!({
                "entries": store.len(),
                "empty": store.is_empty(),
                "exec_history": exec_history,
                "retrieval_feedback": retrieval_feedback,
            });
            if markdown {
                print!("{}", format_stats_markdown(&report));
            } else {
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
        }
        Commands::Image { input, json } => {
            let report = inspect_image(&input)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                let dims = match (report.width, report.height) {
                    (Some(w), Some(h)) => format!("{w}x{h}"),
                    _ => "unknown dimensions".to_string(),
                };
                println!(
                    "{}: {} bytes, {}, {}",
                    report.format, report.bytes, dims, report.recommendation
                );
            }
        }
        Commands::Voice { input, json, clean } => {
            let text = read_input(input.as_deref()).await?;
            let report = analyze_voice_transcript(&text);
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else if clean {
                print!("{}", report.cleaned);
            } else {
                println!(
                    "Voice transcript: {} filler tokens, {} -> {} chars",
                    report.filler_count, report.original_chars, report.cleaned_chars
                );
            }
        }
        Commands::MlStatus { json } => {
            let report = ml_status_report();
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "Magika enabled: {}; model: {}; ONNX runtime: {}; hot path: {}",
                    report.magika_enabled,
                    report.magika_model.as_deref().unwrap_or("not configured"),
                    report.onnx_runtime,
                    report.hot_path
                );
            }
        }
        Commands::Tee { command } => run_tee_command(command)?,
        Commands::TrustFilters { path, json } => {
            let report = trust_filter_file(&path)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("trusted {} ({})", report.path, report.hash);
            }
        }
        Commands::ListTrustedFilters { json } => {
            let report = ListTrustedFiltersReport {
                entries: load_trusted_filter_records()?,
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else if report.entries.is_empty() {
                println!("No trusted filter files.");
            } else {
                for entry in report.entries {
                    println!("{} {}", entry.hash, entry.path);
                }
            }
        }
        Commands::UntrustFilters { path, json } => {
            let report = untrust_filter_file(&path)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else if report.removed {
                println!("untrusted {}", report.path);
            } else {
                println!("not trusted {}", report.path);
            }
        }
        Commands::AuditFilters { path, json, review } => {
            let report = audit_filter_file(&path)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else if review {
                print!("{}", render_filter_audit_review(&report));
            } else {
                println!("Filter file: {}", report.path);
                println!("Hash: {}", report.hash);
                println!("Trust status: {}", report.trust_status);
                if let Some(hash) = &report.trusted_hash {
                    println!("Trusted hash: {hash}");
                }
                println!(
                    "Verification: {} passed, {} failed",
                    report.verification.passed, report.verification.failed
                );
                for diagnostic in &report.verification.diagnostics {
                    println!("  note: {diagnostic}");
                }
                for filter in report.filters {
                    println!(
                        "- {} matches `{}` actions: {}",
                        filter.name,
                        filter.match_command,
                        filter.actions.join(", ")
                    );
                }
            }
        }
        Commands::VerifyFilters { path, json } => {
            let report = verify_filter_file(&path)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "Verified {} filters, {} tests: {} passed, {} failed",
                    report.filters, report.tests, report.passed, report.failed
                );
                for diagnostic in &report.diagnostics {
                    println!("  note: {diagnostic}");
                }
                for outcome in report.outcomes.iter().filter(|outcome| !outcome.passed) {
                    println!(
                        "  FAIL {} / {}: expected {:?}, got {:?}",
                        outcome.filter, outcome.name, outcome.expected, outcome.actual
                    );
                }
            }
            if report.failed > 0 {
                std::process::exit(1);
            }
        }
        Commands::InitFilters {
            path,
            profile,
            force,
            json,
        } => {
            let report = init_filter_file(&path, profile, force)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else if report.written {
                println!("Created {}", report.path);
                for step in report.next_steps {
                    println!("  - {step}");
                }
            } else {
                println!("Filter file already exists: {}", report.path);
                println!("Use --force to overwrite it.");
            }
        }
        Commands::SanitizeProviderFixture {
            provider,
            input,
            output,
            max_string,
            json,
        } => {
            let report = sanitize_provider_fixture(provider, &input, &output, max_string)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "Wrote sanitized {} fixture to {} ({} redacted fields, {} placeholders)",
                    report.provider,
                    report.output,
                    report.redacted_fields,
                    report.placeholder_strings
                );
            }
        }
        Commands::Discover {
            paths,
            recursive,
            json,
            markdown,
        } => {
            let report = discover_exec_savings(&paths, recursive)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else if markdown {
                print!("{}", format_discover_markdown(&report));
            } else {
                println!(
                    "Scanned {} files, found {} command outputs, estimated {} bytes / {} tokens saved",
                    report.files_scanned,
                    report.command_outputs,
                    report.estimated_bytes_saved,
                    report.estimated_tokens_saved
                );
                for candidate in report.candidates.iter().take(20) {
                    println!(
                        "  {}: {} -> {} bytes via {} ({})",
                        candidate.command,
                        candidate.original_bytes,
                        candidate.filtered_bytes,
                        candidate.filter,
                        candidate.source
                    );
                }
            }
        }
        Commands::DiscoverSessions {
            agent,
            json,
            markdown,
        } => {
            let report = discover_agent_sessions(agent)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else if markdown {
                print!("{}", format_discover_sessions_markdown(&report));
            } else {
                println!("Agent sessions: {}", report.agent);
                println!("Paths scanned: {}", report.paths.len());
                for path in &report.paths {
                    println!("  - {path}");
                }
                if !report.missing.is_empty() {
                    println!("Missing known paths: {}", report.missing.len());
                }
                println!(
                    "Found {} command outputs, estimated {} bytes / {} tokens saved",
                    report.discover.command_outputs,
                    report.discover.estimated_bytes_saved,
                    report.discover.estimated_tokens_saved
                );
            }
        }
        Commands::Eval {
            paths,
            recursive,
            json,
            markdown,
        } => {
            let report = run_eval(&paths, recursive)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else if markdown {
                print!("{}", format_eval_markdown(&report));
            } else {
                println!(
                    "Eval {}: {} files, {} command outputs, {} candidates, {} est. tokens saved",
                    if report.pass { "pass" } else { "warn" },
                    report.files_scanned,
                    report.command_outputs,
                    report.candidates,
                    report.estimated_tokens_saved
                );
                for note in report.notes {
                    println!("  - {note}");
                }
            }
        }
        Commands::Learn {
            paths,
            recursive,
            project_dir,
            json,
            markdown,
            write,
            install,
            client,
        } => {
            let report = run_learn(paths, recursive, project_dir, write, install, &client)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else if markdown {
                print!("{}", report.markdown);
            } else {
                println!(
                    "Learned {} recommendations from {} files and {} command outputs",
                    report.recommendations.len(),
                    report.files_scanned,
                    report.command_outputs
                );
                if let Some(memory_file) = &report.memory_file {
                    println!("  memory: {memory_file}");
                }
                for file in &report.instruction_files {
                    println!("  updated {file}");
                }
                println!();
                print!("{}", report.markdown);
            }
        }
        Commands::InitHooks {
            project_dir,
            force,
            json,
        } => {
            let report = init_hook_helpers(project_dir, force)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("Wrote hook helpers to {}", report.directory);
                for file in report.files {
                    println!("  {file}");
                }
            }
        }
        Commands::InitNativeHooks {
            client,
            project_dir,
            force,
            json,
        } => {
            let report = init_native_hooks(&client, project_dir, force)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("Wrote native hook config under {}", report.project_dir);
                for file in report.files {
                    println!("  {file}");
                }
            }
        }
        Commands::Hook {
            client,
            event,
            json,
        } => {
            let report = run_native_hook(&client, &event);
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else if report.recorded {
                println!(
                    "lm-resizer hook recorded {} via {} ({} bytes saved)",
                    report.client,
                    report.filter.as_deref().unwrap_or("unknown"),
                    report.bytes_saved
                );
            }
        }
        Commands::InitShims {
            project_dir,
            force,
            json,
        } => {
            let report = init_command_shims(project_dir, force)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("Wrote command shims to {}", report.directory);
                println!("{}", report.path_hint);
                for file in report.files {
                    println!("  {file}");
                }
                for skipped in report.skipped {
                    println!("  skipped {skipped}");
                }
            }
        }
        Commands::InstallHooks {
            client,
            project_dir,
            force,
            json,
        } => {
            let report = install_agent_hooks(&client, project_dir, force)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("Installed hook helpers in {}", report.helper_directory);
                for file in report.instruction_files {
                    println!("  updated {file}");
                }
            }
        }
        Commands::UninstallHooks {
            client,
            project_dir,
            json,
        } => {
            let report = uninstall_agent_hooks(&client, project_dir)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("Removed {} lm-resizer hook blocks", report.removed);
                for file in report.instruction_files {
                    println!("  updated {file}");
                }
            }
        }
        Commands::Doctor { json, store } => run_doctor(json, store)?,
        Commands::Mcp { store } => run_mcp(store)?,
        Commands::Install {
            client,
            scope,
            project_dir,
            store,
        } => install_mcp(&client, &scope, project_dir, store)?,
        Commands::Serve {
            bind,
            upstream,
            api_key,
            provider,
            store,
            dashboard,
        } => run_http(bind, upstream, api_key, provider.parse()?, store, dashboard).await?,
        Commands::Wrap {
            agent,
            args,
            bind,
            upstream,
            api_key,
            provider,
            store,
            timeout_sec,
        } => {
            wrap_agent(
                agent,
                args,
                bind,
                upstream,
                api_key,
                provider.parse()?,
                store,
                timeout_sec,
            )
            .await?
        }
    }
    Ok(())
}

async fn read_input(path: Option<&Path>) -> Result<String> {
    if let Some(path) = path {
        return tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("could not read {}", path.display()));
    }
    let mut input = String::new();
    let mut stdin = tokio::io::stdin();
    tokio::io::AsyncReadExt::read_to_string(&mut stdin, &mut input).await?;
    Ok(input)
}

fn default_store_path() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("LM_RESIZER_STORE") {
        return Ok(PathBuf::from(path));
    }
    Ok(default_state_dir()?.join("ccr.sqlite3"))
}

fn default_state_dir() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("LM_RESIZER_STATE_DIR") {
        return Ok(PathBuf::from(path));
    }
    let base = std::env::var("LOCALAPPDATA")
        .or_else(|_| std::env::var("XDG_STATE_HOME"))
        .or_else(|_| std::env::var("HOME"))
        .or_else(|_| std::env::var("USERPROFILE"))
        .context("could not determine a home/state directory")?;
    Ok(PathBuf::from(base).join("lm-resizer"))
}

fn open_store(path: Option<PathBuf>) -> Result<Box<dyn CcrStore>> {
    let path = path.unwrap_or(default_store_path()?);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let cfg = CcrBackendConfig::sqlite_default(path);
    Ok(from_config(&cfg)?)
}

fn build_pipeline() -> CompressionPipeline {
    default_pipeline()
}

fn compress_text(content: &str, query: &str, store: &dyn CcrStore) -> Result<CompressReport> {
    let pipeline = build_pipeline();
    compress_text_with_pipeline(content, query, store, &pipeline, None)
}

fn compress_text_with_pipeline(
    content: &str,
    query: &str,
    store: &dyn CcrStore,
    pipeline: &CompressionPipeline,
    token_budget: Option<usize>,
) -> Result<CompressReport> {
    let detection = detect_content_type(content);
    let ctx = CompressionContext {
        query: query.to_string(),
        token_budget,
    };
    let result = pipeline.run(content, detection.content_type, &ctx, store);
    Ok(CompressReport {
        content_type: detection.content_type.as_str().to_string(),
        original_bytes: content.len(),
        compressed_bytes: result.output.len(),
        bytes_saved: result.bytes_saved,
        steps_applied: result.steps_applied,
        cache_keys: result.cache_keys,
        output: result.output,
    })
}

fn run_exec_command(
    command: &[String],
    query: &str,
    raw_on_failure: bool,
    stream: bool,
    store: &dyn CcrStore,
) -> Result<ExecReport> {
    let started = Instant::now();
    let (program, args) = command.split_first().context("missing command for exec")?;
    let resolved_program = resolve_command_path(program).unwrap_or_else(|| PathBuf::from(program));
    let (exit_code, raw) = if stream {
        run_command_streaming(&resolved_program, args, &command.join(" "))?
    } else {
        let output = Command::new(&resolved_program)
            .args(args)
            .output()
            .with_context(|| format!("failed to execute '{}'", command.join(" ")))?;
        (
            output.status.code().unwrap_or(1),
            combine_command_output(&output.stdout, &output.stderr),
        )
    };

    let (filter, filtered) = if raw_on_failure && exit_code != 0 {
        ("raw_on_failure".to_string(), raw.clone())
    } else {
        filter_command_output(command, &raw)
    };

    let compressed = compress_text(&filtered, query, store)?;
    let tee_hint = tee_raw_output_if_useful(command, &raw, &filtered, exit_code)?;
    let mut final_output = compressed.output;
    if let Some(hint) = &tee_hint {
        if !final_output.ends_with('\n') && !final_output.is_empty() {
            final_output.push('\n');
        }
        final_output.push_str(hint);
        final_output.push('\n');
    }

    let report = ExecReport {
        command: command.join(" "),
        exit_code,
        filter,
        original_bytes: raw.len(),
        filtered_bytes: filtered.len(),
        compressed_bytes: final_output.len(),
        bytes_saved: raw.len().saturating_sub(final_output.len()),
        compression_steps: compressed.steps_applied,
        cache_keys: compressed.cache_keys,
        tee_hint,
        output: final_output,
    };
    record_exec_history(&report, started.elapsed())?;
    Ok(report)
}

fn run_command_streaming(program: &Path, args: &[String], display: &str) -> Result<(i32, String)> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to execute '{display}'"))?;

    let stdout = child.stdout.take().context("failed to capture stdout")?;
    let stderr = child.stderr.take().context("failed to capture stderr")?;
    let stdout_handle = std::thread::spawn(move || stream_reader(stdout, false));
    let stderr_handle = std::thread::spawn(move || stream_reader(stderr, true));
    let status = child.wait()?;
    let stdout = stdout_handle
        .join()
        .map_err(|_| anyhow::anyhow!("stdout stream thread panicked"))??;
    let stderr = stderr_handle
        .join()
        .map_err(|_| anyhow::anyhow!("stderr stream thread panicked"))??;
    Ok((
        status.code().unwrap_or(1),
        combine_command_output(&stdout, &stderr),
    ))
}

fn stream_reader<R: std::io::Read>(reader: R, stderr: bool) -> Result<Vec<u8>> {
    let mut reader = std::io::BufReader::new(reader);
    let mut captured = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let read = std::io::Read::read(&mut reader, &mut buf)?;
        if read == 0 {
            break;
        }
        captured.extend_from_slice(&buf[..read]);
        if stderr {
            std::io::stderr().write_all(&buf[..read])?;
            std::io::stderr().flush()?;
        } else {
            std::io::stdout().write_all(&buf[..read])?;
            std::io::stdout().flush()?;
        }
    }
    Ok(captured)
}

fn combine_command_output(stdout: &[u8], stderr: &[u8]) -> String {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    match (stdout.trim().is_empty(), stderr.trim().is_empty()) {
        (false, false) => format!("{stdout}\n[stderr]\n{stderr}"),
        (false, true) => stdout.into_owned(),
        (true, false) => stderr.into_owned(),
        (true, true) => String::new(),
    }
}

fn rewrite_command_report(command: &[String]) -> RewriteReport {
    let command_text = command.join(" ");
    if command.is_empty() {
        return RewriteReport {
            command: command_text,
            supported: false,
            filter: "none".to_string(),
            rewritten: None,
            argv: Vec::new(),
        };
    }

    let (filter, _) = filter_command_output(command, "");
    let supported = filter != "none" && filter != "generic";
    let mut argv = vec![
        "lm-resizer".to_string(),
        "exec".to_string(),
        "--".to_string(),
    ];
    argv.extend(command.iter().cloned());
    let rewritten = supported.then(|| shell_join(&argv));

    RewriteReport {
        command: command_text,
        supported,
        filter,
        rewritten,
        argv,
    }
}

fn rewrite_shell_report(command: &str) -> RewriteShellReport {
    let tokens = split_shell_operators(command);
    let mut output = String::new();
    let mut rewrites = Vec::new();
    let mut after_pipe = false;

    for token in tokens {
        match token {
            ShellToken::Operator(op) => {
                if !output.is_empty() && !output.ends_with(' ') {
                    output.push(' ');
                }
                output.push_str(&op);
                output.push(' ');
                after_pipe = op == "|";
            }
            ShellToken::Segment(segment) => {
                let trimmed = segment.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let rewritten = if after_pipe {
                    None
                } else {
                    rewrite_shell_segment(trimmed)
                };
                if let Some((rewritten_segment, filter)) = rewritten {
                    output.push_str(&rewritten_segment);
                    rewrites.push(RewriteShellSegment {
                        original: trimmed.to_string(),
                        rewritten: rewritten_segment,
                        filter,
                    });
                } else {
                    output.push_str(trimmed);
                }
                output.push(' ');
                after_pipe = false;
            }
        }
    }

    let rewritten = output.trim().to_string();
    RewriteShellReport {
        command: command.to_string(),
        changed: rewritten != command.trim(),
        rewritten,
        rewrites,
    }
}

fn rewrite_shell_segment(segment: &str) -> Option<(String, String)> {
    let (body, suffix) = split_trailing_redirects(segment);
    if body.trim().is_empty() {
        return None;
    }
    let args = split_shell_words(body.trim())?;
    let report = rewrite_command_report(&args);
    let rewritten = report.rewritten?;
    let suffix = suffix.trim();
    if suffix.is_empty() {
        Some((rewritten, report.filter))
    } else {
        Some((format!("{rewritten} {suffix}"), report.filter))
    }
}

fn split_trailing_redirects(segment: &str) -> (&str, &str) {
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    for (idx, ch) in segment.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if !in_single => escaped = true,
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '>' | '<' if !in_single && !in_double => return segment.split_at(idx),
            _ => {}
        }
    }
    (segment, "")
}

#[derive(Debug, PartialEq, Eq)]
enum ShellToken {
    Segment(String),
    Operator(String),
}

fn split_shell_operators(command: &str) -> Vec<ShellToken> {
    let mut tokens = Vec::new();
    let mut start = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    let chars = command.char_indices().collect::<Vec<_>>();
    let mut i = 0usize;

    while i < chars.len() {
        let (idx, ch) = chars[i];
        if escaped {
            escaped = false;
            i += 1;
            continue;
        }
        match ch {
            '\\' if !in_single => escaped = true,
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '&' | '|' if !in_single && !in_double => {
                if i + 1 < chars.len() && chars[i + 1].1 == ch {
                    push_shell_segment(&mut tokens, &command[start..idx]);
                    tokens.push(ShellToken::Operator(format!("{ch}{ch}")));
                    start = chars[i + 1].0 + chars[i + 1].1.len_utf8();
                    i += 1;
                } else if ch == '|' {
                    push_shell_segment(&mut tokens, &command[start..idx]);
                    tokens.push(ShellToken::Operator("|".to_string()));
                    start = idx + ch.len_utf8();
                }
            }
            ';' if !in_single && !in_double => {
                push_shell_segment(&mut tokens, &command[start..idx]);
                tokens.push(ShellToken::Operator(";".to_string()));
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
        i += 1;
    }
    push_shell_segment(&mut tokens, &command[start..]);
    tokens
}

fn push_shell_segment(tokens: &mut Vec<ShellToken>, segment: &str) {
    if !segment.trim().is_empty() {
        tokens.push(ShellToken::Segment(segment.trim().to_string()));
    }
}

fn split_shell_words(segment: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    for ch in segment.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' if !in_single => escaped = true,
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            ch if ch.is_whitespace() && !in_single && !in_double => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if escaped || in_single || in_double {
        return None;
    }
    if !current.is_empty() {
        words.push(current);
    }
    Some(words)
}

fn shell_join(args: &[String]) -> String {
    args.iter()
        .map(|arg| {
            if arg.chars().all(|c| {
                c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | '\\' | ':')
            }) {
                arg.clone()
            } else {
                format!("\"{}\"", arg.replace('"', "\\\""))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn filter_command_output(command: &[String], raw: &str) -> (String, String) {
    let command_text = normalized_command_text(command);
    if let Some((filter, filtered)) = apply_toml_filters(&command_text, raw) {
        return (filter, filtered);
    }

    let Some(program) = command.first().map(|s| command_basename(s)) else {
        return ("none".to_string(), raw.to_string());
    };
    let sub = command.get(1).map(String::as_str).unwrap_or("");

    match (program.as_str(), sub) {
        ("git", "status") => ("git_status".to_string(), filter_git_status(raw)),
        ("git", "diff") => ("diff_summary".to_string(), filter_diff_summary(raw)),
        ("git", "log") => ("git_log".to_string(), filter_git_log(raw)),
        ("git", "show") => ("diff_summary".to_string(), filter_diff_summary(raw)),
        ("cargo", "test") => ("cargo_test".to_string(), filter_cargo_test(raw)),
        ("cargo", "check" | "build" | "clippy") => {
            ("cargo_diagnostics".to_string(), filter_diagnostics(raw))
        }
        ("tsc", _) => ("tsc".to_string(), filter_tsc(raw)),
        ("pytest", _) => ("pytest".to_string(), filter_pytest(raw)),
        ("npm" | "pnpm" | "yarn", "test" | "run") => {
            ("js_test".to_string(), filter_diagnostics(raw))
        }
        ("rg" | "grep", _) => (
            "search_results".to_string(),
            filter_search_results(raw, 80, 6),
        ),
        ("find" | "fd" | "ls" | "dir" | "tree", _) => {
            ("listing".to_string(), filter_listing(raw, 120))
        }
        _ => ("generic".to_string(), filter_generic(raw, 240)),
    }
}

fn normalized_command_text(command: &[String]) -> String {
    let Some((program, args)) = command.split_first() else {
        return String::new();
    };
    let mut parts = Vec::with_capacity(command.len());
    parts.push(command_basename(program));
    parts.extend(args.iter().cloned());
    parts.join(" ")
}

fn apply_toml_filters(command_text: &str, raw: &str) -> Option<(String, String)> {
    if std::env::var("LM_RESIZER_NO_TOML_FILTERS").ok().as_deref() == Some("1") {
        return None;
    }
    let filters = load_toml_filters().ok()?;
    for filter in filters {
        if filter.match_command.is_match(command_text) {
            return Some((
                format!("toml:{}", filter.name),
                apply_toml_filter(&filter, raw),
            ));
        }
    }
    None
}

fn load_toml_filters() -> Result<Vec<CompiledTomlFilter>> {
    let mut filters = Vec::new();
    for content in toml_filter_sources()? {
        let file: TomlFilterFile = toml::from_str(&content)?;
        for def in file.filters {
            filters.push(compile_toml_filter(def)?);
        }
    }
    Ok(filters)
}

fn toml_filter_sources() -> Result<Vec<String>> {
    let mut sources = Vec::new();
    if let Ok(path) = std::env::var("LM_RESIZER_FILTERS") {
        sources.push(std::fs::read_to_string(path)?);
    }
    let project = Path::new(".lm-resizer").join("filters.toml");
    if project.exists() {
        let content = std::fs::read_to_string(&project)?;
        if project_filter_is_trusted(&project, &content)? {
            sources.push(content);
        }
    }
    sources.push(BUILTIN_EXEC_FILTERS_TOML.to_string());
    Ok(sources)
}

fn project_filter_is_trusted(path: &Path, content: &str) -> Result<bool> {
    if std::env::var("LM_RESIZER_TRUST_PROJECT_FILTERS")
        .ok()
        .as_deref()
        == Some("1")
    {
        return Ok(true);
    }
    let canonical = canonical_or_absolute(path)?;
    let hash = sha256_hex(content.as_bytes());
    Ok(load_trusted_filter_records()?
        .into_iter()
        .any(|record| record.path == canonical.display().to_string() && record.hash == hash))
}

fn trust_filter_file(path: &Path) -> Result<TrustFilterReport> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;
    let verification = verify_filter_file(path)?;
    if verification.failed > 0 {
        anyhow::bail!(
            "filter verification failed: {} of {} tests failed",
            verification.failed,
            verification.tests
        );
    }
    let canonical = canonical_or_absolute(path)?;
    let hash = sha256_hex(content.as_bytes());
    let mut records = load_trusted_filter_records()?;
    let canonical_string = canonical.display().to_string();
    records.retain(|record| record.path != canonical_string);
    records.push(TrustedFilterRecord {
        path: canonical_string.clone(),
        hash: hash.clone(),
    });
    save_trusted_filter_records(&records)?;
    Ok(TrustFilterReport {
        path: canonical_string,
        hash,
        trusted: true,
    })
}

fn untrust_filter_file(path: &Path) -> Result<UntrustFilterReport> {
    let canonical = canonical_or_absolute(path)?;
    let canonical_string = canonical.display().to_string();
    let mut records = load_trusted_filter_records()?;
    let before = records.len();
    records.retain(|record| record.path != canonical_string);
    let removed = records.len() != before;
    save_trusted_filter_records(&records)?;
    Ok(UntrustFilterReport {
        path: canonical_string,
        removed,
    })
}

fn audit_filter_file(path: &Path) -> Result<AuditFiltersReport> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;
    let file: TomlFilterFile = toml::from_str(&content)
        .with_context(|| format!("invalid filter TOML: {}", path.display()))?;
    let canonical = canonical_or_absolute(path)?;
    let path_string = canonical.display().to_string();
    let hash = sha256_hex(content.as_bytes());
    let trusted_hash = load_trusted_filter_records()?
        .into_iter()
        .find(|record| record.path == path_string)
        .map(|record| record.hash);
    let trust_status = match trusted_hash.as_deref() {
        Some(value) if value == hash => "trusted-current",
        Some(_) => "trusted-stale",
        None => "untrusted",
    }
    .to_string();
    let filters = file
        .filters
        .iter()
        .map(audit_filter_item)
        .collect::<Vec<_>>();
    Ok(AuditFiltersReport {
        path: path_string,
        hash,
        trusted_hash,
        trust_status,
        filters,
        verification: verify_filter_file(path)?,
    })
}

fn audit_filter_item(def: &TomlFilterDef) -> AuditFilterItem {
    let mut actions = Vec::new();
    if def.strip_ansi {
        actions.push("strip_ansi".to_string());
    }
    if !def.strip_lines_matching.is_empty() {
        actions.push(format!(
            "strip_lines_matching({})",
            def.strip_lines_matching.len()
        ));
    }
    if !def.keep_lines_matching.is_empty() {
        actions.push(format!(
            "keep_lines_matching({})",
            def.keep_lines_matching.len()
        ));
    }
    if !def.replace.is_empty() {
        actions.push(format!("replace({})", def.replace.len()));
    }
    if def.truncate_lines_at.is_some() {
        actions.push("truncate_lines_at".to_string());
    }
    if def.head_lines.is_some() {
        actions.push("head_lines".to_string());
    }
    if def.tail_lines.is_some() {
        actions.push("tail_lines".to_string());
    }
    if def.max_lines.is_some() {
        actions.push("max_lines".to_string());
    }
    if def.on_empty.is_some() {
        actions.push("on_empty".to_string());
    }
    if actions.is_empty() {
        actions.push("match_only".to_string());
    }
    AuditFilterItem {
        name: def.name.clone(),
        match_command: def.match_command.clone(),
        actions,
    }
}

fn render_filter_audit_review(report: &AuditFiltersReport) -> String {
    let mut out = String::new();
    out.push_str("# lm-resizer Filter Review\n\n");
    out.push_str(&format!("- Path: `{}`\n", report.path));
    out.push_str(&format!("- Current hash: `{}`\n", report.hash));
    out.push_str(&format!("- Trust status: `{}`\n", report.trust_status));
    match &report.trusted_hash {
        Some(hash) => out.push_str(&format!("- Trusted hash: `{hash}`\n")),
        None => out.push_str("- Trusted hash: none\n"),
    }
    out.push_str(&format!(
        "- Verification: {} passed, {} failed\n",
        report.verification.passed, report.verification.failed
    ));

    if !report.verification.diagnostics.is_empty() {
        out.push_str("\n## Diagnostics\n\n");
        for diagnostic in &report.verification.diagnostics {
            out.push_str(&format!("- {diagnostic}\n"));
        }
    }

    if !report.verification.outcomes.is_empty() {
        out.push_str("\n## Inline Tests\n\n");
        out.push_str("| Filter | Test | Result |\n");
        out.push_str("| --- | --- | --- |\n");
        for outcome in &report.verification.outcomes {
            let result = if outcome.passed { "passed" } else { "failed" };
            out.push_str(&format!(
                "| `{}` | `{}` | {} |\n",
                markdown_cell(&outcome.filter),
                markdown_cell(&outcome.name),
                result
            ));
        }
    }

    out.push_str("\n## Filter Actions\n\n");
    if report.filters.is_empty() {
        out.push_str("No filters found.\n");
    } else {
        for filter in &report.filters {
            out.push_str(&format!("### `{}`\n\n", filter.name));
            out.push_str(&format!("- Match command: `{}`\n", filter.match_command));
            out.push_str(&format!("- Actions: {}\n\n", filter.actions.join(", ")));
        }
    }

    out.push_str("## Approval\n\n");
    match report.trust_status.as_str() {
        "trusted-current" => {
            out.push_str("This filter file already matches the trusted hash.\n");
        }
        "trusted-stale" => {
            out.push_str("Review the changed filter behavior, then run:\n\n");
            out.push_str(&format!(
                "```bash\nlm-resizer verify-filters --path {}\nlm-resizer trust-filters --path {}\n```\n",
                shell_arg(&report.path),
                shell_arg(&report.path)
            ));
        }
        _ => {
            out.push_str("Review the filter behavior, then run:\n\n");
            out.push_str(&format!(
                "```bash\nlm-resizer verify-filters --path {}\nlm-resizer trust-filters --path {}\n```\n",
                shell_arg(&report.path),
                shell_arg(&report.path)
            ));
        }
    }
    out
}

fn markdown_cell(value: &str) -> String {
    value.replace('|', "\\|")
}

fn shell_arg(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '/' | '\\' | '-' | '_' | ':'))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\"'\"'"))
    }
}

fn load_trusted_filter_records() -> Result<Vec<TrustedFilterRecord>> {
    let path = trusted_filters_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content).unwrap_or_default())
}

fn save_trusted_filter_records(records: &[TrustedFilterRecord]) -> Result<()> {
    let path = trusted_filters_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(records)?)?;
    Ok(())
}

fn trusted_filters_path() -> Result<PathBuf> {
    Ok(default_state_dir()?.join("trusted-filters.json"))
}

fn canonical_or_absolute(path: &Path) -> Result<PathBuf> {
    if let Ok(canonical) = path.canonicalize() {
        return Ok(canonical);
    }
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn compile_toml_filter(def: TomlFilterDef) -> Result<CompiledTomlFilter> {
    let strip_lines_matching = compile_regex_set(def.strip_lines_matching)?;
    let keep_lines_matching = compile_regex_set(def.keep_lines_matching)?;
    let mut replace = Vec::new();
    for rule in def.replace {
        replace.push((Regex::new(&rule.pattern)?, rule.replacement));
    }
    Ok(CompiledTomlFilter {
        name: def.name,
        match_command: Regex::new(&def.match_command)?,
        strip_ansi: def.strip_ansi,
        strip_lines_matching,
        keep_lines_matching,
        replace,
        truncate_lines_at: def.truncate_lines_at,
        head_lines: def.head_lines,
        tail_lines: def.tail_lines,
        max_lines: def.max_lines,
        on_empty: def.on_empty,
    })
}

fn verify_filter_file(path: &Path) -> Result<VerifyFiltersReport> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;
    let file: TomlFilterFile = toml::from_str(&content)
        .with_context(|| format!("invalid filter TOML: {}", path.display()))?;

    let mut filters = std::collections::BTreeMap::new();
    let mut diagnostics = Vec::new();
    for def in file.filters {
        let name = def.name.clone();
        if filters.contains_key(&name) {
            diagnostics.push(format!(
                "duplicate filter `{name}` overrides an earlier definition"
            ));
        }
        let filter = compile_toml_filter(def)?;
        filters.insert(filter.name.clone(), filter);
    }

    let mut outcomes = Vec::new();
    let mut tested_filters = std::collections::BTreeSet::new();
    for test in file.tests {
        tested_filters.insert(test.filter.clone());
        let Some(filter) = filters.get(&test.filter) else {
            outcomes.push(FilterTestOutcome {
                filter: test.filter,
                name: test.name,
                passed: false,
                expected: test.expected,
                actual: "<missing filter>".to_string(),
            });
            continue;
        };
        let actual = apply_toml_filter(filter, &test.input);
        outcomes.push(FilterTestOutcome {
            filter: test.filter,
            name: test.name,
            passed: actual == test.expected,
            expected: test.expected,
            actual,
        });
    }

    if filters.is_empty() {
        diagnostics.push("no [[filters]] entries found".to_string());
    }
    if outcomes.is_empty() {
        diagnostics.push("no [[tests]] entries found; add fixtures before trusting".to_string());
    }
    for name in filters.keys() {
        if !tested_filters.contains(name) {
            diagnostics.push(format!("filter `{name}` has no inline [[tests]] coverage"));
        }
    }

    let passed = outcomes.iter().filter(|outcome| outcome.passed).count();
    let failed = outcomes.len().saturating_sub(passed);
    Ok(VerifyFiltersReport {
        path: path.display().to_string(),
        filters: filters.len(),
        tests: outcomes.len(),
        passed,
        failed,
        diagnostics,
        outcomes,
    })
}

fn init_filter_file(path: &Path, profile: FilterProfile, force: bool) -> Result<InitFiltersReport> {
    if path.exists() && !force {
        return Ok(InitFiltersReport {
            path: path.display().to_string(),
            written: false,
            next_steps: init_filter_next_steps(path),
        });
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    std::fs::write(path, starter_filters_toml(profile))
        .with_context(|| format!("could not write {}", path.display()))?;
    Ok(InitFiltersReport {
        path: path.display().to_string(),
        written: true,
        next_steps: init_filter_next_steps(path),
    })
}

fn starter_filters_toml(profile: FilterProfile) -> &'static str {
    match profile {
        FilterProfile::Generic => STARTER_FILTERS_TOML,
        FilterProfile::Rust => RUST_FILTERS_TOML,
        FilterProfile::Node => NODE_FILTERS_TOML,
        FilterProfile::Python => PYTHON_FILTERS_TOML,
        FilterProfile::Infra => INFRA_FILTERS_TOML,
    }
}

fn init_filter_next_steps(path: &Path) -> Vec<String> {
    let path = path.display();
    vec![
        format!("Edit `{path}` for repeated project-specific output shapes"),
        format!("Run `lm-resizer verify-filters --path {path}`"),
        format!("Run `lm-resizer audit-filters --path {path}`"),
        format!("Run `lm-resizer trust-filters --path {path}` when the audit is acceptable"),
    ]
}

const STARTER_FILTERS_TOML: &str = r#"# Project-local lm-resizer filters.
# Edit this file for repeated noisy output that is specific to this repository.

[[filters]]
name = "project-build"
match_command = "(^|\\s)(just|make|task)\\s+(build|check|test)(\\s|$)"
strip_ansi = true
keep_lines_matching = [
  "(?i)error|failed|failure|panic",
  "(?i)test result|summary|finished",
]
max_lines = 80
on_empty = "build completed without high-signal output\n"

[[tests]]
filter = "project-build"
name = "keeps build errors and summary"
input = "Compiling demo\nerror: failed\nFinished dev profile\n"
expected = "error: failed\nFinished dev profile\n"
"#;

const RUST_FILTERS_TOML: &str = r#"# Rust project lm-resizer filters.

[[filters]]
name = "rust-cargo"
match_command = "^cargo\\s+(test|check|build|clippy)\\b"
strip_ansi = true
keep_lines_matching = [
  "^error(\\[|:)",
  "^warning(\\[|:)",
  "^failures:",
  "^test result:",
  "^\\s*Finished ",
]
max_lines = 160
on_empty = "cargo: completed without diagnostics\n"

[[tests]]
filter = "rust-cargo"
name = "keeps cargo diagnostics and summary"
input = "Compiling demo\nwarning: unused\nerror[E0001]: failed\ntest result: FAILED\n"
expected = "warning: unused\nerror[E0001]: failed\ntest result: FAILED\n"
"#;

const NODE_FILTERS_TOML: &str = r#"# Node/JS project lm-resizer filters.

[[filters]]
name = "node-quality"
match_command = "^(npm|pnpm|yarn|bun)\\s+(run\\s+)?(test|lint|build)\\b|^(vitest|eslint|playwright|next)\\b"
strip_ansi = true
keep_lines_matching = [
  "FAIL",
  "failed",
  "Error",
  "error",
  "Warning",
  "warning",
  "Tests",
  "Duration",
  "Compiled",
]
max_lines = 180
on_empty = "node quality: completed\n"

[[tests]]
filter = "node-quality"
name = "keeps js failures"
input = "transforming modules\nFAIL src/app.test.ts\nDuration 1.2s\n"
expected = "FAIL src/app.test.ts\nDuration 1.2s\n"
"#;

const PYTHON_FILTERS_TOML: &str = r#"# Python project lm-resizer filters.

[[filters]]
name = "python-quality"
match_command = "^(pytest|ruff|mypy|uv\\s+run\\s+(pytest|ruff|mypy))\\b"
strip_ansi = true
keep_lines_matching = [
  "^FAILED ",
  "^ERROR ",
  "^=+ .* =+$",
  "^.+:[0-9]+:",
  "^Found ",
  "^Success:",
]
max_lines = 180
on_empty = "python quality: clean\n"

[[tests]]
filter = "python-quality"
name = "keeps pytest failures"
input = "collecting tests\nFAILED tests/test_app.py::test_app\n===== short test summary info =====\n"
expected = "FAILED tests/test_app.py::test_app\n===== short test summary info =====\n"
"#;

const INFRA_FILTERS_TOML: &str = r#"# Infrastructure project lm-resizer filters.

[[filters]]
name = "infra-plan"
match_command = "^(terraform|tofu|kubectl|helm|aws)\\b"
strip_ansi = true
keep_lines_matching = [
  "Plan:",
  "Error",
  "ERROR",
  "Warning",
  "Failed",
  "BackOff",
  "CrashLoop",
  "CREATE",
  "UPDATE",
  "DELETE",
]
max_lines = 180
on_empty = "infra command: no high-signal output\n"

[[tests]]
filter = "infra-plan"
name = "keeps terraform plan summary"
input = "Refreshing state\nPlan: 1 to add, 0 to change, 0 to destroy.\n"
expected = "Plan: 1 to add, 0 to change, 0 to destroy.\n"
"#;

fn sanitize_provider_fixture(
    provider: ProviderKind,
    input: &Path,
    output: &Path,
    max_string: usize,
) -> Result<SanitizedProviderFixtureReport> {
    let content = std::fs::read_to_string(input)
        .with_context(|| format!("could not read {}", input.display()))?;
    let mut value: Value = serde_json::from_str(&content)
        .with_context(|| format!("invalid provider JSON: {}", input.display()))?;
    let mut report = SanitizeStats::default();
    sanitize_json_value(&mut value, max_string, &mut report);
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    std::fs::write(output, serde_json::to_string_pretty(&value)? + "\n")
        .with_context(|| format!("could not write {}", output.display()))?;
    Ok(SanitizedProviderFixtureReport {
        provider: provider_label(provider).to_string(),
        input: input.display().to_string(),
        output: output.display().to_string(),
        redacted_fields: report.redacted_fields,
        placeholder_strings: report.placeholder_strings,
    })
}

#[derive(Default)]
struct SanitizeStats {
    redacted_fields: usize,
    placeholder_strings: usize,
}

fn sanitize_json_value(value: &mut Value, max_string: usize, stats: &mut SanitizeStats) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if is_sensitive_key(key) {
                    *child = Value::String("__REDACTED__".to_string());
                    stats.redacted_fields += 1;
                } else {
                    sanitize_json_value(child, max_string, stats);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                sanitize_json_value(item, max_string, stats);
            }
        }
        Value::String(text) if text.len() >= max_string => {
            let placeholder = if looks_like_json_payload(text) {
                "__LARGE_JSON_ARRAY__"
            } else {
                "__LONG_STRING__"
            };
            *text = placeholder.to_string();
            stats.placeholder_strings += 1;
        }
        _ => {}
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "apikey"
            | "authorization"
            | "bearer"
            | "token"
            | "accesstoken"
            | "refreshtoken"
            | "secret"
            | "secretkey"
            | "clientsecret"
            | "password"
            | "privatekey"
            | "signature"
    )
}

fn looks_like_json_payload(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with('[') || trimmed.starts_with('{')
}

fn compile_regex_set(patterns: Vec<String>) -> Result<Option<RegexSet>> {
    if patterns.is_empty() {
        Ok(None)
    } else {
        Ok(Some(RegexSet::new(patterns)?))
    }
}

fn apply_toml_filter(filter: &CompiledTomlFilter, raw: &str) -> String {
    let ansi = Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]").expect("valid ansi regex");
    let mut text = if filter.strip_ansi {
        ansi.replace_all(raw, "").into_owned()
    } else {
        raw.to_string()
    };

    for (pattern, replacement) in &filter.replace {
        text = text
            .lines()
            .map(|line| pattern.replace_all(line, replacement.as_str()).into_owned())
            .collect::<Vec<_>>()
            .join("\n");
    }

    let mut lines = Vec::new();
    for line in text.lines() {
        if filter
            .strip_lines_matching
            .as_ref()
            .is_some_and(|set| set.is_match(line))
        {
            continue;
        }
        if filter
            .keep_lines_matching
            .as_ref()
            .is_some_and(|set| !set.is_match(line))
        {
            continue;
        }
        lines.push(match filter.truncate_lines_at {
            Some(max) => truncate_chars(line, max),
            None => line.to_string(),
        });
    }

    if let Some(head) = filter.head_lines {
        lines.truncate(head);
    }
    if let Some(tail) = filter.tail_lines {
        if lines.len() > tail {
            lines = lines[lines.len() - tail..].to_vec();
        }
    }
    if let Some(max) = filter.max_lines {
        lines.truncate(max);
    }

    if lines.is_empty() {
        return filter.on_empty.clone().unwrap_or_default();
    }
    lines.join("\n") + "\n"
}

fn truncate_chars(line: &str, max: usize) -> String {
    if line.chars().count() <= max {
        return line.to_string();
    }
    let keep = max.saturating_sub(3);
    let mut out = line.chars().take(keep).collect::<String>();
    out.push_str("...");
    out
}

fn command_basename(command: &str) -> String {
    Path::new(command)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(command)
        .to_ascii_lowercase()
}

fn filter_git_status(raw: &str) -> String {
    let mut kept = Vec::new();
    let mut skipped = 0usize;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("(use ")
            || trimmed.starts_with("use ")
            || trimmed.starts_with("no changes added")
        {
            skipped += 1;
            continue;
        }
        kept.push(line.to_string());
    }

    if kept.is_empty() {
        return raw.trim().to_string();
    }
    append_omitted(kept, skipped)
}

fn filter_diff_summary(raw: &str) -> String {
    let mut kept = Vec::new();
    let mut skipped = 0usize;
    let mut hunk_lines = 0usize;

    for line in raw.lines() {
        if line.starts_with("diff --git")
            || line.starts_with("+++ ")
            || line.starts_with("--- ")
            || line.starts_with("@@")
        {
            kept.push(line.to_string());
            if line.starts_with("@@") {
                hunk_lines = 0;
            }
            continue;
        }

        if (line.starts_with('+') || line.starts_with('-'))
            && !line.starts_with("+++")
            && !line.starts_with("---")
        {
            if hunk_lines < 8 {
                kept.push(line.to_string());
                hunk_lines += 1;
            } else {
                skipped += 1;
            }
            continue;
        }

        skipped += 1;
    }

    append_omitted(kept, skipped)
}

fn filter_diagnostics(raw: &str) -> String {
    let mut kept = Vec::new();
    let mut keep_following = 0usize;
    let mut skipped = 0usize;

    for line in raw.lines() {
        let lower = line.to_ascii_lowercase();
        let important = lower.contains("error")
            || lower.contains("failed")
            || lower.contains("failures:")
            || lower.contains("panic")
            || lower.contains("warning:")
            || lower.contains("test result")
            || lower.contains("could not compile")
            || lower.contains("compilation failed");

        if important {
            kept.push(line.to_string());
            keep_following = 3;
        } else if keep_following > 0 {
            kept.push(line.to_string());
            keep_following -= 1;
        } else {
            skipped += 1;
        }
    }

    if kept.is_empty() {
        filter_generic(raw, 120)
    } else {
        append_omitted(kept, skipped)
    }
}

fn filter_cargo_test(raw: &str) -> String {
    let mut kept = Vec::new();
    let mut in_failure = false;
    let mut failure_lines = 0usize;
    let mut skipped = 0usize;

    for line in raw.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("Compiling")
            || trimmed.starts_with("Checking")
            || trimmed.starts_with("Finished")
            || trimmed.starts_with("running ")
            || (line.starts_with("test ") && line.ends_with("... ok"))
        {
            skipped += 1;
            continue;
        }
        if line == "failures:" || line.starts_with("---- ") {
            in_failure = true;
            failure_lines = 0;
            kept.push(line.to_string());
            continue;
        }
        if line.starts_with("test result:") {
            in_failure = false;
            kept.push(line.to_string());
            continue;
        }
        if in_failure {
            if failure_lines < 20 {
                kept.push(line.to_string());
                failure_lines += 1;
            } else {
                skipped += 1;
            }
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower.contains("error")
            || lower.contains("failed")
            || lower.contains("panic")
            || lower.contains("could not compile")
        {
            kept.push(line.to_string());
        } else {
            skipped += 1;
        }
    }

    if kept.is_empty() {
        "cargo test: passed\n".to_string()
    } else {
        append_omitted(kept, skipped)
    }
}

fn filter_git_log(raw: &str) -> String {
    let mut out = Vec::new();
    let mut skipped = 0usize;
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("commit ") {
            let hash = trimmed.strip_prefix("commit ").unwrap_or(trimmed);
            out.push(format!(
                "commit {}",
                hash.chars().take(12).collect::<String>()
            ));
        } else if trimmed.starts_with("Author:")
            || trimmed.starts_with("Date:")
            || trimmed.starts_with("Merge:")
        {
            skipped += 1;
        } else if !trimmed.is_empty() {
            out.push(trimmed.to_string());
        }
        if out.len() >= 80 {
            skipped += 1;
            break;
        }
    }
    append_omitted(out, skipped)
}

fn filter_tsc(raw: &str) -> String {
    let mut by_file = std::collections::BTreeMap::<String, Vec<String>>::new();
    let mut skipped = 0usize;

    for line in raw.lines() {
        if let Some((file, rest)) = line.split_once('(') {
            if rest.contains("): error TS") || rest.contains("): warning TS") {
                by_file
                    .entry(file.to_string())
                    .or_default()
                    .push(truncate_chars(line, 180));
                continue;
            }
        }
        if line.contains("Found 0 errors") {
            return "TypeScript: no errors\n".to_string();
        }
        skipped += 1;
    }

    if by_file.is_empty() {
        return filter_diagnostics(raw);
    }

    let error_count = by_file.values().map(Vec::len).sum::<usize>();
    let mut out = vec![format!(
        "TypeScript: {error_count} diagnostics in {} files",
        by_file.len()
    )];
    for (file, lines) in by_file {
        out.push(format!("{file}: {} diagnostics", lines.len()));
        for line in lines.into_iter().take(5) {
            out.push(format!("  {line}"));
        }
    }
    append_omitted(out, skipped)
}

fn filter_pytest(raw: &str) -> String {
    let mut kept = Vec::new();
    let mut keep_following = 0usize;
    let mut skipped = 0usize;

    for line in raw.lines() {
        let trimmed = line.trim();
        let important = trimmed.starts_with("FAILED ")
            || trimmed.starts_with("ERROR ")
            || trimmed.starts_with("====")
            || trimmed.contains(" failed")
            || trimmed.contains(" passed")
            || trimmed.contains(" error")
            || trimmed.contains(" warnings")
            || line.starts_with("E   ")
            || line.starts_with("File ");

        if important {
            kept.push(line.to_string());
            keep_following = 2;
        } else if keep_following > 0 {
            kept.push(line.to_string());
            keep_following -= 1;
        } else {
            skipped += 1;
        }
    }

    if kept.is_empty() {
        "pytest: passed\n".to_string()
    } else {
        append_omitted(kept, skipped)
    }
}

fn filter_search_results(raw: &str, max_total: usize, max_per_file: usize) -> String {
    use std::collections::BTreeMap;

    let mut by_file: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut passthrough = Vec::new();

    for line in raw.lines() {
        if let Some((file, _rest)) = line.split_once(':') {
            by_file
                .entry(file.to_string())
                .or_default()
                .push(line.to_string());
        } else {
            passthrough.push(line.to_string());
        }
    }

    let mut out = Vec::new();
    let mut skipped = 0usize;

    for (file, lines) in by_file {
        out.push(format!("{file}: {} matches", lines.len()));
        for line in lines.iter().take(max_per_file) {
            if out.len() >= max_total {
                skipped += 1;
                continue;
            }
            out.push(format!("  {line}"));
        }
        skipped += lines.len().saturating_sub(max_per_file);
    }

    for line in passthrough {
        if out.len() < max_total {
            out.push(line);
        } else {
            skipped += 1;
        }
    }

    append_omitted(out, skipped)
}

fn filter_listing(raw: &str, max_lines: usize) -> String {
    let mut out = Vec::new();
    let mut skipped = 0usize;
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("total 0") {
            skipped += 1;
            continue;
        }
        if out.len() < max_lines {
            out.push(line.to_string());
        } else {
            skipped += 1;
        }
    }
    append_omitted(out, skipped)
}

fn filter_generic(raw: &str, max_lines: usize) -> String {
    let mut out = Vec::new();
    let mut skipped = 0usize;
    let mut last = "";
    let mut repeat_count = 0usize;

    for line in raw.lines() {
        if line == last {
            repeat_count += 1;
            skipped += 1;
            continue;
        }
        if repeat_count > 0 && out.len() < max_lines {
            out.push(format!("... previous line repeated {repeat_count} times"));
        }
        repeat_count = 0;
        last = line;
        if out.len() < max_lines {
            out.push(line.to_string());
        } else {
            skipped += 1;
        }
    }
    if repeat_count > 0 && out.len() < max_lines {
        out.push(format!("... previous line repeated {repeat_count} times"));
    }
    append_omitted(out, skipped)
}

fn append_omitted(mut lines: Vec<String>, skipped: usize) -> String {
    if skipped > 0 {
        lines.push(format!("... omitted {skipped} low-signal lines"));
    }
    if lines.is_empty() {
        String::new()
    } else {
        lines.join("\n") + "\n"
    }
}

fn tee_raw_output_if_useful(
    command: &[String],
    raw: &str,
    filtered: &str,
    exit_code: i32,
) -> Result<Option<String>> {
    if std::env::var("LM_RESIZER_TEE").ok().as_deref() == Some("0") || raw.len() < 500 {
        return Ok(None);
    }
    let materially_filtered = filtered.len().saturating_mul(2) < raw.len();
    if exit_code == 0 && !materially_filtered {
        return Ok(None);
    }

    let tee_dir = default_state_dir()?.join("tee");
    std::fs::create_dir_all(&tee_dir)?;
    let path = tee_dir.join(format!(
        "{}_{}.log",
        unix_timestamp(),
        sanitize_slug(&command.join("_"))
    ));
    std::fs::write(&path, raw)?;
    cleanup_tee_files(&tee_dir, 20)?;
    Ok(Some(format!("[full output: {}]", path.display())))
}

fn cleanup_tee_files(dir: &Path, max_files: usize) -> Result<()> {
    let mut entries = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "log"))
        .collect::<Vec<_>>();
    if entries.len() <= max_files {
        return Ok(());
    }
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries.iter().take(entries.len() - max_files) {
        let _ = std::fs::remove_file(entry.path());
    }
    Ok(())
}

fn run_tee_command(command: TeeCommand) -> Result<()> {
    match command {
        TeeCommand::List { json } => {
            let report = list_tee_files()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else if report.files.is_empty() {
                println!("No tee files in {}", report.directory);
            } else {
                for file in report.files {
                    println!("{} {} bytes {}", file.name, file.bytes, file.path);
                }
            }
        }
        TeeCommand::Read { file } => {
            let path = resolve_tee_file(&file)?;
            print!("{}", std::fs::read_to_string(path)?);
        }
        TeeCommand::Purge { all, file, json } => {
            let report = purge_tee_files(all, file.as_deref())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("Deleted {} tee files", report.deleted);
            }
        }
    }
    Ok(())
}

fn tee_dir() -> Result<PathBuf> {
    Ok(default_state_dir()?.join("tee"))
}

fn list_tee_files() -> Result<TeeListReport> {
    let dir = tee_dir()?;
    let mut files = Vec::new();
    if dir.exists() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("log") {
                continue;
            }
            let metadata = entry.metadata()?;
            files.push(TeeFileReport {
                name: entry.file_name().to_string_lossy().to_string(),
                path: path.display().to_string(),
                bytes: metadata.len(),
            });
        }
    }
    files.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(TeeListReport {
        directory: dir.display().to_string(),
        files,
    })
}

fn resolve_tee_file(file: &str) -> Result<PathBuf> {
    let dir = tee_dir()?;
    let candidate = PathBuf::from(file);
    let path = if candidate.components().count() == 1 {
        dir.join(candidate)
    } else {
        candidate
    };
    let canonical = path
        .canonicalize()
        .with_context(|| format!("tee file not found: {file}"))?;
    let canonical_dir = dir.canonicalize().unwrap_or(dir);
    if !canonical.starts_with(&canonical_dir) {
        anyhow::bail!(
            "refusing to read tee file outside {}",
            canonical_dir.display()
        );
    }
    Ok(canonical)
}

fn purge_tee_files(all: bool, file: Option<&str>) -> Result<TeePurgeReport> {
    if !all && file.is_none() {
        anyhow::bail!("use --all or --file <name>");
    }
    let mut deleted_files = Vec::new();
    if all {
        for tee in list_tee_files()?.files {
            let path = PathBuf::from(&tee.path);
            if std::fs::remove_file(&path).is_ok() {
                deleted_files.push(tee.path);
            }
        }
    } else if let Some(file) = file {
        let path = resolve_tee_file(file)?;
        std::fs::remove_file(&path)?;
        deleted_files.push(path.display().to_string());
    }
    Ok(TeePurgeReport {
        deleted: deleted_files.len(),
        files: deleted_files,
    })
}

fn sanitize_slug(value: &str) -> String {
    let mut slug = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    slug.truncate(48);
    if slug.is_empty() {
        "command".to_string()
    } else {
        slug
    }
}

fn record_exec_history(report: &ExecReport, elapsed: Duration) -> Result<()> {
    if std::env::var("LM_RESIZER_TRACKING").ok().as_deref() == Some("0") {
        return Ok(());
    }
    let dir = default_state_dir()?;
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("exec-history.jsonl");
    let record = ExecHistoryRecord {
        timestamp_unix: unix_timestamp(),
        command: report.command.clone(),
        exit_code: report.exit_code,
        filter: report.filter.clone(),
        original_bytes: report.original_bytes,
        filtered_bytes: report.filtered_bytes,
        compressed_bytes: report.compressed_bytes,
        bytes_saved: report.bytes_saved,
        duration_ms: elapsed.as_millis(),
    };
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{}", serde_json::to_string(&record)?)?;
    Ok(())
}

fn run_native_hook(client: &str, event: &str) -> NativeHookRunReport {
    let mut input = String::new();
    let read_result = io::stdin().read_to_string(&mut input);
    let value = read_result
        .ok()
        .and_then(|_| serde_json::from_str::<Value>(&input).ok());
    let command = value.as_ref().and_then(extract_hook_command);
    let output = value.as_ref().and_then(extract_hook_output);
    let mut report = NativeHookRunReport {
        client: client.to_string(),
        event: event.to_string(),
        command_found: command.is_some(),
        output_found: output.is_some(),
        recorded: false,
        filter: None,
        bytes_saved: 0,
        error: None,
    };

    let (Some(command), Some(output)) = (command, output) else {
        return report;
    };
    let parts = split_command_for_filter(&command);
    if parts.is_empty() || output.is_empty() {
        return report;
    }
    let (filter, filtered) = filter_command_output(&parts, &output);
    let store = InMemoryCcrStore::default();
    match compress_text(&filtered, &format!("{client} {event} hook"), &store).and_then(
        |compressed| {
            let exec_report = ExecReport {
                command: command.clone(),
                exit_code: extract_hook_exit_code(value.as_ref()).unwrap_or(0),
                filter: filter.clone(),
                original_bytes: output.len(),
                filtered_bytes: filtered.len(),
                compressed_bytes: compressed.compressed_bytes,
                bytes_saved: output.len().saturating_sub(compressed.compressed_bytes),
                compression_steps: compressed.steps_applied,
                cache_keys: compressed.cache_keys,
                tee_hint: None,
                output: compressed.output,
            };
            record_exec_history(&exec_report, Duration::ZERO)?;
            Ok(exec_report)
        },
    ) {
        Ok(exec_report) => {
            report.recorded = true;
            report.filter = Some(exec_report.filter);
            report.bytes_saved = exec_report.bytes_saved;
        }
        Err(err) => {
            report.error = Some(err.to_string());
        }
    }
    report
}

fn extract_hook_command(value: &Value) -> Option<String> {
    for path in [
        "/tool_input/command",
        "/tool_input/cmd",
        "/tool_input/input/command",
        "/tool_input/arguments/command",
        "/input/command",
        "/input/cmd",
        "/arguments/command",
        "/command",
        "/cmd",
    ] {
        if let Some(command) = value.pointer(path).and_then(value_to_command_string) {
            return Some(command);
        }
    }
    find_string_by_key(value, &["command", "cmd"])
}

fn extract_hook_output(value: &Value) -> Option<String> {
    for path in [
        "/tool_response/output",
        "/tool_response/stdout",
        "/tool_response/stderr",
        "/tool_response/content",
        "/tool_output",
        "/output",
        "/stdout",
        "/stderr",
        "/result/output",
        "/result/content",
    ] {
        if let Some(output) = value.pointer(path).and_then(value_to_output_string) {
            return Some(output);
        }
    }
    find_string_by_key(
        value,
        &[
            "tool_output",
            "output",
            "stdout",
            "stderr",
            "content",
            "result",
        ],
    )
}

fn extract_hook_exit_code(value: Option<&Value>) -> Option<i32> {
    let value = value?;
    for path in ["/tool_response/exit_code", "/exit_code", "/status"] {
        if let Some(code) = value.pointer(path).and_then(Value::as_i64) {
            return Some(code as i32);
        }
    }
    None
}

fn value_to_command_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => {
            let parts = items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(" "))
            }
        }
        _ => None,
    }
}

fn value_to_output_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => {
            let text = items
                .iter()
                .filter_map(value_to_output_string)
                .collect::<Vec<_>>()
                .join("\n");
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        }
        Value::Object(_) => serde_json::to_string(value).ok(),
        _ => None,
    }
}

fn find_string_by_key(value: &Value, keys: &[&str]) -> Option<String> {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if keys
                    .iter()
                    .any(|candidate| key.eq_ignore_ascii_case(candidate))
                {
                    if let Some(text) = value_to_output_string(child) {
                        return Some(text);
                    }
                }
            }
            for child in map.values() {
                if let Some(text) = find_string_by_key(child, keys) {
                    return Some(text);
                }
            }
            None
        }
        Value::Array(items) => items
            .iter()
            .find_map(|child| find_string_by_key(child, keys)),
        _ => None,
    }
}

fn summarize_exec_history() -> Result<Value> {
    let path = default_state_dir()?.join("exec-history.jsonl");
    if !path.exists() {
        return Ok(json!({
            "commands": 0,
            "original_bytes": 0,
            "compressed_bytes": 0,
            "bytes_saved": 0,
            "estimated_tokens_saved": 0,
            "by_filter": [],
            "by_command": [],
        }));
    }

    let content = std::fs::read_to_string(path)?;
    let mut commands = 0usize;
    let mut original_bytes = 0usize;
    let mut compressed_bytes = 0usize;
    let mut bytes_saved = 0usize;
    let mut by_filter = std::collections::BTreeMap::<String, (usize, usize)>::new();
    let mut by_command = std::collections::BTreeMap::<String, (usize, usize)>::new();
    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        commands += 1;
        original_bytes += record
            .get("original_bytes")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        compressed_bytes += record
            .get("compressed_bytes")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        bytes_saved += record
            .get("bytes_saved")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let saved = record
            .get("bytes_saved")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let filter = record
            .get("filter")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let command = record
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" ");
        add_history_bucket(&mut by_filter, filter, saved);
        add_history_bucket(&mut by_command, command, saved);
    }

    Ok(json!({
        "commands": commands,
        "original_bytes": original_bytes,
        "compressed_bytes": compressed_bytes,
        "bytes_saved": bytes_saved,
        "estimated_tokens_saved": bytes_saved / 4,
        "by_filter": history_bucket_json(by_filter),
        "by_command": history_bucket_json(by_command),
    }))
}

fn record_retrieval_feedback(hash: &str, bytes: usize, source: &str) -> Result<()> {
    if std::env::var("LM_RESIZER_TRACKING").ok().as_deref() == Some("0") {
        return Ok(());
    }
    let dir = default_state_dir()?;
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("retrieval-feedback.jsonl");
    let record = json!({
        "timestamp_unix": unix_timestamp(),
        "hash": hash,
        "bytes": bytes,
        "source": source,
    });
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{}", serde_json::to_string(&record)?)?;
    Ok(())
}

fn summarize_retrieval_feedback() -> Result<Value> {
    let path = default_state_dir()?.join("retrieval-feedback.jsonl");
    if !path.exists() {
        return Ok(json!({
            "retrievals": 0,
            "bytes": 0,
            "unique_hashes": 0,
            "duplicate_retrievals": 0,
            "by_source": [],
        }));
    }
    let content = std::fs::read_to_string(path)?;
    let mut retrievals = 0usize;
    let mut bytes = 0usize;
    let mut by_source = std::collections::BTreeMap::<String, (usize, usize)>::new();
    let mut by_hash = std::collections::BTreeMap::<String, usize>::new();
    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        retrievals += 1;
        let row_bytes = record.get("bytes").and_then(Value::as_u64).unwrap_or(0) as usize;
        bytes += row_bytes;
        let source = record
            .get("source")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let entry = by_source.entry(source).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += row_bytes;
        if let Some(hash) = record.get("hash").and_then(Value::as_str) {
            *by_hash.entry(hash.to_string()).or_insert(0) += 1;
        }
    }
    let by_source = by_source
        .into_iter()
        .map(|(source, (retrievals, bytes))| {
            json!({ "source": source, "retrievals": retrievals, "bytes": bytes })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "retrievals": retrievals,
        "bytes": bytes,
        "unique_hashes": by_hash.len(),
        "duplicate_retrievals": by_hash.values().map(|count| count.saturating_sub(1)).sum::<usize>(),
        "by_source": by_source,
    }))
}

fn add_history_bucket(
    buckets: &mut std::collections::BTreeMap<String, (usize, usize)>,
    key: String,
    saved: usize,
) {
    let entry = buckets.entry(key).or_insert((0, 0));
    entry.0 += 1;
    entry.1 += saved;
}

fn history_bucket_json(buckets: std::collections::BTreeMap<String, (usize, usize)>) -> Vec<Value> {
    let mut rows = buckets
        .into_iter()
        .map(|(name, (commands, bytes_saved))| {
            json!({
                "name": name,
                "commands": commands,
                "bytes_saved": bytes_saved,
                "estimated_tokens_saved": bytes_saved / 4,
            })
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| {
        std::cmp::Reverse(row.get("bytes_saved").and_then(Value::as_u64).unwrap_or(0))
    });
    rows.truncate(20);
    rows
}

fn format_stats_markdown(report: &Value) -> String {
    let history = report.get("exec_history").unwrap_or(&Value::Null);
    let retrieval_feedback = report.get("retrieval_feedback").unwrap_or(&Value::Null);
    let mut out = String::new();
    out.push_str("# lm-resizer Stats\n\n");
    out.push_str(&format!(
        "- CCR entries: {}\n",
        report.get("entries").and_then(Value::as_u64).unwrap_or(0)
    ));
    out.push_str(&format!(
        "- Exec commands: {}\n",
        history.get("commands").and_then(Value::as_u64).unwrap_or(0)
    ));
    out.push_str(&format!(
        "- Bytes saved: {}\n",
        history
            .get("bytes_saved")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    ));
    out.push_str(&format!(
        "- Estimated tokens saved: {}\n\n",
        history
            .get("estimated_tokens_saved")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    ));
    out.push_str(&format!(
        "- CCR retrievals: {}\n",
        retrieval_feedback
            .get("retrievals")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    ));
    out.push_str(&format!(
        "- Retrieved bytes: {}\n\n",
        retrieval_feedback
            .get("bytes")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    ));

    if let Some(filters) = history.get("by_filter").and_then(Value::as_array) {
        out.push_str("## Top Filters\n\n");
        out.push_str("| Filter | Commands | Bytes saved | Est. tokens saved |\n");
        out.push_str("| --- | ---: | ---: | ---: |\n");
        for row in filters.iter().take(10) {
            out.push_str(&format!(
                "| `{}` | {} | {} | {} |\n",
                markdown_escape(row.get("name").and_then(Value::as_str).unwrap_or("")),
                row.get("commands").and_then(Value::as_u64).unwrap_or(0),
                row.get("bytes_saved").and_then(Value::as_u64).unwrap_or(0),
                row.get("estimated_tokens_saved")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
            ));
        }
        out.push('\n');
    }
    out
}

fn inspect_image(path: &Path) -> Result<ImageReport> {
    let bytes =
        std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let (format, width, height) = image_dimensions(&bytes);
    let recommendation = image_recommendation(bytes.len() as u64, width, height);
    Ok(ImageReport {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        format,
        width,
        height,
        recommendation,
    })
}

fn image_dimensions(bytes: &[u8]) -> (String, Option<u32>, Option<u32>) {
    if bytes.len() >= 24 && bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        return ("png".to_string(), Some(width), Some(height));
    }
    if bytes.len() >= 10 && bytes.starts_with(b"GIF") {
        let width = u16::from_le_bytes([bytes[6], bytes[7]]) as u32;
        let height = u16::from_le_bytes([bytes[8], bytes[9]]) as u32;
        return ("gif".to_string(), Some(width), Some(height));
    }
    if bytes.len() >= 4 && bytes[0] == 0xff && bytes[1] == 0xd8 {
        if let Some((width, height)) = jpeg_dimensions(bytes) {
            return ("jpeg".to_string(), Some(width), Some(height));
        }
        return ("jpeg".to_string(), None, None);
    }
    ("unknown".to_string(), None, None)
}

fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2usize;
    while i + 9 < bytes.len() {
        if bytes[i] != 0xff {
            i += 1;
            continue;
        }
        while i < bytes.len() && bytes[i] == 0xff {
            i += 1;
        }
        if i >= bytes.len() {
            return None;
        }
        let marker = bytes[i];
        i += 1;
        if matches!(marker, 0xd8 | 0xd9) {
            continue;
        }
        if i + 2 > bytes.len() {
            return None;
        }
        let len = u16::from_be_bytes([bytes[i], bytes[i + 1]]) as usize;
        if len < 2 || i + len > bytes.len() {
            return None;
        }
        if matches!(
            marker,
            0xc0 | 0xc1
                | 0xc2
                | 0xc3
                | 0xc5
                | 0xc6
                | 0xc7
                | 0xc9
                | 0xca
                | 0xcb
                | 0xcd
                | 0xce
                | 0xcf
        ) {
            if i + 7 >= bytes.len() {
                return None;
            }
            let height = u16::from_be_bytes([bytes[i + 3], bytes[i + 4]]) as u32;
            let width = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
            return Some((width, height));
        }
        i += len;
    }
    None
}

fn image_recommendation(bytes: u64, width: Option<u32>, height: Option<u32>) -> String {
    let megapixels = width
        .zip(height)
        .map(|(w, h)| (w as u64 * h as u64) as f64 / 1_000_000.0)
        .unwrap_or(0.0);
    if bytes > 2_000_000 || megapixels > 4.0 {
        "large image: downsample or summarize before sending to an LLM".to_string()
    } else if bytes > 500_000 {
        "medium image: prefer resizing if visual detail is not required".to_string()
    } else {
        "small image: safe to keep inline when the model needs visual detail".to_string()
    }
}

fn analyze_voice_transcript(text: &str) -> VoiceReport {
    let fillers = ["um", "uh", "erm", "ah", "like", "basically", "actually"];
    let mut filler_count = 0usize;
    let mut cleaned_words = Vec::new();
    for word in text.split_whitespace() {
        let normalized = word
            .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '\'')
            .to_ascii_lowercase();
        if fillers.contains(&normalized.as_str()) {
            filler_count += 1;
            continue;
        }
        cleaned_words.push(word);
    }
    let cleaned = cleaned_words.join(" ");
    VoiceReport {
        original_chars: text.len(),
        cleaned_chars: cleaned.len(),
        filler_count,
        cleaned,
    }
}

fn ml_status_report() -> MlStatusReport {
    // Whether the ONNX detection path is compiled in (the `magika` feature).
    let magika_compiled = cfg!(feature = "magika");
    // Whether the operator asked for it at runtime.
    let flag_set = std::env::var("LM_RESIZER_ENABLE_MAGIKA")
        .ok()
        .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
    // ONNX actually runs only when both are true.
    let magika_enabled = magika_compiled && flag_set;
    MlStatusReport {
        magika_enabled,
        magika_model: if magika_compiled {
            Some("standard_v3_3 (bundled via the `magika` crate)".to_string())
        } else {
            env_first(&["LM_RESIZER_MAGIKA_MODEL", "MAGIKA_MODEL"])
        },
        onnx_runtime: match (magika_compiled, flag_set) {
            (true, true) => "active (ort runtime, bundled Magika model)".to_string(),
            (true, false) => "compiled in; set LM_RESIZER_ENABLE_MAGIKA=1 to activate".to_string(),
            (false, _) => {
                "not compiled in (build with `--features magika` for ONNX detection)".to_string()
            }
        },
        hot_path: "deterministic local detection unless Magika is compiled in and enabled"
            .to_string(),
    }
}

fn discover_exec_savings(paths: &[PathBuf], recursive: bool) -> Result<DiscoverReport> {
    let files = collect_discover_files(paths, recursive)?;
    let mut report = DiscoverReport {
        files_scanned: files.len(),
        ..DiscoverReport::default()
    };

    for file in files {
        let content = match std::fs::read_to_string(&file) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let source = file.display().to_string();
        let mut file_report = discover_in_content(&content, &source);
        report.command_outputs += file_report.command_outputs;
        report.rewritable_commands += file_report.rewritable_commands;
        report.original_bytes += file_report.original_bytes;
        report.filtered_bytes += file_report.filtered_bytes;
        report.candidates.append(&mut file_report.candidates);
    }

    report.estimated_bytes_saved = report.original_bytes.saturating_sub(report.filtered_bytes);
    report.estimated_tokens_saved = report.estimated_bytes_saved / 4;
    report
        .candidates
        .sort_by_key(|candidate| std::cmp::Reverse(candidate.estimated_bytes_saved));
    report.candidates.truncate(50);
    Ok(report)
}

fn format_discover_markdown(report: &DiscoverReport) -> String {
    let mut out = String::new();
    out.push_str("# lm-resizer Discover Audit\n\n");
    out.push_str(&format!("- Files scanned: {}\n", report.files_scanned));
    out.push_str(&format!(
        "- Command outputs found: {}\n",
        report.command_outputs
    ));
    out.push_str(&format!(
        "- Rewritable commands detected: {}\n",
        report.rewritable_commands
    ));
    out.push_str(&format!(
        "- Estimated bytes saved: {}\n",
        report.estimated_bytes_saved
    ));
    out.push_str(&format!(
        "- Estimated tokens saved: {}\n\n",
        report.estimated_tokens_saved
    ));

    if report.candidates.is_empty() {
        out.push_str("No rewrite candidates found.\n");
        return out;
    }

    out.push_str("| Command | Filter | Original bytes | Filtered bytes | Saved bytes |\n");
    out.push_str("| --- | --- | ---: | ---: | ---: |\n");
    for candidate in report.candidates.iter().take(20) {
        out.push_str(&format!(
            "| `{}` | `{}` | {} | {} | {} |\n",
            markdown_escape(&candidate.command),
            markdown_escape(&candidate.filter),
            candidate.original_bytes,
            candidate.filtered_bytes,
            candidate.estimated_bytes_saved
        ));
    }
    out
}

fn discover_agent_sessions(agent: AgentSessionKind) -> Result<DiscoverSessionsReport> {
    let candidates = agent_session_candidates(agent);
    let mut paths = Vec::new();
    let mut missing = Vec::new();
    for path in candidates {
        if path.exists() {
            paths.push(path);
        } else {
            missing.push(path.display().to_string());
        }
    }
    let discover = if paths.is_empty() {
        DiscoverReport::default()
    } else {
        discover_exec_savings(&paths, true)?
    };
    Ok(DiscoverSessionsReport {
        agent: agent.as_str().to_string(),
        paths: paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>(),
        missing,
        discover,
    })
}

fn format_discover_sessions_markdown(report: &DiscoverSessionsReport) -> String {
    let mut out = String::new();
    out.push_str("# lm-resizer Agent Session Discover\n\n");
    out.push_str(&format!("- Agent: {}\n", markdown_escape(&report.agent)));
    out.push_str(&format!("- Paths scanned: {}\n", report.paths.len()));
    out.push_str(&format!(
        "- Missing known paths: {}\n\n",
        report.missing.len()
    ));
    if !report.paths.is_empty() {
        out.push_str("## Paths\n\n");
        for path in &report.paths {
            out.push_str(&format!("- `{}`\n", markdown_escape(path)));
        }
        out.push('\n');
    }
    out.push_str(&format_discover_markdown(&report.discover));
    out
}

fn agent_session_candidates(agent: AgentSessionKind) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if matches!(agent, AgentSessionKind::All | AgentSessionKind::Codex) {
        if let Ok(codex_home) = std::env::var("CODEX_HOME") {
            paths.extend(codex_session_candidates_from_home(Path::new(&codex_home)));
        }
    }
    if matches!(agent, AgentSessionKind::All | AgentSessionKind::Claude) {
        if let Ok(claude_home) = std::env::var("CLAUDE_CONFIG_DIR") {
            paths.extend(claude_session_candidates_from_home(Path::new(&claude_home)));
        }
    }
    if let Some(home) = user_home_dir() {
        if matches!(agent, AgentSessionKind::All | AgentSessionKind::Codex) {
            paths.extend(codex_session_candidates_from_home(&home.join(".codex")));
        }
        if matches!(agent, AgentSessionKind::All | AgentSessionKind::Claude) {
            paths.extend(claude_session_candidates_from_home(&home.join(".claude")));
            paths.push(home.join(".claude.json"));
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

fn codex_session_candidates_from_home(codex_home: &Path) -> Vec<PathBuf> {
    vec![
        codex_home.join("sessions"),
        codex_home.join("history.jsonl"),
        codex_home.join("logs"),
    ]
}

fn claude_session_candidates_from_home(claude_home: &Path) -> Vec<PathBuf> {
    vec![
        claude_home.join("projects"),
        claude_home.join("todos"),
        claude_home.join("transcripts"),
    ]
}

fn user_home_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(PathBuf::from)
}

fn run_eval(paths: &[PathBuf], recursive: bool) -> Result<EvalReport> {
    let discover = discover_exec_savings(paths, recursive)?;
    let mut notes = Vec::new();
    if discover.command_outputs == 0 {
        notes.push("no command outputs found in fixtures".to_string());
    }
    if discover.candidates.is_empty() {
        notes.push("no compression candidates found".to_string());
    }
    if discover.estimated_tokens_saved > 0 {
        notes.push(format!(
            "estimated {} tokens saved by command-output filtering",
            discover.estimated_tokens_saved
        ));
    }
    Ok(EvalReport {
        files_scanned: discover.files_scanned,
        command_outputs: discover.command_outputs,
        candidates: discover.candidates.len(),
        estimated_bytes_saved: discover.estimated_bytes_saved,
        estimated_tokens_saved: discover.estimated_tokens_saved,
        pass: discover.files_scanned > 0,
        notes,
    })
}

fn format_eval_markdown(report: &EvalReport) -> String {
    let mut out = String::new();
    out.push_str("# lm-resizer Eval\n\n");
    out.push_str(&format!("- Pass: {}\n", report.pass));
    out.push_str(&format!("- Files scanned: {}\n", report.files_scanned));
    out.push_str(&format!("- Command outputs: {}\n", report.command_outputs));
    out.push_str(&format!("- Candidates: {}\n", report.candidates));
    out.push_str(&format!(
        "- Estimated bytes saved: {}\n",
        report.estimated_bytes_saved
    ));
    out.push_str(&format!(
        "- Estimated tokens saved: {}\n\n",
        report.estimated_tokens_saved
    ));
    if !report.notes.is_empty() {
        out.push_str("## Notes\n\n");
        for note in &report.notes {
            out.push_str(&format!("- {}\n", markdown_escape(note)));
        }
    }
    out
}

fn markdown_escape(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

fn run_learn(
    paths: Vec<PathBuf>,
    recursive: bool,
    project_dir: Option<PathBuf>,
    write: bool,
    install: bool,
    client: &str,
) -> Result<LearnReport> {
    let project_dir = project_dir.unwrap_or(std::env::current_dir()?);
    let discover = discover_exec_savings(&paths, recursive)?;
    let exec_history = summarize_exec_history().unwrap_or_else(|_| json!({}));
    let recommendations = build_learn_recommendations(&discover, &exec_history);
    let markdown = format_learn_markdown(&recommendations, &discover, &exec_history);
    let mut memory_file = None;

    if write || install {
        let learning_dir = project_dir.join(".lm-resizer").join("learning");
        std::fs::create_dir_all(&learning_dir)?;
        let path = learning_dir.join("recommendations.json");
        let memory = json!({
            "generated_at_unix": unix_timestamp(),
            "discover": discover,
            "exec_history": exec_history,
            "recommendations": recommendations,
        });
        std::fs::write(&path, serde_json::to_string_pretty(&memory)?)?;
        let markdown_path = learning_dir.join("AGENT_RECOMMENDATIONS.md");
        std::fs::write(&markdown_path, &markdown)?;
        memory_file = Some(path.display().to_string());
    }

    let mut instruction_files = Vec::new();
    if install {
        for target in hook_instruction_targets(client, &project_dir)? {
            upsert_learning_block(&target, &markdown)?;
            instruction_files.push(target.display().to_string());
        }
    }

    Ok(LearnReport {
        project_dir: project_dir.display().to_string(),
        files_scanned: discover.files_scanned,
        command_outputs: discover.command_outputs,
        recommendations,
        memory_file,
        instruction_files,
        markdown,
    })
}

fn build_learn_recommendations(
    discover: &DiscoverReport,
    exec_history: &Value,
) -> Vec<LearnRecommendation> {
    let mut recommendations = Vec::new();

    if discover.estimated_bytes_saved > 0 {
        recommendations.push(LearnRecommendation {
            title: "Route noisy commands through lm-resizer".to_string(),
            reason: format!(
                "Session mining found {} rewritable command outputs with about {} tokens saved.",
                discover.rewritable_commands, discover.estimated_tokens_saved
            ),
            instruction: "Before running noisy commands such as tests, builds, searches, package installs, infra CLIs, or large listings, run `lm-resizer rewrite-shell \"<command>\"`; if it returns `lm-resizer exec -- ...`, use the rewritten command.".to_string(),
            evidence: discover
                .candidates
                .iter()
                .take(5)
                .map(|candidate| {
                    format!(
                        "{} via {} saved {} bytes",
                        candidate.command, candidate.filter, candidate.estimated_bytes_saved
                    )
                })
                .collect(),
        });
    }

    let commands = exec_history
        .get("commands")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let history_saved = exec_history
        .get("estimated_tokens_saved")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if commands > 0 {
        recommendations.push(LearnRecommendation {
            title: "Keep command-output savings visible".to_string(),
            reason: format!(
                "`lm-resizer exec` history contains {commands} commands and about {history_saved} estimated tokens saved."
            ),
            instruction: "Use `lm-resizer stats --markdown` during long agent sessions to review which filters save context and which command families deserve project-specific TOML filters.".to_string(),
            evidence: learn_history_evidence(exec_history),
        });
    }

    if discover
        .candidates
        .iter()
        .any(|candidate| candidate.filter == "generic")
    {
        recommendations.push(LearnRecommendation {
            title: "Create project TOML filters for repeated generic output".to_string(),
            reason: "Some large outputs only matched the generic filter, which means a project-specific rule can usually preserve better signal.".to_string(),
            instruction: "Add repeated project log shapes to `.lm-resizer/filters.toml`, cover them with inline `[[tests]]`, then run `lm-resizer verify-filters` and `lm-resizer trust-filters` before relying on them.".to_string(),
            evidence: discover
                .candidates
                .iter()
                .filter(|candidate| candidate.filter == "generic")
                .take(5)
                .map(|candidate| {
                    format!(
                        "{} from {} saved {} bytes",
                        candidate.command, candidate.source, candidate.estimated_bytes_saved
                    )
                })
                .collect(),
        });
    }

    if recommendations.is_empty() {
        recommendations.push(LearnRecommendation {
            title: "No durable lm-resizer guidance yet".to_string(),
            reason: "The scanned sessions did not contain enough compressible command output to justify installing agent rules.".to_string(),
            instruction: "Keep using `lm-resizer exec` for known-noisy commands; rerun `lm-resizer learn` after longer Claude/Codex sessions.".to_string(),
            evidence: vec![format!("files scanned: {}", discover.files_scanned)],
        });
    }

    recommendations
}

fn learn_history_evidence(exec_history: &Value) -> Vec<String> {
    exec_history
        .get("by_filter")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(5)
        .map(|row| {
            format!(
                "{}: {} commands, {} bytes saved",
                row.get("name").and_then(Value::as_str).unwrap_or("unknown"),
                row.get("commands").and_then(Value::as_u64).unwrap_or(0),
                row.get("bytes_saved").and_then(Value::as_u64).unwrap_or(0)
            )
        })
        .collect()
}

fn format_learn_markdown(
    recommendations: &[LearnRecommendation],
    discover: &DiscoverReport,
    exec_history: &Value,
) -> String {
    let mut out = String::new();
    out.push_str("# lm-resizer Learned Agent Guidance\n\n");
    out.push_str(&format!("- Files scanned: {}\n", discover.files_scanned));
    out.push_str(&format!(
        "- Command outputs found: {}\n",
        discover.command_outputs
    ));
    out.push_str(&format!(
        "- Estimated discover tokens saved: {}\n",
        discover.estimated_tokens_saved
    ));
    out.push_str(&format!(
        "- Exec history commands: {}\n\n",
        exec_history
            .get("commands")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    ));

    for recommendation in recommendations {
        out.push_str(&format!("## {}\n\n", recommendation.title));
        out.push_str(&format!("Reason: {}\n\n", recommendation.reason));
        out.push_str(&format!("Instruction: {}\n\n", recommendation.instruction));
        if !recommendation.evidence.is_empty() {
            out.push_str("Evidence:\n");
            for item in &recommendation.evidence {
                out.push_str(&format!("- {}\n", markdown_escape(item)));
            }
            out.push('\n');
        }
    }

    out
}

fn init_hook_helpers(project_dir: Option<PathBuf>, force: bool) -> Result<InitHooksReport> {
    let project_dir = project_dir.unwrap_or(std::env::current_dir()?);
    let hook_dir = project_dir.join(".lm-resizer").join("hooks");
    std::fs::create_dir_all(&hook_dir)?;
    let exe_path = std::env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "lm-resizer".to_string());

    let files = [
        ("rewrite.sh", hook_rewrite_sh(&exe_path)),
        ("rewrite.ps1", hook_rewrite_ps1(&exe_path)),
        ("AGENT_RULES.md", hook_agent_rules()),
        ("README.md", hook_readme()),
    ];

    let mut written = Vec::new();
    for (name, content) in files {
        let path = hook_dir.join(name);
        if path.exists() && !force {
            anyhow::bail!(
                "hook helper already exists: {} (rerun with --force to overwrite)",
                path.display()
            );
        }
        std::fs::write(&path, content)?;
        written.push(path.display().to_string());
    }

    Ok(InitHooksReport {
        directory: hook_dir.display().to_string(),
        files: written,
    })
}

fn init_native_hooks(
    client: &str,
    project_dir: Option<PathBuf>,
    force: bool,
) -> Result<NativeHooksReport> {
    let project_dir = project_dir.unwrap_or(std::env::current_dir()?);
    let exe_path = std::env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "lm-resizer".to_string());
    let mut files = Vec::new();
    for target in native_hook_targets(client, &project_dir)? {
        if target.path.exists() && !force {
            anyhow::bail!(
                "native hook config already exists: {} (rerun with --force to overwrite)",
                target.path.display()
            );
        }
        if let Some(parent) = target.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = match target.client.as_str() {
            "codex" => codex_native_hooks_json(&exe_path)?,
            "claude" => claude_native_hooks_json(&exe_path)?,
            _ => unreachable!("validated native hook target"),
        };
        std::fs::write(&target.path, content)?;
        files.push(target.path.display().to_string());
    }
    Ok(NativeHooksReport {
        project_dir: project_dir.display().to_string(),
        files,
    })
}

struct NativeHookTarget {
    client: String,
    path: PathBuf,
}

fn native_hook_targets(client: &str, project_dir: &Path) -> Result<Vec<NativeHookTarget>> {
    match client {
        "codex" => Ok(vec![NativeHookTarget {
            client: "codex".to_string(),
            path: project_dir.join(".codex").join("hooks.json"),
        }]),
        "claude" | "claude-code" => Ok(vec![NativeHookTarget {
            client: "claude".to_string(),
            path: project_dir.join(".claude").join("settings.json"),
        }]),
        "all" => Ok(vec![
            NativeHookTarget {
                client: "codex".to_string(),
                path: project_dir.join(".codex").join("hooks.json"),
            },
            NativeHookTarget {
                client: "claude".to_string(),
                path: project_dir.join(".claude").join("settings.json"),
            },
        ]),
        other => {
            anyhow::bail!("unsupported native hook client '{other}'. Use codex, claude, or all")
        }
    }
}

fn codex_native_hooks_json(exe_path: &str) -> Result<String> {
    native_hooks_json(exe_path, "codex", "PostToolUse", "Bash")
}

fn claude_native_hooks_json(exe_path: &str) -> Result<String> {
    native_hooks_json(exe_path, "claude", "PostToolUse", "Bash")
}

fn native_hooks_json(exe_path: &str, client: &str, event: &str, matcher: &str) -> Result<String> {
    let command = format!("\"{exe_path}\" hook --client {client} --event {event}");
    let mut hook = json!({
        "type": "command",
        "command": command,
        "timeout": 30
    });
    if cfg!(windows) {
        hook["commandWindows"] = hook["command"].clone();
    }
    let config = json!({
        "hooks": {
            event: [{
                "matcher": matcher,
                "hooks": [hook]
            }]
        }
    });
    Ok(serde_json::to_string_pretty(&config)?)
}

fn init_command_shims(project_dir: Option<PathBuf>, force: bool) -> Result<ShimReport> {
    let project_dir = project_dir.unwrap_or(std::env::current_dir()?);
    let shim_dir = project_dir.join(".lm-resizer").join("shims");
    std::fs::create_dir_all(&shim_dir)?;
    let exe_path = std::env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "lm-resizer".to_string());
    let commands = [
        "git",
        "cargo",
        "rg",
        "grep",
        "find",
        "fd",
        "ls",
        "tree",
        "npm",
        "pnpm",
        "yarn",
        "pytest",
        "tsc",
        "terraform",
        "tofu",
        "docker",
        "podman",
        "kubectl",
        "aws",
        "go",
        "dotnet",
        "mvn",
        "gradle",
        "pip",
        "uv",
        "make",
        "gh",
    ];
    let mut files = Vec::new();
    let mut skipped = Vec::new();
    for command in commands {
        let Some(original) = resolve_command_path(command) else {
            skipped.push(format!("{command}: not found on PATH"));
            continue;
        };
        if original.starts_with(&shim_dir) {
            skipped.push(format!("{command}: resolves inside shim directory"));
            continue;
        }
        let name = if cfg!(windows) {
            format!("{command}.cmd")
        } else {
            command.to_string()
        };
        let path = shim_dir.join(name);
        if path.exists() && !force {
            anyhow::bail!(
                "shim already exists: {} (rerun with --force to overwrite)",
                path.display()
            );
        }
        let content = if cfg!(windows) {
            command_shim_cmd(&exe_path, &original)
        } else {
            command_shim_sh(&exe_path, &original)
        };
        std::fs::write(&path, content)?;
        files.push(path.display().to_string());
    }

    Ok(ShimReport {
        directory: shim_dir.display().to_string(),
        files,
        skipped,
        path_hint: shim_path_hint(&shim_dir),
    })
}

fn command_shim_cmd(exe_path: &str, original: &Path) -> String {
    format!(
        r#"@echo off
"{exe_path}" exec -- "{original}" %*
exit /b %ERRORLEVEL%
"#,
        original = original.display()
    )
}

fn command_shim_sh(exe_path: &str, original: &Path) -> String {
    format!(
        r#"#!/usr/bin/env sh
exec "{exe_path}" exec -- "{original}" "$@"
"#,
        original = original.display()
    )
}

fn shim_path_hint(shim_dir: &Path) -> String {
    if cfg!(windows) {
        format!(
            "PowerShell: $env:PATH = '{};' + $env:PATH",
            shim_dir.display()
        )
    } else {
        format!("sh: export PATH=\"{}:$PATH\"", shim_dir.display())
    }
}

fn hook_rewrite_sh(exe_path: &str) -> String {
    format!(
        r#"#!/usr/bin/env sh
set -eu

LM_RESIZER_BIN="${{LM_RESIZER_BIN:-{exe_path}}}"
if [ "$#" -eq 1 ]; then
  exec "$LM_RESIZER_BIN" rewrite-shell "$1"
fi
exec "$LM_RESIZER_BIN" rewrite -- "$@"
"#
    )
}

fn hook_rewrite_ps1(exe_path: &str) -> String {
    format!(
        r#"$ErrorActionPreference = "Stop"
$bin = if ($env:LM_RESIZER_BIN) {{ $env:LM_RESIZER_BIN }} else {{ "{exe_path}" }}
if ($args.Count -eq 1) {{
  & $bin rewrite-shell $args[0]
  exit $LASTEXITCODE
}}
& $bin rewrite -- @args
"#
    )
}

fn hook_readme() -> String {
    r#"# lm-resizer hook helpers

These helpers are opt-in building blocks for agent integrations.

They do not execute the target command. They call:

```bash
lm-resizer rewrite -- <command>
```

When a hook passes a full shell line as one argument, they call:

```bash
lm-resizer rewrite-shell "<command && next>"
```

Use `rewrite.sh` for Bash-like hooks and `rewrite.ps1` for PowerShell-based
hooks. Set `LM_RESIZER_BIN` to override the binary path captured when the files
were generated.

Example:

```bash
.lm-resizer/hooks/rewrite.sh git status
```

Expected output for supported commands:

```bash
lm-resizer exec -- git status
```
"#
    .to_string()
}

fn hook_agent_rules() -> String {
    r#"# lm-resizer Agent Rules

When you are about to run a noisy shell command, ask lm-resizer how it should be
rewritten first.

For argv-style command calls:

```bash
.lm-resizer/hooks/rewrite.sh git status
```

For a full shell line:

```bash
.lm-resizer/hooks/rewrite.sh "cargo test && git status"
```

If the helper returns a command beginning with `lm-resizer exec --`, run that
returned command instead of the original. If it returns the original command,
run the original command.

Useful target command families include `git`, `cargo`, `rg`/`grep`, listings,
Terraform/OpenTofu, Docker/Podman, `tsc`, and `pytest`.

Do not use lm-resizer for commands whose raw output is intentionally consumed by
another program unless `rewrite-shell` left that pipe consumer unchanged.
"#
    .to_string()
}

const HOOK_BLOCK_START: &str = "<!-- LM-RESIZER:HOOKS:START -->";
const HOOK_BLOCK_END: &str = "<!-- LM-RESIZER:HOOKS:END -->";
const LEARN_BLOCK_START: &str = "<!-- LM-RESIZER:LEARN:START -->";
const LEARN_BLOCK_END: &str = "<!-- LM-RESIZER:LEARN:END -->";

fn install_agent_hooks(
    client: &str,
    project_dir: Option<PathBuf>,
    force: bool,
) -> Result<AgentHooksReport> {
    let project_dir = project_dir.unwrap_or(std::env::current_dir()?);
    let helpers = init_hook_helpers(Some(project_dir.clone()), force)?;
    let targets = hook_instruction_targets(client, &project_dir)?;
    let block = hook_instruction_block();
    let mut updated = Vec::new();
    for path in targets {
        upsert_marked_block(&path, &block)?;
        updated.push(path.display().to_string());
    }
    Ok(AgentHooksReport {
        helper_directory: helpers.directory,
        instruction_files: updated,
    })
}

fn uninstall_agent_hooks(
    client: &str,
    project_dir: Option<PathBuf>,
) -> Result<UninstallHooksReport> {
    let project_dir = project_dir.unwrap_or(std::env::current_dir()?);
    let targets = hook_instruction_targets(client, &project_dir)?;
    let mut updated = Vec::new();
    let mut removed = 0usize;
    for path in targets {
        if remove_marked_block(&path)? {
            removed += 1;
            updated.push(path.display().to_string());
        }
    }
    Ok(UninstallHooksReport {
        instruction_files: updated,
        removed,
    })
}

fn hook_instruction_targets(client: &str, project_dir: &Path) -> Result<Vec<PathBuf>> {
    match client {
        "codex" => Ok(vec![project_dir.join("AGENTS.md")]),
        "claude" | "claude-code" => Ok(vec![project_dir.join("CLAUDE.md")]),
        "all" => Ok(vec![
            project_dir.join("AGENTS.md"),
            project_dir.join("CLAUDE.md"),
        ]),
        other => anyhow::bail!("unsupported hook client '{other}'. Use codex, claude, or all"),
    }
}

fn hook_instruction_block() -> String {
    format!(
        r#"{HOOK_BLOCK_START}

## lm-resizer command output reduction

Before running noisy shell commands, ask the local helper how to rewrite them:

```bash
.lm-resizer/hooks/rewrite.sh "cargo test && git status"
```

If the helper returns a command beginning with `lm-resizer exec --`, run that
returned command instead of the original. If it returns the original command,
run the original command.

Use this for command families such as `git`, `cargo`, `rg`/`grep`, listings,
Terraform/OpenTofu, package installs, Go, .NET, Python linters, JS test/build
tools, Docker/Podman, Kubernetes, and AWS CLI.

{HOOK_BLOCK_END}
"#
    )
}

fn upsert_learning_block(path: &Path, markdown: &str) -> Result<()> {
    let block = format!(
        "{LEARN_BLOCK_START}\n\n{}\n{LEARN_BLOCK_END}\n",
        learn_agent_block(markdown).trim()
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let existing = if path.exists() {
        std::fs::read_to_string(path)?
    } else {
        String::new()
    };
    let stripped = strip_block_between(&existing, LEARN_BLOCK_START, LEARN_BLOCK_END);
    let mut next = stripped.trim_end().to_string();
    if !next.is_empty() {
        next.push_str("\n\n");
    }
    next.push_str(block.trim());
    next.push('\n');
    std::fs::write(path, next)?;
    Ok(())
}

fn learn_agent_block(markdown: &str) -> String {
    let mut out = String::new();
    out.push_str("## lm-resizer learned guidance\n\n");
    let mut copied = 0usize;
    for line in markdown.lines() {
        if line.starts_with("# ") {
            continue;
        }
        if line.starts_with("- ") || line.starts_with("Reason:") || line.starts_with("Evidence:") {
            continue;
        }
        if line.starts_with("## ") || line.starts_with("Instruction:") {
            out.push_str(line);
            out.push('\n');
            copied += 1;
        }
        if copied >= 12 {
            break;
        }
    }
    out.push_str("\nRefresh this block with `lm-resizer learn <session logs> --install` after long sessions.\n");
    out
}

fn upsert_marked_block(path: &Path, block: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let existing = if path.exists() {
        std::fs::read_to_string(path)?
    } else {
        String::new()
    };
    let stripped = strip_marked_block(&existing);
    let mut next = stripped.trim_end().to_string();
    if !next.is_empty() {
        next.push_str("\n\n");
    }
    next.push_str(block.trim());
    next.push('\n');
    std::fs::write(path, next)?;
    Ok(())
}

fn remove_marked_block(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let existing = std::fs::read_to_string(path)?;
    let stripped = strip_marked_block(&existing);
    let removed = stripped != existing;
    if removed {
        std::fs::write(path, stripped.trim_end().to_string() + "\n")?;
    }
    Ok(removed)
}

fn strip_marked_block(content: &str) -> String {
    strip_block_between(content, HOOK_BLOCK_START, HOOK_BLOCK_END)
}

fn strip_block_between(content: &str, start_marker: &str, end_marker: &str) -> String {
    let Some(start) = content.find(start_marker) else {
        return content.to_string();
    };
    let Some(end_rel) = content[start..].find(end_marker) else {
        return content.to_string();
    };
    let end = start + end_rel + end_marker.len();
    let mut out = String::new();
    out.push_str(&content[..start]);
    out.push_str(&content[end..]);
    out.trim().to_string()
}

fn collect_discover_files(paths: &[PathBuf], recursive: bool) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_file() {
            files.push(path.clone());
        } else if path.is_dir() {
            if recursive {
                for entry in WalkDir::new(path).into_iter().filter_map(Result::ok) {
                    if entry.file_type().is_file() && discover_file_allowed(entry.path()) {
                        files.push(entry.path().to_path_buf());
                    }
                }
            } else {
                for entry in std::fs::read_dir(path)? {
                    let entry = entry?;
                    let entry_path = entry.path();
                    if entry_path.is_file() && discover_file_allowed(&entry_path) {
                        files.push(entry_path);
                    }
                }
            }
        }
    }
    Ok(files)
}

fn discover_file_allowed(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("jsonl" | "json" | "log" | "txt" | "md")
    )
}

fn discover_in_content(content: &str, source: &str) -> DiscoverReport {
    let mut report = DiscoverReport::default();
    let mut pending_command: Option<String> = None;

    for line in content.lines() {
        if let Ok(value) = serde_json::from_str::<Value>(line) {
            if let Some(command) = extract_command_from_value(&value) {
                report.rewritable_commands += usize::from(command_has_specific_filter(&command));
                pending_command = Some(command);
            }
            if let Some(output) = extract_output_from_value(&value) {
                if let Some(command) = pending_command.as_deref() {
                    add_discover_candidate(&mut report, command, &output, source);
                }
            }
            continue;
        }

        if let Some(command) = extract_plain_command(line) {
            report.rewritable_commands += usize::from(command_has_specific_filter(&command));
            pending_command = Some(command);
        }
    }

    report
}

fn add_discover_candidate(report: &mut DiscoverReport, command: &str, output: &str, source: &str) {
    if output.trim().is_empty() {
        return;
    }
    let command_parts = split_command_for_filter(command);
    if command_parts.is_empty() {
        return;
    }
    let (filter, filtered) = filter_command_output(&command_parts, output);
    report.command_outputs += 1;
    report.original_bytes += output.len();
    report.filtered_bytes += filtered.len();
    if filter != "generic" || filtered.len() < output.len() {
        report.candidates.push(DiscoverCandidate {
            command: command.to_string(),
            filter,
            original_bytes: output.len(),
            filtered_bytes: filtered.len(),
            estimated_bytes_saved: output.len().saturating_sub(filtered.len()),
            source: source.to_string(),
        });
    }
}

fn extract_command_from_value(value: &Value) -> Option<String> {
    if let Some(command) =
        find_string_for_keys(value, &["command", "cmd", "shell_command", "bash_command"])
    {
        if split_command_for_filter(&command).len() >= 2 {
            return Some(command);
        }
    }

    if json_tool_name(value)
        .as_deref()
        .is_some_and(|name| matches!(name, "Bash" | "bash" | "shell" | "exec_command"))
    {
        return find_string_for_keys(value, &["input", "arguments"]).and_then(|text| {
            serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|parsed| extract_command_from_value(&parsed))
                .or_else(|| {
                    if command_has_specific_filter(&text) {
                        Some(text)
                    } else {
                        None
                    }
                })
        });
    }

    None
}

fn extract_output_from_value(value: &Value) -> Option<String> {
    let output = find_string_for_keys(
        value,
        &[
            "output",
            "stdout",
            "stderr",
            "tool_output",
            "result",
            "content",
            "text",
        ],
    )?;
    if output.trim().is_empty() {
        None
    } else {
        Some(output)
    }
}

fn json_tool_name(value: &Value) -> Option<String> {
    find_string_for_keys(value, &["name", "tool_name", "tool"])
}

fn find_string_for_keys(value: &Value, keys: &[&str]) -> Option<String> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(found) = map.get(*key).and_then(Value::as_str) {
                    return Some(found.to_string());
                }
                if let Some(found) = map.get(*key).and_then(json_content_to_text) {
                    return Some(found);
                }
            }
            for child in map.values() {
                if let Some(found) = find_string_for_keys(child, keys) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(items) => items
            .iter()
            .find_map(|child| find_string_for_keys(child, keys)),
        _ => None,
    }
}

fn json_content_to_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => {
            let mut parts = Vec::new();
            for item in items {
                if let Some(text) = item.as_str() {
                    parts.push(text.to_string());
                    continue;
                }
                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    parts.push(text.to_string());
                }
            }
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n"))
            }
        }
        _ => None,
    }
}

fn extract_plain_command(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let command = trimmed
        .strip_prefix("$ ")
        .or_else(|| trimmed.strip_prefix("> "))
        .unwrap_or(trimmed);
    if command_has_specific_filter(command) {
        Some(command.to_string())
    } else {
        None
    }
}

fn command_has_specific_filter(command: &str) -> bool {
    let parts = split_command_for_filter(command);
    if parts.is_empty() {
        return false;
    }
    let (filter, _) = filter_command_output(&parts, "");
    filter != "generic" && filter != "none"
}

fn split_command_for_filter(command: &str) -> Vec<String> {
    command
        .split_whitespace()
        .map(|part| {
            part.trim_matches('"')
                .trim_matches('\'')
                .trim_end_matches(';')
                .to_string()
        })
        .filter(|part| !part.is_empty())
        .collect()
}

fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

const BUILTIN_EXEC_FILTERS_TOML: &str = r#"
[[filters]]
name = "terraform-plan"
match_command = "^(terraform|tofu)\\s+plan\\b"
strip_ansi = true
keep_lines_matching = [
  "^Plan:",
  "^No changes",
  "^\\s*[~+\\-]",
  "^Error:",
  "^Warning:",
]
max_lines = 120
on_empty = "terraform plan: no relevant changes"

[[filters]]
name = "docker-ps"
match_command = "^(docker|podman)\\s+ps\\b"
strip_ansi = true
max_lines = 80

[[filters]]
name = "systemctl-status"
match_command = "^systemctl\\s+status\\b"
strip_ansi = true
keep_lines_matching = [
  "Loaded:",
  "Active:",
  "Main PID:",
  "^\\s*Process:",
  "^\\s*[A-Z][a-z]{2} ",
  "error|failed|warning",
]
max_lines = 80

[[filters]]
name = "package-install"
match_command = "^(npm|pnpm|yarn)\\s+(install|i|add)\\b"
strip_ansi = true
strip_lines_matching = [
  "^\\s*$",
  "^Progress:",
  "^\\s*[\\|/\\-\\\\]$",
  "^\\s*resolved ",
  "^\\s*reused ",
  "^\\s*downloaded ",
]
keep_lines_matching = [
  "added ",
  "removed ",
  "changed ",
  "audited ",
  "vulnerab",
  "deprecated",
  "WARN",
  "ERR!",
  "error",
  "failed",
]
max_lines = 120
on_empty = "package install: completed"

[[filters]]
name = "brew-install"
match_command = "^brew\\s+(install|upgrade)\\b"
strip_ansi = true
strip_lines_matching = [
  "^==> Downloading",
  "^==> Pouring",
  "^Already downloaded:",
  "^\\s*$",
]
keep_lines_matching = [
  "^==>",
  "Error:",
  "Warning:",
  "installed",
  "upgraded",
  "Pouring",
]
max_lines = 120
on_empty = "brew: completed"

[[filters]]
name = "make"
match_command = "^(g?make|make)\\b"
strip_ansi = true
keep_lines_matching = [
  "error",
  "Error",
  "warning",
  "Warning",
  "failed",
  "FAILED",
  "Entering directory",
  "Leaving directory",
]
max_lines = 160
on_empty = "make: completed"

[[filters]]
name = "gh"
match_command = "^gh\\s+(pr|issue|run|workflow)\\b"
strip_ansi = true
strip_lines_matching = ["^\\s*$"]
max_lines = 120

[[filters]]
name = "go-test"
match_command = "^go\\s+test\\b"
strip_ansi = true
keep_lines_matching = [
  "^--- FAIL:",
  "^FAIL",
  "^ok\\s",
  "^\\?",
  "panic:",
  "Error Trace:",
  "error",
]
max_lines = 160
on_empty = "go test: passed"

[[filters]]
name = "dotnet"
match_command = "^dotnet\\s+(test|build)\\b"
strip_ansi = true
keep_lines_matching = [
  "FAILED",
  "Failed",
  "Error",
  "error",
  "Warning",
  "warning",
  "Passed!",
  "Total tests:",
  "Build FAILED",
  "Build succeeded",
]
max_lines = 180
on_empty = "dotnet: completed"

[[filters]]
name = "jvm-build"
match_command = "^(mvn|gradle|\\.\\/gradlew|gradlew)(\\s|$)"
strip_ansi = true
keep_lines_matching = [
  "\\[ERROR\\]",
  "\\[WARNING\\]",
  "BUILD SUCCESS",
  "BUILD FAILURE",
  "FAILURE:",
  "FAILED",
  "Failed",
  "error",
  "warning",
  "Tests run:",
]
max_lines = 180
on_empty = "jvm build: completed"

[[filters]]
name = "python-package"
match_command = "^(pip|pipx|uv)\\s+(install|sync|add|remove|pip)\\b"
strip_ansi = true
strip_lines_matching = [
  "^\\s*$",
  "^Collecting ",
  "^Downloading ",
  "^Installing collected packages:",
  "^Using cached ",
]
keep_lines_matching = [
  "Successfully installed",
  "Successfully uninstalled",
  "Resolved ",
  "Installed ",
  "Audited ",
  "WARNING:",
  "ERROR:",
  "error",
  "failed",
]
max_lines = 140
on_empty = "python package: completed"

[[filters]]
name = "python-lint"
match_command = "^(ruff|mypy)\\b"
strip_ansi = true
keep_lines_matching = [
  "^[^\\s].*:[0-9]+",
  "^error:",
  "^warning:",
  "Found ",
  "Success:",
]
max_lines = 180
on_empty = "python lint: clean"

[[filters]]
name = "js-quality"
match_command = "^(eslint|vitest|playwright|next)\\b|^(npm|pnpm|yarn)\\s+(run\\s+)?(lint|test|build)\\b"
strip_ansi = true
keep_lines_matching = [
  "FAIL",
  "failed",
  "Failed",
  "Error",
  "error",
  "Warning",
  "warning",
  "✓",
  "passed",
  "Tests",
  "Duration",
  "Compiled",
]
max_lines = 180
on_empty = "js quality: completed"

[[filters]]
name = "docker-logs"
match_command = "^(docker|podman)\\s+logs\\b"
strip_ansi = true
keep_lines_matching = [
  "error",
  "ERROR",
  "warn",
  "WARN",
  "failed",
  "FAILED",
  "panic",
  "Exception",
]
tail_lines = 160
on_empty = "container logs: no warnings or errors"

[[filters]]
name = "kubectl"
match_command = "^kubectl\\s+(get|describe|logs|events)\\b"
strip_ansi = true
keep_lines_matching = [
  "^NAME\\s",
  "Error",
  "Warning",
  "Failed",
  "BackOff",
  "CrashLoop",
  "Pending",
  "Running",
  "Ready",
  "Events:",
]
max_lines = 180
on_empty = "kubectl: no relevant warnings"

[[filters]]
name = "aws"
match_command = "^aws\\s+"
strip_ansi = true
keep_lines_matching = [
  "Error",
  "ERROR",
  "Failed",
  "FAILED",
  "Arn",
  "Name",
  "State",
  "Status",
  "FunctionName",
  "InstanceId",
  "StackName",
]
max_lines = 180
on_empty = "aws: completed"
"#;

fn compress_batch(options: BatchOptions) -> Result<BatchReport> {
    let files = collect_batch_files(&options.paths, options.recursive, &options.extensions)?;
    if let Some(write_dir) = &options.write_dir {
        std::fs::create_dir_all(write_dir)?;
    }

    let store: Arc<dyn CcrStore> = Arc::from(open_store(options.store)?);
    let pipeline = Arc::new(build_pipeline());
    let query = Arc::new(options.query);
    let write_dir = Arc::new(options.write_dir);

    let run = || {
        files
            .par_iter()
            .map(|path| {
                compress_batch_file(
                    path,
                    query.as_str(),
                    store.as_ref(),
                    pipeline.as_ref(),
                    write_dir.as_deref(),
                )
            })
            .collect::<Vec<_>>()
    };

    let items = if let Some(jobs) = options.jobs {
        rayon::ThreadPoolBuilder::new()
            .num_threads(jobs)
            .build()?
            .install(run)
    } else {
        run()
    };

    let mut report = BatchReport {
        files: items.len(),
        ok: 0,
        failed: 0,
        original_bytes: 0,
        compressed_bytes: 0,
        bytes_saved: 0,
        items,
    };
    for item in &report.items {
        if item.ok {
            report.ok += 1;
            report.original_bytes += item.original_bytes.unwrap_or_default();
            report.compressed_bytes += item.compressed_bytes.unwrap_or_default();
            report.bytes_saved += item.bytes_saved.unwrap_or_default();
        } else {
            report.failed += 1;
        }
    }
    Ok(report)
}

fn collect_batch_files(
    paths: &[PathBuf],
    recursive: bool,
    extensions: &[String],
) -> Result<Vec<PathBuf>> {
    let ext_filter = extensions
        .iter()
        .map(|ext| ext.trim_start_matches('.').to_ascii_lowercase())
        .filter(|ext| !ext.is_empty())
        .collect::<Vec<_>>();
    let mut files = Vec::new();

    for path in paths {
        if path.is_file() {
            push_if_allowed(path, &ext_filter, &mut files);
            continue;
        }
        if path.is_dir() {
            if recursive {
                for entry in WalkDir::new(path).into_iter().filter_map(Result::ok) {
                    if entry.file_type().is_file() {
                        push_if_allowed(entry.path(), &ext_filter, &mut files);
                    }
                }
            } else {
                for entry in std::fs::read_dir(path)? {
                    let entry = entry?;
                    let entry_path = entry.path();
                    if entry_path.is_file() {
                        push_if_allowed(&entry_path, &ext_filter, &mut files);
                    }
                }
            }
            continue;
        }
        anyhow::bail!("path does not exist: {}", path.display());
    }

    files.sort();
    files.dedup();
    Ok(files)
}

fn push_if_allowed(path: &Path, extensions: &[String], files: &mut Vec<PathBuf>) {
    if extensions.is_empty() {
        files.push(path.to_path_buf());
        return;
    }
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase);
    if ext.as_ref().is_some_and(|ext| extensions.contains(ext)) {
        files.push(path.to_path_buf());
    }
}

fn compress_batch_file(
    path: &Path,
    query: &str,
    store: &dyn CcrStore,
    pipeline: &CompressionPipeline,
    write_dir: Option<&Path>,
) -> BatchItemReport {
    let path_display = path.display().to_string();
    match std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))
        .and_then(|content| {
            let report = compress_text_with_pipeline(&content, query, store, pipeline, None)?;
            let output_path = if let Some(write_dir) = write_dir {
                let file_name = path
                    .file_name()
                    .context("input path has no file name")?
                    .to_owned();
                let output_path = write_dir.join(file_name);
                std::fs::write(&output_path, &report.output)?;
                Some(output_path.display().to_string())
            } else {
                None
            };
            Ok((report, output_path))
        }) {
        Ok((report, output_path)) => BatchItemReport {
            path: path_display,
            ok: true,
            content_type: Some(report.content_type),
            original_bytes: Some(report.original_bytes),
            compressed_bytes: Some(report.compressed_bytes),
            bytes_saved: Some(report.bytes_saved),
            steps_applied: report.steps_applied,
            cache_keys: report.cache_keys,
            output_path,
            error: None,
        },
        Err(err) => BatchItemReport {
            path: path_display,
            ok: false,
            content_type: None,
            original_bytes: None,
            compressed_bytes: None,
            bytes_saved: None,
            steps_applied: Vec::new(),
            cache_keys: Vec::new(),
            output_path: None,
            error: Some(err.to_string()),
        },
    }
}

fn run_doctor(json_output: bool, store: Option<PathBuf>) -> Result<()> {
    let store_path = store.clone().unwrap_or(default_store_path()?);
    let store_ok = open_store(store).is_ok();
    let report = DoctorReport {
        binary: std::env::current_exe()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| "lm-resizer".to_string()),
        store_path: store_path.display().to_string(),
        store_ok,
        mcp_tools: vec![
            "lm_resizer_compress".to_string(),
            "lm_resizer_retrieve".to_string(),
            "lm_resizer_stats".to_string(),
        ],
        clients: vec![
            check_client("Claude Code", "claude", &["--version"]),
            check_client("Codex", "codex", &["--version"]),
            check_client("Cursor", "cursor", &["--version"]),
            check_client("VS Code", "code", &["--version"]),
            check_client("Aider", "aider", &["--version"]),
            check_client("Copilot", "copilot", &["--version"]),
        ],
    };

    if json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("lm-resizer doctor");
        println!("  Binary: {}", report.binary);
        println!(
            "  Store:  {} ({})",
            report.store_path,
            if report.store_ok { "ok" } else { "error" }
        );
        println!("  MCP tools: {}", report.mcp_tools.join(", "));
        println!("  Clients:");
        for client in &report.clients {
            if client.available {
                println!(
                    "    OK  {} ({}) {}",
                    client.name,
                    client.command,
                    client.version.as_deref().unwrap_or("")
                );
            } else {
                println!(
                    "    MISS {} ({}) {}",
                    client.name,
                    client.command,
                    client.error.as_deref().unwrap_or("")
                );
            }
        }
    }
    Ok(())
}

fn check_client(name: &str, command: &str, args: &[&str]) -> ClientCheck {
    let resolved = resolve_command_path(command).unwrap_or_else(|| PathBuf::from(command));
    let output = Command::new(&resolved).args(args).output().or_else(|err| {
        if cfg!(windows) {
            let mut cmd_args = vec!["/C", command];
            cmd_args.extend(args);
            Command::new("cmd").args(cmd_args).output()
        } else {
            Err(err)
        }
    });
    match output {
        Ok(output) => {
            let text = String::from_utf8_lossy(if output.stdout.is_empty() {
                &output.stderr
            } else {
                &output.stdout
            })
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
            ClientCheck {
                name: name.to_string(),
                command: resolved.display().to_string(),
                available: output.status.success(),
                version: if text.is_empty() { None } else { Some(text) },
                error: if output.status.success() {
                    None
                } else {
                    Some(format!("exit status {}", output.status))
                },
            }
        }
        Err(err) => ClientCheck {
            name: name.to_string(),
            command: command.to_string(),
            available: false,
            version: None,
            error: Some(err.to_string()),
        },
    }
}

fn run_mcp(store_path: Option<PathBuf>) -> Result<()> {
    let store_path = store_path.unwrap_or(default_store_path()?);
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(err) => {
                write_json(
                    &mut stdout,
                    json!({"jsonrpc":"2.0","error":{"code":-32700,"message":err.to_string()},"id":null}),
                )?;
                continue;
            }
        };
        if req.get("id").is_none() {
            continue;
        }
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req.get("method").and_then(Value::as_str).unwrap_or("");
        let response = match method {
            "initialize" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "lm-resizer", "version": env!("CARGO_PKG_VERSION") }
                }
            }),
            "tools/list" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "tools": mcp_tools() }
            }),
            "tools/call" => handle_mcp_tool_call(
                id,
                req.get("params").cloned().unwrap_or_default(),
                &store_path,
            ),
            _ => {
                json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"method not found"}})
            }
        };
        write_json(&mut stdout, response)?;
    }
    Ok(())
}

fn mcp_tools() -> Value {
    json!([
        {
            "name": "lm_resizer_compress",
            "description": "Compress tool output, logs, diffs, JSON, or text and return CCR retrieval keys.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "content": { "type": "string" },
                    "query": { "type": "string" }
                },
                "required": ["content"]
            }
        },
        {
            "name": "lm_resizer_retrieve",
            "description": "Retrieve original content by CCR hash.",
            "inputSchema": {
                "type": "object",
                "properties": { "hash": { "type": "string" } },
                "required": ["hash"]
            }
        },
        {
            "name": "lm_resizer_stats",
            "description": "Return CCR store statistics.",
            "inputSchema": { "type": "object", "properties": {} }
        }
    ])
}

fn handle_mcp_tool_call(id: Value, params: Value, store_path: &Path) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let result: Result<Value> = (|| match name {
        "lm_resizer_compress" => {
            let content = args.get("content").and_then(Value::as_str).unwrap_or("");
            let query = args.get("query").and_then(Value::as_str).unwrap_or("");
            let store = open_store(Some(store_path.to_path_buf()))?;
            let report = compress_text(content, query, store.as_ref())?;
            Ok(json!(report))
        }
        "lm_resizer_retrieve" => {
            let hash = args
                .get("hash")
                .and_then(Value::as_str)
                .context("missing hash")?;
            let store = open_store(Some(store_path.to_path_buf()))?;
            let content = store
                .get(hash)
                .with_context(|| format!("CCR entry not found: {hash}"))?;
            Ok(json!({ "hash": hash, "content": content }))
        }
        "lm_resizer_stats" => {
            let store = open_store(Some(store_path.to_path_buf()))?;
            Ok(json!({ "entries": store.len(), "empty": store.is_empty() }))
        }
        _ => Err(anyhow::anyhow!("unknown tool: {name}")),
    })();

    match result {
        Ok(payload) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string())
                }]
            }
        }),
        Err(err) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32000, "message": err.to_string() }
        }),
    }
}

fn write_json(stdout: &mut io::Stdout, value: Value) -> Result<()> {
    writeln!(stdout, "{}", serde_json::to_string(&value)?)?;
    stdout.flush()?;
    Ok(())
}

fn install_mcp(
    client: &str,
    scope: &str,
    project_dir: Option<PathBuf>,
    store: Option<PathBuf>,
) -> Result<()> {
    let project_dir = project_dir.unwrap_or(std::env::current_dir()?);
    let exe_path = std::env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "lm-resizer".to_string());
    match client {
        "claude" | "claude-code" => {
            install_json_mcp(scope, &exe_path, store, ClientConfig::Claude, &project_dir)
        }
        "codex" => install_codex(scope, &exe_path, store),
        "cursor" => install_json_mcp(scope, &exe_path, store, ClientConfig::Cursor, &project_dir),
        "vscode" | "vs-code" => {
            install_json_mcp(scope, &exe_path, store, ClientConfig::VsCode, &project_dir)
        }
        "all" => {
            install_json_mcp(
                scope,
                &exe_path,
                store.clone(),
                ClientConfig::Claude,
                &project_dir,
            )?;
            install_codex("global", &exe_path, store.clone())?;
            install_json_mcp(
                scope,
                &exe_path,
                store.clone(),
                ClientConfig::Cursor,
                &project_dir,
            )?;
            install_json_mcp(scope, &exe_path, store, ClientConfig::VsCode, &project_dir)
        }
        other => {
            anyhow::bail!("unsupported client '{other}'. Use claude, codex, cursor, vscode, or all")
        }
    }
}

#[derive(Clone, Copy)]
enum ClientConfig {
    Claude,
    Cursor,
    VsCode,
}

impl ClientConfig {
    fn name(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Cursor => "Cursor",
            Self::VsCode => "VS Code",
        }
    }

    fn path(self, scope: &str, project_dir: &Path) -> Result<PathBuf> {
        match (self, scope) {
            (Self::Claude, "project") => Ok(project_dir.join(".mcp.json")),
            (Self::Claude, "global") => Ok(home_dir()?.join(".mcp.json")),
            (Self::Cursor, "project") => Ok(project_dir.join(".cursor").join("mcp.json")),
            (Self::Cursor, "global") => Ok(home_dir()?.join(".cursor").join("mcp.json")),
            (Self::VsCode, "project") => Ok(project_dir.join(".vscode").join("mcp.json")),
            (Self::VsCode, "global") => {
                anyhow::bail!("VS Code global MCP config is profile-dependent; use --scope project")
            }
            (_, other) => anyhow::bail!("unsupported scope '{other}'. Use project or global"),
        }
    }

    fn root_key(self) -> &'static str {
        match self {
            Self::VsCode => "servers",
            Self::Claude | Self::Cursor => "mcpServers",
        }
    }
}

fn install_json_mcp(
    scope: &str,
    exe_path: &str,
    store: Option<PathBuf>,
    client: ClientConfig,
    project_dir: &Path,
) -> Result<()> {
    let config_path = client.path(scope, project_dir)?;
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut config: Value = if config_path.exists() {
        serde_json::from_str(&std::fs::read_to_string(&config_path)?)?
    } else {
        json!({})
    };
    let root_key = client.root_key();
    if config.get(root_key).is_none() {
        config[root_key] = json!({});
    }
    let mut server = json!({
        "command": exe_path,
        "args": mcp_args(store),
    });
    if matches!(client, ClientConfig::VsCode) {
        server["type"] = json!("stdio");
    }
    config[root_key]["lm-resizer"] = server;
    std::fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;
    println!(
        "Configured {} MCP server at {}",
        client.name(),
        config_path.display()
    );
    Ok(())
}

fn install_codex(scope: &str, exe_path: &str, store: Option<PathBuf>) -> Result<()> {
    if scope != "global" {
        anyhow::bail!("Codex MCP config is user-scoped; use --client codex --scope global");
    }
    let config_path = codex_home_dir()?.join("config.toml");
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let existing = if config_path.exists() {
        std::fs::read_to_string(&config_path)?
    } else {
        String::new()
    };
    let next = build_codex_mcp_config(&existing, exe_path, store)?;
    std::fs::write(&config_path, next)?;
    println!("Configured Codex MCP server at {}", config_path.display());
    Ok(())
}

fn mcp_args(store: Option<PathBuf>) -> Vec<String> {
    let mut args = vec!["mcp".to_string()];
    if let Some(store) = store {
        args.push("--store".to_string());
        args.push(store.display().to_string());
    }
    args
}

fn build_codex_mcp_config(
    existing: &str,
    exe_path: &str,
    store: Option<PathBuf>,
) -> Result<String> {
    let mut content = remove_toml_table(existing, "mcp_servers.lm_resizer");
    trim_blank_suffix(&mut content);
    if !content.is_empty() {
        content.push_str("\n\n");
    }
    content.push_str("[mcp_servers.lm_resizer]\n");
    content.push_str(&format!("command = {}\n", serde_json::to_string(exe_path)?));
    let args = mcp_args(store)
        .into_iter()
        .map(|arg| serde_json::to_string(&arg))
        .collect::<Result<Vec<_>, _>>()?;
    content.push_str(&format!("args = [{}]\n", args.join(", ")));
    content.push_str("enabled = true\n");
    content.push_str("startup_timeout_sec = 30\n");
    Ok(content)
}

fn remove_toml_table(existing: &str, table_name: &str) -> String {
    let header = format!("[{table_name}]");
    let mut out = Vec::new();
    let mut skipping = false;
    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed == header {
            skipping = true;
            continue;
        }
        if skipping && trimmed.starts_with('[') && trimmed.ends_with(']') {
            skipping = false;
        }
        if !skipping {
            out.push(line);
        }
    }
    out.join("\n")
}

fn trim_blank_suffix(content: &mut String) {
    while content.ends_with('\n') || content.ends_with('\r') {
        content.pop();
    }
}

fn home_dir() -> Result<PathBuf> {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .context("could not determine home directory")
}

fn codex_home_dir() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("CODEX_HOME") {
        return Ok(PathBuf::from(path));
    }
    Ok(home_dir()?.join(".codex"))
}

async fn wrap_agent(
    agent: String,
    args: Vec<String>,
    bind: SocketAddr,
    upstream: Option<String>,
    api_key: Option<String>,
    provider: ProviderKind,
    store: Option<PathBuf>,
    timeout_sec: Option<u64>,
) -> Result<()> {
    let proxy_url = format!("http://{bind}");
    let mut proxy = spawn_proxy(bind, upstream, api_key, provider, store)?;
    if let Err(err) = wait_for_proxy(&proxy_url).await {
        let _ = proxy.kill();
        return Err(err);
    }

    let resolved = resolve_agent_command(&agent);
    let mut command = Command::new(&resolved);
    command.args(args);
    apply_agent_env(&mut command, &agent, &proxy_url);
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to launch agent '{agent}' using command '{resolved}'"))?;
    let status = wait_for_child(&mut child, timeout_sec);
    let _ = proxy.kill();
    let _ = proxy.wait();
    let status = status?;
    if !status.success() {
        anyhow::bail!("agent exited with status {status}");
    }
    Ok(())
}

fn wait_for_child(child: &mut Child, timeout_sec: Option<u64>) -> Result<ExitStatus> {
    let Some(timeout_sec) = timeout_sec else {
        return Ok(child.wait()?);
    };
    let deadline = Instant::now() + Duration::from_secs(timeout_sec);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("wrapped agent timed out after {timeout_sec}s");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn spawn_proxy(
    bind: SocketAddr,
    upstream: Option<String>,
    api_key: Option<String>,
    provider: ProviderKind,
    store: Option<PathBuf>,
) -> Result<Child> {
    let exe = std::env::current_exe().context("could not resolve current executable")?;
    let mut cmd = Command::new(exe);
    cmd.arg("serve").arg("--bind").arg(bind.to_string());
    if let Some(upstream) = upstream {
        cmd.arg("--upstream").arg(upstream);
    }
    if let Some(api_key) = api_key {
        cmd.arg("--api-key").arg(api_key);
    }
    cmd.arg("--provider").arg(provider_label(provider));
    if let Some(store) = store {
        cmd.arg("--store").arg(store);
    }
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());
    cmd.spawn().context("failed to start lm-resizer proxy")
}

async fn wait_for_proxy(proxy_url: &str) -> Result<()> {
    let client = Client::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    let url = format!("{}/health", proxy_url.trim_end_matches('/'));
    while Instant::now() < deadline {
        if let Ok(resp) = client.get(&url).send().await {
            if resp.status().is_success() {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    anyhow::bail!("proxy did not become ready at {url}")
}

fn resolve_agent_command(agent: &str) -> String {
    let command = match agent {
        "claude" | "claude-code" => "claude".to_string(),
        "codex" => "codex".to_string(),
        "aider" => "aider".to_string(),
        "copilot" | "copilot-cli" => "copilot".to_string(),
        other => other.to_string(),
    };
    resolve_command_path(&command)
        .map(|path| path.display().to_string())
        .unwrap_or(command)
}

fn apply_agent_env(command: &mut Command, agent: &str, proxy_url: &str) {
    command.env("LM_RESIZER_PROXY", proxy_url);
    command.env("OPENAI_BASE_URL", format!("{proxy_url}/v1"));
    command.env("OPENAI_API_BASE", format!("{proxy_url}/v1"));
    command.env("ANTHROPIC_BASE_URL", proxy_url);
    command.env("ANTHROPIC_API_URL", proxy_url);

    match agent {
        "codex" => {
            command.env("OPENAI_BASE_URL", format!("{proxy_url}/v1"));
        }
        "claude" | "claude-code" => {
            command.env("ANTHROPIC_BASE_URL", proxy_url);
        }
        "aider" | "cursor" | "cursor-agent" | "opencode" | "openclaw" => {
            command.env("OPENAI_API_BASE", format!("{proxy_url}/v1"));
            command.env("OPENAI_BASE_URL", format!("{proxy_url}/v1"));
        }
        "copilot" | "copilot-cli" => {
            command.env("OPENAI_BASE_URL", format!("{proxy_url}/v1"));
        }
        _ => {}
    }
}

fn resolve_command_path(command: &str) -> Option<PathBuf> {
    let path = Path::new(command);
    if path.components().count() > 1 && path.exists() {
        return Some(path.to_path_buf());
    }

    let path_var = std::env::var_os("PATH")?;
    let extensions = command_extensions(command);
    for dir in std::env::split_paths(&path_var) {
        for ext in &extensions {
            let candidate = dir.join(format!("{command}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn command_extensions(command: &str) -> Vec<String> {
    if Path::new(command).extension().is_some() {
        return vec![String::new()];
    }
    if cfg!(windows) {
        let mut extensions = vec![
            ".exe".to_string(),
            ".cmd".to_string(),
            ".bat".to_string(),
            ".com".to_string(),
            ".ps1".to_string(),
            String::new(),
        ];
        if let Ok(pathext) = std::env::var("PATHEXT") {
            for ext in pathext.split(';').map(str::to_ascii_lowercase) {
                if !extensions.contains(&ext) {
                    extensions.push(ext);
                }
            }
        }
        extensions
    } else {
        vec![String::new()]
    }
}

async fn run_http(
    bind: SocketAddr,
    upstream: Option<String>,
    api_key: Option<String>,
    provider: ProviderKind,
    store: Option<PathBuf>,
    dashboard_enabled: bool,
) -> Result<()> {
    let state = AppState {
        store_path: store.unwrap_or(default_store_path()?),
        upstream,
        api_key,
        provider,
        client: Client::new(),
        dashboard_enabled,
    };
    let app = Router::new()
        .route("/health", get(|| async { Json(json!({"ok": true})) }))
        .route("/compress", post(http_compress))
        .route("/retrieve/:hash", get(http_retrieve))
        .route("/stats", get(http_stats))
        .route("/dashboard", get(http_dashboard))
        .route("/v1/chat/completions", post(http_openai_chat_completions))
        .route("/v1/responses", post(http_openai_responses))
        .route("/v1/messages", post(http_anthropic_messages))
        .route(
            "/v1/*provider_path",
            post(http_provider_original_uri).get(http_websocket_preview),
        )
        .route("/model/:model_id/invoke", post(http_provider_original_uri))
        .route(
            "/model/:model_id/invoke-with-response-stream",
            post(http_provider_original_uri),
        )
        .route(
            "/v1/projects/:project/locations/:location/publishers/:publisher/models/*model_method",
            post(http_provider_original_uri),
        )
        .route(
            "/v1beta/projects/:project/locations/:location/publishers/:publisher/models/*model_method",
            post(http_provider_original_uri),
        )
        .with_state(Arc::new(state));

    let listener = tokio::net::TcpListener::bind(bind).await?;
    println!("lm-resizer listening on http://{bind}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn http_compress(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CompressRequest>,
) -> Result<Json<CompressReport>, HttpError> {
    let store = open_store(Some(state.store_path.clone()))?;
    Ok(Json(compress_text(
        &req.content,
        &req.query,
        store.as_ref(),
    )?))
}

async fn http_retrieve(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(hash): axum::extract::Path<String>,
) -> Result<Json<Value>, HttpError> {
    let store = open_store(Some(state.store_path.clone()))?;
    let content = store
        .get(&hash)
        .with_context(|| format!("CCR entry not found: {hash}"))?;
    let _ = record_retrieval_feedback(&hash, content.len(), "http");
    Ok(Json(json!({ "hash": hash, "content": content })))
}

async fn http_stats(State(state): State<Arc<AppState>>) -> Result<Json<Value>, HttpError> {
    let store = open_store(Some(state.store_path.clone()))?;
    Ok(Json(
        json!({ "entries": store.len(), "empty": store.is_empty() }),
    ))
}

async fn http_dashboard(State(state): State<Arc<AppState>>) -> Result<Response, HttpError> {
    if !state.dashboard_enabled {
        return Response::builder()
            .status(axum::http::StatusCode::NOT_FOUND)
            .body(Body::from("dashboard disabled"))
            .map_err(|err| HttpError(anyhow::anyhow!(err)));
    }
    let store = open_store(Some(state.store_path.clone()))?;
    let exec_history = summarize_exec_history().unwrap_or_default();
    let html = dashboard_html(store.len(), store.is_empty(), &exec_history);
    Response::builder()
        .status(axum::http::StatusCode::OK)
        .header(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(html))
        .map_err(|err| HttpError(anyhow::anyhow!(err)))
}

fn dashboard_html(entries: usize, empty: bool, exec_history: &Value) -> String {
    let commands = exec_history
        .get("commands")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let bytes_saved = exec_history
        .get("bytes_saved")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let tokens_saved = exec_history
        .get("estimated_tokens_saved")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>lm-resizer dashboard</title>
<style>
body{{font-family:system-ui,-apple-system,Segoe UI,sans-serif;margin:2rem;line-height:1.4;color:#171717;background:#f8fafc}}
main{{max-width:920px;margin:auto}}
.grid{{display:grid;grid-template-columns:repeat(auto-fit,minmax(160px,1fr));gap:12px}}
.card{{background:white;border:1px solid #d4d4d4;border-radius:8px;padding:14px}}
.metric{{font-size:1.7rem;font-weight:700}}
code{{background:#eef2f7;padding:2px 5px;border-radius:4px}}
</style>
</head>
<body>
<main>
<h1>lm-resizer dashboard</h1>
<p>Local opt-in dashboard. No background telemetry collector is enabled.</p>
<section class="grid">
<div class="card"><div>CCR entries</div><div class="metric">{entries}</div></div>
<div class="card"><div>Store empty</div><div class="metric">{empty}</div></div>
<div class="card"><div>Exec commands</div><div class="metric">{commands}</div></div>
<div class="card"><div>Bytes saved</div><div class="metric">{bytes_saved}</div></div>
<div class="card"><div>Est. tokens saved</div><div class="metric">{tokens_saved}</div></div>
</section>
<p>JSON stats remain available at <code>/stats</code>.</p>
</main>
</body>
</html>"#
    )
}

async fn http_openai_chat_completions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, HttpError> {
    let body = proxy_body_to_json(&headers, &body)?;
    proxy_or_preview(state, "/v1/chat/completions", body).await
}

async fn http_openai_responses(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, HttpError> {
    let body = proxy_body_to_json(&headers, &body)?;
    proxy_or_preview(state, "/v1/responses", body).await
}

async fn http_anthropic_messages(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, HttpError> {
    let body = proxy_body_to_json(&headers, &body)?;
    proxy_or_preview(state, "/v1/messages", body).await
}

async fn http_provider_original_uri(
    State(state): State<Arc<AppState>>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, HttpError> {
    let path = uri
        .path_and_query()
        .map(|part| part.as_str())
        .unwrap_or_else(|| uri.path());
    match proxy_body_to_json(&headers, &body) {
        Ok(body) => proxy_or_preview(state, path, body).await,
        Err(err) if !is_json_like_content_type(&headers) => {
            proxy_raw_or_preview(state, path, headers, body, err.to_string()).await
        }
        Err(err) => Err(HttpError(err)),
    }
}

async fn http_websocket_preview(
    State(state): State<Arc<AppState>>,
    OriginalUri(uri): OriginalUri,
    ws: WebSocketUpgrade,
) -> Response {
    let path = uri
        .path_and_query()
        .map(|part| part.as_str().to_string())
        .unwrap_or_else(|| uri.path().to_string());
    if let Some(upstream) = state.upstream.clone() {
        let api_key = state.api_key.clone();
        let provider = state.provider;
        ws.on_upgrade(move |socket| websocket_bridge(socket, path, upstream, api_key, provider))
    } else {
        ws.on_upgrade(move |socket| websocket_preview(socket, path, false))
    }
}

async fn proxy_or_preview(
    state: Arc<AppState>,
    path: &str,
    mut body: Value,
) -> Result<Response, HttpError> {
    let stream_requested = body.get("stream").and_then(Value::as_bool).unwrap_or(false)
        || is_streaming_proxy_path(path);
    let store = open_store(Some(state.store_path.clone()))?;
    let mut stats = ProxyCompressionStats::default();
    // Provider-aware live-zone compression for the JSON chat/messages/responses
    // routes; fall back to the generic field-walk for everything else (Bedrock,
    // Vertex, /model/:id/invoke, unknown routes) or when the dispatcher errors.
    if !try_live_zone_compress(path, &mut body, store.as_ref(), &mut stats) {
        let pipeline = build_pipeline();
        compress_json_payload(&mut body, store.as_ref(), &pipeline, &mut stats)?;
    }
    stats.provider_cache_policy = provider_cache_policy(state.provider).to_string();

    if let Some(upstream) = &state.upstream {
        let url = format!("{}{}", upstream.trim_end_matches('/'), path);
        let payload = serde_json::to_vec(&body)?;
        let mut req = state
            .client
            .post(url.clone())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(payload.clone());
        if matches!(state.provider, ProviderKind::Anthropic) {
            req = req.header("anthropic-version", "2023-06-01");
        }
        req = apply_provider_auth(
            req,
            &state.client,
            state.provider,
            &url,
            &payload,
            state.api_key.as_deref(),
        )
        .await?;
        let response = req.send().await?;
        let status = response.status();
        if stream_requested {
            let mut builder = Response::builder().status(status);
            if let Some(content_type) = response.headers().get(reqwest::header::CONTENT_TYPE) {
                builder = builder.header(axum::http::header::CONTENT_TYPE, content_type);
            } else {
                builder = builder.header(axum::http::header::CONTENT_TYPE, "text/event-stream");
            }
            builder = builder.header(
                "x-lm-resizer-compression",
                serde_json::to_string(&stats).unwrap_or_else(|_| "{}".to_string()),
            );
            let stream = response.bytes_stream();
            return builder
                .body(Body::from_stream(stream))
                .map_err(|err| HttpError(anyhow::anyhow!(err)));
        }

        let response_headers = response.headers().clone();
        let response_bytes = response.bytes().await?;
        let decoded_response = decode_http_body(&response_headers, &response_bytes)?;
        let mut value = serde_json::from_slice::<Value>(&decoded_response)?;
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                "lm_resizer".to_string(),
                serde_json::to_value(&stats).unwrap_or_else(|_| json!({})),
            );
        }
        if !status.is_success() {
            return Err(HttpError(anyhow::anyhow!(
                "upstream returned HTTP {status}: {value}"
            )));
        }
        return Ok(Json(value).into_response());
    }

    let preview = json!({
        "mode": "preview",
        "message": "set --upstream or LM_RESIZER_UPSTREAM to forward this compressed request",
        "compression": stats,
        "request": body
    });
    if stream_requested {
        return Response::builder()
            .status(axum::http::StatusCode::OK)
            .header(axum::http::header::CONTENT_TYPE, "text/event-stream")
            .body(Body::from(preview_sse_body(&preview)))
            .map_err(|err| HttpError(anyhow::anyhow!(err)));
    }
    Ok(Json(preview).into_response())
}

async fn proxy_raw_or_preview(
    state: Arc<AppState>,
    path: &str,
    headers: HeaderMap,
    body: Bytes,
    parse_error: String,
) -> Result<Response, HttpError> {
    if let Some(upstream) = &state.upstream {
        let url = format!("{}{}", upstream.trim_end_matches('/'), path);
        let mut req = state.client.post(url.clone()).body(body.clone());
        if let Some(content_type) = headers.get(reqwest::header::CONTENT_TYPE) {
            req = req.header(reqwest::header::CONTENT_TYPE, content_type.clone());
        }
        req = apply_provider_auth(
            req,
            &state.client,
            state.provider,
            &url,
            &body,
            state.api_key.as_deref(),
        )
        .await?;
        let response = req.send().await?;
        let status = response.status();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .cloned();
        let response_bytes = response.bytes().await?;
        let mut builder = Response::builder().status(status);
        if let Some(content_type) = content_type {
            builder = builder.header(axum::http::header::CONTENT_TYPE, content_type);
        }
        return builder
            .body(Body::from(response_bytes))
            .map_err(|err| HttpError(anyhow::anyhow!(err)));
    }

    Ok(Json(json!({
        "mode": "preview",
        "message": "set --upstream or LM_RESIZER_UPSTREAM to forward this non-JSON request",
        "request": {
            "path": path,
            "bytes": body.len(),
            "content_type": headers
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("application/octet-stream"),
            "json_parse_error": parse_error,
        }
    }))
    .into_response())
}

fn is_streaming_proxy_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with("/invoke-with-response-stream")
        || lower.contains(":streamgeneratecontent")
        || lower.contains("streamgeneratecontent")
}

fn proxy_body_to_json(headers: &HeaderMap, body: &[u8]) -> Result<Value> {
    let decoded = decode_http_body(headers, body)?;
    serde_json::from_slice(&decoded).context("proxy request body is not valid JSON")
}

fn is_json_like_content_type(headers: &HeaderMap) -> bool {
    headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            let lower = value.to_ascii_lowercase();
            lower.contains("application/json") || lower.contains("+json")
        })
        .unwrap_or(false)
}

fn preview_sse_body(value: &Value) -> String {
    let data = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
    format!("event: lm_resizer_preview\ndata: {data}\n\nevent: done\ndata: [DONE]\n\n")
}

async fn websocket_preview(mut socket: WebSocket, path: String, upstream_configured: bool) {
    let message = websocket_preview_message(&path, upstream_configured);
    let _ = socket.send(WsMessage::Text(message)).await;
    let _ = socket.close().await;
}

async fn websocket_bridge(
    mut client_socket: WebSocket,
    path: String,
    upstream: String,
    api_key: Option<String>,
    provider: ProviderKind,
) {
    let url = match websocket_upstream_url(&upstream, &path) {
        Ok(url) => url,
        Err(err) => {
            let _ = client_socket
                .send(WsMessage::Text(websocket_error_message(
                    &path,
                    &err.to_string(),
                )))
                .await;
            let _ = client_socket.close().await;
            return;
        }
    };
    let request = match websocket_connect_request(&url, api_key.as_deref(), provider) {
        Ok(request) => request,
        Err(err) => {
            let _ = client_socket
                .send(WsMessage::Text(websocket_error_message(
                    &path,
                    &err.to_string(),
                )))
                .await;
            let _ = client_socket.close().await;
            return;
        }
    };
    let upstream_socket = match connect_async(request).await {
        Ok((socket, _response)) => socket,
        Err(err) => {
            let _ = client_socket
                .send(WsMessage::Text(websocket_error_message(
                    &path,
                    &err.to_string(),
                )))
                .await;
            let _ = client_socket.close().await;
            return;
        }
    };

    let (mut client_tx, mut client_rx) = client_socket.split();
    let (mut upstream_tx, mut upstream_rx) = upstream_socket.split();

    loop {
        tokio::select! {
            client_msg = client_rx.next() => {
                let Some(Ok(message)) = client_msg else { break; };
                match axum_ws_to_tungstenite(message) {
                    Some(message) => {
                        if upstream_tx.send(message).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            upstream_msg = upstream_rx.next() => {
                let Some(Ok(message)) = upstream_msg else { break; };
                match tungstenite_to_axum_ws(message) {
                    Some(message) => {
                        if client_tx.send(message).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
        }
    }
    let _ = client_tx.close().await;
    let _ = upstream_tx.close().await;
}

fn websocket_upstream_url(upstream: &str, path: &str) -> Result<String> {
    let mut base = upstream.trim_end_matches('/').to_string();
    if base.starts_with("http://") {
        base = format!("ws://{}", &base["http://".len()..]);
    } else if base.starts_with("https://") {
        base = format!("wss://{}", &base["https://".len()..]);
    } else if !base.starts_with("ws://") && !base.starts_with("wss://") {
        anyhow::bail!("WebSocket upstream must start with http://, https://, ws://, or wss://");
    }
    Ok(format!("{base}{path}"))
}

fn websocket_connect_request(
    url: &str,
    api_key: Option<&str>,
    provider: ProviderKind,
) -> Result<tokio_tungstenite::tungstenite::handshake::client::Request> {
    let mut request = url.into_client_request()?;
    if let Some(api_key) = api_key {
        let headers = request.headers_mut();
        match provider {
            ProviderKind::Anthropic => {
                headers.insert(
                    "x-api-key",
                    api_key.parse().context("invalid websocket x-api-key")?,
                );
            }
            ProviderKind::OpenAi | ProviderKind::Vertex | ProviderKind::Bedrock => {
                headers.insert(
                    reqwest::header::AUTHORIZATION,
                    format!("Bearer {api_key}")
                        .parse()
                        .context("invalid websocket authorization header")?,
                );
            }
        }
    }
    Ok(request)
}

fn axum_ws_to_tungstenite(message: WsMessage) -> Option<TungsteniteMessage> {
    match message {
        WsMessage::Text(text) => Some(TungsteniteMessage::Text(text)),
        WsMessage::Binary(bytes) => Some(TungsteniteMessage::Binary(bytes.to_vec())),
        WsMessage::Ping(bytes) => Some(TungsteniteMessage::Ping(bytes)),
        WsMessage::Pong(bytes) => Some(TungsteniteMessage::Pong(bytes)),
        WsMessage::Close(frame) => Some(TungsteniteMessage::Close(frame.map(|frame| {
            tokio_tungstenite::tungstenite::protocol::CloseFrame {
                code: frame.code.into(),
                reason: frame.reason.to_string().into(),
            }
        }))),
    }
}

fn tungstenite_to_axum_ws(message: TungsteniteMessage) -> Option<WsMessage> {
    match message {
        TungsteniteMessage::Text(text) => Some(WsMessage::Text(text)),
        TungsteniteMessage::Binary(bytes) => Some(WsMessage::Binary(bytes.into())),
        TungsteniteMessage::Ping(bytes) => Some(WsMessage::Ping(bytes)),
        TungsteniteMessage::Pong(bytes) => Some(WsMessage::Pong(bytes)),
        TungsteniteMessage::Close(frame) => Some(WsMessage::Close(frame.map(|frame| {
            axum::extract::ws::CloseFrame {
                code: frame.code.into(),
                reason: frame.reason.to_string().into(),
            }
        }))),
        TungsteniteMessage::Frame(_) => None,
    }
}

fn websocket_preview_message(path: &str, upstream_configured: bool) -> String {
    serde_json::json!({
        "mode": "preview",
        "path": path,
        "websocket": true,
        "message": if upstream_configured {
            "WebSocket path detected; upstream WebSocket bridging is not enabled in this build"
        } else {
            "WebSocket path detected; set an upstream and use HTTP JSON endpoints for compression"
        }
    })
    .to_string()
}

fn websocket_error_message(path: &str, error: &str) -> String {
    serde_json::json!({
        "mode": "error",
        "path": path,
        "websocket": true,
        "error": error,
    })
    .to_string()
}

fn decode_http_body(headers: &HeaderMap, body: &[u8]) -> Result<Vec<u8>> {
    let Some(encoding) = headers
        .get(reqwest::header::CONTENT_ENCODING)
        .and_then(|value| value.to_str().ok())
    else {
        return Ok(body.to_vec());
    };
    match encoding.trim().to_ascii_lowercase().as_str() {
        "" | "identity" => Ok(body.to_vec()),
        "gzip" | "x-gzip" => {
            let mut decoder = GzDecoder::new(body);
            let mut decoded = Vec::new();
            decoder.read_to_end(&mut decoded)?;
            Ok(decoded)
        }
        "deflate" => {
            let mut decoder = ZlibDecoder::new(body);
            let mut decoded = Vec::new();
            decoder.read_to_end(&mut decoded)?;
            Ok(decoded)
        }
        other => anyhow::bail!("unsupported content-encoding: {other}"),
    }
}

async fn apply_provider_auth(
    req: reqwest::RequestBuilder,
    client: &Client,
    provider: ProviderKind,
    url: &str,
    payload: &[u8],
    api_key: Option<&str>,
) -> Result<reqwest::RequestBuilder> {
    match provider {
        ProviderKind::OpenAi => Ok(if let Some(api_key) = api_key {
            req.bearer_auth(api_key)
        } else {
            req
        }),
        ProviderKind::Anthropic => Ok(if let Some(api_key) = api_key {
            req.header("x-api-key", api_key)
        } else {
            req
        }),
        ProviderKind::Bedrock => {
            if let Some(creds) = aws_credentials(client).await? {
                Ok(apply_aws_sigv4(req, url, payload, &creds)?)
            } else if let Some(api_key) = api_key {
                Ok(req.header(axum::http::header::AUTHORIZATION, api_key))
            } else {
                Ok(req)
            }
        }
        ProviderKind::Vertex => {
            if let Some(api_key) = api_key {
                Ok(req.bearer_auth(api_key))
            } else if let Some(token) = google_adc_access_token(client).await? {
                Ok(req.bearer_auth(token))
            } else {
                Ok(req)
            }
        }
    }
}

#[derive(Debug, Clone)]
struct AwsCredentials {
    access_key: String,
    secret_key: String,
    session_token: Option<String>,
    region: String,
    service: String,
}

async fn aws_credentials(client: &Client) -> Result<Option<AwsCredentials>> {
    if let Some(creds) = aws_credentials_from_env()? {
        return Ok(Some(creds));
    }
    if let Some(creds) = aws_credentials_from_profile()? {
        return Ok(Some(creds));
    }
    aws_credentials_from_imds(client).await
}

fn aws_credentials_from_env() -> Result<Option<AwsCredentials>> {
    let Some(access_key) = env_first(&["AWS_ACCESS_KEY_ID", "AWS_ACCESS_KEY"]) else {
        return Ok(None);
    };
    let Some(secret_key) = env_first(&["AWS_SECRET_ACCESS_KEY", "AWS_SECRET_KEY"]) else {
        anyhow::bail!("AWS_ACCESS_KEY_ID is set but AWS_SECRET_ACCESS_KEY is missing");
    };
    let region = env_first(&["LM_RESIZER_AWS_REGION", "AWS_REGION", "AWS_DEFAULT_REGION"])
        .unwrap_or_else(|| "us-east-1".to_string());
    Ok(Some(AwsCredentials {
        access_key,
        secret_key,
        session_token: env_first(&["AWS_SESSION_TOKEN"]),
        region,
        service: std::env::var("LM_RESIZER_AWS_SERVICE").unwrap_or_else(|_| "bedrock".to_string()),
    }))
}

fn aws_credentials_from_profile() -> Result<Option<AwsCredentials>> {
    let profile = std::env::var("AWS_PROFILE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "default".to_string());
    let home = match home_dir() {
        Ok(home) => home,
        Err(_) => return Ok(None),
    };
    let credentials_path = std::env::var("AWS_SHARED_CREDENTIALS_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home.join(".aws").join("credentials"));
    let config_path = std::env::var("AWS_CONFIG_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home.join(".aws").join("config"));
    let credentials_content = std::fs::read_to_string(credentials_path).unwrap_or_default();
    let config_content = std::fs::read_to_string(config_path).unwrap_or_default();
    aws_credentials_from_profile_content(&credentials_content, &config_content, &profile)
}

fn aws_credentials_from_profile_content(
    credentials_content: &str,
    config_content: &str,
    profile: &str,
) -> Result<Option<AwsCredentials>> {
    let credentials = parse_ini_sections(credentials_content);
    let config = parse_ini_sections(config_content);
    let config_section = if profile == "default" {
        "default".to_string()
    } else {
        format!("profile {profile}")
    };
    let Some(access_key) = credentials
        .get(profile)
        .and_then(|section| section.get("aws_access_key_id"))
        .cloned()
        .or_else(|| {
            config
                .get(&config_section)
                .and_then(|section| section.get("aws_access_key_id"))
                .cloned()
        })
    else {
        return Ok(None);
    };
    let Some(secret_key) = credentials
        .get(profile)
        .and_then(|section| section.get("aws_secret_access_key"))
        .cloned()
        .or_else(|| {
            config
                .get(&config_section)
                .and_then(|section| section.get("aws_secret_access_key"))
                .cloned()
        })
    else {
        anyhow::bail!("AWS profile '{profile}' has access key but no secret access key");
    };
    let session_token = credentials
        .get(profile)
        .and_then(|section| section.get("aws_session_token"))
        .cloned()
        .or_else(|| {
            config
                .get(&config_section)
                .and_then(|section| section.get("aws_session_token"))
                .cloned()
        });
    let region = env_first(&["LM_RESIZER_AWS_REGION", "AWS_REGION", "AWS_DEFAULT_REGION"])
        .or_else(|| {
            credentials
                .get(profile)
                .and_then(|section| section.get("region"))
                .cloned()
        })
        .or_else(|| {
            config
                .get(&config_section)
                .and_then(|section| section.get("region"))
                .cloned()
        })
        .unwrap_or_else(|| "us-east-1".to_string());
    Ok(Some(AwsCredentials {
        access_key,
        secret_key,
        session_token,
        region,
        service: std::env::var("LM_RESIZER_AWS_SERVICE").unwrap_or_else(|_| "bedrock".to_string()),
    }))
}

fn parse_ini_sections(
    content: &str,
) -> std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>> {
    let mut sections = std::collections::BTreeMap::new();
    let mut current = String::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }
        if let Some(section) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            current = section.trim().to_string();
            sections
                .entry(current.clone())
                .or_insert_with(std::collections::BTreeMap::new);
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if current.is_empty() {
            continue;
        }
        sections
            .entry(current.clone())
            .or_insert_with(std::collections::BTreeMap::new)
            .insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
    }
    sections
}

async fn aws_credentials_from_imds(client: &Client) -> Result<Option<AwsCredentials>> {
    if std::env::var("AWS_EC2_METADATA_DISABLED")
        .ok()
        .is_some_and(|value| value.eq_ignore_ascii_case("true"))
    {
        return Ok(None);
    }
    let base = std::env::var("LM_RESIZER_AWS_IMDS_BASE_URL")
        .unwrap_or_else(|_| "http://169.254.169.254".to_string());
    let token_url = format!("{}/latest/api/token", base.trim_end_matches('/'));
    let token = client
        .put(token_url)
        .header("x-aws-ec2-metadata-token-ttl-seconds", "21600")
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .ok()
        .and_then(|response| {
            if response.status().is_success() {
                Some(response)
            } else {
                None
            }
        });
    let token = match token {
        Some(response) => response.text().await.ok(),
        None => None,
    };
    let role_url = format!(
        "{}/latest/meta-data/iam/security-credentials/",
        base.trim_end_matches('/')
    );
    let mut role_req = client.get(role_url).timeout(Duration::from_secs(2));
    if let Some(token) = &token {
        role_req = role_req.header("x-aws-ec2-metadata-token", token);
    }
    let role = match role_req.send().await {
        Ok(response) if response.status().is_success() => response.text().await?,
        _ => return Ok(None),
    };
    let role = role.lines().next().unwrap_or("").trim();
    if role.is_empty() {
        return Ok(None);
    }
    let creds_url = format!(
        "{}/latest/meta-data/iam/security-credentials/{}",
        base.trim_end_matches('/'),
        role
    );
    let mut creds_req = client.get(creds_url).timeout(Duration::from_secs(2));
    if let Some(token) = &token {
        creds_req = creds_req.header("x-aws-ec2-metadata-token", token);
    }
    let response = match creds_req.send().await {
        Ok(response) if response.status().is_success() => response,
        _ => return Ok(None),
    };
    let value = response.json::<Value>().await?;
    let Some(access_key) = value.get("AccessKeyId").and_then(Value::as_str) else {
        return Ok(None);
    };
    let Some(secret_key) = value.get("SecretAccessKey").and_then(Value::as_str) else {
        return Ok(None);
    };
    let region = env_first(&["LM_RESIZER_AWS_REGION", "AWS_REGION", "AWS_DEFAULT_REGION"])
        .unwrap_or_else(|| "us-east-1".to_string());
    Ok(Some(AwsCredentials {
        access_key: access_key.to_string(),
        secret_key: secret_key.to_string(),
        session_token: value
            .get("Token")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        region,
        service: std::env::var("LM_RESIZER_AWS_SERVICE").unwrap_or_else(|_| "bedrock".to_string()),
    }))
}

fn env_first(names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        std::env::var(name)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

fn apply_aws_sigv4(
    req: reqwest::RequestBuilder,
    url: &str,
    payload: &[u8],
    creds: &AwsCredentials,
) -> Result<reqwest::RequestBuilder> {
    let (amz_date, date) = aws_sigv4_timestamp();
    let headers = aws_sigv4_headers(url, payload, creds, &amz_date, &date)?;
    Ok(req.headers(headers))
}

fn aws_sigv4_headers(
    url: &str,
    payload: &[u8],
    creds: &AwsCredentials,
    amz_date: &str,
    date: &str,
) -> Result<HeaderMap> {
    let parsed =
        reqwest::Url::parse(url).with_context(|| format!("invalid upstream URL: {url}"))?;
    let host = parsed
        .host_str()
        .context("upstream URL must include a host")?
        .to_string();
    let host = match parsed.port() {
        Some(port) => format!("{host}:{port}"),
        None => host,
    };
    let payload_hash = sha256_hex(payload);
    let canonical_uri = if parsed.path().is_empty() {
        "/"
    } else {
        parsed.path()
    };
    let canonical_query = canonical_query_string(&parsed);
    let mut canonical_headers =
        format!("content-type:application/json\nhost:{host}\nx-amz-date:{amz_date}\n");
    let mut signed_headers = "content-type;host;x-amz-date".to_string();
    if let Some(token) = &creds.session_token {
        canonical_headers.push_str(&format!("x-amz-security-token:{token}\n"));
        signed_headers.push_str(";x-amz-security-token");
    }
    let canonical_request = format!(
        "POST\n{canonical_uri}\n{canonical_query}\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
    );
    let credential_scope = format!("{date}/{}/{}/aws4_request", creds.region, creds.service);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let signing_key = aws_sigv4_signing_key(&creds.secret_key, date, &creds.region, &creds.service);
    let signature = hex_lower(&hmac_sha256(&signing_key, string_to_sign.as_bytes()));
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}",
        creds.access_key
    );

    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(
        HeaderName::from_static("x-amz-date"),
        HeaderValue::from_str(amz_date)?,
    );
    headers.insert(
        reqwest::header::AUTHORIZATION,
        HeaderValue::from_str(&authorization)?,
    );
    if let Some(token) = &creds.session_token {
        headers.insert(
            HeaderName::from_static("x-amz-security-token"),
            HeaderValue::from_str(token)?,
        );
    }
    Ok(headers)
}

fn canonical_query_string(url: &reqwest::Url) -> String {
    let mut pairs = url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    pairs.sort();
    pairs
        .into_iter()
        .map(|(key, value)| format!("{}={}", uri_encode(&key), uri_encode(&value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn uri_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

fn aws_sigv4_signing_key(secret: &str, date: &str, region: &str, service: &str) -> Vec<u8> {
    let k_date = hmac_sha256(format!("AWS4{secret}").as_bytes(), date.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    hmac_sha256(&k_service, b"aws4_request")
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    const BLOCK_SIZE: usize = 64;
    let mut key_block = [0u8; BLOCK_SIZE];
    if key.len() > BLOCK_SIZE {
        key_block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut outer = [0x5c_u8; BLOCK_SIZE];
    let mut inner = [0x36_u8; BLOCK_SIZE];
    for i in 0..BLOCK_SIZE {
        outer[i] ^= key_block[i];
        inner[i] ^= key_block[i];
    }
    let mut inner_hash = Sha256::new();
    inner_hash.update(inner);
    inner_hash.update(data);
    let inner_digest = inner_hash.finalize();
    let mut outer_hash = Sha256::new();
    outer_hash.update(outer);
    outer_hash.update(inner_digest);
    outer_hash.finalize().to_vec()
}

fn aws_sigv4_timestamp() -> (String, String) {
    if let Ok(value) = std::env::var("LM_RESIZER_AWS_DATE") {
        let trimmed = value.trim();
        if trimmed.len() == 16 && trimmed.ends_with('Z') && trimmed.as_bytes()[8] == b'T' {
            return (trimmed.to_string(), trimmed[..8].to_string());
        }
    }
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    aws_sigv4_timestamp_from_unix(seconds)
}

fn aws_sigv4_timestamp_from_unix(seconds: i64) -> (String, String) {
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = day_seconds / 3_600;
    let minute = (day_seconds % 3_600) / 60;
    let second = day_seconds % 60;
    let date = format!("{year:04}{month:02}{day:02}");
    (format!("{date}T{hour:02}{minute:02}{second:02}Z"), date)
}

fn civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year, m, d)
}

async fn google_adc_access_token(client: &Client) -> Result<Option<String>> {
    if let Some(token) = env_first(&[
        "LM_RESIZER_GOOGLE_ACCESS_TOKEN",
        "GOOGLE_OAUTH_ACCESS_TOKEN",
        "CLOUDSDK_AUTH_ACCESS_TOKEN",
    ]) {
        return Ok(Some(token));
    }
    if let Some(token) = google_service_account_access_token(client).await? {
        return Ok(Some(token));
    }
    let metadata_url = std::env::var("LM_RESIZER_GCP_METADATA_TOKEN_URL").unwrap_or_else(|_| {
        "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token"
            .to_string()
    });
    let response = match client
        .get(metadata_url)
        .header("Metadata-Flavor", "Google")
        .timeout(Duration::from_secs(2))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => response,
        Ok(_) | Err(_) => return Ok(None),
    };
    let value = response.json::<Value>().await?;
    Ok(value
        .get("access_token")
        .and_then(Value::as_str)
        .map(ToString::to_string))
}

#[derive(Debug, Deserialize)]
struct GoogleServiceAccountKey {
    #[serde(rename = "type")]
    key_type: Option<String>,
    client_email: Option<String>,
    private_key: Option<String>,
    token_uri: Option<String>,
}

async fn google_service_account_access_token(client: &Client) -> Result<Option<String>> {
    let Some(path) = google_application_credentials_path() else {
        return Ok(None);
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(_) => return Ok(None),
    };
    google_service_account_access_token_from_json(client, &content).await
}

fn google_application_credentials_path() -> Option<PathBuf> {
    if let Some(path) = env_first(&["GOOGLE_APPLICATION_CREDENTIALS"]) {
        return Some(PathBuf::from(path));
    }
    let home = home_dir().ok()?;
    if cfg!(windows) {
        std::env::var("APPDATA")
            .ok()
            .map(PathBuf::from)
            .map(|path| {
                path.join("gcloud")
                    .join("application_default_credentials.json")
            })
    } else {
        Some(
            home.join(".config")
                .join("gcloud")
                .join("application_default_credentials.json"),
        )
    }
}

async fn google_service_account_access_token_from_json(
    client: &Client,
    content: &str,
) -> Result<Option<String>> {
    let key: GoogleServiceAccountKey = serde_json::from_str(content)
        .context("invalid GOOGLE_APPLICATION_CREDENTIALS service-account JSON")?;
    if key.key_type.as_deref() != Some("service_account") {
        return Ok(None);
    }
    let assertion = google_service_account_jwt(&key)?;
    let token_uri = key
        .token_uri
        .as_deref()
        .unwrap_or("https://oauth2.googleapis.com/token");
    let body = format!(
        "grant_type={}&assertion={}",
        uri_encode("urn:ietf:params:oauth:grant-type:jwt-bearer"),
        uri_encode(&assertion)
    );
    let response = client
        .post(token_uri)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(body)
        .send()
        .await?;
    if !response.status().is_success() {
        return Ok(None);
    }
    let value = response.json::<Value>().await?;
    Ok(value
        .get("access_token")
        .and_then(Value::as_str)
        .map(ToString::to_string))
}

fn google_service_account_jwt(key: &GoogleServiceAccountKey) -> Result<String> {
    let email = key
        .client_email
        .as_deref()
        .context("service-account JSON missing client_email")?;
    let private_key = key
        .private_key
        .as_deref()
        .context("service-account JSON missing private_key")?;
    let token_uri = key
        .token_uri
        .as_deref()
        .unwrap_or("https://oauth2.googleapis.com/token");
    let scope = std::env::var("LM_RESIZER_GOOGLE_SCOPE")
        .unwrap_or_else(|_| "https://www.googleapis.com/auth/cloud-platform".to_string());
    let now = unix_now();
    google_service_account_jwt_at(key, email, private_key, token_uri, &scope, now)
}

fn google_service_account_jwt_at(
    _key: &GoogleServiceAccountKey,
    email: &str,
    private_key: &str,
    token_uri: &str,
    scope: &str,
    now: i64,
) -> Result<String> {
    let header = json!({ "alg": "RS256", "typ": "JWT" });
    let claims = json!({
        "iss": email,
        "scope": scope,
        "aud": token_uri,
        "iat": now,
        "exp": now + 3600
    });
    let signing_input = format!(
        "{}.{}",
        base64_url_json(&header)?,
        base64_url_json(&claims)?
    );
    let key_der = pem_private_key_der(private_key)?;
    let key_pair = RsaKeyPair::from_pkcs8(&key_der)
        .map_err(|_| anyhow::anyhow!("service-account private_key is not valid PKCS#8 RSA"))?;
    let rng = SystemRandom::new();
    let mut signature = vec![0; key_pair.public().modulus_len()];
    key_pair
        .sign(
            &RSA_PKCS1_SHA256,
            &rng,
            signing_input.as_bytes(),
            &mut signature,
        )
        .map_err(|_| anyhow::anyhow!("failed to sign service-account JWT"))?;
    Ok(format!("{signing_input}.{}", base64_url_bytes(&signature)))
}

fn base64_url_json(value: &Value) -> Result<String> {
    Ok(base64_url_bytes(&serde_json::to_vec(value)?))
}

fn base64_url_bytes(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn pem_private_key_der(pem: &str) -> Result<Vec<u8>> {
    let body = pem
        .lines()
        .filter(|line| !line.starts_with("-----BEGIN ") && !line.starts_with("-----END "))
        .map(str::trim)
        .collect::<String>();
    base64::engine::general_purpose::STANDARD
        .decode(body.as_bytes())
        .context("service-account private_key PEM is not valid base64")
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Default, Debug, Serialize)]
struct ProxyCompressionStats {
    fields_seen: usize,
    fields_compressed: usize,
    original_bytes: usize,
    compressed_bytes: usize,
    bytes_saved: usize,
    cache_keys: Vec<String>,
    provider_cache_policy: String,
}

fn provider_cache_policy(provider: ProviderKind) -> &'static str {
    match provider {
        ProviderKind::OpenAi => {
            "preserve OpenAI prompt-cache markers and compress live-zone payload strings"
        }
        ProviderKind::Anthropic => {
            "preserve Anthropic cache_control blocks and compress message/tool strings"
        }
        ProviderKind::Bedrock => {
            "preserve Bedrock provider envelope and compress Anthropic-compatible payload strings"
        }
        ProviderKind::Vertex => "preserve Vertex contents/parts shape and compress text parts",
    }
}

fn compress_json_payload(
    value: &mut Value,
    store: &dyn CcrStore,
    pipeline: &CompressionPipeline,
    stats: &mut ProxyCompressionStats,
) -> Result<()> {
    match value {
        Value::Array(items) => {
            for item in items {
                compress_json_payload(item, store, pipeline, stats)?;
            }
        }
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if should_compress_json_string(key) {
                    if let Value::String(text) = child {
                        stats.fields_seen += 1;
                        if text.len() >= 512 {
                            let report =
                                compress_text_with_pipeline(text, "", store, pipeline, None)?;
                            stats.original_bytes += report.original_bytes;
                            stats.compressed_bytes += report.compressed_bytes;
                            stats.bytes_saved += report.bytes_saved;
                            if report.compressed_bytes < report.original_bytes {
                                stats.fields_compressed += 1;
                                stats.cache_keys.extend(report.cache_keys);
                                *text = report.output;
                            }
                        }
                        continue;
                    }
                }
                compress_json_payload(child, store, pipeline, stats)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Run the provider-aware live-zone dispatcher for the JSON chat routes.
///
/// Returns `true` when the live-zone path handled the body (a successful
/// `Modified`/`NoChange` outcome) — `body` and `stats` are updated in place.
/// Returns `false` when the route has no live-zone dispatcher (Bedrock,
/// Vertex, `/model/:id/invoke`, streaming, unknown) or the dispatcher errored,
/// so the caller falls back to the generic field-walk compressor.
fn try_live_zone_compress(
    path: &str,
    body: &mut Value,
    store: &dyn CcrStore,
    stats: &mut ProxyCompressionStats,
) -> bool {
    let model = body
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let original = match serde_json::to_vec(body) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    // Auth-mode detection is not wired into the proxy yet (PR-F2); the
    // dispatchers currently treat every request as `Payg`.
    let outcome = if path.ends_with("/v1/chat/completions") {
        compress_openai_chat_live_zone(&original, AuthMode::Payg, &model)
    } else if path.ends_with("/v1/responses") {
        compress_openai_responses_live_zone(&original, AuthMode::Payg, &model)
    } else if path.ends_with("/v1/messages") {
        let frozen = compute_frozen_count(body);
        compress_anthropic_live_zone_with_ccr(
            &original,
            frozen,
            AuthMode::Payg,
            &model,
            Some(store),
        )
    } else {
        return false;
    };

    match outcome {
        Ok(LiveZoneOutcome::Modified { new_body, manifest }) => {
            let compressed = new_body.get().to_string();
            match serde_json::from_str::<Value>(&compressed) {
                Ok(value) => *body = value,
                // The dispatcher emitted invalid JSON (should be unreachable);
                // fall back rather than forward a broken body.
                Err(_) => return false,
            }
            record_live_zone_stats(
                stats,
                &manifest,
                original.len(),
                compressed.len(),
                &compressed,
            );
            true
        }
        Ok(LiveZoneOutcome::NoChange { manifest }) => {
            record_live_zone_stats(stats, &manifest, original.len(), original.len(), "");
            true
        }
        Err(err) => {
            // Recoverable: log and let the caller fall back to the generic
            // field-walk compressor. Never on stdout (MCP/JSON-RPC purity is
            // irrelevant here, but stderr keeps proxy logs clean either way).
            eprintln!("lm-resizer: live-zone dispatch failed for {path}: {err}; using generic compression");
            false
        }
    }
}

/// Populate `ProxyCompressionStats` from a live-zone `CompressionManifest`
/// plus the pre/post body byte sizes. `compressed_body` is scanned for
/// `<<ccr:HASH>>` recovery markers (empty string when nothing changed).
fn record_live_zone_stats(
    stats: &mut ProxyCompressionStats,
    manifest: &CompressionManifest,
    original_bytes: usize,
    compressed_bytes: usize,
    compressed_body: &str,
) {
    stats.fields_seen += manifest.block_outcomes.len();
    stats.fields_compressed += manifest.transforms_applied().len();
    stats.original_bytes += original_bytes;
    stats.compressed_bytes += compressed_bytes;
    stats.bytes_saved += original_bytes.saturating_sub(compressed_bytes);
    stats.cache_keys.extend(extract_ccr_keys(compressed_body));
}

/// Extract `<<ccr:HASH>>` recovery-marker hashes from a compressed body.
/// The marker format mirrors `lm-resizer-core`'s CCR injection: a 24-char
/// hex hash. Used only for proxy observability stats.
fn extract_ccr_keys(body: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("<<ccr:") {
        let after = &rest[start + "<<ccr:".len()..];
        if let Some(end) = after.find(">>") {
            let hash: String = after[..end]
                .chars()
                .take_while(|c| c.is_ascii_hexdigit())
                .collect();
            if !hash.is_empty() {
                keys.push(hash);
            }
            rest = &after[end + 2..];
        } else {
            break;
        }
    }
    keys
}

fn should_compress_json_string(key: &str) -> bool {
    matches!(
        key,
        "content" | "text" | "input" | "output" | "tool_output" | "arguments"
    )
}

struct HttpError(anyhow::Error);

impl<E> From<E> for HttpError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

impl axum::response::IntoResponse for HttpError {
    fn into_response(self) -> axum::response::Response {
        let status = axum::http::StatusCode::BAD_REQUEST;
        let body = Json(json!({ "error": self.0.to_string() }));
        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_resizer_core::ccr::InMemoryCcrStore;

    #[test]
    fn codex_config_replaces_existing_table() {
        let existing = r#"model = "gpt-test"

[mcp_servers.lm_resizer]
command = "old"
args = ["mcp"]

[mcp_servers.other]
command = "node"
"#;
        let config = build_codex_mcp_config(
            existing,
            "lm-resizer.exe",
            Some(PathBuf::from("C:/tmp/ccr.sqlite3")),
        )
        .unwrap();
        assert_eq!(config.matches("[mcp_servers.lm_resizer]").count(), 1);
        assert!(config.contains("model = \"gpt-test\""));
        assert!(config.contains("[mcp_servers.other]"));
        assert!(config.contains("command = \"lm-resizer.exe\""));
        assert!(config.contains("\"--store\""));
        assert!(!config.contains("command = \"old\""));
    }

    #[test]
    fn command_extensions_include_windows_cmd_variants() {
        let extensions = command_extensions("codex");
        if cfg!(windows) {
            assert!(extensions
                .iter()
                .any(|ext| ext.eq_ignore_ascii_case(".cmd")));
            assert!(extensions
                .iter()
                .any(|ext| ext.eq_ignore_ascii_case(".exe")));
        } else {
            assert_eq!(extensions, vec![String::new()]);
        }
    }

    #[test]
    fn exec_generic_filter_collapses_repeated_lines() {
        let filtered = filter_generic("same\nsame\nsame\nnext\n", 20);
        assert!(filtered.contains("same"));
        assert!(filtered.contains("... previous line repeated 2 times"));
        assert!(filtered.contains("next"));
    }

    #[test]
    fn exec_search_filter_groups_matches_by_file() {
        let raw = "src/a.rs:1:match one\nsrc/a.rs:2:match two\nsrc/a.rs:3:match three\nsrc/b.rs:4:match four\n";
        let filtered = filter_search_results(raw, 20, 2);
        assert!(filtered.contains("src/a.rs: 3 matches"));
        assert!(filtered.contains("src/b.rs: 1 matches"));
        assert!(filtered.contains("src/a.rs:1:match one"));
        assert!(!filtered.contains("src/a.rs:3:match three"));
        assert!(filtered.contains("omitted 1 low-signal lines"));
    }

    #[test]
    fn live_zone_compresses_openai_chat_tool_payload() {
        // A noisy `role: "tool"` message holding a large uniform-schema JSON
        // array is exactly what the provider-aware OpenAI chat dispatcher
        // (SmartCrusher) should compress — not the generic field-walk.
        let rows: Vec<Value> = (0..60)
            .map(|i| json!({"id": i, "name": format!("item-{i}"), "status": "ok", "score": 100}))
            .collect();
        let tool_content = serde_json::to_string(&Value::Array(rows)).unwrap();
        assert!(
            tool_content.len() > 512,
            "fixture must exceed live-zone byte floor"
        );
        let mut body = json!({
            "model": "gpt-4o",
            "messages": [
                {"role": "user", "content": "summarize the results"},
                {"role": "assistant", "content": "calling tool"},
                {"role": "tool", "tool_call_id": "t1", "content": tool_content},
            ]
        });
        let store = InMemoryCcrStore::new();
        let mut stats = ProxyCompressionStats::default();
        let handled = try_live_zone_compress("/v1/chat/completions", &mut body, &store, &mut stats);
        assert!(handled, "live-zone should handle /v1/chat/completions");
        assert!(
            stats.fields_seen > 0,
            "should record at least one block outcome"
        );
        assert!(
            stats.bytes_saved > 0,
            "noisy uniform JSON in the tool message should compress (provider-aware)"
        );
    }

    #[test]
    fn live_zone_skips_non_chat_routes() {
        // Bedrock/Vertex/model-invoke/unknown routes have no provider-aware
        // dispatcher → the caller must fall back to the generic compressor.
        let mut body = json!({"model": "anthropic.claude", "input": "hi"});
        let store = InMemoryCcrStore::new();
        let mut stats = ProxyCompressionStats::default();
        let handled = try_live_zone_compress(
            "/model/anthropic.claude/invoke",
            &mut body,
            &store,
            &mut stats,
        );
        assert!(
            !handled,
            "non-live-zone routes must fall back to generic compression"
        );
    }

    #[test]
    fn exec_diagnostic_filter_keeps_errors_and_summary() {
        let raw = "Compiling crate\nnoise\nerror[E0001]: broken\n  --> src/main.rs:1:1\nnote: details\nmore details\ntest result: FAILED. 0 passed; 1 failed\n";
        let filtered = filter_diagnostics(raw);
        assert!(filtered.contains("error[E0001]: broken"));
        assert!(filtered.contains("test result: FAILED"));
        assert!(filtered.contains("omitted"));
        assert!(!filtered.contains("Compiling crate"));
    }

    #[test]
    fn exec_filter_dispatches_known_commands() {
        let (filter, _text) =
            filter_command_output(&["git".into(), "status".into()], "On branch main\n");
        assert_eq!(filter, "git_status");

        let (filter, _text) =
            filter_command_output(&["cargo".into(), "test".into()], "test result: ok\n");
        assert_eq!(filter, "cargo_test");

        let (filter, _text) =
            filter_command_output(&["rg".into(), "needle".into()], "src/main.rs:1:needle\n");
        assert_eq!(filter, "search_results");
    }

    #[test]
    fn rewrite_reports_supported_command() {
        let report = rewrite_command_report(&["git".into(), "status".into()]);
        assert!(report.supported);
        assert_eq!(report.filter, "git_status");
        assert_eq!(
            report.rewritten.as_deref(),
            Some("lm-resizer exec -- git status")
        );
    }

    #[test]
    fn rewrite_leaves_generic_command_unsupported() {
        let report = rewrite_command_report(&["unknown-tool".into(), "arg".into()]);
        assert!(!report.supported);
        assert_eq!(report.filter, "generic");
        assert!(report.rewritten.is_none());
    }

    #[test]
    fn rewrite_shell_rewrites_compound_segments() {
        let report = rewrite_shell_report("cargo test && git status");
        assert!(report.changed);
        assert_eq!(
            report.rewritten,
            "lm-resizer exec -- cargo test && lm-resizer exec -- git status"
        );
        assert_eq!(report.rewrites.len(), 2);
    }

    #[test]
    fn rewrite_shell_preserves_redirect_suffix() {
        let report = rewrite_shell_report("git status > status.txt");
        assert_eq!(
            report.rewritten,
            "lm-resizer exec -- git status > status.txt"
        );
    }

    #[test]
    fn rewrite_shell_does_not_rewrite_pipe_consumer() {
        let report = rewrite_shell_report("git status | grep modified");
        assert_eq!(
            report.rewritten,
            "lm-resizer exec -- git status | grep modified"
        );
        assert_eq!(report.rewrites.len(), 1);
    }

    #[test]
    fn split_shell_words_respects_quotes() {
        assert_eq!(
            split_shell_words("git commit -m \"hello world\"").unwrap(),
            vec!["git", "commit", "-m", "hello world"]
        );
    }

    #[test]
    fn exec_toml_filter_keeps_matching_lines() {
        let def = TomlFilterDef {
            name: "sample".to_string(),
            match_command: "^sample".to_string(),
            strip_ansi: false,
            strip_lines_matching: Vec::new(),
            keep_lines_matching: vec!["error|warning".to_string()],
            replace: Vec::new(),
            truncate_lines_at: Some(20),
            head_lines: None,
            tail_lines: None,
            max_lines: Some(2),
            on_empty: Some("empty".to_string()),
        };
        let filter = compile_toml_filter(def).unwrap();
        let output = apply_toml_filter(
            &filter,
            "noise\nwarning: a long warning message that should be cut\nerror: bad\n",
        );
        assert!(output.contains("warning: a long w..."));
        assert!(output.contains("error: bad"));
        assert!(!output.contains("noise"));
    }

    #[test]
    fn verify_filters_runs_inline_tests() {
        let root =
            std::env::temp_dir().join(format!("lm-resizer-filter-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("filters.toml");
        std::fs::write(
            &path,
            r#"[[filters]]
name = "sample"
match_command = "^sample"
keep_lines_matching = ["error"]

[[tests]]
filter = "sample"
name = "keeps errors"
input = "noise\nerror: bad\n"
expected = "error: bad\n"
"#,
        )
        .unwrap();

        let report = verify_filter_file(&path).unwrap();
        assert_eq!(report.filters, 1);
        assert_eq!(report.tests, 1);
        assert_eq!(report.passed, 1);
        assert_eq!(report.failed, 0);
    }

    #[test]
    fn verify_filters_reports_missing_filter() {
        let root =
            std::env::temp_dir().join(format!("lm-resizer-filter-missing-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("filters.toml");
        std::fs::write(
            &path,
            r#"[[tests]]
filter = "missing"
name = "fails"
input = "x"
expected = "x"
"#,
        )
        .unwrap();

        let report = verify_filter_file(&path).unwrap();
        assert_eq!(report.failed, 1);
        assert_eq!(report.outcomes[0].actual, "<missing filter>");
    }

    #[test]
    fn verify_filters_reports_schema_diagnostics() {
        let root =
            std::env::temp_dir().join(format!("lm-resizer-filter-schema-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("filters.toml");
        std::fs::write(
            &path,
            r#"[[filters]]
name = "sample"
match_command = "^sample"
keep_lines_matching = ["error"]

[[filters]]
name = "sample"
match_command = "^sample --again"
max_lines = 5
"#,
        )
        .unwrap();

        let report = verify_filter_file(&path).unwrap();
        assert_eq!(report.filters, 1);
        assert!(report
            .diagnostics
            .iter()
            .any(|item| item.contains("duplicate filter `sample`")));
        assert!(report
            .diagnostics
            .iter()
            .any(|item| item.contains("no [[tests]] entries")));
        assert!(report
            .diagnostics
            .iter()
            .any(|item| item.contains("filter `sample` has no inline")));
    }

    #[test]
    fn verify_filters_rejects_unknown_fields() {
        let root =
            std::env::temp_dir().join(format!("lm-resizer-filter-unknown-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("filters.toml");
        std::fs::write(
            &path,
            r#"[[filters]]
name = "sample"
match_command = "^sample"
unknown_action = true
"#,
        )
        .unwrap();

        let err = verify_filter_file(&path).unwrap_err().to_string();
        assert!(err.contains("invalid filter TOML"));
    }

    #[test]
    fn init_filters_writes_verifiable_template() {
        let root =
            std::env::temp_dir().join(format!("lm-resizer-filter-init-{}", std::process::id()));
        let path = root.join("filters.toml");

        let report = init_filter_file(&path, FilterProfile::Generic, false).unwrap();
        assert!(report.written);
        assert!(path.exists());

        let verify = verify_filter_file(&path).unwrap();
        assert_eq!(verify.failed, 0);
        assert_eq!(verify.diagnostics.len(), 0);

        let second = init_filter_file(&path, FilterProfile::Generic, false).unwrap();
        assert!(!second.written);
    }

    #[test]
    fn init_filter_profiles_are_verifiable() {
        for profile in [
            FilterProfile::Generic,
            FilterProfile::Rust,
            FilterProfile::Node,
            FilterProfile::Python,
            FilterProfile::Infra,
        ] {
            let root = std::env::temp_dir().join(format!(
                "lm-resizer-filter-profile-{}-{}",
                std::process::id(),
                format!("{profile:?}").to_ascii_lowercase()
            ));
            let path = root.join("filters.toml");
            init_filter_file(&path, profile, false).unwrap();
            let verify = verify_filter_file(&path).unwrap();
            assert_eq!(verify.failed, 0, "profile {profile:?}");
            assert_eq!(verify.diagnostics.len(), 0, "profile {profile:?}");
        }
    }

    #[test]
    fn sanitize_provider_fixture_redacts_secrets_and_long_strings() {
        let root = std::env::temp_dir().join(format!(
            "lm-resizer-provider-sanitize-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let input = root.join("input.json");
        let output = root.join("fixture.json");
        std::fs::write(
            &input,
            serde_json::to_string(&json!({
                "authorization": "Bearer secret",
                "messages": [{
                    "role": "user",
                    "content": "[{\"large\": true}, {\"large\": true}]"
                }]
            }))
            .unwrap(),
        )
        .unwrap();

        let report = sanitize_provider_fixture(ProviderKind::OpenAi, &input, &output, 10).unwrap();
        assert_eq!(report.redacted_fields, 1);
        assert_eq!(report.placeholder_strings, 1);
        let value: Value = serde_json::from_str(&std::fs::read_to_string(output).unwrap()).unwrap();
        assert_eq!(value["authorization"], "__REDACTED__");
        assert_eq!(value["messages"][0]["content"], "__LARGE_JSON_ARRAY__");
    }

    #[test]
    fn audit_filter_reports_actions() {
        let root =
            std::env::temp_dir().join(format!("lm-resizer-filter-audit-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("filters.toml");
        std::fs::write(
            &path,
            r#"[[filters]]
name = "sample"
match_command = "^sample"
strip_ansi = true
keep_lines_matching = ["error"]
max_lines = 10
"#,
        )
        .unwrap();

        let report = audit_filter_file(&path).unwrap();
        assert_eq!(report.trust_status, "untrusted");
        assert!(report.trusted_hash.is_none());
        assert_eq!(report.filters.len(), 1);
        assert!(report.filters[0]
            .actions
            .contains(&"strip_ansi".to_string()));
        assert!(report.filters[0]
            .actions
            .contains(&"keep_lines_matching(1)".to_string()));
    }

    #[test]
    fn filter_audit_review_is_markdown_and_actionable() {
        let root =
            std::env::temp_dir().join(format!("lm-resizer-filter-review-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("filters.toml");
        std::fs::write(
            &path,
            r#"[[filters]]
name = "sample"
match_command = "^sample"
keep_lines_matching = ["error"]

[[tests]]
filter = "sample"
name = "keeps errors"
input = "noise\nerror: bad\n"
expected = "error: bad\n"
"#,
        )
        .unwrap();

        let report = audit_filter_file(&path).unwrap();
        let review = render_filter_audit_review(&report);
        assert!(review.contains("# lm-resizer Filter Review"));
        assert!(review.contains("- Trust status: `untrusted`"));
        assert!(review.contains("| `sample` | `keeps errors` | passed |"));
        assert!(review.contains("### `sample`"));
        assert!(review.contains("lm-resizer verify-filters --path"));
        assert!(review.contains("lm-resizer trust-filters --path"));
    }

    #[test]
    fn exec_builtin_toml_filter_handles_terraform_plan() {
        let (filter, text) = filter_command_output(
            &["terraform".into(), "plan".into()],
            "Refreshing state...\nPlan: 1 to add, 0 to change, 0 to destroy.\n",
        );
        assert_eq!(filter, "toml:terraform-plan");
        assert!(text.contains("Plan: 1 to add"));
        assert!(!text.contains("Refreshing state"));
    }

    #[test]
    fn exec_builtin_toml_filters_common_install_noise() {
        let (filter, text) = filter_command_output(
            &["npm".into(), "install".into()],
            "Progress: resolved 100\nadded 12 packages\naudited 12 packages\n",
        );
        assert_eq!(filter, "toml:package-install");
        assert!(text.contains("added 12 packages"));
        assert!(!text.contains("Progress:"));

        let (filter, text) = filter_command_output(
            &["brew".into(), "install".into(), "demo".into()],
            "==> Downloading demo\n==> Installing demo\nWarning: already installed\n",
        );
        assert_eq!(filter, "toml:brew-install");
        assert!(text.contains("Warning: already installed"));
        assert!(!text.contains("Downloading demo"));
    }

    #[test]
    fn exec_builtin_toml_filter_handles_make_errors() {
        let (filter, text) = filter_command_output(
            &["make".into()],
            "cc main.c\nwarning: unused\nerror: failed\n",
        );
        assert_eq!(filter, "toml:make");
        assert!(text.contains("warning: unused"));
        assert!(text.contains("error: failed"));
        assert!(!text.contains("cc main.c"));
    }

    #[test]
    fn exec_builtin_toml_filters_more_ecosystems() {
        let cases = [
            (
                vec!["go", "test"],
                "=== RUN test\n--- FAIL: TestThing\nFAIL\n",
                "toml:go-test",
            ),
            (
                vec!["dotnet", "test"],
                "noise\nTotal tests: 3. Passed: 2. Failed: 1\n",
                "toml:dotnet",
            ),
            (
                vec!["mvn", "test"],
                "Downloading dependency\n[ERROR] Failed to execute goal\nBUILD FAILURE\n",
                "toml:jvm-build",
            ),
            (
                vec!["uv", "sync"],
                "Resolved 42 packages\nDownloading wheels\nInstalled 42 packages\n",
                "toml:python-package",
            ),
            (
                vec!["ruff", "check"],
                "src/main.py:1:1: E402 bad\nFound 1 error.\n",
                "toml:python-lint",
            ),
            (
                vec!["eslint", "."],
                "file.ts\nError: bad\n",
                "toml:js-quality",
            ),
            (
                vec!["docker", "logs", "app"],
                "info\nERROR failed\n",
                "toml:docker-logs",
            ),
            (
                vec!["kubectl", "get", "pods"],
                "NAME READY STATUS\napp 0/1 CrashLoopBackOff\n",
                "toml:kubectl",
            ),
            (
                vec!["aws", "lambda", "list-functions"],
                "{\"FunctionName\":\"demo\"}\n",
                "toml:aws",
            ),
        ];

        for (command, raw, expected_filter) in cases {
            let command = command.into_iter().map(String::from).collect::<Vec<_>>();
            let (filter, text) = filter_command_output(&command, raw);
            assert_eq!(filter, expected_filter, "command {command:?}");
            assert!(!text.trim().is_empty(), "command {command:?}");
        }
    }

    #[test]
    fn exec_tsc_filter_groups_by_file() {
        let raw = "src/a.ts(1,2): error TS2322: Type 'string' is not assignable\nsrc/a.ts(2,3): error TS7006: Parameter has any\n";
        let filtered = filter_tsc(raw);
        assert!(filtered.contains("TypeScript: 2 diagnostics in 1 files"));
        assert!(filtered.contains("src/a.ts: 2 diagnostics"));
        assert!(filtered.contains("TS2322"));
    }

    #[test]
    fn exec_cargo_test_filter_summarizes_passes() {
        let raw =
            "Compiling demo\nrunning 1 test\ntest ok ... ok\ntest result: ok. 1 passed; 0 failed\n";
        let filtered = filter_cargo_test(raw);
        assert!(filtered.contains("test result: ok"));
        assert!(!filtered.contains("Compiling demo"));
    }

    #[test]
    fn normalized_command_text_strips_windows_script_extension() {
        let command = vec![
            "C:/tmp/terraform.cmd".to_string(),
            "plan".to_string(),
            "-no-color".to_string(),
        ];
        assert_eq!(
            normalized_command_text(&command),
            "terraform plan -no-color"
        );
    }

    #[test]
    fn trust_hash_is_stable_sha256() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn discover_pairs_jsonl_command_and_output() {
        let content = r#"{"command":"cargo test"}
{"output":"Compiling demo\nrunning 1 test\ntest ok ... ok\ntest result: ok. 1 passed; 0 failed\n"}
"#;
        let report = discover_in_content(content, "session.jsonl");
        assert_eq!(report.command_outputs, 1);
        assert_eq!(report.rewritable_commands, 1);
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(report.candidates[0].filter, "cargo_test");
        assert!(report.filtered_bytes < report.original_bytes);
    }

    #[test]
    fn discover_extracts_claude_style_tool_messages() {
        let content = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"git status"}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","content":"On branch main\nnothing to commit\n"}]}}
"#;
        let report = discover_in_content(content, "claude.jsonl");
        assert_eq!(report.command_outputs, 1);
        assert_eq!(report.rewritable_commands, 1);
        assert_eq!(report.candidates[0].filter, "git_status");
    }

    #[test]
    fn discover_extracts_codex_style_arguments() {
        let content = r#"{"tool_name":"exec_command","arguments":"{\"command\":\"cargo test\"}"}
{"tool_output":"Compiling demo\nrunning 1 test\ntest ok ... ok\ntest result: ok. 1 passed; 0 failed\n"}
"#;
        let report = discover_in_content(content, "codex.jsonl");
        assert_eq!(report.command_outputs, 1);
        assert_eq!(report.rewritable_commands, 1);
        assert_eq!(report.candidates[0].filter, "cargo_test");
    }

    #[test]
    fn discover_markdown_summarizes_candidates() {
        let content = r#"{"command":"cargo test"}
{"output":"Compiling demo\nrunning 1 test\ntest ok ... ok\ntest result: ok. 1 passed; 0 failed\n"}
"#;
        let mut report = discover_in_content(content, "session.jsonl");
        report.files_scanned = 1;
        report.estimated_bytes_saved = report.original_bytes - report.filtered_bytes;
        report.estimated_tokens_saved = report.estimated_bytes_saved / 4;
        let markdown = format_discover_markdown(&report);
        assert!(markdown.contains("# lm-resizer Discover Audit"));
        assert!(markdown.contains("| `cargo test` | `cargo_test` |"));
    }

    #[test]
    fn agent_session_candidates_cover_codex_and_claude_shapes() {
        let home = PathBuf::from("C:/Users/example");
        let codex = codex_session_candidates_from_home(&home.join(".codex"));
        assert!(codex.contains(&home.join(".codex").join("sessions")));
        assert!(codex.contains(&home.join(".codex").join("history.jsonl")));

        let claude = claude_session_candidates_from_home(&home.join(".claude"));
        assert!(claude.contains(&home.join(".claude").join("projects")));
        assert!(claude.contains(&home.join(".claude").join("transcripts")));
    }

    #[test]
    fn discover_sessions_markdown_includes_paths_and_discover_summary() {
        let report = DiscoverSessionsReport {
            agent: "codex".to_string(),
            paths: vec!["C:/Users/example/.codex/sessions".to_string()],
            missing: vec!["C:/Users/example/.codex/history.jsonl".to_string()],
            discover: DiscoverReport {
                files_scanned: 1,
                command_outputs: 1,
                rewritable_commands: 1,
                original_bytes: 100,
                filtered_bytes: 40,
                estimated_bytes_saved: 60,
                estimated_tokens_saved: 15,
                candidates: Vec::new(),
            },
        };
        let markdown = format_discover_sessions_markdown(&report);
        assert!(markdown.contains("Agent: codex"));
        assert!(markdown.contains("Paths scanned: 1"));
        assert!(markdown.contains(".codex/sessions"));
        assert!(markdown.contains("Estimated tokens saved: 15"));
    }

    #[test]
    fn learn_recommends_rewrite_for_compressible_sessions() {
        let content = r#"{"command":"cargo test"}
{"output":"Compiling demo\nrunning 1 test\ntest ok ... ok\ntest result: ok. 1 passed; 0 failed\n"}
"#;
        let mut discover = discover_in_content(content, "session.jsonl");
        discover.files_scanned = 1;
        discover.estimated_bytes_saved = discover.original_bytes - discover.filtered_bytes;
        discover.estimated_tokens_saved = discover.estimated_bytes_saved / 4;

        let recommendations = build_learn_recommendations(&discover, &json!({"commands": 0}));
        assert!(recommendations
            .iter()
            .any(|rec| rec.instruction.contains("lm-resizer rewrite-shell")));
        let markdown = format_learn_markdown(&recommendations, &discover, &json!({"commands": 0}));
        assert!(markdown.contains("# lm-resizer Learned Agent Guidance"));
        assert!(markdown.contains("Route noisy commands"));
    }

    #[test]
    fn eval_report_summarizes_discover_fixture() {
        let root = std::env::temp_dir().join(format!("lm-resizer-eval-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("session.jsonl");
        std::fs::write(
            &path,
            r#"{"command":"cargo test"}
{"output":"Compiling demo\nrunning 1 test\ntest ok ... ok\ntest result: ok. 1 passed; 0 failed\n"}
"#,
        )
        .unwrap();
        let report = run_eval(&[path], false).unwrap();
        assert!(report.pass);
        assert_eq!(report.command_outputs, 1);
        assert!(report.estimated_bytes_saved > 0);
        assert!(format_eval_markdown(&report).contains("# lm-resizer Eval"));
    }

    #[test]
    fn learning_block_is_reversible_and_separate_from_hooks() {
        let root =
            std::env::temp_dir().join(format!("lm-resizer-learn-block-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("AGENTS.md");
        std::fs::write(&path, "base\n").unwrap();

        upsert_learning_block(
            &path,
            "# lm-resizer Learned Agent Guidance\n\n## Route noisy commands\n\nInstruction: use lm-resizer\n",
        )
        .unwrap();
        upsert_learning_block(
            &path,
            "# lm-resizer Learned Agent Guidance\n\n## Keep stats visible\n\nInstruction: run stats\n",
        )
        .unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains(LEARN_BLOCK_START));
        assert!(content.contains("Keep stats visible"));
        assert!(!content.contains("Route noisy commands"));
        assert!(!content.contains(HOOK_BLOCK_START));
    }

    #[test]
    fn stats_markdown_includes_exec_summary() {
        let report = json!({
            "entries": 2,
            "empty": false,
            "exec_history": {
                "commands": 3,
                "bytes_saved": 120,
                "estimated_tokens_saved": 30,
                "by_filter": [
                    {"name": "cargo_test", "commands": 2, "bytes_saved": 100, "estimated_tokens_saved": 25}
                ]
            },
            "retrieval_feedback": {
                "retrievals": 4,
                "bytes": 2048
            }
        });
        let markdown = format_stats_markdown(&report);
        assert!(markdown.contains("# lm-resizer Stats"));
        assert!(markdown.contains("| `cargo_test` | 2 | 100 | 25 |"));
        assert!(markdown.contains("- CCR retrievals: 4"));
    }

    #[test]
    fn image_dimensions_reads_png_header() {
        let mut png = b"\x89PNG\r\n\x1a\n00000000".to_vec();
        png.extend_from_slice(&640u32.to_be_bytes());
        png.extend_from_slice(&480u32.to_be_bytes());
        let (format, width, height) = image_dimensions(&png);
        assert_eq!(format, "png");
        assert_eq!(width, Some(640));
        assert_eq!(height, Some(480));
    }

    #[test]
    fn voice_transcript_removes_fillers() {
        let report = analyze_voice_transcript("um we should actually ship this");
        assert_eq!(report.filler_count, 2);
        assert_eq!(report.cleaned, "we should ship this");
    }

    #[test]
    fn ml_status_defaults_to_deterministic_detection() {
        std::env::remove_var("LM_RESIZER_ENABLE_MAGIKA");
        let report = ml_status_report();
        assert!(!report.magika_enabled);
        assert!(report.hot_path.contains("deterministic"));
    }

    #[test]
    fn sanitize_slug_limits_unsafe_filename_chars() {
        assert_eq!(sanitize_slug("git status > out"), "git_status___out");
    }

    #[test]
    fn hook_templates_call_rewrite_without_execution() {
        let sh = hook_rewrite_sh("lm-resizer");
        assert!(sh.contains("rewrite --"));
        assert!(!sh.contains("exec --"));

        let ps1 = hook_rewrite_ps1("lm-resizer.exe");
        assert!(ps1.contains("rewrite --"));
        assert!(!ps1.contains("exec --"));

        let readme = hook_readme();
        assert!(readme.contains("do not execute"));

        let rules = hook_agent_rules();
        assert!(rules.contains("lm-resizer exec --"));
        assert!(rules.contains("rewrite-shell"));
    }

    #[test]
    fn native_hook_configs_target_codex_and_claude_post_tool_use() {
        let codex = codex_native_hooks_json("lm-resizer").unwrap();
        assert!(codex.contains("PostToolUse"));
        assert!(codex.contains("\"matcher\": \"Bash\""));
        assert!(codex.contains("hook --client codex --event PostToolUse"));

        let claude = claude_native_hooks_json("lm-resizer").unwrap();
        assert!(claude.contains("PostToolUse"));
        assert!(claude.contains("\"matcher\": \"Bash\""));
        assert!(claude.contains("hook --client claude --event PostToolUse"));
    }

    #[test]
    fn native_hook_extracts_codex_and_claude_command_output() {
        let codex = json!({
            "tool_name": "Bash",
            "tool_input": {"command": "cargo test"},
            "tool_response": {"stdout": "Compiling demo\ntest result: ok\n", "exit_code": 0}
        });
        assert_eq!(extract_hook_command(&codex).as_deref(), Some("cargo test"));
        assert_eq!(
            extract_hook_output(&codex).as_deref(),
            Some("Compiling demo\ntest result: ok\n")
        );
        assert_eq!(extract_hook_exit_code(Some(&codex)), Some(0));

        let claude = json!({
            "tool_name": "Bash",
            "tool_input": {"command": "git status"},
            "tool_response": {"content": "On branch main\nnothing to commit\n"}
        });
        assert_eq!(extract_hook_command(&claude).as_deref(), Some("git status"));
        assert_eq!(
            extract_hook_output(&claude).as_deref(),
            Some("On branch main\nnothing to commit\n")
        );
    }

    #[test]
    fn command_shims_call_exec_with_original_path() {
        let original = if cfg!(windows) {
            PathBuf::from("C:/tools/git.exe")
        } else {
            PathBuf::from("/usr/bin/git")
        };
        let content = if cfg!(windows) {
            command_shim_cmd("lm-resizer.exe", &original)
        } else {
            command_shim_sh("lm-resizer", &original)
        };
        assert!(content.contains("exec --"));
        assert!(content.contains(&original.display().to_string()));
        assert!(shim_path_hint(Path::new(".lm-resizer/shims")).contains("PATH"));
    }

    #[test]
    fn marked_hook_block_is_reversible() {
        let original = "# Project\n\nKeep this.\n";
        let block = hook_instruction_block();
        let mut content = original.to_string();
        content.push_str(&block);
        assert!(content.contains(HOOK_BLOCK_START));
        let stripped = strip_marked_block(&content);
        assert_eq!(stripped, original.trim());
    }

    #[test]
    fn project_scoped_client_paths_use_project_dir() {
        let project = PathBuf::from("C:/work/example");
        assert_eq!(
            ClientConfig::Claude.path("project", &project).unwrap(),
            project.join(".mcp.json")
        );
        assert_eq!(
            ClientConfig::Cursor.path("project", &project).unwrap(),
            project.join(".cursor").join("mcp.json")
        );
        assert_eq!(
            ClientConfig::VsCode.path("project", &project).unwrap(),
            project.join(".vscode").join("mcp.json")
        );
    }

    #[test]
    fn agent_env_maps_cursor_and_opencode_to_openai_base() {
        let mut command = if cfg!(windows) {
            let mut cmd = Command::new("cmd");
            cmd.arg("/C").arg("set OPENAI_BASE_URL");
            cmd
        } else {
            let mut cmd = Command::new("sh");
            cmd.arg("-c").arg("printf %s \"$OPENAI_BASE_URL\"");
            cmd
        };
        apply_agent_env(&mut command, "cursor", "http://127.0.0.1:8787");
        let output = command.output().unwrap();
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(text.contains("http://127.0.0.1:8787/v1"));
    }

    #[test]
    fn proxy_payload_compression_updates_stats() {
        let store = InMemoryCcrStore::default();
        let pipeline = build_pipeline();
        let rows = (0..80)
            .map(|i| json!({ "id": i, "status": "ok", "payload": "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx" }))
            .collect::<Vec<_>>();
        let mut body = json!({
            "model": "test",
            "messages": [{ "role": "user", "content": serde_json::to_string(&rows).unwrap() }]
        });
        let mut stats = ProxyCompressionStats::default();
        compress_json_payload(&mut body, &store, &pipeline, &mut stats).unwrap();
        assert_eq!(stats.fields_seen, 1);
        assert_eq!(stats.fields_compressed, 1);
        assert!(stats.bytes_saved > 0);
    }

    #[test]
    fn provider_kind_accepts_bedrock_and_vertex() {
        assert!(matches!(
            "bedrock".parse::<ProviderKind>().unwrap(),
            ProviderKind::Bedrock
        ));
        assert!(matches!(
            "vertex-ai".parse::<ProviderKind>().unwrap(),
            ProviderKind::Vertex
        ));
    }

    #[test]
    fn provider_streaming_paths_are_detected() {
        assert!(is_streaming_proxy_path(
            "/model/anthropic.claude-3-sonnet/invoke-with-response-stream"
        ));
        assert!(is_streaming_proxy_path(
            "/v1/projects/p/locations/us/publishers/google/models/gemini:streamGenerateContent"
        ));
        assert!(!is_streaming_proxy_path(
            "/v1/projects/p/locations/us/publishers/google/models/gemini:generateContent"
        ));
    }

    #[test]
    fn preview_sse_body_emits_event_stream_frames() {
        let body = preview_sse_body(&json!({"mode": "preview", "request": {"stream": true}}));
        assert!(body.starts_with("event: lm_resizer_preview\n"));
        assert!(body.contains("\"mode\":\"preview\""));
        assert!(body.ends_with("event: done\ndata: [DONE]\n\n"));
    }

    #[test]
    fn websocket_preview_message_is_structured() {
        let message = websocket_preview_message("/v1/realtime", true);
        assert!(message.contains("\"websocket\":true"));
        assert!(message.contains("/v1/realtime"));
        assert!(message.contains("upstream WebSocket bridging is not enabled"));
    }

    #[test]
    fn websocket_upstream_url_converts_http_schemes() {
        assert_eq!(
            websocket_upstream_url("http://127.0.0.1:9000", "/v1/realtime").unwrap(),
            "ws://127.0.0.1:9000/v1/realtime"
        );
        assert_eq!(
            websocket_upstream_url("https://api.example.com/", "/v1/realtime?model=x").unwrap(),
            "wss://api.example.com/v1/realtime?model=x"
        );
    }

    #[test]
    fn websocket_request_applies_provider_auth() {
        let request = websocket_connect_request(
            "ws://localhost/v1/realtime",
            Some("token"),
            ProviderKind::OpenAi,
        )
        .unwrap();
        assert_eq!(
            request
                .headers()
                .get(reqwest::header::AUTHORIZATION)
                .unwrap(),
            "Bearer token"
        );
        let request = websocket_connect_request(
            "ws://localhost/v1/messages",
            Some("anthropic-token"),
            ProviderKind::Anthropic,
        )
        .unwrap();
        assert_eq!(
            request.headers().get("x-api-key").unwrap(),
            "anthropic-token"
        );
    }

    #[test]
    fn dashboard_html_reports_existing_counters() {
        let html = dashboard_html(
            2,
            false,
            &json!({"commands": 3, "bytes_saved": 120, "estimated_tokens_saved": 30}),
        );
        assert!(html.contains("lm-resizer dashboard"));
        assert!(html.contains(">2</div>"));
        assert!(html.contains(">3</div>"));
        assert!(html.contains("No background telemetry collector"));
    }

    #[test]
    fn provider_payload_shapes_are_compressed() {
        let store = InMemoryCcrStore::default();
        let pipeline = build_pipeline();
        let rows = (0..80)
            .map(|i| json!({ "id": i, "status": "ok", "payload": "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx" }))
            .collect::<Vec<_>>();
        let large = serde_json::to_string(&rows).unwrap();
        let mut bedrock_body = json!({
            "anthropic_version": "bedrock-2023-05-31",
            "messages": [{ "role": "user", "content": [{ "type": "text", "text": large }] }]
        });
        let mut vertex_body = json!({
            "contents": [{ "role": "user", "parts": [{ "text": large }] }]
        });
        let mut bedrock_stats = ProxyCompressionStats::default();
        let mut vertex_stats = ProxyCompressionStats::default();
        compress_json_payload(&mut bedrock_body, &store, &pipeline, &mut bedrock_stats).unwrap();
        compress_json_payload(&mut vertex_body, &store, &pipeline, &mut vertex_stats).unwrap();
        assert_eq!(bedrock_stats.fields_compressed, 1);
        assert_eq!(vertex_stats.fields_compressed, 1);
        assert!(bedrock_stats.bytes_saved > 0);
        assert!(vertex_stats.bytes_saved > 0);
    }

    #[test]
    fn provider_cache_fixtures_preserve_envelopes_and_cache_markers() {
        let store = InMemoryCcrStore::default();
        let pipeline = build_pipeline();
        let rows = (0..100)
            .map(|i| json!({ "id": i, "status": "ok", "payload": "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy" }))
            .collect::<Vec<_>>();
        let large = serde_json::to_string(&rows).unwrap();
        let fixtures = vec![
            (
                ProviderKind::OpenAi,
                provider_fixture("openai-chat.json", &large),
                vec![("model", json!("gpt-test"))],
            ),
            (
                ProviderKind::Anthropic,
                provider_fixture("anthropic-messages.json", &large),
                vec![
                    ("model", json!("claude-test")),
                    (
                        "messages/0/content/0/cache_control/type",
                        json!("ephemeral"),
                    ),
                ],
            ),
            (
                ProviderKind::Bedrock,
                provider_fixture("bedrock-anthropic.json", &large),
                vec![("anthropic_version", json!("bedrock-2023-05-31"))],
            ),
            (
                ProviderKind::Vertex,
                provider_fixture("vertex-gemini.json", &large),
                vec![
                    ("contents/0/role", json!("user")),
                    ("generationConfig/temperature", json!(0.2)),
                ],
            ),
        ];

        for (provider, mut body, expectations) in fixtures {
            let mut stats = ProxyCompressionStats {
                provider_cache_policy: provider_cache_policy(provider).to_string(),
                ..ProxyCompressionStats::default()
            };
            compress_json_payload(&mut body, &store, &pipeline, &mut stats).unwrap();
            assert!(
                stats.fields_compressed > 0,
                "{provider:?} fixture should compress at least one field"
            );
            assert!(
                !stats.provider_cache_policy.is_empty(),
                "{provider:?} should report a cache policy"
            );
            for (pointer, expected) in expectations {
                let actual = body
                    .pointer(&format!("/{}", pointer))
                    .unwrap_or_else(|| panic!("missing provider fixture pointer: {pointer}"));
                assert_eq!(actual, &expected, "{provider:?} changed {pointer}");
            }
        }
    }

    fn provider_fixture(name: &str, large_payload: &str) -> Value {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("provider-cache")
            .join(name);
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
        let mut value: Value = serde_json::from_str(&content).unwrap();
        replace_fixture_placeholder(&mut value, large_payload);
        value
    }

    fn replace_fixture_placeholder(value: &mut Value, large_payload: &str) {
        match value {
            Value::String(text) if text == "__LARGE_JSON_ARRAY__" => {
                *text = large_payload.to_string();
            }
            Value::Array(items) => {
                for item in items {
                    replace_fixture_placeholder(item, large_payload);
                }
            }
            Value::Object(map) => {
                for item in map.values_mut() {
                    replace_fixture_placeholder(item, large_payload);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn provider_cache_policy_documents_supported_provider_shapes() {
        assert!(provider_cache_policy(ProviderKind::OpenAi).contains("OpenAI"));
        assert!(provider_cache_policy(ProviderKind::Anthropic).contains("Anthropic"));
        assert!(provider_cache_policy(ProviderKind::Bedrock).contains("Bedrock"));
        assert!(provider_cache_policy(ProviderKind::Vertex).contains("Vertex"));
    }

    #[test]
    fn proxy_body_to_json_accepts_gzip() {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder
            .write_all(br#"{"stream":false,"input":"hello"}"#)
            .unwrap();
        let body = encoder.finish().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_ENCODING,
            HeaderValue::from_static("gzip"),
        );
        let value = proxy_body_to_json(&headers, &body).unwrap();
        assert_eq!(value["input"], "hello");
    }

    #[test]
    fn proxy_body_to_json_accepts_deflate() {
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder
            .write_all(br#"{"stream":false,"input":"hello"}"#)
            .unwrap();
        let body = encoder.finish().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_ENCODING,
            HeaderValue::from_static("deflate"),
        );
        let value = proxy_body_to_json(&headers, &body).unwrap();
        assert_eq!(value["input"], "hello");
    }

    #[test]
    fn json_like_content_type_detects_suffixes() {
        let mut headers = HeaderMap::new();
        assert!(!is_json_like_content_type(&headers));
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("application/vnd.api+json"),
        );
        assert!(is_json_like_content_type(&headers));
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("multipart/form-data"),
        );
        assert!(!is_json_like_content_type(&headers));
    }

    #[test]
    fn decode_http_body_rejects_unknown_encoding() {
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_ENCODING,
            HeaderValue::from_static("br"),
        );
        let err = decode_http_body(&headers, b"{}").unwrap_err();
        assert!(err.to_string().contains("unsupported content-encoding"));
    }

    #[test]
    fn google_service_account_json_is_detected() {
        let content = r#"{
            "type": "service_account",
            "client_email": "svc@example.iam.gserviceaccount.com",
            "private_key": "-----BEGIN PRIVATE KEY-----\nQUJD\n-----END PRIVATE KEY-----\n",
            "token_uri": "https://oauth2.googleapis.com/token"
        }"#;
        let key: GoogleServiceAccountKey = serde_json::from_str(content).unwrap();
        assert_eq!(key.key_type.as_deref(), Some("service_account"));
        assert_eq!(
            key.client_email.as_deref(),
            Some("svc@example.iam.gserviceaccount.com")
        );
        assert_eq!(
            pem_private_key_der(key.private_key.as_deref().unwrap()).unwrap(),
            b"ABC"
        );
    }

    #[test]
    fn google_adc_non_service_account_is_ignored_before_signing() {
        let content = r#"{
            "type": "authorized_user",
            "client_id": "id",
            "client_secret": "secret",
            "refresh_token": "refresh"
        }"#;
        let key: GoogleServiceAccountKey = serde_json::from_str(content).unwrap();
        assert_ne!(key.key_type.as_deref(), Some("service_account"));
    }

    #[test]
    fn base64_url_bytes_uses_no_padding() {
        assert_eq!(base64_url_bytes(b"\xfb\xff"), "-_8");
    }

    #[test]
    fn hmac_sha256_matches_known_vector() {
        let digest = hmac_sha256(b"key", b"The quick brown fox jumps over the lazy dog");
        assert_eq!(
            hex_lower(&digest),
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
    }

    #[test]
    fn aws_timestamp_formats_unix_epoch() {
        let (amz_date, date) = aws_sigv4_timestamp_from_unix(0);
        assert_eq!(amz_date, "19700101T000000Z");
        assert_eq!(date, "19700101");
    }

    #[test]
    fn aws_sigv4_headers_include_signed_authorization() {
        let creds = AwsCredentials {
            access_key: "AKIDEXAMPLE".to_string(),
            secret_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".to_string(),
            session_token: Some("session-token".to_string()),
            region: "us-east-1".to_string(),
            service: "bedrock".to_string(),
        };
        let headers = aws_sigv4_headers(
            "https://bedrock-runtime.us-east-1.amazonaws.com/model/test/invoke?b=2&a=1",
            br#"{"inputText":"hello"}"#,
            &creds,
            "20260102T030405Z",
            "20260102",
        )
        .unwrap();
        let auth = headers
            .get(reqwest::header::AUTHORIZATION)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(auth.contains("Credential=AKIDEXAMPLE/20260102/us-east-1/bedrock/aws4_request"));
        assert!(auth.contains("SignedHeaders=content-type;host;x-amz-date;x-amz-security-token"));
        assert!(auth.contains("Signature="));
        assert_eq!(
            headers
                .get(HeaderName::from_static("x-amz-date"))
                .unwrap()
                .to_str()
                .unwrap(),
            "20260102T030405Z"
        );
    }

    #[test]
    fn aws_profile_content_reads_credentials_and_config_region() {
        let credentials = r#"
[default]
aws_access_key_id = DEFAULTKEY
aws_secret_access_key = DEFAULTSECRET

[work]
aws_access_key_id = WORKKEY
aws_secret_access_key = WORKSECRET
aws_session_token = WORKTOKEN
"#;
        let config = r#"
[profile work]
region = eu-west-3
"#;
        let creds = aws_credentials_from_profile_content(credentials, config, "work")
            .unwrap()
            .unwrap();
        assert_eq!(creds.access_key, "WORKKEY");
        assert_eq!(creds.secret_key, "WORKSECRET");
        assert_eq!(creds.session_token.as_deref(), Some("WORKTOKEN"));
        assert_eq!(creds.region, "eu-west-3");
    }

    #[test]
    fn aws_profile_content_supports_default_config_section() {
        let credentials = "";
        let config = r#"
[default]
aws_access_key_id = DEFAULTKEY
aws_secret_access_key = DEFAULTSECRET
region = us-west-2
"#;
        let creds = aws_credentials_from_profile_content(credentials, config, "default")
            .unwrap()
            .unwrap();
        assert_eq!(creds.access_key, "DEFAULTKEY");
        assert_eq!(creds.region, "us-west-2");
    }

    #[test]
    fn parse_ini_sections_ignores_comments_and_blank_lines() {
        let parsed = parse_ini_sections(
            r#"
# comment
[demo]
key = value
; other comment
"#,
        );
        assert_eq!(parsed["demo"]["key"], "value");
    }

    #[tokio::test]
    async fn google_adc_token_reads_env_first() {
        std::env::set_var("LM_RESIZER_GOOGLE_ACCESS_TOKEN", "vertex-token");
        let token = google_adc_access_token(&Client::new()).await.unwrap();
        std::env::remove_var("LM_RESIZER_GOOGLE_ACCESS_TOKEN");
        assert_eq!(token.as_deref(), Some("vertex-token"));
    }

    #[test]
    fn repository_has_no_python_runtime_surface() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let forbidden_names = [
            "pyproject.toml",
            "requirements.txt",
            "setup.py",
            "Pipfile",
            "poetry.lock",
        ];
        let mut forbidden = Vec::new();
        for entry in WalkDir::new(&root)
            .into_iter()
            .filter_entry(|entry| entry.file_name() != "target")
        {
            let entry = entry.expect("repo walk should succeed");
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            let is_python_file = path.extension().and_then(|ext| ext.to_str()) == Some("py");
            if is_python_file || forbidden_names.contains(&file_name) {
                forbidden.push(
                    path.strip_prefix(&root)
                        .unwrap_or(path)
                        .display()
                        .to_string(),
                );
            }
        }
        assert!(
            forbidden.is_empty(),
            "lm-resizer must remain Rust-only; remove Python runtime files: {forbidden:?}"
        );
    }

    #[test]
    fn release_versions_are_aligned_across_cargo_and_npm() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root_version = cargo_package_version(&root.join("Cargo.toml"));
        let core_version = cargo_package_version(&root.join("crates/lm-resizer-core/Cargo.toml"));
        let wasm_version = cargo_package_version(&root.join("crates/lm-resizer-wasm/Cargo.toml"));
        let npm: Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("packages/wasm/package.json")).unwrap(),
        )
        .unwrap();
        let npm_version = npm.get("version").and_then(Value::as_str).unwrap();

        assert_eq!(core_version, root_version);
        assert_eq!(wasm_version, root_version);
        assert_eq!(npm_version, root_version);
    }

    fn cargo_package_version(path: &Path) -> String {
        let value: toml::Value = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        value
            .get("package")
            .and_then(|package| package.get("version"))
            .and_then(toml::Value::as_str)
            .unwrap_or_else(|| panic!("missing package.version in {}", path.display()))
            .to_string()
    }
}
