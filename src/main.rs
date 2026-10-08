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

mod advice_cli;
mod agent_hooks;
mod capture_interrupt;
mod command_capture;
mod command_filters;
mod command_views;
mod container_views;
mod conversation_views;
mod diagnostic_views;
mod file_views;
mod inspection_views;
mod lossless_filters;
mod mcp_proxy;
mod outline_view;
mod package_views;
mod parity_filters;
mod patch_view;
mod provider_usage;
mod reversible_views;
mod shared_context;
#[cfg(test)]
mod shell_rewrite_tests;
mod smart_ast;
mod structured_views;
mod test_views;
mod token_metrics;

use token_metrics::{TokenCounts, TOKENIZER};

use lm_resizer_core::transforms::diagnostic_gate::FAILURE_SIGNAL;

// Native flags remain owned by the native program, not reinterpreted by clap.
const NATIVE_TOOLS: &[&str] = &[
    "git",
    "gh",
    "glab",
    "gt",
    "grep",
    "rg",
    "ast-grep",
    "sg",
    "find",
    "fd",
    "ls",
    "tree",
    "cat",
    "head",
    "tail",
    "nl",
    "diff",
    "wc",
    "cargo",
    "pytest",
    "ruff",
    "mypy",
    "pip",
    "uv",
    "sqlfluff",
    "npm",
    "npx",
    "pnpm",
    "yarn",
    "bun",
    "bunx",
    "deno",
    "jest",
    "vitest",
    "tsc",
    "eslint",
    "prettier",
    "playwright",
    "next",
    "prisma",
    "black",
    "go",
    "golangci-lint",
    "dotnet",
    "mvn",
    "gradle",
    "gradlew",
    "sbt",
    "ctest",
    "make",
    "php",
    "phpunit",
    "phpstan",
    "pest",
    "paratest",
    "ecs",
    "pint",
    "phpt",
    "rake",
    "rspec",
    "rubocop",
    "docker",
    "podman",
    "kubectl",
    "oc",
    "aws",
    "psql",
    "curl",
    "wget",
];

#[derive(Parser)]
#[command(name = "lm-resizer")]
#[command(about = "Rust-native context compression for LLM agents")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Summarize an arbitrary command; the complete output stays recoverable.
    Observe {
        /// Output policy: errors, tests, or summary.
        #[arg(long, value_enum, default_value = "summary")]
        mode: inspection_views::Mode,
        #[arg(long)]
        json: bool,
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Keep diagnostic paragraphs from any failed command.
    Err(InspectionArgs),
    /// Summarize test output from any command.
    Test(InspectionArgs),
    /// Summarize any command while retaining unknown diagnostics.
    Summary(InspectionArgs),
    /// Execute literally while retaining raw recovery and measured history.
    Proxy(InspectionArgs),
    /// Execute a literal shell command with recovery and measured history.
    Run {
        #[arg(short = 'c', long)]
        command: String,
    },
    /// Show a compact JSON schema (all input bytes remain recoverable).
    Json { input: Option<PathBuf> },
    /// Show callable signatures using a fresh local syntax index; bodies stay in tee.
    Outline { input: PathBuf },
    /// Read a file literally, optionally showing only its first lines.
    Read {
        input: PathBuf,
        #[arg(long)]
        head_lines: Option<usize>,
    },
    /// List direct dependencies from local manifests.
    Deps {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Show sorted environment variables with secrets redacted.
    Env { filter: Option<String> },
    /// Run a formatter through the native capture and view engine.
    Format {
        #[arg(value_parser=["prettier", "ruff", "black", "biome"])]
        tool: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Fold exact repeated tool blocks inside a supplied JSON message window.
    Dedup { input: Option<PathBuf> },
    /// Count locally recorded hook rewrites by client.
    HookAudit {},
    /// Show effective local storage and capture configuration.
    Config {},
    /// Filter stdin without executing any command; accepts native pipe filter names.
    Pipe {
        #[arg(short, long)]
        filter: String,
        #[arg(short, long)]
        input: Option<PathBuf>,
        #[arg(long)]
        json: bool,
        #[arg(long, default_value_t = 0)]
        exit_code: i32,
        #[arg(long)]
        store: Option<PathBuf>,
    },
    /// Expand a reversible view from stdin or a file, without accessing tee.
    Expand {
        #[arg(short, long)]
        input: Option<PathBuf>,
    },
    /// Execute a supported native tool directly, preserving every argument.
    #[command(external_subcommand)]
    Native(Vec<String>),
    /// Compress stdin or a file and persist originals for CCR retrieval.
    ///
    /// Diagnostic guard: `compress` reinjects the failure lines its compression
    /// dropped. They are appended in their original order under the marker
    /// `[lm-resizer: N lignes d'échec omises par la compression, réinjectées
    /// ci-dessous]`, and `steps_applied` records `diagnostic_reinjection:N`.
    /// If the result is not smaller than the input, the original is returned
    /// unchanged. `exec` does this on the default capture path only for the
    /// `native:native_owned` filter (a nested `lm-resizer`). `exec --stream`
    /// and `tool-output` instead keep the filtered body and record
    /// `diagnostic_gate:kept_filtered` when that generic pass would drop a
    /// failure line the filter had kept.
    Compress {
        /// Input file. Reads stdin when omitted.
        #[arg(short, long)]
        input: Option<PathBuf>,
        /// User query used by relevance-aware compressors.
        #[arg(short, long, default_value = "")]
        query: String,
        /// Token budget: force lossy row-dropping so the output fits ~N tokens,
        /// keeping the rows most relevant to --query. Applies to structured row
        /// data (a JSON array of objects); free text and logs are left as the
        /// lossless pipeline renders them, whatever the budget. Omit for
        /// lossless-first.
        #[arg(long)]
        token_budget: Option<usize>,
        /// Emit JSON metadata instead of raw compressed text.
        #[arg(long)]
        json: bool,
        /// CCR SQLite database path.
        #[arg(long)]
        store: Option<PathBuf>,
        /// Structural advice (RetentionAdvice JSON) about the input. Opt-in:
        /// only fresh advice (sha256 of these exact bytes) with callable kinds
        /// elides bodies; anything else falls back to ordinary compression.
        #[arg(long, conflicts_with = "advice_from_code_explorer")]
        advice: Option<PathBuf>,
        /// Require an attempt with indexed Code Explorer symbols (`cypher`).
        /// Indexed source files are also tried automatically when available.
        /// Binary: $LM_RESIZER_CODE_EXPLORER_BIN, else `code-explorer`.
        #[arg(long, requires = "input")]
        advice_from_code_explorer: bool,
    },
    /// Summarize a source file with indexed Code Explorer symbols when available.
    Smart {
        /// Source file to summarize.
        input: PathBuf,
        /// Résumer la structure syntaxique localement, avec signatures et lignes.
        #[arg(long)]
        ast: bool,
        /// User query used by relevance-aware compressors.
        #[arg(short, long, default_value = "")]
        query: String,
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
    /// Execute a command with the native command view and recoverable raw output.
    ///
    /// Diagnostic guard: two losses are watched separately. If a command filter
    /// itself drops a failure line, the raw output is returned and `filter`
    /// gains a `:diagnostic-guard` suffix. The default capture path also
    /// prefixes a non-native name with `native:`. The suffix is not added on
    /// every path: `--raw-on-failure` on a non-zero exit skips the filter and
    /// returns the raw output under the name `raw_on_failure` (no suffix, no
    /// step, no stderr message). On the default capture path, generic
    /// compression runs only for `native:native_owned` (a nested `lm-resizer`).
    /// That call reinjects omitted failure lines (`diagnostic_reinjection:N`)
    /// and keeps the compressed view when none are still missing; it does not
    /// return that filter's result as is. Every other filter on that path is
    /// returned as the filter left it. With `--stream`, if the generic pass
    /// would drop a kept failure line, the filtered body is kept,
    /// `compression_steps` gains `diagnostic_gate:kept_filtered`, and stderr
    /// reports `compression générique annulée, elle omettait « … »`. A
    /// `[tee:<id>]` line may follow when the saving pays for it. `tool-output`
    /// applies the same pipeline guard silently.
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
    /// Filter output already captured by a host or benchmark; never execute the command.
    ///
    /// Diagnostic guard: the command filter runs first; a generic pass then
    /// ranks its lines by frequency. Two distinct losses are guarded. If the
    /// filter itself dropped a failure line — for example a line containing
    /// `exit code 2`, `##[error]`, `error`, `panic` or a source `file:line` —
    /// the raw output is returned and `filter` gains a `:diagnostic-guard`
    /// suffix. `--raw-on-failure` on a non-zero exit skips that filter
    /// (`filter` is `raw_on_failure`): no suffix, no step, no message. If the
    /// generic pass would drop a failure line the filter had kept, the filtered
    /// body is kept (the filter's own reduction is kept) and
    /// `compression_steps` records `diagnostic_gate:kept_filtered`. This
    /// command stays silent; `exec --stream` reports the omitted line on
    /// stderr. When the view differs from the raw text, the original stays
    /// recoverable with `retrieve`. A `[tee:<id>]` trailer is added only when
    /// the saving pays for it, and that trailer is not part of the filtered body.
    ToolOutput {
        /// Command that produced the supplied text.
        #[arg(long)]
        command: String,
        /// File containing the captured text; stdin when omitted.
        #[arg(short, long)]
        input: Option<PathBuf>,
        /// Exit code from the original command.
        #[arg(long, default_value_t = 0)]
        exit_code: i32,
        /// Return a failed command's output verbatim.
        #[arg(long)]
        raw_on_failure: bool,
        /// Query used by the compression pipeline.
        #[arg(short, long, default_value = "")]
        query: String,
        /// Emit metadata and the output as JSON.
        #[arg(long)]
        json: bool,
        /// CCR SQLite database path.
        #[arg(long)]
        store: Option<PathBuf>,
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
    #[command(visible_alias = "recall")]
    Retrieve {
        #[arg(required_unless_present = "list")]
        hash: Option<String>,
        /// List recoverable command outputs.
        #[arg(long, conflicts_with = "hash")]
        list: bool,
        /// CCR SQLite database path.
        #[arg(long)]
        store: Option<PathBuf>,
    },
    /// Save a named, compressed handoff for other agents sharing this store.
    Share {
        /// Name of the handoff.
        key: String,
        /// Input file. Reads stdin when omitted.
        #[arg(short, long)]
        input: Option<PathBuf>,
        /// User query used by relevance-aware compressors.
        #[arg(short, long, default_value = "")]
        query: String,
        /// CCR SQLite database path (the handoffs live beside it).
        #[arg(long)]
        store: Option<PathBuf>,
        /// Emit JSON metadata.
        #[arg(long)]
        json: bool,
    },
    /// Read a named handoff, compressed by default or verbatim with --full.
    SharedGet {
        /// Name of the handoff.
        key: String,
        /// Print the original text instead of the compressed handoff.
        #[arg(long)]
        full: bool,
        /// CCR SQLite database path.
        #[arg(long)]
        store: Option<PathBuf>,
    },
    /// List names of handoffs in the shared store.
    SharedList {
        /// CCR SQLite database path.
        #[arg(long)]
        store: Option<PathBuf>,
    },
    /// Show CCR store statistics.
    #[command(visible_alias = "gain")]
    Stats {
        /// Emit machine-readable JSON (the default for stats).
        #[arg(long)]
        json: bool,
        /// Include recent executions with exact counts, duration and exit status.
        #[arg(short = 'H', long)]
        history: bool,
        /// Restrict execution statistics to the current working directory.
        #[arg(short, long)]
        project: bool,
        /// Number of recent executions shown by --history.
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// CCR SQLite database path.
        #[arg(long)]
        store: Option<PathBuf>,
        /// Emit a Markdown stats summary.
        #[arg(long, conflicts_with = "json")]
        markdown: bool,
    },
    /// Inspect image payload size and dimensions for context-budget decisions.
    Image {
        /// Image file to inspect.
        input: PathBuf,
        /// Write a smaller image in the input format (PNG or JPEG). The file is
        /// created only when the re-encode is strictly smaller; otherwise the
        /// command succeeds and writes nothing (no `saved` line). The inspection
        /// line is still printed: format, byte size, dimensions and a
        /// recommendation such as `small image: safe to keep inline when the
        /// model needs visual detail`. The output extension must match the
        /// input format (`input and output formats must match`; PNG to .jpg is
        /// refused). Never overwrites (`output already exists`).
        #[arg(long)]
        output: Option<PathBuf>,
        /// Optional maximum width or height for explicit downscaling (minimum 64).
        #[arg(long)]
        max_dimension: Option<u32>,
        /// JPEG quality when writing JPEG (1-100; default 90).
        #[arg(long)]
        quality: Option<u8>,
        /// Add a deterministic visual summary (brightness and color only).
        #[arg(long)]
        describe: bool,
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
    /// Install native agent hooks that call `lm-resizer hook`.
    #[command(visible_alias = "init")]
    InitNativeHooks {
        /// Agent: codex, claude, gemini, copilot, cursor, or all (codex + claude).
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
        /// Agent: codex, claude, or all (codex + claude). Gemini, Copilot and Cursor
        /// use init-native-hooks.
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
    /// Remove generated lm-resizer hook instructions, helpers, and matching native configs.
    UninstallHooks {
        /// Agent to unconfigure: codex, claude, gemini, copilot, cursor, or all.
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
    /// Relay an MCP stdio server and compress successful tool text results.
    McpProxy {
        /// CCR SQLite database path shared with `retrieve`.
        #[arg(long)]
        store: Option<PathBuf>,
        /// Upstream MCP server command and arguments.
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    #[command(hide = true)]
    MockMcpServer {
        #[arg(long, default_value = "default")]
        scenario: String,
    },
    /// Install lm-resizer as an MCP server for common agent clients.
    Install {
        /// Client to configure: claude, codex, cursor, vscode, all.
        #[arg(long, default_value = "claude")]
        client: String,
        /// Installation scope: project or global. Codex is user-scoped:
        /// `--client codex` requires `--scope global`, and `--client all` writes
        /// Codex to `~/.codex/config.toml` even with `project`.
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
        /// Optional bearer token for the upstream provider. Its value is never
        /// shown by `--help`, even when taken from the environment. Prefer
        /// `LM_RESIZER_API_KEY` or `--api-key-file`: a value given on the command
        /// line is readable by every local account through `ps`.
        #[arg(long, env = "LM_RESIZER_API_KEY", hide_env_values = true)]
        api_key: Option<String>,
        /// File holding the upstream token (first line). On Unix it must not be
        /// readable by group or others (mode 0600). Takes precedence over `--api-key`.
        #[arg(long, env = "LM_RESIZER_API_KEY_FILE")]
        api_key_file: Option<PathBuf>,
        /// Accept a non-loopback `--bind` address. The proxy has no client authentication:
        /// anyone who can reach it can spend the upstream key.
        #[arg(long)]
        allow_non_loopback: bool,
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
        /// Optional bearer token for the upstream provider. Its value is never
        /// shown by `--help`, even when taken from the environment. Prefer
        /// `LM_RESIZER_API_KEY` or `--api-key-file`: a value given on the command
        /// line is readable by every local account through `ps`.
        #[arg(long, env = "LM_RESIZER_API_KEY", hide_env_values = true)]
        api_key: Option<String>,
        /// File holding the upstream token (first line). On Unix it must not be
        /// readable by group or others (mode 0600). Takes precedence over `--api-key`.
        #[arg(long, env = "LM_RESIZER_API_KEY_FILE")]
        api_key_file: Option<PathBuf>,
        /// Accept a non-loopback `--bind` address. The proxy has no client authentication:
        /// anyone who can reach it can spend the upstream key.
        #[arg(long)]
        allow_non_loopback: bool,
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

#[derive(clap::Args)]
struct InspectionArgs {
    #[arg(long)]
    json: bool,
    #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
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
    #[serde(flatten)]
    tokens: TokenCounts,
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
struct CapturedStreams {
    stdout_bytes: usize,
    stderr_bytes: usize,
    /// Streams are captured independently; cross-stream chronology is unknown.
    layout: &'static str,
}

impl CapturedStreams {
    fn new(stdout: &[u8], stderr: &[u8]) -> Self {
        Self {
            stdout_bytes: stdout.len(),
            stderr_bytes: stderr.len(),
            layout: "stdout_then_stderr; [stderr] boundary; no cross-stream chronology",
        }
    }
}

#[derive(Debug, Serialize)]
struct ExecReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    streams: Option<CapturedStreams>,
    #[serde(flatten)]
    tokens: TokenCounts,
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
    cwd: String,
    #[serde(flatten)]
    tokens: TokenCounts,
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
    #[serde(flatten)]
    tokens: TokenCounts,
    files_scanned: usize,
    command_outputs: usize,
    rewritable_commands: usize,
    original_bytes: usize,
    filtered_bytes: usize,
    estimated_bytes_saved: usize,
    /// Legacy JSON alias for the exact prospective `tokens.tokens_saved`.
    estimated_tokens_saved: i64,
    candidates: Vec<DiscoverCandidate>,
}

#[derive(Debug, Serialize)]
struct DiscoverCandidate {
    #[serde(flatten)]
    tokens: TokenCounts,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_bytes: Option<u64>,
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
    #[serde(flatten)]
    tokens: TokenCounts,
    files_scanned: usize,
    command_outputs: usize,
    candidates: usize,
    estimated_bytes_saved: usize,
    /// Legacy JSON alias for the exact prospective `tokens.tokens_saved`.
    estimated_tokens_saved: i64,
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
    helpers_removed: Vec<String>,
    native_files_removed: Vec<String>,
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
    /// Keep the lines that follow a matching line, even when they match no
    /// `keep_lines_matching` pattern. A test failure is a header followed by
    /// the lines that explain it (`Expected:`, `Actual:`, a stack frame with a
    /// file and line); a line-by-line keyword filter keeps the header and
    /// throws the explanation away.
    #[serde(default)]
    keep_block_after_matching: Vec<TomlBlockRule>,
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
struct TomlBlockRule {
    /// A line that opens a block; it is kept itself.
    start: String,
    /// A line that closes the block; it is not part of the block and goes
    /// through the ordinary rules (it may open the next block).
    #[serde(default)]
    until: Option<String>,
    /// Hard limit on the lines kept after `start`.
    #[serde(default = "default_block_lines")]
    max_lines: usize,
}

fn default_block_lines() -> usize {
    40
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
    keep_blocks: Vec<(Regex, Option<Regex>, usize)>,
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
    /// Adresse d'écoute à laquelle `Host` doit correspondre (anti rebinding DNS).
    host_guard: Option<SocketAddr>,
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

fn main() {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if !is_broken_pipe_panic(info.payload()) {
            previous_hook(info);
        }
    }));
    match std::panic::catch_unwind(run_on_cli_thread) {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::BrokenPipe)
            {
                std::process::exit(broken_pipe_exit_code());
            }
            // Keep contextual errors readable even when RUST_BACKTRACE is enabled.
            eprintln!("Error: {error:#}");
            std::process::exit(1);
        }
        Err(panic) if is_broken_pipe_panic(panic.as_ref()) => {
            std::process::exit(broken_pipe_exit_code());
        }
        Err(_) => std::process::exit(1),
    }
}

fn is_broken_pipe_panic(payload: &(dyn std::any::Any + Send)) -> bool {
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied());
    message.is_some_and(|text| {
        text.contains("Broken pipe") || text.contains("broken pipe") || text.contains("os error 32")
    })
}

fn broken_pipe_exit_code() -> i32 {
    if cfg!(unix) {
        141
    } else {
        1
    }
}

fn run_on_cli_thread() -> Result<()> {
    // Clap's generated command builder has a large debug stack frame. Windows
    // gives the main thread only 1 MiB, so dispatch on a thread with an explicit
    // stack rather than depending on a platform-specific linker flag.
    std::thread::Builder::new()
        .name("lm-resizer-cli".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let cli = Cli::parse();
            // Short command adapters do synchronous capture/filtering. Creating
            // a worker per CPU here costs more than the work itself. Services
            // retain their multithread runtime and scheduling behaviour.
            let mut runtime = if matches!(
                &cli.command,
                Commands::Exec { .. }
                    | Commands::Pipe { .. }
                    | Commands::Native(_)
                    | Commands::Observe { .. }
                    | Commands::Err(_)
                    | Commands::Test(_)
                    | Commands::Summary(_)
                    | Commands::Format { .. }
            ) {
                tokio::runtime::Builder::new_current_thread()
            } else {
                tokio::runtime::Builder::new_multi_thread()
            };
            runtime.enable_all().build()?.block_on(run(cli))
        })?
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

async fn run(cli: Cli) -> Result<()> {
    let command = match cli.command {
        Commands::Proxy(a) => Commands::Observe {
            mode: inspection_views::Mode::Raw,
            json: a.json,
            command: a.command,
        },
        Commands::Run { command } => Commands::Observe {
            mode: inspection_views::Mode::Raw,
            json: false,
            command: vec![
                if cfg!(windows) { "cmd" } else { "sh" }.into(),
                if cfg!(windows) { "/C" } else { "-c" }.into(),
                command,
            ],
        },
        Commands::Err(a) => Commands::Observe {
            mode: inspection_views::Mode::Errors,
            json: a.json,
            command: a.command,
        },
        Commands::Test(a) => Commands::Observe {
            mode: inspection_views::Mode::Tests,
            json: a.json,
            command: a.command,
        },
        Commands::Summary(a) => Commands::Observe {
            mode: inspection_views::Mode::Summary,
            json: a.json,
            command: a.command,
        },
        other => other,
    };
    match command {
        Commands::Err(_)
        | Commands::Test(_)
        | Commands::Summary(_)
        | Commands::Proxy(_)
        | Commands::Run { .. } => unreachable!(),
        Commands::Observe {
            mode,
            json,
            command,
        } => {
            let report = run_inspected_command(&command, Some(mode), None, "")?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", report.output);
            }
            if report.exit_code != 0 {
                std::process::exit(report.exit_code);
            }
        }
        Commands::Json { input } => {
            let raw = read_input(input.as_deref()).await?;
            let view = inspection_views::json_schema(&raw)?;
            let hint = archive_raw_bytes(raw.as_bytes())?;
            let mut view = view;
            if let Some(hint) = hint {
                append_recovery_instruction(&mut view, &hint, &raw);
            }
            print!("{view}");
        }
        Commands::Outline { input } => {
            let raw = std::fs::read_to_string(&input)?;
            let store = open_store(None)?;
            let mut view = outline_view::outline(&input, &raw, store.as_ref());
            if let Some(hint) = archive_raw_bytes(raw.as_bytes())? {
                append_recovery_instruction(&mut view, &hint, &raw);
            }
            print!("{view}");
        }
        Commands::Read { input, head_lines } => {
            let raw = std::fs::read(&input)?;
            let text = display_captured_bytes(&raw);
            let mut view = if let Some(n) = head_lines {
                text.split_inclusive('\n').take(n).collect::<String>()
            } else {
                text.clone()
            };
            if let Some(hint) = archive_raw_bytes(&raw)? {
                append_recovery_instruction(&mut view, &hint, &text);
            }
            print!("{view}");
        }
        Commands::Deps { path } => print!("{}", inspection_views::dependencies(&path)?),
        Commands::Env { filter } => print!(
            "{}",
            inspection_views::environment(std::env::vars(), filter.as_deref())
        ),
        Commands::Dedup { input } => {
            let raw = read_input(input.as_deref()).await?;
            let output = conversation_views::fold(&raw)?;
            archive_raw_bytes(raw.as_bytes())?;
            println!("{output}");
        }
        Commands::HookAudit {} => {
            let path = default_state_dir()?.join("hook-audit.jsonl");
            let raw = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
                Err(e) => return Err(e.into()),
            };
            let mut clients = std::collections::BTreeMap::<String, usize>::new();
            for line in raw.lines() {
                if let Ok(value) = serde_json::from_str::<Value>(line) {
                    if let Some(client) = value["client"].as_str() {
                        *clients.entry(client.into()).or_default() += 1;
                    }
                }
            }
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &json!({"rewrites":clients.values().sum::<usize>(),"clients":clients})
                )?
            );
        }
        Commands::Config {} => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "state_directory": default_state_dir()?, "tee": std::env::var("LM_RESIZER_TEE").unwrap_or_else(|_|"1".into()),
                    "capture": "native ordered; separate for stream and raw-on-failure", "tokenizer": "o200k_base"
                }))?
            );
        }
        Commands::Format { tool, args } => {
            let mut command = vec![tool.clone()];
            if tool == "ruff" || tool == "biome" {
                command.push("format".into());
            }
            command.extend(args);
            let report = run_native_command(&command)?;
            print!("{}", report.output);
            if report.exit_code != 0 {
                std::process::exit(report.exit_code);
            }
        }

        Commands::Compress {
            input,
            query,
            token_budget,
            json,
            store,
            advice,
            advice_from_code_explorer,
        } => {
            let input_text = read_input(input.as_deref()).await?;
            let store = open_store(store)?;
            let (report, advice_report) = compress_with_optional_advice(
                &input_text,
                input.as_deref(),
                &query,
                store.as_ref(),
                token_budget,
                advice.as_deref(),
                advice_from_code_explorer,
            )?;
            if let Some(a) = advice_report.as_ref().filter(|a| {
                a.status != "applied" && (advice_from_code_explorer || advice.is_some())
            }) {
                // stderr: stdout stays the compressed text, byte for byte.
                eprintln!(
                    "lm-resizer: conseil structurel non appliqué ({}{}), compression ordinaire",
                    a.status,
                    a.detail
                        .as_deref()
                        .map(|d| format!(" : {d}"))
                        .unwrap_or_default()
                );
            }
            if json {
                let mut value = serde_json::to_value(&report)?;
                if let Some(a) = advice_report {
                    value["advice"] = serde_json::to_value(a)?;
                }
                println!("{}", serde_json::to_string_pretty(&value)?);
            } else {
                print!("{}", report.output);
            }
        }
        Commands::Smart {
            input,
            ast,
            query,
            json,
            store,
        } => {
            let source = read_input(Some(input.as_path())).await?;
            let fallback = if ast {
                match smart_ast::summarize(&input, &source) {
                    Ok(output) => {
                        if json {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&json!({
                                    "content_type": "source_code",
                                    "original_bytes": source.len(),
                                    "compressed_bytes": output.len(),
                                    "bytes_saved": source.len().saturating_sub(output.len()),
                                    "original_tokens": source.len() as f64 / 4.0,
                                    "compressed_tokens": output.len() as f64 / 4.0,
                                    "token_count_method": "approximate",
                                    "tokenizer": "bytes/4",
                                    "steps_applied": ["smart_ast"],
                                    "cache_keys": [],
                                    "output": output
                                }))?
                            );
                        } else {
                            print!("{output}");
                        }
                        return Ok(());
                    }
                    Err(reason) => Some(reason.marker()),
                }
            } else {
                None
            };
            let store = open_store(store)?;
            let (mut report, advice) = compress_with_optional_advice(
                &source,
                Some(input.as_path()),
                &query,
                store.as_ref(),
                None,
                None,
                true,
            )?;
            if let Some(marker) = fallback {
                report.output.insert_str(0, marker);
                report.compressed_bytes = report.output.len();
                report.bytes_saved = source.len().saturating_sub(report.output.len());
                report.tokens = TokenCounts::measure(&source, &report.output);
                report.steps_applied.insert(0, "smart_ast:fallback".into());
            }
            if json {
                let mut value = serde_json::to_value(&report)?;
                if let Some(advice) = advice {
                    value["advice"] = serde_json::to_value(advice)?;
                }
                println!("{}", serde_json::to_string_pretty(&value)?);
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
        Commands::Pipe {
            filter,
            input,
            json,
            exit_code,
            store,
        } => {
            let command = pipe_filter_command(&filter)
                .context("unknown pipe filter; use a supported native pipe filter name")?;
            let raw = read_input(input.as_deref()).await?;
            let store = open_store(store)?;
            let started = Instant::now();
            let mut report =
                process_captured_output(&command, &raw, exit_code, false, "", store.as_ref())?;
            attach_report_recovery(&raw, &mut report)?;
            report.command = format!("pipe --filter {filter}");
            record_exec_history(&report, started.elapsed())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", report.output);
            }
            if exit_code != 0 {
                std::process::exit(exit_code);
            }
        }
        Commands::Expand { input } => {
            let view = read_input(input.as_deref()).await?;
            print!("{}", lossless_filters::expand(&view)?);
        }
        Commands::Native(command) => {
            let program = command.first().context("missing native command")?;
            if !NATIVE_TOOLS.contains(&program.as_str()) {
                anyhow::bail!("unknown command '{program}'; use --help or exec -- <program>");
            }
            let store = open_store_or_warn(None);
            let report = run_exec_command(&command, "", false, false, store.as_deref())?;
            print!("{}", report.output);
            if report.exit_code != 0 {
                std::process::exit(report.exit_code);
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
            let store = open_store_or_warn(store);
            let report =
                run_exec_command(&command, &query, raw_on_failure, stream, store.as_deref())?;
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
        Commands::ToolOutput {
            command,
            input,
            exit_code,
            raw_on_failure,
            query,
            json,
            store,
        } => {
            let started = Instant::now();
            let raw = read_input(input.as_deref()).await?;
            let parts = split_shell_words(&command).context("invalid --command quoting")?;
            let store = open_store(store)?;
            let mut report = process_captured_output(
                &parts,
                &raw,
                exit_code,
                raw_on_failure,
                &query,
                store.as_ref(),
            )?;
            attach_report_recovery(&raw, &mut report)?;
            record_exec_history(&report, started.elapsed())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", report.output);
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
        Commands::Retrieve { hash, store, list } => {
            if list {
                run_tee_command(TeeCommand::List { json: false })?;
                return Ok(());
            }
            let hash = hash.context("missing recall key")?;
            let store = open_store(store)?;
            let (hash, payload) = get_ccr_entry(store.as_ref(), &hash)?;
            let _ = record_retrieval_feedback(&hash, payload.len(), "cli");
            print!("{payload}");
        }
        Commands::Share {
            key,
            input,
            query,
            store,
            json,
        } => {
            let raw = read_input(input.as_deref()).await?;
            let path = store.unwrap_or(default_store_path()?);
            let ccr = open_store(Some(path.clone()))?;
            let compressed = compress_text_with_pipeline_gate(
                &raw,
                &query,
                ccr.as_ref(),
                &build_pipeline(),
                None,
                true,
            )?;
            let saved = shared_context::put(&path, &key, &raw, &compressed.output)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&saved)?);
            } else {
                println!("{}", saved.key);
            }
        }
        Commands::SharedGet { key, full, store } => {
            let path = store.unwrap_or(default_store_path()?);
            let content = shared_context::get(&path, &key, full)?
                .with_context(|| format!("shared context not found: {key}"))?;
            print!("{content}");
        }
        Commands::SharedList { store } => {
            let path = store.unwrap_or(default_store_path()?);
            for key in shared_context::list(&path)? {
                println!("{key}");
            }
        }
        Commands::Stats {
            json,
            store,
            markdown,
            history,
            project,
            limit,
        } => {
            let store = open_store(store)?;
            let mut exec_history = summarize_exec_history().unwrap_or_default();
            let mut recent = None;
            if history || project {
                let path = default_state_dir()?.join("exec-history.jsonl");
                let content = if path.exists() {
                    std::fs::read_to_string(path)?
                } else {
                    String::new()
                };
                let cwd = std::env::current_dir()?.to_string_lossy().into_owned();
                let (summary, rows, unscoped) =
                    token_metrics::select_history(&content, project.then_some(cwd.as_str()), limit);
                exec_history = summary;
                exec_history["unscoped_records"] = json!(unscoped);
                if project {
                    exec_history["project"] = json!(cwd);
                }
                if history {
                    recent = Some(rows);
                }
            }
            let retrieval_feedback = summarize_retrieval_feedback().unwrap_or_default();
            let proxy_history = summarize_proxy_history().unwrap_or_default();
            let mut report = json!({
                "entries": store.len(),
                "empty": store.is_empty(),
                "exec_history": exec_history,
                "retrieval_feedback": retrieval_feedback,
                "proxy_history": proxy_history,
            });
            if let Some(rows) = recent {
                report["history"] = json!(rows);
            }
            if markdown {
                print!("{}", format_stats_markdown(&report));
                if let Some(rows) = report["history"].as_array() {
                    println!("\n## Recent executions\n\n| Command | Exit | Tokens saved | Duration ms |\n|---|---:|---:|---:|");
                    for row in rows {
                        println!(
                            "| {} | {} | {} | {} |",
                            markdown_escape(row["command"].as_str().unwrap_or("")),
                            row["exit_code"],
                            row["tokens_saved"],
                            row["duration_ms"]
                        );
                    }
                }
            } else if !json && std::env::args().nth(1).as_deref() == Some("gain") {
                print!("{}", format_gain(&report));
            } else {
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
        }
        Commands::Image {
            input,
            output,
            max_dimension,
            quality,
            describe,
            json,
        } => {
            if max_dimension.is_some_and(|limit| limit < 64) {
                anyhow::bail!("max-dimension must be at least 64");
            }
            let mut report = inspect_image(&input)?;
            if describe {
                report.description = Some(describe_image(&input)?);
            }
            if let Some(path) = output.as_deref() {
                let bytes = encode_smaller_image(&input, path, max_dimension, quality)?;
                report.output_bytes = bytes;
                report.output = bytes.map(|_| path.display().to_string());
            }
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
                if let Some(description) = &report.description {
                    println!("{description}");
                }
                if let (Some(output), Some(bytes)) = (&report.output, report.output_bytes) {
                    println!("saved {output}: {bytes} bytes");
                }
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
                    "Scanned {} files, found {} command outputs, prospective {} bytes / {} tokens saved (tiktoken-rs/o200k_base; exact text count)",
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
                    "Found {} command outputs, prospective {} bytes / {} tokens saved (tiktoken-rs/o200k_base; exact text count)",
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
                    "Eval {}: {} files, {} command outputs, {} candidates, {} prospective tokens saved (tiktoken-rs/o200k_base; exact text count)",
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
            // PreToolUse: rewrite a supported Bash command to run through `lm-resizer exec --`
            // (in-place output substitution, the native role). PostToolUse: measure-only telemetry.
            if event.eq_ignore_ascii_case("PreToolUse") || event == "BeforeTool" {
                emit_pretooluse_rewrite(&event, &client);
            } else {
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
                for file in report.helpers_removed {
                    println!("  removed helper {file}");
                }
                for file in report.native_files_removed {
                    println!("  removed native config {file}");
                }
            }
        }
        Commands::Doctor { json, store } => run_doctor(json, store)?,
        Commands::Mcp { store } => run_mcp(store)?,
        Commands::McpProxy { store, command } => mcp_proxy::run_mcp_proxy(command, store)?,
        Commands::MockMcpServer { scenario } => mcp_proxy::run_mock_mcp_server(&scenario)?,
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
            api_key_file,
            allow_non_loopback,
            provider,
            store,
            dashboard,
        } => {
            warn_if_api_key_on_command_line();
            ensure_loopback_bind(bind, allow_non_loopback)?;
            let api_key = resolve_api_key(api_key, api_key_file)?;
            run_http(
                bind,
                upstream,
                api_key,
                provider.parse()?,
                store,
                dashboard,
                allow_non_loopback,
            )
            .await?
        }
        Commands::Wrap {
            agent,
            args,
            bind,
            upstream,
            api_key,
            api_key_file,
            allow_non_loopback,
            provider,
            store,
            timeout_sec,
        } => {
            warn_if_api_key_on_command_line();
            ensure_loopback_bind(bind, allow_non_loopback)?;
            let api_key = resolve_api_key(api_key, api_key_file)?;
            wrap_agent(
                agent,
                args,
                bind,
                upstream,
                api_key,
                provider.parse()?,
                store,
                timeout_sec,
                allow_non_loopback,
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

/// Crée `dir` et ses parents manquants. Sous Unix, chaque dossier créé est en 0700 : l'état
/// contient les lignes de commande et les sorties brutes, lisibles sinon selon le umask.
fn create_private_dir_all(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// Ouvre `path` en ajout ; un fichier créé ici l'est en 0600 sous Unix.
fn open_private_append(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// Écrit `bytes` dans `path` ; un fichier créé ici l'est en 0600 sous Unix.
fn write_private_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(bytes)
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
        create_private_dir_all(parent)?;
    }
    // Créée vide en 0600 avant SQLite : ses fichiers `-wal` et `-shm` héritent de ce mode.
    if !path.exists() {
        open_private_append(&path)?;
    }
    let cfg = CcrBackendConfig::sqlite_default(path);
    Ok(from_config(&cfg)?)
}

/// Un outil de réduction ne doit jamais empêcher la commande qu'il enveloppe :
/// quand l'état n'est pas inscriptible (HOME en lecture seule, bac à sable,
/// disque plein), `exec` continue sans archive. Une seule ligne sur stderr
/// par processus, jamais de changement du code de sortie de la commande.
fn warn_state_unwritable(path: &Path) {
    static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !WARNED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        eprintln!(
            "lm-resizer: état non inscriptible ({}), réduction sans archive",
            path.display()
        );
    }
}

/// Le dossier d'état dont dépend une archive, pour le message d'avertissement.
fn state_path_for_warning(preferred: Option<&Path>) -> PathBuf {
    preferred
        .map(Path::to_path_buf)
        .or_else(|| default_state_dir().ok())
        .unwrap_or_else(|| PathBuf::from("lm-resizer"))
}

/// `open_store` sans échec : `None` (et un avertissement) si la base CCR ne
/// peut pas être ouverte ; l'appelant réduit alors sans archive.
fn open_store_or_warn(path: Option<PathBuf>) -> Option<Box<dyn CcrStore>> {
    let shown = path
        .clone()
        .or_else(|| default_store_path().ok())
        .unwrap_or_else(|| PathBuf::from("lm-resizer"));
    match open_store(path) {
        Ok(store) => Some(store),
        Err(_) => {
            warn_state_unwritable(&shown);
            None
        }
    }
}

/// Cle CCR : hexadecimal, rien d'autre.
fn is_ccr_hash(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_hexdigit())
}

/// Reduit a sa forme nue une cle CCR copiee telle qu'affichee. Seules ces
/// formes entieres sont reconnues : `<h>`, `ccr:<h>`, `<<ccr:<h>>>`,
/// `[full output: <<ccr:<h>>>]`, `hash=<h>` et `hash=<h>]` (fin du marqueur
/// `Retrieve more: hash=<h>]`), ou une vue collee ne portant qu'une cle
/// distincte. Plusieurs cles sont une erreur : jamais de choix silencieux.
fn bare_ccr_key(shown: &str) -> Result<String> {
    let text = shown.trim();
    let inner = text
        .strip_prefix("[full output: <<ccr:")
        .and_then(|rest| rest.strip_suffix(">>]"))
        .or_else(|| {
            text.strip_prefix("<<ccr:")
                .and_then(|rest| rest.strip_suffix(">>"))
        })
        .or_else(|| text.strip_prefix("ccr:"))
        .or_else(|| {
            text.strip_prefix("hash=")
                .map(|rest| rest.strip_suffix(']').unwrap_or(rest))
        })
        .unwrap_or(text);
    if is_ccr_hash(inner) {
        return Ok(inner.to_string());
    }
    // Vue collee (plusieurs mots ou lignes) : les reperes `<<ccr:<12 hex>,..>>`
    // et `<<ccr:<12 hex> N_rows_offloaded>>` ne sont pas des cles du store
    // (l'etat en memoire de la compression JSON n'est pas la base relue ici) ;
    // la cle est le `hash=` affiche avec la vue. Une seule cle distincte
    // restante se resout, plusieurs sont une erreur explicite.
    if text.contains(char::is_whitespace) {
        let marker = regex::Regex::new(r"<<ccr:[0-9A-Fa-f]{12}[ ,][^>\n]*>>")?;
        let stripped = marker.replace_all(text, "");
        let key = regex::Regex::new(r"(?:^|[^A-Za-z0-9_])(?:ccr:|hash=)([0-9A-Fa-f]+)")?;
        let mut keys: Vec<&str> = key
            .captures_iter(&stripped)
            .filter_map(|c| c.get(1))
            .filter(|m| {
                !stripped[m.end()..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
            })
            .map(|m| m.as_str())
            .collect();
        keys.sort_unstable();
        keys.dedup();
        match keys.as_slice() {
            [one] => return Ok((*one).to_string()),
            [] => {}
            many => anyhow::bail!(
                "ambiguous CCR reference: the input holds {} different keys; pass exactly one hash",
                many.len()
            ),
        }
    }
    if text.starts_with("<<ccr:") && inner.contains(',') {
        anyhow::bail!(
            "'{text}' is a marker of the compressed view, not a retrieval key; use the `hash=<key>` shown with the view"
        );
    }
    let markers = text.matches("ccr:").count() + text.matches("hash=").count();
    if markers > 1 {
        anyhow::bail!(
            "ambiguous CCR reference: the input holds {markers} keys; pass exactly one hash"
        );
    }
    anyhow::bail!("CCR entry not found: {shown} (not a recognised CCR key form)")
}

/// Cherche d'abord la cle telle que saisie, puis sa forme nue stricte.
fn get_ccr_entry(store: &dyn CcrStore, shown: &str) -> Result<(String, String)> {
    if let Some(payload) = store.get(shown) {
        return Ok((shown.to_string(), payload));
    }
    let bare = bare_ccr_key(shown)?;
    match store.get(&bare) {
        Some(payload) => Ok((bare, payload)),
        None => anyhow::bail!("CCR entry not found: {shown}"),
    }
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
    compress_text_with_pipeline_gate(content, query, store, pipeline, token_budget, true)
}

/// `reinject = false` is for `exec`, which holds a stronger gate of its own:
/// when the generic step loses a failure line that the command filter kept, it
/// returns the whole filtered text — including the lines that explain the
/// failure (`Expected:`, `Received:`) and that re-injection would not bring
/// back. Measured on a real Playwright run: with re-injection underneath, the
/// `exec` gate no longer fired and 4 of 11 facts were lost.
fn compress_text_with_pipeline_gate(
    content: &str,
    query: &str,
    store: &dyn CcrStore,
    pipeline: &CompressionPipeline,
    token_budget: Option<usize>,
    reinject: bool,
) -> Result<CompressReport> {
    compress_text_with_metrics(
        content,
        query,
        store,
        pipeline,
        token_budget,
        reinject,
        true,
    )
}

// `exec` only needs the final raw/output count; intermediate counts are unused.
fn compress_text_with_metrics(
    content: &str,
    query: &str,
    store: &dyn CcrStore,
    pipeline: &CompressionPipeline,
    token_budget: Option<usize>,
    reinject: bool,
    measure_tokens: bool,
) -> Result<CompressReport> {
    let tokens = |output: &str| {
        if measure_tokens {
            TokenCounts::measure(content, output)
        } else {
            TokenCounts::default()
        }
    };
    if let Some(output) = compact_json_rows(content) {
        let key = lm_resizer_core::ccr::compute_key(content.as_bytes());
        store.put(&key, content);
        return Ok(CompressReport {
            tokens: tokens(&output),
            content_type: "json".to_string(),
            original_bytes: content.len(),
            compressed_bytes: output.len(),
            bytes_saved: content.len() - output.len(),
            steps_applied: vec!["json_table".to_string()],
            cache_keys: vec![key],
            output,
        });
    }
    let detection = detect_content_type(content);
    let ctx = CompressionContext {
        query: query.to_string(),
        token_budget,
    };
    let result = pipeline.run(content, detection.content_type, &ctx, store);

    // Porte de conservation des diagnostics, sur le chemin commun (`compress`,
    // outil MCP, hooks, `exec`). Mesuré sur deux vrais journaux GitHub
    // Actions : la compression générique gardait 0/2 puis 2/10 lignes
    // `##[error]`. Plutôt que de renoncer à toute la réduction, les lignes
    // d'échec omises sont réinjectées, courtes et dans l'ordre, sous un
    // marqueur ; l'original reste dans le CCR.
    let mut steps_applied = result.steps_applied;
    let (mut output, reinjected) = if reinject {
        lm_resizer_core::transforms::diagnostic_gate::reinject_lost_failure_lines(
            content,
            &result.output,
        )
    } else {
        (result.output, 0)
    };
    if reinjected > 0 {
        steps_applied.push(format!("diagnostic_reinjection:{reinjected}"));
        if output.len() >= content.len() {
            return Ok(CompressReport {
                tokens: tokens(content),
                content_type: detection.content_type.as_str().to_string(),
                original_bytes: content.len(),
                compressed_bytes: content.len(),
                bytes_saved: 0,
                steps_applied: Vec::new(),
                cache_keys: Vec::new(),
                output: content.to_string(),
            });
        }
    }

    let bytes_saved = if reinjected > 0 {
        content.len().saturating_sub(output.len())
    } else {
        result.bytes_saved
    };
    // Les clés des offloads désignent parfois une étape déjà minifiée.
    // La première clé du CLI doit toujours retrouver l'entrée exacte, même
    // quand seul un reformat (notamment source_compressor) a été appliqué.
    let mut cache_keys = result.cache_keys;
    if output != content {
        let original_key = lm_resizer_core::ccr::compute_key(content.as_bytes());
        store.put(&original_key, content);
        for key in &cache_keys {
            output = output.replace(&format!("hash={key}]"), &format!("hash={original_key}]"));
        }
        cache_keys = vec![original_key];
    }
    Ok(CompressReport {
        tokens: tokens(&output),
        content_type: detection.content_type.as_str().to_string(),
        original_bytes: content.len(),
        compressed_bytes: output.len(),
        bytes_saved,
        steps_applied,
        cache_keys,
        output,
    })
}

/// A homogeneous JSON object array can share its keys once. `columns` and
/// positional `rows` retain every value, including nested values and nulls.
fn compact_json_rows(content: &str) -> Option<String> {
    let mut document: Value = serde_json::from_str(content).ok()?;
    let object = document.as_object_mut()?;
    if object.contains_key("columns") {
        return None;
    }
    let rows = object.get("rows")?.as_array()?;
    let first = rows.first()?.as_object()?;
    let columns: Vec<String> = first.keys().cloned().collect();
    if columns.is_empty()
        || rows.iter().any(|row| match row.as_object() {
            Some(item) => {
                item.len() != columns.len() || columns.iter().any(|key| !item.contains_key(key))
            }
            None => true,
        })
    {
        return None;
    }
    let table: Vec<Value> = rows
        .iter()
        .map(|row| Value::Array(columns.iter().map(|key| row[key].clone()).collect()))
        .collect();
    object.insert("columns".to_string(), serde_json::to_value(columns).ok()?);
    object.insert("rows".to_string(), Value::Array(table));
    let output = serde_json::to_string(&document).ok()?;
    (output.len() < content.len()).then_some(output)
}

/// `compress`, with structural advice when the caller asked for it.
///
/// Advice that applies replaces the pipeline for this input: it is the mode
/// the caller chose explicitly. Advice that does not apply is reported and the
/// pipeline runs exactly as without it.
fn compress_with_optional_advice(
    input_text: &str,
    input_path: Option<&Path>,
    query: &str,
    store: &dyn CcrStore,
    token_budget: Option<usize>,
    advice_path: Option<&Path>,
    advice_from_code_explorer: bool,
) -> Result<(CompressReport, Option<advice_cli::AdviceReport>)> {
    let auto_code_explorer = input_path.is_some_and(advice_cli::supports_structural_path);
    let loaded = match (
        advice_path,
        advice_from_code_explorer || auto_code_explorer,
        input_path,
    ) {
        (Some(path), _, _) => Some(advice_cli::load_advice_file(path).map(|a| (a, None))),
        (None, true, Some(input)) => {
            Some(advice_cli::advice_from_code_explorer(input).map(|(a, ms)| (a, Some(ms))))
        }
        _ => None,
    };
    let pipeline_report = |advice_report: Option<advice_cli::AdviceReport>| {
        let pipeline = build_pipeline();
        compress_text_with_pipeline(input_text, query, store, &pipeline, token_budget)
            .map(|r| (r, advice_report))
    };
    let Some(loaded) = loaded else {
        return pipeline_report(None);
    };
    let (advice, producer_ms) = match loaded {
        Ok(ok) => ok,
        Err(failure) => return pipeline_report(Some(failure)),
    };
    let source = match advice_path {
        Some(p) => format!("file:{}", p.display()),
        None => format!("code-explorer:{}", advice_cli::code_explorer_bin()),
    };
    let elision = lm_resizer_core::transforms::advice_structural::elide_bodies_with_advice(
        input_text,
        &advice,
        query,
        Some(store),
    );
    let advice_report = advice_cli::AdviceReport {
        status: elision.status.as_str().to_string(),
        source,
        advisor: advice.advisor.clone(),
        detail: elision.language.clone().map(|l| format!("language={l}")),
        elided_bodies: elision.elided_bodies,
        elided_lines: elision.elided_lines,
        guard_rejects: elision.guard_rejects,
        focused: elision.focused.clone(),
        producer_ms,
    };
    if elision.status != lm_resizer_core::transforms::advice_structural::AdviceStatus::Applied {
        return pipeline_report(Some(advice_report));
    }
    let report = CompressReport {
        tokens: TokenCounts::measure(input_text, &elision.output),
        content_type: "source_code".to_string(),
        original_bytes: input_text.len(),
        compressed_bytes: elision.output.len(),
        bytes_saved: input_text.len() - elision.output.len(),
        steps_applied: vec!["advice_structural".to_string()],
        cache_keys: elision.ccr_key.into_iter().collect(),
        output: elision.output,
    };
    Ok((report, Some(advice_report)))
}

fn perf_stage(stage: &str, elapsed: std::time::Duration) {
    if std::env::var_os("LM_RESIZER_PROFILE").is_some() {
        eprintln!("profile {stage}: {:.6}s", elapsed.as_secs_f64());
    }
}

/// Native capture and native view selection share the existing tee and metrics.
fn run_native_command(command: &[String]) -> Result<ExecReport> {
    run_inspected_command(command, None, None, "")
}
fn run_inspected_command(
    command: &[String],
    mode: Option<inspection_views::Mode>,
    store: Option<&dyn CcrStore>,
    query: &str,
) -> Result<ExecReport> {
    let started = Instant::now();
    let captured = command_capture::run(command)?;
    let raw = display_captured_bytes(&captured.raw);
    let (filter, mut output) = if let Some(error) = captured.launch_error {
        let view = if matches!(mode, Some(inspection_views::Mode::Raw)) {
            error
        } else {
            format!(
                "[FAIL] Command failed (exit code: {})\n{error}",
                captured.code
            )
        };
        ("native:launch-error".to_string(), view)
    } else {
        if let Some(mode) = mode {
            (
                "native:observe".into(),
                inspection_views::summarize(mode, &raw, captured.code),
            )
        } else {
            let (filter, view) = filter_command_output(command, &raw);
            if filter == "lossless:generic" {
                (
                    "generic:summary".into(),
                    inspection_views::summarize(
                        inspection_views::Mode::Summary,
                        &raw,
                        captured.code,
                    ),
                )
            } else {
                (filter, view)
            }
        }
    };
    let filter = if filter.starts_with("native:")
        || filter.starts_with("lossless:")
        || filter.starts_with("code-outline:")
    {
        filter
    } else {
        format!("native:{filter}")
    };
    let filtered_bytes = output.len();
    let mut compression_steps = Vec::new();
    let mut cache_keys = Vec::new();
    if mode.is_none() && filter == "native:native_owned" {
        if let Some(store) = store {
            let compressed = compress_text(&output, query, store)?;
            if first_lost_failure_line(&output, &compressed.output).is_none() {
                output = compressed.output;
                compression_steps = compressed.steps_applied;
                cache_keys = compressed.cache_keys;
            }
        }
    }
    if !matches!(mode, Some(inspection_views::Mode::Raw)) {
        prepend_failure_status(&mut output, captured.code);
    }
    let tee_hint = archive_raw_bytes(&captured.raw)?;
    if let Some(hint) = &tee_hint {
        append_recovery_instruction(&mut output, hint, &raw);
    }
    let report = ExecReport {
        streams: None,
        tokens: TokenCounts::measure(&raw, &output),
        command: command.join(" "),
        exit_code: captured.code,
        filter,
        original_bytes: captured.raw.len(),
        filtered_bytes,
        compressed_bytes: output.len(),
        bytes_saved: captured.raw.len().saturating_sub(output.len()),
        compression_steps,
        cache_keys,
        tee_hint,
        output,
    };
    record_exec_history(&report, started.elapsed())?;
    Ok(report)
}

fn run_exec_command(
    command: &[String],
    query: &str,
    raw_on_failure: bool,
    stream: bool,
    store: Option<&dyn CcrStore>,
) -> Result<ExecReport> {
    if !raw_on_failure && !stream {
        return run_inspected_command(command, None, store, query);
    }
    let started = Instant::now();
    let captured = command_capture::run_separated(command, stream)?;
    let exit_code = captured.code;
    let raw_bytes = captured.raw;
    let view_bytes = captured
        .streams
        .as_ref()
        .map(|streams| combine_command_bytes(&streams.stdout, &streams.stderr))
        .unwrap_or_else(|| raw_bytes.clone());
    let streams = captured
        .streams
        .as_ref()
        .map(|streams| CapturedStreams::new(&streams.stdout, &streams.stderr));
    let launch_error = captured.launch_error;
    let failed_to_launch = launch_error.is_some();
    let raw = launch_error.unwrap_or_else(|| display_captured_bytes(&view_bytes));
    perf_stage("capture", started.elapsed());
    let phase = Instant::now();
    let (filter, mut filtered) = if failed_to_launch {
        ("native:launch-error".to_string(), raw.clone())
    } else if raw_on_failure && exit_code != 0 {
        ("raw_on_failure".to_string(), raw.clone())
    } else {
        filter_command_output(command, &raw)
    };
    if !raw.trim().is_empty() && filtered.trim().is_empty() {
        filtered = raw.clone();
    }

    perf_stage("filter", phase.elapsed());
    let phase = Instant::now();
    // Sans base CCR, pas de compression générique : ses marqueurs `hash=`
    // promettraient une récupération impossible. La vue filtrée est rendue.
    let passthrough = filter == "raw_on_failure"
        || filter.starts_with("native:")
        || filter.starts_with("code-outline:")
        || filter.starts_with("lossless:")
        || matches!(
            filter.as_str(),
            "json-passthrough" | "aws-json" | "aws" | "file-read" | "cargo_test_diagnostics"
        );
    let mut compressed = match store {
        Some(store) if !passthrough => compress_text_with_metrics(
            &filtered,
            query,
            store,
            &build_pipeline(),
            None,
            false,
            false,
        )?,
        _ => CompressReport {
            tokens: TokenCounts::default(),
            content_type: if matches!(filter.as_str(), "json-passthrough" | "aws-json") {
                "json"
            } else {
                "text"
            }
            .to_string(),
            original_bytes: filtered.len(),
            compressed_bytes: filtered.len(),
            bytes_saved: 0,
            steps_applied: Vec::new(),
            cache_keys: Vec::new(),
            output: filtered.clone(),
        },
    };
    // Porte de conservation des diagnostics. Le filtre de commande a choisi
    // les lignes qui comptent ; l'étape générique qui suit ne connaît pas la
    // commande et range ses lignes par fréquence. Mesuré sur un vrai journal
    // GitHub Actions : 61 lignes filtrées ramenées à 10, les dix premières
    // gardées (bruit de stderr des tests) et les `##[error]` omises — omission
    // annoncée et récupérable, mais le diagnostic n'était plus sous les yeux.
    // Une ligne d'échec que le filtre gardait et que la compression perd :
    // on rend la sortie filtrée, qui est déjà réduite.
    perf_stage("pipeline_and_intermediate_tokens", phase.elapsed());
    let phase = Instant::now();
    if let Some(lost) = first_lost_indispensable_line(&filtered, &compressed.output, &filter) {
        eprintln!(
            "lm-resizer: compression générique annulée, elle omettait « {} »",
            truncate_chars(lost.trim(), 80)
        );
        compressed.output = filtered.clone();
        compressed
            .steps_applied
            .push("diagnostic_gate:kept_filtered".to_string());
        compressed.cache_keys.clear();
    }
    perf_stage("diagnostic_gate", phase.elapsed());
    let phase = Instant::now();
    if let (Some(store), false) = (store, compressed.cache_keys.is_empty()) {
        let key = lm_resizer_core::ccr::compute_key(raw.as_bytes());
        store.put(&key, &raw);
        for intermediate in &compressed.cache_keys {
            compressed.output = compressed
                .output
                .replace(&format!("hash={intermediate}]"), &format!("hash={key}]"));
        }
        compressed.cache_keys = vec![key];
    }
    // La pipeline peut réduire une sortie que le filtre n'a pas modifiée.
    // Le tee doit couvrir les omissions de toutes les étapes d'exec.
    let tee_hint = archive_raw_bytes(&raw_bytes)?;
    let mut final_output = compressed.output;
    if streams
        .as_ref()
        .is_some_and(|streams| streams.stdout_bytes > 0 && streams.stderr_bytes > 0)
    {
        if !final_output.ends_with('\n') {
            final_output.push('\n');
        }
        final_output.push_str("[capture: stdout and stderr captured separately; displayed order is not chronological]\n");
    }
    if let Some(hint) = &tee_hint {
        append_recovery_instruction(&mut final_output, hint, &raw);
    }
    prepend_failure_status(&mut final_output, exit_code);

    perf_stage("tee", phase.elapsed());
    let phase = Instant::now();
    let report = ExecReport {
        streams,
        tokens: TokenCounts::measure(&raw, &final_output),
        command: command.join(" "),
        exit_code,
        filter,
        original_bytes: view_bytes.len(),
        filtered_bytes: filtered.len(),
        compressed_bytes: final_output.len(),
        bytes_saved: view_bytes.len().saturating_sub(final_output.len()),
        compression_steps: compressed.steps_applied,
        cache_keys: compressed.cache_keys,
        tee_hint,
        output: final_output,
    };
    perf_stage("final_tokens", phase.elapsed());
    record_exec_history(&report, started.elapsed())?;
    Ok(report)
}

fn process_captured_output(
    command: &[String],
    raw: &str,
    exit_code: i32,
    raw_on_failure: bool,
    query: &str,
    store: &dyn CcrStore,
) -> Result<ExecReport> {
    let keep_raw = raw_on_failure && exit_code != 0;
    let (filter, mut filtered) = if keep_raw {
        ("raw_on_failure".to_string(), raw.to_string())
    } else {
        let (filter, view) = filter_command_output(command, raw);
        if filter == "lossless:generic" {
            (
                "generic:summary".into(),
                inspection_views::summarize(inspection_views::Mode::Summary, raw, exit_code),
            )
        } else {
            (filter, view)
        }
    };
    if !raw.trim().is_empty() && filtered.trim().is_empty() {
        filtered = raw.to_string();
    }
    let (mut output, mut steps, mut keys) = if keep_raw {
        (raw.to_string(), Vec::new(), Vec::new())
    } else if filter.starts_with("native:")
        || filter.starts_with("generic:")
        || filter.starts_with("code-outline:")
        || filter.starts_with("lossless:")
        || matches!(
            filter.as_str(),
            "json-passthrough" | "aws-json" | "aws" | "file-read" | "cargo_test_diagnostics"
        )
    {
        (filtered.clone(), Vec::new(), Vec::new())
    } else {
        let result = compress_text_with_metrics(
            &filtered,
            query,
            store,
            &build_pipeline(),
            None,
            false,
            false,
        )?;
        (result.output, result.steps_applied, result.cache_keys)
    };
    if first_lost_indispensable_line(&filtered, &output, &filter).is_some() {
        output = filtered.clone();
        steps.push("diagnostic_gate:kept_filtered".to_string());
        keys.clear();
    }
    if !filter.starts_with("native:")
        && !filter.starts_with("generic:")
        && output.len() >= raw.len()
        && output != raw
    {
        output = raw.to_string();
        steps.clear();
        keys.clear();
    }
    if output != raw {
        let key = lm_resizer_core::ccr::compute_key(raw.as_bytes());
        store.put(&key, raw);
        for intermediate in &keys {
            output = output.replace(&format!("hash={intermediate}]"), &format!("hash={key}]"));
        }
        keys = vec![key];
    }
    prepend_failure_status(&mut output, exit_code);
    Ok(ExecReport {
        streams: None,
        tokens: TokenCounts::measure(raw, &output),
        command: command.join(" "),
        exit_code,
        filter,
        original_bytes: raw.len(),
        filtered_bytes: filtered.len(),
        compressed_bytes: output.len(),
        bytes_saved: raw.len().saturating_sub(output.len()),
        compression_steps: steps,
        cache_keys: keys,
        tee_hint: None,
        output,
    })
}

fn prepend_failure_status(output: &mut String, exit_code: i32) {
    if exit_code != 0
        && !output.starts_with("[FAIL] Command failed (exit code: ")
        && !inspection_views::failure_verdict(output)
    {
        output.insert_str(
            0,
            &format!("[FAIL] Command failed (exit code: {exit_code})\n"),
        );
    }
}

fn child_exit_code(status: std::process::ExitStatus) -> i32 {
    #[cfg(unix)]
    let interrupted = {
        use std::os::unix::process::ExitStatusExt;
        status.signal().map(|number| 128 + number)
    };
    #[cfg(not(unix))]
    let interrupted = None;
    status.code().or(interrupted).unwrap_or(1)
}

fn combine_command_bytes(stdout: &[u8], stderr: &[u8]) -> Vec<u8> {
    let mut bytes = stdout.to_vec();
    if !stderr.is_empty() {
        if !stdout.is_empty() {
            bytes.push(b'\n');
        }
        bytes.extend_from_slice(b"[stderr]\n");
        bytes.extend_from_slice(stderr);
    }
    bytes
}

fn display_captured_bytes(bytes: &[u8]) -> String {
    // Decode only a complete, valid BOM-tagged UTF-16 capture. The tee always
    // retains the original bytes, including the BOM and Windows line endings.
    let utf16_le = bytes.starts_with(&[0xff, 0xfe]);
    let utf16_be = bytes.starts_with(&[0xfe, 0xff]);
    if (utf16_le || utf16_be) && bytes.len().is_multiple_of(2) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|pair| {
                if utf16_le {
                    u16::from_le_bytes([pair[0], pair[1]])
                } else {
                    u16::from_be_bytes([pair[0], pair[1]])
                }
            })
            .collect();
        if let Ok(text) = String::from_utf16(&units) {
            return text;
        }
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }
    let mut text = String::from(
        "[non-UTF-8 capture: invalid bytes shown as \\xNN, not literal text; exact bytes in tee]\n",
    );
    let mut remaining = bytes;
    while !remaining.is_empty() {
        match std::str::from_utf8(remaining) {
            Ok(valid) => {
                text.push_str(valid);
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                text.push_str(std::str::from_utf8(&remaining[..valid]).expect("validated prefix"));
                remaining = &remaining[valid..];
                let invalid = error.error_len().unwrap_or(remaining.len());
                for byte in &remaining[..invalid] {
                    text.push_str(&format!("\\x{byte:02X}"));
                }
                remaining = &remaining[invalid..];
            }
        }
    }
    text
}

fn rewrite_command_report(command: &[String]) -> RewriteReport {
    if let Some((file, count)) = inspection_views::head_read(command) {
        let argv = vec![
            "lm-resizer".into(),
            "read".into(),
            file,
            "--head-lines".into(),
            count.to_string(),
        ];
        return RewriteReport {
            command: command.join(" "),
            supported: true,
            filter: "file-read".into(),
            rewritten: Some(shell_join(&argv)),
            argv,
        };
    }

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
    let supported = !matches!(filter.as_str(), "none" | "generic" | "lossless:generic");
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

/// Rogne les espaces de bord d'un segment de commande, sauf une espace échappée finale : dans
/// `ls dossier\ ` la dernière espace fait partie du mot (le shell lit `dossier `), la rogner
/// laisserait une barre oblique inverse en fin de ligne.
fn trim_shell(text: &str) -> &str {
    let start = text.trim_start();
    let end = start.trim_end();
    let backslashes = end.bytes().rev().take_while(|byte| *byte == b'\\').count();
    if backslashes % 2 == 1 && end.len() < start.len() {
        if let Some(next) = start[end.len()..].chars().next() {
            return &start[..end.len() + next.len_utf8()];
        }
    }
    end
}

/// Vrai si la ligne contient une nouvelle ligne hors citation (ou une barre oblique inverse
/// devant une nouvelle ligne) : pour le shell c'est un séparateur de commandes ou une
/// continuation, que la reconstruction segment par segment ne sait pas reproduire.
fn has_unquoted_newline(command: &str) -> bool {
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    for ch in trim_shell(command).chars() {
        if matches!(ch, '\n' | '\r') && (escaped || (!in_single && !in_double)) {
            return true;
        }
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if !in_single => escaped = true,
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            _ => {}
        }
    }
    false
}

fn rewrite_shell_report(command: &str) -> RewriteShellReport {
    // Une nouvelle ligne séparant deux commandes serait perdue par la reconstruction (les
    // segments sont rognés) : `A \n && B` est une erreur de syntaxe pour le shell, mais la ligne
    // réécrite `A && B` exécuterait B. Dans le doute, la ligne reste telle quelle.
    if has_unquoted_newline(command) {
        return RewriteShellReport {
            command: command.to_string(),
            changed: false,
            rewritten: trim_shell(command).to_string(),
            rewrites: Vec::new(),
        };
    }
    let tokens = split_shell_operators(command);
    let mut output = String::new();
    let mut rewrites = Vec::new();
    let mut after_pipe = false;

    for (index, token) in tokens.iter().enumerate() {
        match token {
            ShellToken::Operator(op) => {
                if !output.is_empty() && !output.ends_with(' ') {
                    output.push(' ');
                }
                output.push_str(op);
                output.push(' ');
                after_pipe = operator_consumes_output(op);
            }
            ShellToken::Segment(segment) => {
                let trimmed = trim_shell(segment);
                if trimmed.is_empty() {
                    continue;
                }
                // Le producteur d'un tube (`|`, `|&`, donc aussi `| tee`) ne doit
                // pas être enveloppé : le consommateur lirait la vue réduite.
                let feeds_pipe = tokens.get(index + 1).is_some_and(
                    |next| matches!(next, ShellToken::Operator(op) if operator_consumes_output(op)),
                );
                let rewritten =
                    if after_pipe || feeds_pipe || segment_must_not_be_rewritten(trimmed) {
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

    // Aucun segment réécrit : rendre la ligne d'origine, sans normaliser les espaces.
    let rewritten = if rewrites.is_empty() {
        trim_shell(command).to_string()
    } else {
        // `output` se termine par l'espace de séparation ajoutée ci-dessus, pas par une espace
        // échappée d'origine : la retirer seule, sans rogner davantage.
        let mut text = output;
        if text.ends_with(' ') {
            text.pop();
        }
        text.trim_start().to_string()
    };
    RewriteShellReport {
        command: command.to_string(),
        changed: rewritten != trim_shell(command),
        rewritten,
        rewrites,
    }
}

fn operator_consumes_output(op: &str) -> bool {
    matches!(op, "|" | "|&")
}

fn rewrite_shell_segment(segment: &str) -> Option<(String, String)> {
    // Même refus que le hook : ne jamais recoller un suffixe de redirection sur
    // `lm-resizer exec`, sinon `>` capture la vue réduite au lieu des octets d'origine.
    if segment_must_not_be_rewritten(segment) {
        return None;
    }
    let segment = trim_shell(segment);
    // Découpage POSIX strict : s'il refuse la ligne (citation non fermée, opérateur non cité,
    // commentaire, nouvelle ligne), la ligne d'origine reste telle quelle.
    let args = posix_split(segment)?;
    // Un mot `$(...)` ou `` `...` `` qui était littéral ne doit jamais devenir une substitution.
    if args
        .iter()
        .any(|word| word.contains("$(") || word.contains('`'))
    {
        return None;
    }
    let report = rewrite_command_report(&args);
    let rewritten_args = report.rewritten.as_ref()?;
    let rewritten = if report.argv.first().map(String::as_str) == Some("lm-resizer")
        && report.argv.get(1).map(String::as_str) == Some("exec")
    {
        // Enveloppe d'une commande inchangée : les octets d'origine sont recollés tels
        // quels, comme le fait le hook. Aucun mot n'est re-cité, donc aucune citation
        // ne peut être cassée ni une expansion (`$HOME`, `*.rs`) figée.
        let wrapped = format!("lm-resizer exec -- {segment}");
        let mut expected = vec![
            "lm-resizer".to_string(),
            "exec".to_string(),
            "--".to_string(),
        ];
        expected.extend(args.iter().cloned());
        if posix_split(&wrapped).as_ref() != Some(&expected) {
            return None;
        }
        wrapped
    } else {
        // Commande construite à partir de données (`head -n 1 FICHIER` devient
        // `lm-resizer read FICHIER --head-lines 1`) : les arguments sont re-cités en
        // apostrophes POSIX, seulement s'ils sont des littéraux, et la chaîne produite doit
        // se redécouper en exactement ces arguments.
        if segment_has_expansion(segment) {
            return None;
        }
        if posix_split(rewritten_args).as_ref() != Some(&report.argv) {
            return None;
        }
        rewritten_args.clone()
    };
    Some((rewritten, report.filter))
}

/// Vrai si le shell interpréterait quelque chose dans ce segment (variable, substitution,
/// globbing, tilde, accolades) : ses mots ne sont alors pas des littéraux.
fn segment_has_expansion(segment: &str) -> bool {
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    for ch in segment.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if !in_single => escaped = true,
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '$' | '`' if !in_single => return true,
            '*' | '?' | '[' | '{' | '~' if !in_single && !in_double => return true,
            _ => {}
        }
    }
    false
}

/// Découpage POSIX strict d'une ligne de commande simple, sans aucune expansion : les
/// mots rendus sont les octets que verrait le programme si le texte ne contenait ni `$` ni
/// glob. Refuse (`None`) tout ce qui n'est pas un mot ordinaire : citation non fermée,
/// nouvelle ligne ou opérateur non cité, commentaire, parenthèse, barre oblique inverse
/// finale.
fn posix_split(text: &str) -> Option<Vec<String>> {
    #[derive(PartialEq)]
    enum State {
        Plain,
        Single,
        Double,
    }
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut state = State::Plain;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match state {
            State::Single => {
                if ch == '\'' {
                    state = State::Plain;
                } else {
                    current.push(ch);
                }
            }
            State::Double => match ch {
                '"' => state = State::Plain,
                '\\' => match chars.next()? {
                    '\n' => return None,
                    next @ ('$' | '`' | '"' | '\\') => current.push(next),
                    next => {
                        current.push('\\');
                        current.push(next);
                    }
                },
                _ => current.push(ch),
            },
            State::Plain => match ch {
                ' ' | '\t' => {
                    if in_word {
                        words.push(std::mem::take(&mut current));
                        in_word = false;
                    }
                }
                '\n' | '\r' | ';' | '&' | '|' | '<' | '>' | '(' | ')' => return None,
                '#' if !in_word => return None,
                '\'' => {
                    state = State::Single;
                    in_word = true;
                }
                '"' => {
                    state = State::Double;
                    in_word = true;
                }
                '\\' => {
                    let next = chars.next()?;
                    if next == '\n' {
                        return None;
                    }
                    current.push(next);
                    in_word = true;
                }
                _ => {
                    current.push(ch);
                    in_word = true;
                }
            },
        }
    }
    if state != State::Plain {
        return None;
    }
    if in_word {
        words.push(current);
    }
    Some(words)
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
            '>' | '<' if !in_single && !in_double => {
                let mut start_idx = idx;
                if idx > 0 {
                    let before = &segment[..idx];
                    let last_char = before.chars().last().unwrap();
                    if last_char.is_ascii_digit() {
                        let mut all_digits = true;
                        let mut digits_start = idx - last_char.len_utf8();
                        for (i, c) in before.char_indices().rev() {
                            if c.is_whitespace() {
                                digits_start = i + c.len_utf8();
                                break;
                            } else if !c.is_ascii_digit() {
                                all_digits = false;
                                break;
                            }
                            if i == 0 {
                                digits_start = 0;
                            }
                        }
                        if all_digits {
                            start_idx = digits_start;
                        }
                    }
                }
                return segment.split_at(start_idx);
            }
            '&' if !in_single && !in_double => {
                let after = &segment[idx + ch.len_utf8()..];
                if after.starts_with('>') {
                    return segment.split_at(idx);
                }
            }
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
                let is_redirect = if ch == '&' {
                    let mut prev_char = None;
                    if idx > 0 {
                        let before = &command[..idx];
                        prev_char = before.chars().last();
                    }
                    let next_char = if i + 1 < chars.len() {
                        Some(chars[i + 1].1)
                    } else {
                        None
                    };

                    prev_char == Some('>') || prev_char == Some('<') || next_char == Some('>')
                } else {
                    false
                };

                if is_redirect {
                    // Part of a redirection (e.g. 2>&1, >&2, &> /dev/null), not a command separator.
                } else if ch == '|' && i + 1 < chars.len() && chars[i + 1].1 == '&' {
                    // `|&` pipes both stdout and stderr (bash). The producer must stay raw.
                    push_shell_segment(&mut tokens, &command[start..idx]);
                    tokens.push(ShellToken::Operator("|&".to_string()));
                    start = chars[i + 1].0 + chars[i + 1].1.len_utf8();
                    i += 1;
                } else if i + 1 < chars.len() && chars[i + 1].1 == ch {
                    push_shell_segment(&mut tokens, &command[start..idx]);
                    tokens.push(ShellToken::Operator(format!("{ch}{ch}")));
                    start = chars[i + 1].0 + chars[i + 1].1.len_utf8();
                    i += 1;
                } else if ch == '|' || ch == '&' {
                    push_shell_segment(&mut tokens, &command[start..idx]);
                    tokens.push(ShellToken::Operator(ch.to_string()));
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
        tokens.push(ShellToken::Segment(trim_shell(segment).to_string()));
    }
}

fn split_shell_words(segment: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    let mut has_word = false;

    for ch in segment.chars() {
        if escaped {
            current.push(ch);
            has_word = true;
            escaped = false;
            continue;
        }
        match ch {
            '\\' if !in_single => escaped = true,
            '\'' if !in_double => {
                in_single = !in_single;
                has_word = true;
            }
            '"' if !in_single => {
                in_double = !in_double;
                has_word = true;
            }
            ch if ch.is_whitespace() && !in_single && !in_double => {
                if has_word {
                    words.push(std::mem::take(&mut current));
                    has_word = false;
                }
            }
            _ => {
                current.push(ch);
                has_word = true;
            }
        }
    }

    if escaped || in_single || in_double {
        return None;
    }
    if has_word {
        words.push(current);
    }
    Some(words)
}

/// Cite un argument pour un shell POSIX : tel quel s'il ne contient que des caractères
/// sans signification pour le shell, sinon entre apostrophes (`'` devient `'\''`). Rien
/// n'est interprété entre apostrophes, ni `\`, ni `"`, ni `$`.
fn shell_quote(arg: &str) -> String {
    if !arg.is_empty()
        && arg.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '-' | '_' | '.' | '/' | ':' | ',' | '+' | '@' | '%' | '=')
        })
    {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

fn shell_join(args: &[String]) -> String {
    args.iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

fn pipe_filter_command(name: &str) -> Option<Vec<String>> {
    command_views::pipe_command(name)
}

fn filter_command_output(command: &[String], raw: &str) -> (String, String) {
    // Only unwrap a single simple shell command. Never execute/rewrite the
    // shell expression, nor guess the producer of pipelines or compound lists.
    if command.len() == 3
        && matches!(
            command_basename(&command[0]).as_str(),
            "bash" | "sh" | "zsh"
        )
        && matches!(command[1].as_str(), "-c" | "-lc")
    {
        if let [ShellToken::Segment(segment)] = split_shell_operators(&command[2]).as_slice() {
            if let Some(words) = split_shell_words(segment) {
                if !words.is_empty() {
                    return filter_command_output(&words, raw);
                }
            }
        }
    }
    if let Some(result) = command_views::filter(command, raw) {
        return result;
    }
    // An explicitly wrapped native producer is already filtered too. Retain its
    // existing generic-pipeline contract, including pre-pipeline byte counts.
    if command.first().is_some_and(|p| {
        matches!(
            command_basename(p).as_str(),
            "lm-resizer" | "lm-resizer.exe"
        )
    }) {
        return ("native_owned".to_string(), raw.to_owned());
    }
    // Never post-process a command covered by native: extra views apply only
    // to other producers, after the exact pipe dispatch above.
    if command_views::direct_args(command).is_none() {
        if let Some((name, view)) = structured_views::compress(raw) {
            let filter = if name == "diff-metadata" {
                "summary:diff-metadata".to_string()
            } else if name == "code-outline:rust" {
                "code-outline:rust".to_string()
            } else {
                format!("lossless:{name}")
            };
            return (filter, view);
        }
    }
    if command_requests_json(command) && serde_json::from_str::<Value>(raw).is_ok() {
        return ("json-passthrough".to_string(), raw.to_string());
    }
    if let Some((name, output)) = lossless_filters::filter(command, raw) {
        return (format!("lossless:{name}"), output);
    }
    let (name, output) = route_command_filter(command, raw);
    if name == "generic" {
        return ("lossless:generic".to_string(), output);
    }
    if raw.lines().any(|line| line == "[stderr]") && !output.lines().any(|line| line == "[stderr]")
    {
        return (format!("{name}:stream-guard"), raw.to_string());
    }
    // Filters are also a possible source of lost diagnostics. The later
    // pipeline gate only compares its input with its output, so it cannot
    // recover a failure that disappeared here.
    if first_lost_failure_line(raw, &output).is_some() {
        return (format!("{name}:diagnostic-guard"), raw.to_string());
    }
    if output.len() >= raw.len() && output != raw {
        return (name, raw.to_string());
    }
    (name, output)
}

fn route_command_filter(command: &[String], raw: &str) -> (String, String) {
    // native a déjà filtré et reformaté cette sortie pour sa commande. La
    // refiltrer avec les règles de la commande d'origine lit un format qui
    // n'est plus le sien : mesuré sur `native dotnet test --logger detailed`,
    // 11 des 15 faits (Expected/Actual, fichier:ligne) disparaissaient.
    // native garde le filtrage ; lm-resizer n'ajoute que sa compression
    // générique, sous la porte de conservation des diagnostics.
    if command.first().is_some_and(|p| {
        matches!(
            command_basename(p).as_str(),
            "lm-resizer" | "lm-resizer.exe"
        )
    }) {
        return ("native_owned".to_string(), raw.to_string());
    }
    if command_requests_json(command) && serde_json::from_str::<Value>(raw).is_ok() {
        return ("json-passthrough".to_string(), raw.to_string());
    }
    if command
        .first()
        .is_some_and(|program| command_basename(program) == "journalctl")
    {
        return ("journalctl".to_string(), filter_journal(raw));
    }
    if command
        .first()
        .is_some_and(|program| command_basename(program) == "psql")
    {
        return ("psql".to_string(), filter_psql(raw));
    }
    let cargo_test = command
        .first()
        .is_some_and(|program| command_basename(program) == "cargo")
        && command.get(1).is_some_and(|verb| verb == "test");
    if cargo_test
        && raw.lines().any(|line| {
            let trimmed = line.trim_start().to_ascii_lowercase();
            trimmed.starts_with("warning:") || trimmed.starts_with("error:")
        })
    {
        return ("cargo_test_diagnostics".to_string(), raw.to_string());
    }
    if cargo_test
        && raw.lines().any(|line| line.starts_with("test result: ok."))
        && !raw
            .lines()
            .any(|line| line.starts_with("test result: FAILED."))
    {
        let summaries: Vec<_> = raw
            .lines()
            .filter(|line| line.starts_with("test result: ok."))
            .collect();
        if summaries.len() == 1 {
            let summary = summaries[0].trim_start_matches("test result: ok. ");
            let counts = summary.split(';').take(3).collect::<Vec<_>>().join(";");
            return ("cargo_test".to_string(), format!("{counts}\n"));
        }
    }
    if command
        .first()
        .is_some_and(|program| command_basename(program) == "aws")
        && serde_json::from_str::<Value>(raw).is_ok()
    {
        return ("aws-json".to_string(), raw.to_string());
    }
    if let Some((name, summary)) = parity_filters::summarize_for_command(command, raw) {
        return (format!("structured:{name}"), summary);
    }
    if command_filters::is_prisma_migrate(command) {
        let (name, filtered) = command_filters::filter(command, raw).expect("prisma migrate route");
        return (name.to_string(), filtered);
    }
    if command
        .first()
        .is_some_and(|program| command_basename(program) == "aws")
    {
        let (name, filtered) = command_filters::filter(command, raw).expect("aws route");
        return (name.to_string(), filtered);
    }
    let command_text = normalized_command_text(command);
    if let Some((filter, filtered)) = apply_toml_filters(&command_text, raw) {
        return (filter, filtered);
    }
    if let Some((name, filtered)) = command_filters::filter(command, raw) {
        return (name.to_string(), filtered);
    }

    let Some(program) = command.first().map(|s| command_basename(s)) else {
        return ("none".to_string(), raw.to_string());
    };
    let sub = command.get(1).map(String::as_str).unwrap_or("");

    // JS test runners (vitest/jest) — directly or via npx/pnpm/yarn/bunx. This is the single
    // biggest token sink in agent sessions; route it before the generic fallback.
    if command_runs_js_test(command) {
        return ("js_test_runner".to_string(), filter_vitest(raw));
    }

    // `docker build` sous ses quatre formes. Les filtres intégrés couvraient
    // `docker ps` et `docker logs` ; le build, lui, tombait dans le générique
    // alors que c'est la commande docker la plus bavarde.
    if command_runs_docker_build(command) {
        return ("docker_build".to_string(), filter_docker_build(raw));
    }

    match (program.as_str(), sub) {
        ("diff", _) => ("diff_summary".to_string(), filter_diff_summary(raw)),
        ("cargo", "clippy") => ("cargo_diagnostics".to_string(), filter_diagnostics(raw)),
        ("yarn", "test" | "run") => ("js_test".to_string(), filter_diagnostics(raw)),
        ("fd" | "dir", _) => ("listing".to_string(), filter_listing(raw)),
        _ => ("generic".to_string(), filter_generic(raw)),
    }
}

fn command_requests_json(command: &[String]) -> bool {
    command.iter().enumerate().any(|(index, arg)| {
        matches!(arg.as_str(), "--json" | "--format=json" | "--output=json")
            || (matches!(arg.as_str(), "--format" | "--output" | "-o")
                && command.get(index + 1).is_some_and(|next| next == "json"))
    })
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
    let mut keep_blocks = Vec::new();
    for rule in def.keep_block_after_matching {
        let until = match rule.until {
            Some(u) => Some(Regex::new(&u)?),
            None => None,
        };
        keep_blocks.push((Regex::new(&rule.start)?, until, rule.max_lines));
    }
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
        keep_blocks,
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
    // Noms d'en-têtes et de variables composés : `x-api-key`, `x-goog-api-key`,
    // `openai_api_key`, `aws_secret_access_key`, `proxy-authorization`, `db_password`…
    const CONTAINS: &[&str] = &[
        "apikey",
        "authorization",
        "secret",
        "password",
        "privatekey",
        "credential",
    ];
    if CONTAINS.iter().any(|part| normalized.contains(part)) {
        return true;
    }
    // `token` isolé ou suffixe (`accesstoken`, `x-amz-security-token`), jamais le pluriel :
    // `max_tokens`, `input_tokens` sont des compteurs que les fixtures doivent garder.
    normalized.ends_with("token") || matches!(normalized.as_str(), "bearer" | "signature")
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
    let mut block_left = 0usize;
    let mut block_until: Option<&Regex> = None;
    for line in text.lines() {
        if block_left > 0 && block_until.is_some_and(|u| u.is_match(line)) {
            block_left = 0;
        }
        let in_block = block_left > 0;
        if in_block {
            block_left -= 1;
        }
        let opens_block = match filter
            .keep_blocks
            .iter()
            .find(|(start, _, _)| start.is_match(line))
        {
            Some((_, until, max)) => {
                block_left = *max;
                block_until = until.as_ref();
                true
            }
            None => false,
        };
        if !opens_block
            && filter
                .strip_lines_matching
                .as_ref()
                .is_some_and(|set| set.is_match(line))
        {
            continue;
        }
        if !(in_block || opens_block)
            && filter
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
        lines = keep_signal_within(lines, head, TruncateFrom::Tail, &filter.name);
    }
    if let Some(tail) = filter.tail_lines {
        lines = keep_signal_within(lines, tail, TruncateFrom::Head, &filter.name);
    }
    if let Some(max) = filter.max_lines {
        lines = keep_signal_within(lines, max, TruncateFrom::Middle, &filter.name);
    }

    if lines.is_empty() {
        return filter.on_empty.clone().unwrap_or_default();
    }
    lines.join("\n") + "\n"
}

/// A failure line of `before` that `after` no longer shows, if any.
fn first_lost_failure_line<'a>(before: &'a str, after: &str) -> Option<&'a str> {
    if before == after {
        return None;
    }
    let lost = lm_resizer_core::transforms::diagnostic_gate::lost_failure_lines(before, after);
    let lost: std::collections::HashSet<&str> = lost.iter().map(|line| line.trim()).collect();
    before.lines().find(|line| {
        let trimmed = line.trim();
        if trimmed.starts_with("% Total    % Received")
            || trimmed.starts_with("diff --git ")
            || trimmed.starts_with("test result: ok.")
            || (line.contains(" | ") && line.contains("//"))
        {
            return false;
        }
        lost.contains(trimmed)
    })
}

fn first_lost_indispensable_line<'a>(
    before: &'a str,
    after: &str,
    filter: &str,
) -> Option<&'a str> {
    if before == after {
        return None;
    }
    if before.lines().any(|line| line == "[stderr]")
        && !after.lines().any(|line| line == "[stderr]")
    {
        return Some("[stderr]");
    }
    first_lost_failure_line(before, after).or_else(|| {
        if matches!(filter, "search_results" | "listing") {
            let remaining: std::collections::HashSet<&str> = after.lines().map(str::trim).collect();
            return before
                .lines()
                .filter(|line| !line.trim().is_empty())
                .find(|line| !remaining.contains(line.trim()));
        }
        if filter != "diff_summary" && filter != "git_show" {
            return None;
        }
        let remaining: std::collections::HashSet<&str> = after.lines().collect();
        before.lines().find(|line| {
            ((line.starts_with('+') && !line.starts_with("+++"))
                || (line.starts_with('-') && !line.starts_with("---")))
                && !remaining.contains(line)
        })
    })
}

/// Which end of the output a line budget cuts first.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TruncateFrom {
    Head,
    Tail,
    Middle,
}

/// Bring `lines` within `budget`, cutting from `from`, but keep every failure
/// line, and say how many lines went and where to find them. The full output
/// stays in the tee file and the CCR store; the marker points there.
fn keep_signal_within(
    lines: Vec<String>,
    budget: usize,
    from: TruncateFrom,
    filter_name: &str,
) -> Vec<String> {
    if lines.len() <= budget {
        return lines;
    }
    let n = lines.len();
    let mut keep = vec![false; n];
    match from {
        TruncateFrom::Tail => keep[..budget].iter_mut().for_each(|k| *k = true),
        TruncateFrom::Head => keep[n - budget..].iter_mut().for_each(|k| *k = true),
        TruncateFrom::Middle => {
            let head = budget / 2;
            let tail = budget - head;
            keep[..head].iter_mut().for_each(|k| *k = true);
            keep[n - tail..].iter_mut().for_each(|k| *k = true);
        }
    }
    for (i, line) in lines.iter().enumerate() {
        if FAILURE_SIGNAL.is_match(line) {
            keep[i] = true;
        }
    }
    let mut out = Vec::with_capacity(budget + 8);
    let mut skipped = 0usize;
    for (i, line) in lines.into_iter().enumerate() {
        if keep[i] {
            if skipped > 0 {
                out.push(format!(
                    "[lm-resizer: {skipped} lignes omises par le filtre {filter_name} ; sortie complète dans le fichier tee]"
                ));
                skipped = 0;
            }
            out.push(line);
        } else {
            skipped += 1;
        }
    }
    if skipped > 0 {
        out.push(format!(
            "[lm-resizer: {skipped} lignes omises par le filtre {filter_name} ; sortie complète dans le fichier tee]"
        ));
    }
    out
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

fn filter_diff_summary(raw: &str) -> String {
    let has_unified_headers = raw
        .lines()
        .any(|line| line.starts_with("diff --git") || line.starts_with("@@"))
        || (raw.lines().any(|line| line.starts_with("--- "))
            && raw.lines().any(|line| line.starts_with("+++ ")));
    if !has_unified_headers {
        return raw.to_string();
    }
    let mut kept = Vec::new();

    let lines: Vec<&str> = raw.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        if line.starts_with("diff --git") {
            // `---` and `+++` already name both paths. Keep the Git header
            // when either marker is absent (binary or truncated diffs).
            let file_lines = lines[index + 1..]
                .iter()
                .take_while(|next| !next.starts_with("diff --git"));
            let mut old_path = false;
            let mut new_path = false;
            for next in file_lines {
                old_path |= next.starts_with("--- ");
                new_path |= next.starts_with("+++ ");
            }
            if !(old_path && new_path) {
                kept.push((*line).to_string());
            }
            continue;
        }
        if line.starts_with("+++ ") || line.starts_with("--- ") || line.starts_with("@@") {
            kept.push(line.to_string());
            continue;
        }

        if (line.starts_with('+') || line.starts_with('-'))
            && !line.starts_with("+++")
            && !line.starts_with("---")
        {
            kept.push(line.to_string());
            continue;
        }
    }

    append_omitted(kept, 0)
}

fn filter_journal(raw: &str) -> String {
    let mut info = 0;
    let mut kept = Vec::new();
    for line in raw.lines() {
        if line.split_whitespace().nth(1) == Some("INFO") && !FAILURE_SIGNAL.is_match(line) {
            info += 1;
        } else {
            kept.push(line.to_string());
        }
    }
    if info > 0 {
        kept.push(format!("{info} INFO lines; raw: tee list"));
    }
    append_omitted(kept, 0)
}

fn filter_psql(raw: &str) -> String {
    let mut inserts = 0;
    let mut kept = Vec::new();
    for line in raw.lines() {
        if line.starts_with("INSERT 0 ") {
            inserts += 1;
        } else {
            kept.push(line.to_string());
        }
    }
    if inserts > 0 {
        kept.insert(0, format!("{inserts} INSERT results"));
    }
    append_omitted(kept, 0)
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
            || lower.contains("clippy::")
            || lower.trim_start().starts_with("help:")
            || lower.trim_start().starts_with("= note:")
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
        filter_generic(raw)
    } else {
        append_omitted(kept, skipped)
    }
}

/// Les verbes docker qui excluent un build s'ils viennent en premier.
///
/// Ils servent de butée : `build` ne compte que s'il arrive avant eux. Sans
/// cette butée, `docker run build` — une image nommée « build » — serait pris
/// pour un build, et `docker ps` / `docker logs` se verraient voler leur
/// filtre.
const DOCKER_NON_BUILD_VERBS: [&str; 14] = [
    "run", "ps", "logs", "exec", "pull", "push", "up", "down", "images", "inspect", "rm", "rmi",
    "cp", "start",
];

/// Les drapeaux globaux de `docker`/`podman` qui consomment l'argument suivant.
///
/// Sans cette liste, la **valeur** d'un drapeau est lue comme une
/// sous-commande : `docker --context build ps` devient un build parce que le
/// contexte s'appelle « build ». On ne peut pas non plus sauter l'argument qui
/// suit n'importe quel drapeau : `--debug` et `--tls` sont des booléens, et
/// `docker --debug build .` est bien un build. Seuls ces drapeaux-ci prennent
/// une valeur séparée ; la forme collée `--context=build` se traite toute
/// seule, elle commence par `-`.
const DOCKER_GLOBAL_FLAGS_WITH_VALUE: [&str; 16] = [
    "--context",
    "-c",
    "--host",
    "-H",
    "--log-level",
    "-l",
    "--config",
    "--tlscacert",
    "--tlscert",
    "--tlskey",
    "--connection",
    "--url",
    "--identity",
    "--root",
    "--runroot",
    "--storage-driver",
];

/// Reconnaît un build d'image, sous les formes que docker et podman acceptent.
///
/// La position du verbe n'est pas fixe : `docker --context distant build .`
/// place un drapeau global et **sa valeur** avant `build`. On lit donc les
/// arguments dans l'ordre, en sautant la valeur des drapeaux qui en prennent
/// une, et on s'arrête au premier verbe concurrent. `bake` est la forme buildx
/// multi-cibles ; sa sortie a la même forme. `compose up --build` n'est pas
/// pris : ses logs de services ne sont pas un build.
///
/// Un `bash -lc "docker build …"` n'est pas reconnu : le programme est le shell.
fn command_runs_docker_build(command: &[String]) -> bool {
    let Some(first) = command.first().map(|s| command_basename(s)) else {
        return false;
    };
    if !matches!(
        first.as_str(),
        "docker" | "podman" | "docker-compose" | "podman-compose"
    ) {
        return false;
    }
    let mut args = command.iter().skip(1).map(String::as_str);
    while let Some(tok) = args.next() {
        if DOCKER_GLOBAL_FLAGS_WITH_VALUE.contains(&tok) {
            // Ce qui suit est la valeur du drapeau, jamais la sous-commande.
            args.next();
            continue;
        }
        if tok.starts_with('-') {
            continue;
        }
        if tok == "build" || tok == "bake" {
            return true;
        }
        if DOCKER_NON_BUILD_VERBS.contains(&tok) {
            return false;
        }
    }
    false
}

/// Une ligne de `docker build` qui annonce un échec ou une annulation.
///
/// BuildKit dit l'échec trois fois — sur l'étape (`#8 ERROR:`), dans le rappel
/// (`> [4/9] RUN …`) et en conclusion (`failed to solve:`). Le builder
/// classique (`DOCKER_BUILDKIT=0`) ne dit rien de tout cela : sa seule phrase
/// d'échec est `returned a non-zero code`. Une annulation n'est souvent qu'une
/// ligne `CANCELED`. Aucune de ces formes ne doit être lue comme un succès.
fn docker_build_line_is_failure(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("error:")
        || lower.contains(" error ")
        || lower.starts_with("error:")
        || lower.starts_with("error ")
        || lower.contains("failed to solve")
        || lower.contains("did not complete successfully")
        || lower.contains("returned a non-zero code")
        || lower.contains("executor failed")
        || lower.contains("canceled")
        || lower.contains("cancelled")
}

/// En-tête d'étape, builder classique (`Step N/M`), Buildah (`STEP N/M`) ou
/// BuildKit plain (`#N [stage] RUN …`). Sert à rattacher la sortie d'un `RUN`
/// à l'échec qui la conclut : le classique ne répète pas cette sortie après.
fn docker_build_line_is_step_header(line: &str) -> bool {
    let t = line.trim_start();
    if t.starts_with("Step ") || t.starts_with("STEP ") {
        return true;
    }
    let Some(rest) = t.strip_prefix('#') else {
        return false;
    };
    let Some((step, rest)) = rest.split_once(' ') else {
        return false;
    };
    step.chars().all(|c| c.is_ascii_digit()) && rest.starts_with('[')
}

/// Identité de l'image produite, sous les formes que les builders écrivent
/// vraiment : BuildKit (`writing image`, `naming to`, `unpacking to`,
/// `pushing manifest for`), builder classique (`Successfully built` /
/// `tagged`), `docker build -q` (une ligne `sha256:` et rien d'autre) et
/// Podman (`COMMIT`).
fn docker_build_line_is_identity(line: &str) -> bool {
    let trimmed = line.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("writing image")
        || lower.contains("naming to")
        || lower.contains("unpacking to")
        || lower.contains("pushing manifest for")
        || lower.contains("successfully built")
        || lower.contains("successfully tagged")
    {
        return true;
    }
    if trimmed.starts_with("COMMIT ") {
        return true;
    }
    let Some(hex) = trimmed.strip_prefix("sha256:") else {
        return false;
    };
    (12..=64).contains(&hex.len()) && hex.chars().all(|c| c.is_ascii_hexdigit())
}

/// Le bruit mécanique de BuildKit : un préfixe `#N` suivi d'un mot d'état.
///
/// Ces lignes ne portent que des octets transférés, des durées et des
/// condensats de couches. Elles ne disent rien qu'un agent puisse utiliser, et
/// elles constituent l'essentiel du volume d'un build qui se passe bien.
fn docker_build_line_is_mechanical(line: &str) -> bool {
    let Some(rest) = line.strip_prefix('#') else {
        return false;
    };
    let Some((step, rest)) = rest.split_once(' ') else {
        return false;
    };
    if !step.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    // `writing image` et `naming to` portent l'identité de ce qui a été
    // produit : ce sont les deux lignes d'export qu'on garde.
    matches!(
        rest.split_whitespace().next(),
        Some(
            "DONE" | "CACHED" | "transferring" | "extracting" | "resolve" | "preparing" | "sha256:"
        )
    ) || rest.starts_with("exporting layers")
        || rest.starts_with("exporting manifest")
        || rest.starts_with("exporting config")
        || rest.starts_with("sha256:")
}

/// Sortie d'un `docker build` (BuildKit, builder classique, buildx, podman).
///
/// Un build qui réussit est presque entièrement mécanique : numéros d'étape,
/// octets transférés, durées, couches en cache. On n'en garde que les
/// identités d'image et les avertissements. Un build qui échoue se lit depuis
/// l'en-tête de l'étape fautive : le classique écrit la sortie du `RUN` *avant*
/// `returned a non-zero code` et ne la répète pas. Remplacer ce bloc par
/// `docker build: completed` serait un faux succès. Ce message n'est émis que
/// lorsqu'il ne reste aucune ligne utile et aucun signal d'échec.
fn filter_docker_build(raw: &str) -> String {
    let lines: Vec<&str> = raw.lines().collect();
    let keep_from = lines
        .iter()
        .position(|line| docker_build_line_is_failure(line))
        .map(|idx| {
            let mut start = idx;
            for i in (0..idx).rev() {
                if docker_build_line_is_step_header(lines[i]) {
                    start = i;
                    break;
                }
            }
            start
        });

    let mut kept = Vec::new();
    let mut skipped = 0usize;
    for (i, line) in lines.iter().enumerate() {
        if keep_from.is_some_and(|start| i >= start) {
            kept.push((*line).to_string());
            continue;
        }
        if line.trim().is_empty() || docker_build_line_is_mechanical(line) {
            skipped += 1;
            continue;
        }
        let lower = line.to_ascii_lowercase();
        // `npm ERR!` ne contient pas le mot « error ». « deprecat » couvre
        // `deprecated` et `DEPRECATION NOTICE`, que le builder écrit sans
        // le mot « warn ».
        if lower.contains("error")
            || lower.contains("err!")
            || lower.contains("warn")
            || lower.contains("deprecat")
            || lower.contains("failed")
            || lower.contains("fatal")
            || docker_build_line_is_identity(line)
        {
            kept.push((*line).to_string());
        } else {
            skipped += 1;
        }
    }

    if kept.is_empty() {
        if lines.iter().any(|line| docker_build_line_is_failure(line)) {
            return append_omitted(lines.iter().map(|line| (*line).to_string()).collect(), 0);
        }
        "docker build: completed\n".to_string()
    } else {
        append_omitted(kept, skipped)
    }
}

/// True when the command invokes the vitest/jest JS test runners — directly (`vitest run`,
/// `jest`) or via a JS launcher (`npx vitest`, `bunx jest`, `pnpm exec vitest`, `yarn jest`).
/// Deliberately does NOT match `grep vitest` / `cat jest.config.js`: the runner must be the
/// program itself or an argument to a known launcher, never an arbitrary search pattern/path.
fn command_runs_js_test(command: &[String]) -> bool {
    let Some(first) = command.first().map(|s| command_basename(s)) else {
        return false;
    };
    if first == "vitest" || first == "jest" {
        return true;
    }
    const LAUNCHERS: [&str; 6] = ["npx", "bunx", "pnpm", "yarn", "npm", "bun"];
    if LAUNCHERS.contains(&first.as_str()) {
        return command
            .iter()
            .skip(1)
            .map(|tok| command_basename(tok))
            .any(|b| b == "vitest" || b == "jest");
    }
    false
}

/// Filter vitest/jest output down to the real signal: failing files/tests, assertion diffs,
/// stack frames, and the final `Test Files` / `Tests` summary. Passing-noise, the RUN banner,
/// deprecation notices and timing footers are dropped. When everything passed, collapse hard to
/// just the summary line(s) — matching native's `vitest run` semantic collapse.
fn filter_vitest(raw: &str) -> String {
    let mut kept = Vec::new();
    let mut keep_following = 0usize;
    let mut skipped = 0usize;
    let mut saw_failure = false;

    for line in raw.lines() {
        let trimmed = line.trim();

        // Final counters (vitest: "Test Files …" / "Tests …"; jest: "Tests:" / "Test Suites:").
        let is_summary = trimmed.starts_with("Test Files")
            || trimmed.starts_with("Test Suites")
            || trimmed.starts_with("Tests")
            || trimmed.starts_with("Snapshots:");

        // Failure markers across vitest + jest output shapes.
        let is_failure = trimmed.starts_with("FAIL ")
            || trimmed.starts_with('\u{00D7}') // × vitest failed test
            || trimmed.starts_with('\u{2715}') // ✕ jest failed test
            || trimmed.starts_with('\u{276F}') // ❯ vitest failing file header / stack frame
            || trimmed.starts_with('\u{25CF}') // ● jest failure header
            || trimmed.contains("Failed Tests")
            || trimmed.contains("AssertionError")
            || trimmed.contains("Error:")
            || trimmed.contains("Expected")
            || trimmed.contains("Received")
            || trimmed.starts_with("expect(");

        if is_summary {
            kept.push(line.to_string());
            keep_following = 0; // final counters end the failure block; drop trailing Start at/Duration
        } else if is_failure {
            saw_failure = true;
            kept.push(line.to_string());
            keep_following = 3; // grab a little trailing context (diff/stack/code frame)
        } else if keep_following > 0 && !trimmed.is_empty() {
            kept.push(line.to_string());
            keep_following -= 1;
        } else {
            keep_following = keep_following.saturating_sub(1);
            skipped += 1;
        }
    }

    if kept.is_empty() {
        return "vitest: passed\n".to_string();
    }
    if !saw_failure {
        // All green: collapse to the summary counters only.
        let summary: Vec<String> = kept
            .into_iter()
            .filter(|l| {
                let t = l.trim();
                t.starts_with("Test Files")
                    || t.starts_with("Test Suites")
                    || t.starts_with("Tests")
            })
            .collect();
        return if summary.is_empty() {
            "vitest: passed\n".to_string()
        } else {
            summary.join("\n") + "\n"
        };
    }
    append_omitted(kept, skipped)
}

fn filter_listing(raw: &str) -> String {
    let mut out = Vec::new();
    let mut skipped = 0usize;
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("total 0") {
            skipped += 1;
            continue;
        }
        out.push(line.to_string());
    }
    append_omitted(out, skipped)
}

fn filter_generic(raw: &str) -> String {
    // Unknown output has no safe semantic grammar: do not hide facts in a codec.
    raw.to_string()
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

fn attach_report_recovery(raw: &str, report: &mut ExecReport) -> Result<()> {
    report.tee_hint = tee_raw_output_if_useful(raw, &report.output)?;
    if let Some(hint) = &report.tee_hint {
        if !report.output.contains("lm-resizer tee read ") {
            append_recovery_instruction(&mut report.output, hint, raw);
        }
        report.tokens = TokenCounts::measure(raw, &report.output);
        report.compressed_bytes = report.output.len();
        report.bytes_saved = raw.len().saturating_sub(report.output.len());
    }
    Ok(())
}

fn append_recovery_instruction(output: &mut String, hint: &str, raw: &str) {
    // Unchanged output cannot pay for a trailer. Avoid tokenizing a candidate
    // which the contract would always reject; final metrics still count raw.
    if raw == output {
        return;
    }
    let Some(id) = hint
        .strip_prefix("[raw: ")
        .and_then(|s| s.strip_suffix(']'))
    else {
        return;
    };
    let mut candidate = output.clone();
    if !candidate.ends_with('\n') && !candidate.is_empty() {
        candidate.push('\n');
    }
    candidate.push_str(&format!("[tee:{id}]\n"));
    // Storage and JSON metadata remain available even when the visible hint
    // would consume more tokens than the reduction pays for. Reserve a visible
    // trailer for substantial savings: at least 30% including the trailer.
    // Small reductions keep their entire token benefit; tee list / JSON still
    // expose every archive. This threshold is independent of the benchmark.
    let counts = TokenCounts::measure(raw, &candidate);
    if counts.compressed_tokens <= counts.original_tokens.saturating_mul(7) / 10 && raw != output {
        *output = candidate;
    }
}

fn tee_raw_output_if_useful(raw: &str, output: &str) -> Result<Option<String>> {
    tee_raw_bytes_if_useful(raw.as_bytes(), output)
}

fn tee_raw_bytes_if_useful(raw: &[u8], output: &str) -> Result<Option<String>> {
    if std::env::var("LM_RESIZER_TEE").ok().as_deref() == Some("0") || raw == output.as_bytes() {
        return Ok(None);
    }

    archive_raw_bytes(raw)
}

fn archive_raw_bytes(raw: &[u8]) -> Result<Option<String>> {
    if std::env::var("LM_RESIZER_TEE").ok().as_deref() == Some("0") {
        return Ok(None);
    }
    let Ok(state_dir) = default_state_dir() else {
        warn_state_unwritable(&state_path_for_warning(None));
        return Ok(None);
    };
    let tee_dir = state_dir.join("tee");
    let digest = format!("{:x}", Sha256::digest(raw));
    let path = tee_dir.join(format!("{digest}.log"));
    let written = create_private_dir_all(&tee_dir).and_then(|()| write_private_file(&path, raw));
    if written.is_err() {
        warn_state_unwritable(&state_dir);
        return Ok(None);
    }
    Ok(Some(format!("[raw: {}]", &digest[..12])))
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
            std::io::stdout().write_all(&std::fs::read(path)?)?;
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
    let path = if candidate.components().count() == 1 && !dir.join(&candidate).exists() {
        let matches: Vec<_> = list_tee_files()?
            .files
            .into_iter()
            .filter(|entry| entry.name.starts_with(file))
            .collect();
        if matches.len() != 1 {
            anyhow::bail!("tee reference {file} matched {} files", matches.len());
        }
        PathBuf::from(&matches[0].path)
    } else if candidate.components().count() == 1 {
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

/// Les compteurs sont un confort : un échec d'écriture n'a jamais d'effet sur
/// la commande ni sur son code de sortie.
fn record_exec_history(report: &ExecReport, elapsed: Duration) -> Result<()> {
    if std::env::var("LM_RESIZER_TRACKING").ok().as_deref() == Some("0") {
        return Ok(());
    }
    if write_exec_history(report, elapsed).is_err() {
        warn_state_unwritable(&state_path_for_warning(None));
    }
    Ok(())
}

fn write_exec_history(report: &ExecReport, elapsed: Duration) -> Result<()> {
    let dir = default_state_dir()?;
    create_private_dir_all(&dir)?;
    let path = dir.join("exec-history.jsonl");
    let record = ExecHistoryRecord {
        cwd: std::env::current_dir()?.to_string_lossy().into_owned(),
        tokens: report.tokens.clone(),
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
    let mut file = open_private_append(&path)?;
    writeln!(file, "{}", serde_json::to_string(&record)?)?;
    Ok(())
}

/// True when the segment ends with a shell redirect (`>`, `<`, `>>`) — its output goes to a file,
/// not the model, so wrapping it would be pointless and would rewrite the file's contents.
fn segment_has_redirect(seg: &str) -> bool {
    let (_, suffix) = split_trailing_redirects(seg);
    !suffix.trim().is_empty()
}

/// Partagé par `rewrite-shell` et le hook PreToolUse. Vrai quand envelopper le segment dans
/// `lm-resizer exec` remplacerait les octets qu'un fichier, un tube, une substitution, un
/// here-doc ou un terminal attendent. Le hook refuse en plus tout opérateur (`&&`, `;`, `&`).
fn segment_must_not_be_rewritten(segment: &str) -> bool {
    segment_has_redirect(segment)
        || segment_has_command_substitution(segment)
        || segment_has_heredoc(segment)
        || segment_is_interactive(segment)
}

/// `$(...)` et les backticks, y compris entre guillemets doubles. Pas `${var}` ni `'$(littéral)'`.
fn segment_has_command_substitution(segment: &str) -> bool {
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    let chars: Vec<char> = segment.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if escaped {
            escaped = false;
            index += 1;
            continue;
        }
        match ch {
            '\\' if !in_single => escaped = true,
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '`' if !in_single => return true,
            '$' if !in_single && index + 1 < chars.len() && chars[index + 1] == '(' => {
                return true;
            }
            _ => {}
        }
        index += 1;
    }
    false
}

/// Here-document (`<<` / `<<-`), y compris `<<'EOF'`. Le corps est des octets littéraux
/// destinés à l'écrivain ou à l'interpréteur ; l'envelopper les ferait filtrer ou détacher.
fn segment_has_heredoc(segment: &str) -> bool {
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    let chars: Vec<char> = segment.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if escaped {
            escaped = false;
            index += 1;
            continue;
        }
        match ch {
            '\\' if !in_single => escaped = true,
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '<' if !in_single
                && !in_double
                && index + 1 < chars.len()
                && chars[index + 1] == '<' =>
            {
                return true;
            }
            _ => {}
        }
        index += 1;
    }
    false
}

fn segment_is_interactive(segment: &str) -> bool {
    let Some(words) = split_shell_words(segment) else {
        return false;
    };
    command_is_interactive(&words)
}

fn command_is_interactive(words: &[String]) -> bool {
    let Some(program) = words.first() else {
        return false;
    };
    let base = command_basename(program);
    const ALWAYS: &[&str] = &[
        "vim",
        "nvim",
        "vi",
        "nano",
        "emacs",
        "less",
        "more",
        "most",
        "top",
        "htop",
        "btop",
        "watch",
        "man",
        "ssh",
        "sftp",
        "ftp",
        "telnet",
        "tmux",
        "screen",
        "fzf",
        "mysql",
        "mongo",
        "mongosh",
        "redis-cli",
        "irb",
        "ipython",
        "pgcli",
        "lazygit",
        "tig",
    ];
    if ALWAYS.contains(&base.as_str()) {
        return true;
    }
    if matches!(base.as_str(), "python" | "python3" | "node" | "nodejs") {
        return repl_without_script(words);
    }
    if base == "psql" {
        return !psql_is_scripted(words);
    }
    if matches!(base.as_str(), "docker" | "podman" | "kubectl")
        && container_exec_is_interactive(words)
    {
        return true;
    }
    if base == "git" {
        return git_command_is_interactive(words);
    }
    false
}

fn repl_without_script(words: &[String]) -> bool {
    if words
        .iter()
        .any(|word| word == "-i" || word == "--interactive")
    {
        return true;
    }
    let scripted = words.iter().skip(1).any(|word| {
        matches!(
            word.as_str(),
            "-c" | "-m" | "-e" | "--eval" | "--version" | "-V" | "--help" | "-h"
        ) || !word.starts_with('-')
    });
    !scripted
}

fn psql_is_scripted(words: &[String]) -> bool {
    words.iter().any(|word| {
        matches!(
            word.as_str(),
            "-c" | "--command"
                | "-f"
                | "--file"
                | "-l"
                | "--list"
                | "--version"
                | "-V"
                | "--help"
                | "-?"
        ) || word.starts_with("--command=")
            || word.starts_with("--file=")
    })
}

fn container_exec_is_interactive(words: &[String]) -> bool {
    let Some(pos) = words.iter().position(|word| word == "exec") else {
        return false;
    };
    words[pos + 1..].iter().any(|word| {
        matches!(
            word.as_str(),
            "-i" | "-t" | "-it" | "-ti" | "--interactive" | "--tty"
        ) || word.starts_with("--interactive=")
            || word.starts_with("--tty=")
    })
}

fn git_command_is_interactive(words: &[String]) -> bool {
    let Some(sub_index) = git_subcommand_index(words) else {
        return false;
    };
    let sub = words[sub_index].as_str();
    // Uniquement les arguments de la sous-commande : `git -C dir` n'est pas un `-C <commit>`.
    let flags = &words[sub_index + 1..];
    match sub {
        "difftool" | "mergetool" => true,
        "rebase" => flags
            .iter()
            .any(|word| word == "-i" || word == "--interactive"),
        "add" | "checkout" | "restore" => flags
            .iter()
            .any(|word| matches!(word.as_str(), "-i" | "--interactive" | "-p" | "--patch")),
        "commit" | "merge" | "cherry-pick" => !git_supplies_message(flags),
        _ => false,
    }
}

/// Index de la sous-commande git après les options globales (`-C`, `-c`, `--git-dir`, …).
fn git_subcommand_index(words: &[String]) -> Option<usize> {
    let mut index = 1;
    while index < words.len() {
        match words[index].as_str() {
            "-C" | "-c" | "--git-dir" | "--work-tree" | "--namespace" => index += 2,
            flag if flag.starts_with('-') => index += 1,
            _ => return Some(index),
        }
    }
    None
}

fn git_supplies_message(flags: &[String]) -> bool {
    flags.iter().any(|word| {
        matches!(
            word.as_str(),
            "-m" | "--message" | "-F" | "--file" | "-C" | "--no-edit"
        ) || word.starts_with("--message=")
            || word.starts_with("--file=")
            || (word.starts_with("-m") && word.len() > 2 && !word.starts_with("--"))
    })
}

/// Rewrite a Bash command to run through `"{exe}" exec -- <cmd>` for the hook, ONLY when it is a
/// single simple command (no `&&`/`||`/`;`/`|`/`&`, and [`segment_must_not_be_rewritten`] is
/// false) whose program has a real filter. Crucially the wrap is VERBATIM — the original segment
/// bytes are reused untouched, never re-tokenized — so quoting/backslash-escaping (e.g. a grep BRE
/// `"\|"`) can't be corrupted the way a split-and-rejoin would. Compound, piped, redirected,
/// substituted, here-document and interactive commands run raw (safety over coverage).
fn rewrite_command_for_hook(command: &str, exe: &str) -> Option<String> {
    let tokens = split_shell_operators(command.trim());
    let [ShellToken::Segment(seg)] = tokens.as_slice() else {
        return None; // operators present → don't touch (avoid pipe/&& semantics + re-quoting)
    };
    let seg = seg.trim();
    if segment_must_not_be_rewritten(seg) {
        return None;
    }
    // Découpage POSIX strict : refuse citation non fermée, commentaire, nouvelle ligne.
    let words = posix_split(seg)?;
    // empty, or our own exec invocation (anti-recursion) → leave raw
    if words.is_empty() || command_basename(&words[0]) == "lm-resizer" {
        return None;
    }
    if !rewrite_command_report(&words).supported {
        return None;
    }
    Some(format!("{} exec -- {seg}", agent_hooks::quote_program(exe)))
}

/// Build the PreToolUse `hookSpecificOutput` that rewrites a supported Bash command to run
/// through `"{exe}" exec -- <cmd>`, preserving other tool_input fields. Returns `None` (→ emit
/// nothing → run raw) when the command is unsupported/compound/redirected or is our own exec
/// invocation. Pure/testable: takes the parsed event + resolved exe path.
fn pretooluse_rewrite_json(value: &Value, exe: &str, event: &str) -> Option<Value> {
    let command = extract_hook_command(value)?;
    let mut rewritten = rewrite_command_for_hook(&command, exe)?;
    let shell = value
        .pointer("/tool_input/shell")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    let tool = value.get("tool_name").and_then(Value::as_str).unwrap_or("");
    if shell.contains("powershell")
        || shell.contains("pwsh")
        || tool.eq_ignore_ascii_case("powershell")
        || (cfg!(windows) && shell.is_empty() && tool == "exec_command")
    {
        rewritten = format!("& '{}' exec -- {}", exe.replace('\'', "''"), command.trim());
    }
    let updated_input = match value.pointer("/tool_input") {
        Some(Value::Object(map)) => {
            let mut map = map.clone();
            let key = if map.contains_key("cmd") && !map.contains_key("command") {
                "cmd"
            } else {
                "command"
            };
            map.insert(key.to_string(), Value::String(rewritten));
            Value::Object(map)
        }
        _ => serde_json::json!({ "command": rewritten }),
    };
    Some(serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": event,
            // Codex exige `permissionDecision: "allow"` à côté d'`updatedInput`, sinon il marque
            // le hook « Failed » et exécute la commande d'origine (constaté Codex 0.153.4, 07/09/2026).
            // Claude Code accepte aussi ce champ.
            "permissionDecision": "allow",
            "permissionDecisionReason": "lm-resizer auto-rewrite",
            "updatedInput": updated_input,
        }
    }))
}

/// PreToolUse hook entry: read the event from stdin, print the rewrite JSON if any, else nothing.
/// Never-throws / never-blocks — any parse failure silently lets the command run raw.
fn emit_pretooluse_rewrite(event: &str, client: &str) {
    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        return;
    }
    let Ok(value) = serde_json::from_str::<Value>(&input) else {
        return;
    };
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| "lm-resizer".to_string());
    if let Some(out) = agent_hooks::rewrite(&value, &exe, event, client) {
        if std::env::var("LM_RESIZER_TRACKING").as_deref() != Ok("0") {
            // Local counters only; no command arguments or remote telemetry.
            let _ = (|| -> Result<()> {
                let dir = default_state_dir()?;
                create_private_dir_all(&dir)?;
                let mut file = open_private_append(&dir.join("hook-audit.jsonl"))?;
                writeln!(
                    file,
                    "{}",
                    json!({"timestamp_unix":unix_timestamp(),"client":client,"event":event})
                )?;
                Ok(())
            })();
        }
        println!("{out}");
    }
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
                streams: None,
                tokens: TokenCounts::measure(&output, &compressed.output),
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
    let content = if path.exists() {
        std::fs::read_to_string(path)?
    } else {
        String::new()
    };
    Ok(token_metrics::summarize_history(&content))
}

fn record_retrieval_feedback(hash: &str, bytes: usize, source: &str) -> Result<()> {
    if std::env::var("LM_RESIZER_TRACKING").ok().as_deref() == Some("0") {
        return Ok(());
    }
    let dir = default_state_dir()?;
    create_private_dir_all(&dir)?;
    let path = dir.join("retrieval-feedback.jsonl");
    let record = json!({
        "timestamp_unix": unix_timestamp(),
        "hash": hash,
        "bytes": bytes,
        "source": source,
    });
    let mut file = open_private_append(&path)?;
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
        "- Legacy estimated tokens saved (bytes / 4, unmeasured records only): {}\n\n",
        history
            .get("estimated_tokens_saved")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    ));
    out.push_str(&format!(
        "- Tokens saved: {} ({}; exact text count, {} measured / {} unmeasured commands)\n\n",
        history
            .get("tokens_saved")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        TOKENIZER,
        history
            .get("measured_commands")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        history
            .get("unmeasured_commands")
            .and_then(Value::as_u64)
            .unwrap_or(0),
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
        out.push_str("| Filter | Commands | Bytes saved | Legacy est. tokens saved (bytes / 4) | Tokens saved (o200k_base) |\n");
        out.push_str("| --- | ---: | ---: | ---: | ---: |\n");
        for row in filters.iter().take(10) {
            out.push_str(&format!(
                "| `{}` | {} | {} | {} | {} |\n",
                markdown_escape(row.get("name").and_then(Value::as_str).unwrap_or("")),
                row.get("commands").and_then(Value::as_u64).unwrap_or(0),
                row.get("bytes_saved").and_then(Value::as_u64).unwrap_or(0),
                row.get("estimated_tokens_saved")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                row.get("tokens_saved").and_then(Value::as_i64).unwrap_or(0)
            ));
        }
        out.push('\n');
    }
    out
}

fn format_gain(report: &Value) -> String {
    let history = &report["exec_history"];
    let commands = history["commands"].as_u64().unwrap_or(0);
    let original = history["original_tokens"].as_u64().unwrap_or(0);
    let compressed = history["compressed_tokens"].as_u64().unwrap_or(0);
    let saved = history["tokens_saved"].as_i64().unwrap_or(0);
    let pct = if original == 0 {
        0.0
    } else {
        saved as f64 * 100.0 / original as f64
    };
    let mut output = "LM Resizer gain\n".to_string();
    if let Some(project) = history["project"].as_str() {
        output.push_str(&format!("Project: {project}\n"));
    }
    output.push_str(&format!("Total commands: {commands}\nOriginal tokens: {original}\nOutput tokens: {compressed}\nTokens saved: {saved} ({pct:.1}%)\nTokenizer: {TOKENIZER}\n"));
    if let Some(rows) = report["history"].as_array() {
        output.push_str("Recent executions:\n");
        for row in rows {
            let command = row["command"]
                .as_str()
                .unwrap_or("")
                .replace(['\n', '\r'], " ");
            output.push_str(&format!(
                "  exit {} | {} tokens saved | {}\n",
                row["exit_code"], row["tokens_saved"], command
            ));
        }
    }
    output
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
        description: None,
        output: None,
        output_bytes: None,
    })
}

fn describe_image(path: &Path) -> Result<String> {
    use image::GenericImageView;
    let img = image::open(path).with_context(|| format!("could not decode {}", path.display()))?;
    let (width, height) = img.dimensions();
    if width == 0 || height == 0 {
        anyhow::bail!("empty image");
    }
    let stride = (((width as u64 * height as u64) / 4096) as f64)
        .sqrt()
        .floor()
        .max(1.0) as usize;
    let mut total = [0u64; 3];
    let mut transparent = 0u64;
    let mut count = 0u64;
    for y in (0..height).step_by(stride) {
        for x in (0..width).step_by(stride) {
            let rgba = img.get_pixel(x, y).0;
            for i in 0..3 {
                total[i] += u64::from(rgba[i]);
            }
            transparent += u64::from(rgba[3] < 255);
            count += 1;
        }
    }
    let rgb = total.map(|v| (v / count) as u8);
    let lightness = (u16::from(rgb[0]) + u16::from(rgb[1]) + u16::from(rgb[2])) / 3;
    let tone = if lightness < 85 {
        "dark"
    } else if lightness > 170 {
        "light"
    } else {
        "mid-tone"
    };
    let channel_range = *rgb.iter().max().unwrap() - *rgb.iter().min().unwrap();
    let palette = if channel_range < 20 {
        "near-grayscale"
    } else {
        "colored"
    };
    Ok(format!(
        "{width}x{height} {tone}, {palette}, transparency: {} (sampled color metadata; no OCR or scene recognition)",
        if transparent > 0 { "yes" } else { "no" }
    ))
}

fn encode_smaller_image(
    input: &Path,
    output: &Path,
    max_dimension: Option<u32>,
    quality: Option<u8>,
) -> Result<Option<u64>> {
    use image::{GenericImageView, ImageFormat, ImageOutputFormat};
    if output.exists() {
        anyhow::bail!("output already exists: {}", output.display());
    }
    let format = ImageFormat::from_path(input)?;
    if ImageFormat::from_path(output)? != format {
        anyhow::bail!("input and output formats must match");
    }
    let mut img = image::open(input)?;
    if let Some(limit) = max_dimension {
        if limit < 64 {
            anyhow::bail!("max-dimension must be at least 64");
        }
        let (width, height) = img.dimensions();
        if width > limit || height > limit {
            img = img.resize(limit, limit, image::imageops::FilterType::Triangle);
        }
    }
    let encoded_format = match format {
        ImageFormat::Png => ImageOutputFormat::Png,
        ImageFormat::Jpeg => {
            let q = quality.unwrap_or(90);
            if !(1..=100).contains(&q) {
                anyhow::bail!("JPEG quality must be between 1 and 100");
            }
            ImageOutputFormat::Jpeg(q)
        }
        _ => anyhow::bail!("only PNG and JPEG encoding is supported"),
    };
    let original_bytes = std::fs::metadata(input)?.len();
    let mut encoded = std::io::Cursor::new(Vec::new());
    img.write_to(&mut encoded, encoded_format)?;
    let encoded = encoded.into_inner();
    if encoded.len() as u64 >= original_bytes {
        return Ok(None);
    }
    let mut target = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    target.write_all(&encoded)?;
    Ok(Some(encoded.len() as u64))
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
        report.tokens.add(&file_report.tokens);
        report.candidates.append(&mut file_report.candidates);
    }

    report.estimated_bytes_saved = report.original_bytes.saturating_sub(report.filtered_bytes);
    report.estimated_tokens_saved = report.tokens.tokens_saved;
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
        "- Prospective tokens saved (tiktoken-rs/o200k_base; exact text count): {}\n\n",
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
            "prospective {} tokens saved by command-output filtering (tiktoken-rs/o200k_base; exact text count)",
            discover.estimated_tokens_saved
        ));
    }
    Ok(EvalReport {
        tokens: discover.tokens.clone(),
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
        "- Prospective tokens saved (tiktoken-rs/o200k_base; exact text count): {}\n\n",
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
                "Session mining found {} rewritable command outputs with {} prospective tokens saved (tiktoken-rs/o200k_base; exact text count).",
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
        .get("tokens_saved")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if commands > 0 {
        recommendations.push(LearnRecommendation {
            title: "Keep command-output savings visible".to_string(),
            reason: format!(
                "`lm-resizer exec` history contains {commands} commands and {history_saved} measured tokens saved (tiktoken-rs/o200k_base; unmeasured legacy commands excluded)."
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
        "- Prospective discover tokens saved (tiktoken-rs/o200k_base; exact text count): {}\n",
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

#[cfg(unix)]
fn make_script_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
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
        write_managed_text_file(&path, &content, force, "hook helper")?;
        #[cfg(unix)]
        if name.ends_with(".sh") {
            make_script_executable(&path)?;
        }
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
        let content = match target.client.as_str() {
            "codex" => codex_native_hooks_json(&exe_path)?,
            "claude" => claude_native_hooks_json(&exe_path)?,
            client => serde_json::to_string_pretty(
                &agent_hooks::config(&exe_path, client).context("unknown hook schema")?,
            )?,
        };
        write_managed_text_file(&target.path, &content, force, "native hook config")?;
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
        "gemini" | "copilot" | "cursor" => Ok(vec![NativeHookTarget {
            client: client.into(),
            path: project_dir.join(match client {
                "gemini" => ".gemini/settings.json",
                "copilot" => ".github/hooks/lm-resizer.json",
                _ => ".cursor/hooks.json",
            }),
        }]),
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
            anyhow::bail!("unsupported native hook client '{other}'. Use codex, claude, gemini, copilot, cursor, or all")
        }
    }
}

fn codex_native_hooks_json(exe_path: &str) -> Result<String> {
    native_hooks_json(
        exe_path,
        "codex",
        &["PreToolUse", "PostToolUse"],
        "Bash|exec_command",
    )
}

fn claude_native_hooks_json(exe_path: &str) -> Result<String> {
    native_hooks_json(exe_path, "claude", &["PreToolUse", "PostToolUse"], "Bash")
}

/// One hook entry per event: PreToolUse rewrites supported commands through
/// `exec` (in-place output substitution — the active native role; never blocks:
/// unsupported commands emit nothing and run raw), PostToolUse records
/// command-output savings telemetry.
fn native_hooks_json(
    exe_path: &str,
    client: &str,
    events: &[&str],
    matcher: &str,
) -> Result<String> {
    let mut hooks = serde_json::Map::new();
    for event in events {
        let command = format!(
            "{} hook --client {client} --event {event}",
            agent_hooks::quote_program(exe_path)
        );
        let mut hook = json!({
            "type": "command",
            "command": command,
            "timeout": 30
        });
        if cfg!(windows) {
            hook["commandWindows"] = Value::String(format!(
                "powershell -NoProfile -Command \"& '{}' hook --client {client} --event {event}\"",
                exe_path.replace('\'', "''")
            ));
        }
        hooks.insert(
            (*event).to_string(),
            json!([{
                "matcher": matcher,
                "hooks": [hook]
            }]),
        );
    }
    let config = json!({ "hooks": hooks });
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
        #[cfg(unix)]
        make_script_executable(&path)?;
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
exec {exe} exec -- {original} "$@"
"#,
        exe = agent_hooks::quote_program(exe_path),
        original = agent_hooks::quote_program(&original.display().to_string())
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
    // Chemin cité par `quote_program` : un `$(...)` ou un guillemet dans un nom de dossier
    // ne doit pas être exécuté par le script.
    let exe_word = agent_hooks::quote_program(exe_path);
    format!(
        r#"#!/usr/bin/env sh
set -eu

[ -n "${{LM_RESIZER_BIN:-}}" ] || LM_RESIZER_BIN={exe_word}
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

Do not use lm-resizer when stdout or stderr is redirected, piped (including `| tee`),
captured by `$(...)` or backticks, fed by a here-document, or when the command is
interactive. `rewrite-shell` leaves those lines unchanged.
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
    // Instruction clients only: gemini/copilot/cursor read a native config, never
    // AGENTS.md/CLAUDE.md, so writing helpers alone would report a useless success.
    if matches!(client, "gemini" | "copilot" | "cursor") {
        anyhow::bail!(
            "install-hooks writes AGENTS.md/CLAUDE.md instructions, which {client} does not read. \
             Use `lm-resizer init-native-hooks --client {client}` instead"
        );
    }
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
    // Helpers are produced by install-hooks for instruction clients; native-only
    // clients (gemini/copilot/cursor) leave them alone so a mixed setup survives.
    let helpers_removed = if matches!(client, "codex" | "claude" | "claude-code" | "all") {
        uninstall_hook_helpers(&project_dir)?
    } else {
        Vec::new()
    };
    let native_files_removed = uninstall_native_hook_files(client, &project_dir)?;
    Ok(UninstallHooksReport {
        instruction_files: updated,
        removed,
        helpers_removed,
        native_files_removed,
    })
}

fn hook_instruction_targets(client: &str, project_dir: &Path) -> Result<Vec<PathBuf>> {
    match client {
        "codex" => Ok(vec![project_dir.join("AGENTS.md")]),
        "claude" | "claude-code" => Ok(vec![project_dir.join("CLAUDE.md")]),
        "gemini" | "copilot" | "cursor" => Ok(vec![]),
        "all" => Ok(vec![
            project_dir.join("AGENTS.md"),
            project_dir.join("CLAUDE.md"),
        ]),
        other => anyhow::bail!(
            "unsupported hook client '{other}'. Use codex, claude, gemini, copilot, cursor, or all"
        ),
    }
}

fn uninstall_hook_helpers(project_dir: &Path) -> Result<Vec<String>> {
    let hook_dir = project_dir.join(".lm-resizer").join("hooks");
    let mut removed = Vec::new();
    for name in ["rewrite.sh", "rewrite.ps1", "AGENT_RULES.md", "README.md"] {
        let path = hook_dir.join(name);
        if path.exists() {
            std::fs::remove_file(&path)?;
            removed.push(path.display().to_string());
        }
    }
    let _ = std::fs::remove_dir(&hook_dir);
    let lm = project_dir.join(".lm-resizer");
    if lm.is_dir() && std::fs::read_dir(&lm)?.next().is_none() {
        let _ = std::fs::remove_dir(&lm);
    }
    Ok(removed)
}

fn uninstall_native_hook_files(client: &str, project_dir: &Path) -> Result<Vec<String>> {
    let exe_path = std::env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "lm-resizer".to_string());
    let mut removed = Vec::new();
    for target in native_hook_targets(client, project_dir)? {
        if !target.path.exists() {
            continue;
        }
        let expected = match target.client.as_str() {
            "codex" => codex_native_hooks_json(&exe_path)?,
            "claude" => claude_native_hooks_json(&exe_path)?,
            name => serde_json::to_string_pretty(
                &agent_hooks::config(&exe_path, name).context("unknown hook schema")?,
            )?,
        };
        let existing = std::fs::read_to_string(&target.path)?;
        if existing != expected {
            // Leave a hand-edited or foreign config alone; uninstall stays safe.
            continue;
        }
        std::fs::remove_file(&target.path)?;
        removed.push(target.path.display().to_string());
        if let Some(parent) = target.path.parent() {
            let _ = std::fs::remove_dir(parent);
            if let Some(grand) = parent.parent() {
                if grand.ends_with(".github") {
                    let _ = std::fs::remove_dir(grand);
                }
            }
        }
    }
    Ok(removed)
}

/// Write `content` when missing; leave identical content alone; refuse a divergent
/// file unless `force` overwrites it. Creates parent directories as needed.
fn write_managed_text_file(path: &Path, content: &str, force: bool, kind: &str) -> Result<()> {
    if path.exists() {
        let existing = std::fs::read_to_string(path)?;
        if existing == content {
            return Ok(());
        }
        if !force {
            anyhow::bail!(
                "{kind} already exists: {} (rerun with --force to overwrite)",
                path.display()
            );
        }
    } else if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    Ok(())
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
    let mut next = existing.clone();
    if let Some(start) = existing.find(HOOK_BLOCK_START) {
        if let Some(end) = existing[start..].find(HOOK_BLOCK_END) {
            let end = start + end + HOOK_BLOCK_END.len();
            next.replace_range(start..end, block.trim());
            std::fs::write(path, next)?;
            return Ok(());
        }
    }
    next = next.trim_end().to_string();
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
                for entry in WalkDir::new(path) {
                    let entry = entry?;
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
        } else {
            anyhow::bail!(
                "session/log path does not exist or is not a file/directory: {}",
                path.display()
            );
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
    let tokens = TokenCounts::measure(output, &filtered);
    report.tokens.add(&tokens);
    report.estimated_tokens_saved = report.tokens.tokens_saved;
    if filter != "generic" || filtered.len() < output.len() {
        report.candidates.push(DiscoverCandidate {
            tokens,
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

fn record_proxy_history(
    provider: &str,
    path: &str,
    stats: &ProxyCompressionStats,
    upstream_status: Option<u16>,
) -> Result<()> {
    if std::env::var("LM_RESIZER_TRACKING").ok().as_deref() == Some("0") {
        return Ok(());
    }
    let dir = default_state_dir()?;
    create_private_dir_all(&dir)?;
    let history_path = dir.join("proxy-history.jsonl");
    // `bytes_saved` is measured (bytes in, bytes out). `provider_usage` is what
    // the provider reported. They are never added together.
    let record = json!({
        "timestamp_unix": unix_timestamp(),
        "provider": provider,
        "path": path,
        "input_bytes_saved": stats.bytes_saved,
        "upstream_status": upstream_status,
        "provider_usage": stats.provider_usage,
    });
    let line = serde_json::to_string(&record)?;
    use std::io::Write;
    let mut file = open_private_append(&history_path)?;
    writeln!(file, "{line}")?;
    Ok(())
}

fn summarize_proxy_history() -> Result<Value> {
    let path = default_state_dir()?.join("proxy-history.jsonl");
    if !path.exists() {
        return Ok(json!({
            "requests": 0,
            "provider_reported": {
                "requests_with_usage": 0,
                "input_tokens": 0,
                "output_tokens": 0,
                "cache_read_input_tokens": 0,
                "cache_creation_input_tokens": 0,
                "streams_incomplete": 0,
                "conventions": [],
            },
        }));
    }
    let content = std::fs::read_to_string(path)?;
    let mut requests = 0usize;
    let mut reported = json!({
        "requests_with_usage": 0u64, "input_tokens": 0u64, "output_tokens": 0u64,
        "cache_read_input_tokens": 0u64, "cache_creation_input_tokens": 0u64,
        "streams_incomplete": 0u64, "conventions": [],
    });
    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        requests += 1;
        if let Some(usage) = record.get("provider_usage").filter(|u| u.is_object()) {
            let add = |r: &mut Value, key: &str| {
                let n = usage.get(key).and_then(Value::as_u64).unwrap_or(0);
                r[key] = json!(r[key].as_u64().unwrap_or(0) + n);
            };
            reported["requests_with_usage"] =
                json!(reported["requests_with_usage"].as_u64().unwrap_or(0) + 1);
            for key in [
                "input_tokens",
                "output_tokens",
                "cache_read_input_tokens",
                "cache_creation_input_tokens",
            ] {
                add(&mut reported, key);
            }
            if usage.get("stream_completed").and_then(Value::as_bool) == Some(false) {
                reported["streams_incomplete"] =
                    json!(reported["streams_incomplete"].as_u64().unwrap_or(0) + 1);
            }
            if let Some(conv) = usage.get("convention").and_then(Value::as_str) {
                let list = reported["conventions"].as_array_mut().expect("array");
                if !list.iter().any(|c| c == conv) {
                    list.push(json!(conv));
                }
            }
        }
    }
    Ok(json!({
        "requests": requests,
        // Provider counters, summed as reported. Conventions differ: Anthropic
        // input excludes cache, OpenAI input includes cached tokens — do not
        // add them across conventions without reading `conventions`.
        "provider_reported": reported,
    }))
}

fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

const BUILTIN_EXEC_FILTERS_TOML: &str = r###"
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

# Job logs put the failure at the end. Keep failure lines and what follows
# them; the budget is paid with setup noise, never with `##[error]`.
[[filters]]
name = "gh-run-log"
match_command = "^gh\\s+run\\s+view\\b.*--log"
strip_ansi = true
keep_lines_matching = [
  "##\\[error\\]",
  "##\\[warning\\]",
  "exit code",
  "\\bFAIL\\b",
  "✘|✗|×",
  "Error",
  "error",
  "failed",
  "Failed",
  "Tests? ",
  "Assertion",
  "Expected",
  "Actual",
  "Received",
  ":line [0-9]+",
]
# The explanation of a failure usually sits *before* the final `##[error]`
# annotation, inside the step's group: `Failed X` / `Error Message:` then the
# message, Expected/Actual, a frame with file:line. A block opened on those
# headers is not cut by `##[group]`, which only frames the step.
keep_block_after_matching = [
  { start = "\\bFAIL\\s", until = "\\bFAIL\\s|⎯⎯⎯", max_lines = 40 },
  { start = "\\bFailed\\s+[A-Za-z_][A-Za-z0-9_.]*(\\s\\[|\\()", until = "\\bFailed\\s+[A-Za-z_][A-Za-z0-9_.]*(\\s\\[|\\()|\\b(Passed|Failed)!", max_lines = 60 },
  { start = "Error Message:", until = "\\bFailed\\s+[A-Za-z_][A-Za-z0-9_.]*(\\s\\[|\\()|\\b(Passed|Failed)!", max_lines = 60 },
  { start = "##\\[error\\]", until = "##\\[(group|endgroup)\\]", max_lines = 6 },
]
max_lines = 160
on_empty = "gh run log: no failure lines"

[[filters]]
name = "gh"
match_command = "^gh\\s+(pr|issue|run|workflow)\\b"
strip_ansi = true
strip_lines_matching = ["^\\s*$"]

[[filters]]
name = "go-test"
match_command = "^go\\s+test\\b"
strip_ansi = true
keep_block_after_matching = [
  { start = "^--- FAIL:", until = "^--- FAIL:|^FAIL|^ok\\s", max_lines = 40 },
]
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
# Frames of the runtime and the test framework say nothing about the failure.
# Only frames *without* a source location are stripped: a frame with a file and
# a line (`in /src/X.cs:line 12`) is kept whatever its namespace — tests of
# `Microsoft.Extensions.*` live under `Microsoft.` too.
strip_lines_matching = [
  "^\\s*at (System|Microsoft|Xunit|NUnit|InvokeStub_|Castle)[.A-Za-z_][^:]*$",
]
# A failed test is a header followed by its explanation: message, expected and
# actual values, stack trace. None of those lines carries a keyword.
keep_block_after_matching = [
  { start = "^\\s*Failed\\s+\\S", until = "^\\s*(Passed|Failed|Skipped)\\s+\\S|^\\[xUnit\\.net|^\\s*(Failed|Passed)!", max_lines = 60 },
]
# Diagnostics, not words: `-v n` prints hundreds of property lines such as
# `TreatWarningsAsErrors = false` that a bare "error" pattern keeps.
keep_lines_matching = [
  "\\[FAIL\\]",
  "FAILED",
  "Failed",
  "Error Message",
  "\\b(error|warning) [A-Z]{2,}[0-9]+",
  "\\b(error|warning)\\s*:",
  "^\\s*[0-9]+ (Warning|Error)\\(s\\)",
  "Unhandled exception",
  "aborted|crashed",
  "Passed!",
  "Failed!",
  "Total tests:",
  "Test Run Successful",
  "^\\s*Passed:\\s*[0-9]+",
  "Build FAILED",
  "Build succeeded",
]
max_lines = 180
on_empty = "dotnet: completed"

# `dotnet format --verify-no-changes` prints one diagnostic per line, then
# analyzer noise ("Running 154 analyzers") that is not a formatting fact.
[[filters]]
name = "dotnet-format"
match_command = "^dotnet\\s+format\\b|^dotnet-format\\b"
strip_ansi = true
keep_lines_matching = [
  "\\berror\\b",
  "\\bwarning\\b",
  "WHITESPACE",
  "Formatted ",
  "Format complete",
  "verify",
]
max_lines = 80
on_empty = "dotnet format: no changes"

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
  "^[^ ]+\\.(js|jsx|ts|tsx)$",
  "^\\s*[^\\s✓].*\\.(js|jsx|ts|tsx):[0-9]+(:[0-9]+)?",
  "^Route \\(app\\)",
  "^[┌├└]",
  "FAIL",
  "failed",
  "Failed",
  "Error",
  "error",
  "Warning",
  "warning",
  "✘",
  "×",
  "Expected",
  "Received",
  "Timeout",
  "timed out",
  "passed",
  "flaky",
  "skipped",
  "interrupted",
  "did not run",
  "Tests",
  "Duration",
  "Compiled",
]
# Playwright/Vitest failure sections: `  1) file:line › title` or `FAIL file > test`,
# then the message, Expected/Received, call log and code frame.
keep_block_after_matching = [
  { start = "^\\s+\\d+\\) \\S", until = "^\\s+\\d+\\) \\S|^\\s+\\d+ (failed|passed|flaky|skipped)", max_lines = 60 },
  { start = "^\\s*FAIL\\s", until = "^\\s*(FAIL|✓|Test Files)\\s|^⎯", max_lines = 40 },
  { start = "^\\s*❯\\s", until = "^\\s*❯\\s|^\\s*Test Files", max_lines = 40 },
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

# ruby-prisma-debut
# RSpec : l'échec est un en-tête numéroté, puis `expected:` / `got:` et le
# cadre `# ./fichier.rb:ligne`. `got:` n'est pas un mot du filtre ligne à
# ligne. `FF` et le chronomètre de chargement ne sont pas des faits.
[[filters]]
name = "rspec"
match_command = "^(rspec|bundle\\s+exec\\s+rspec|ruby\\s+-S\\s+rspec)(\\s|$)"
strip_ansi = true
strip_lines_matching = ["^\\s*$"]
keep_block_after_matching = [
  { start = "^\\s+\\d+\\) ", until = "^\\s+\\d+\\) |^Finished in |^Failed examples:|^\\d+ examples?", max_lines = 40 },
]
keep_lines_matching = [
  "^Failures:",
  "^\\d+ examples?,",
  "^Failed examples:",
  "^rspec\\s+\\S+:\\d+",
  "^Randomized with seed ",
]
max_lines = 160
on_empty = "rspec: no examples"

# Minitest : `N) Failure:` ou `N) Error:` puis `Classe#methode [fichier:ligne]`,
# `Expected:` et `Actual:`. `--seed` fixe l'ordre des échecs. `# Running:`,
# `FF` et le débit en runs/s ne sont pas des faits.
[[filters]]
name = "minitest"
match_command = "^ruby\\b.*(?:minitest|_test\\.rb)(?:\\s|$)|^rake\\s+(?:test|minitest)\\b|^rails\\s+test\\b|^bundle\\s+exec\\s+(?:rake\\s+(?:test|minitest)\\b|rails\\s+test\\b|ruby\\b.*(?:minitest|_test\\.rb))"
strip_ansi = true
strip_lines_matching = ["^\\s*$"]
keep_block_after_matching = [
  { start = "^\\s+\\d+\\) (?:Failure|Error|Skipped):", until = "^\\s+\\d+\\) (?:Failure|Error|Skipped):|^\\d+ runs,", max_lines = 50 },
]
keep_lines_matching = [
  "^Run options:",
  "^\\d+ runs,",
]
max_lines = 160
on_empty = "minitest: no runs"

# Prisma (migrate, generate, validate). L'en-tête `Error:` / `error:` ou un
# code P1xxx / P3xxx ouvre le bloc : le message et le cadre
# `schema.prisma:ligne` ne portent pas un mot-clé sur chaque ligne. Le
# préambule et « Prisma CLI Version » ne sont pas des faits.
[[filters]]
name = "prisma"
match_command = "^(?:prisma|npx(?:\\s+(?:--yes|-y))?\\s+prisma|bunx\\s+prisma|npm\\s+exec\\s+prisma|pnpm\\s+(?:exec\\s+)?prisma|yarn\\s+(?:exec\\s+)?prisma)(?:\\s|$)"
strip_ansi = true
strip_lines_matching = ["^\\s*$"]
keep_block_after_matching = [
  { start = "^(?:Error|error)\\b|\\bP[13][0-9]{3}\\b", until = "^Prisma CLI Version\\b|^Environment variables loaded\\b", max_lines = 80 },
]
keep_lines_matching = [
  "\\bP[13][0-9]{3}\\b",
  "^Validation Error Count:",
  "^\\[Context:",
  "^Database error",
  "^Migration name:",
  "^\\s*-->",
  "^\\s*\\d+\\s+\\|",
  "^\\s*\\|",
  "Generated Prisma Client",
  "in sync with your schema",
  "have been applied",
]
max_lines = 180
on_empty = "prisma: completed"
# ruby-prisma-fin
"###;

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
            "lm_resizer_tool_output".to_string(),
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
            "name": "lm_resizer_tool_output",
            "description": "Compress already captured output using the named command's filter; never execute that command.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "content": { "type": "string" },
                    "command": { "type": "string" },
                    "exit_code": { "type": "integer", "default": 0 },
                    "raw_on_failure": { "type": "boolean", "default": false },
                    "query": { "type": "string" }
                },
                "required": ["content", "command"]
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
        "lm_resizer_tool_output" => {
            let started = Instant::now();
            let content = args
                .get("content")
                .and_then(Value::as_str)
                .context("missing content")?;
            let command = args
                .get("command")
                .and_then(Value::as_str)
                .context("missing command")?;
            let parts = split_shell_words(command).context("invalid command quoting")?;
            if parts.is_empty() {
                anyhow::bail!("empty command");
            }
            let exit_code = args.get("exit_code").and_then(Value::as_i64).unwrap_or(0);
            let exit_code = i32::try_from(exit_code).context("exit_code out of range")?;
            let raw_on_failure = args
                .get("raw_on_failure")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let query = args.get("query").and_then(Value::as_str).unwrap_or("");
            let store = open_store(Some(store_path.to_path_buf()))?;
            let mut report = process_captured_output(
                &parts,
                content,
                exit_code,
                raw_on_failure,
                query,
                store.as_ref(),
            )?;
            attach_report_recovery(content, &mut report)?;
            record_exec_history(&report, started.elapsed())?;
            Ok(json!(report))
        }
        "lm_resizer_retrieve" => {
            let hash = args
                .get("hash")
                .and_then(Value::as_str)
                .context("missing hash")?;
            let store = open_store(Some(store_path.to_path_buf()))?;
            let (hash, content) = get_ccr_entry(store.as_ref(), hash)?;
            Ok(json!({ "hash": hash, "content": content }))
        }
        "lm_resizer_stats" => {
            let store = open_store(Some(store_path.to_path_buf()))?;
            Ok(json!({
                "entries": store.len(), "empty": store.is_empty(),
                "exec_history": summarize_exec_history()?,
                "retrieval_feedback": summarize_retrieval_feedback()?,
                "proxy_history": summarize_proxy_history()?,
            }))
        }
        _ => Err(anyhow::anyhow!("unknown tool: {name}")),
    })();

    match result {
        Ok(payload) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "isError": name == "lm_resizer_tool_output"
                    && payload["exit_code"].as_i64().is_some_and(|code| code != 0),
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

#[allow(clippy::too_many_arguments)] // Mirrors the CLI arguments at this boundary.
async fn wrap_agent(
    agent: String,
    args: Vec<String>,
    bind: SocketAddr,
    upstream: Option<String>,
    api_key: Option<String>,
    provider: ProviderKind,
    store: Option<PathBuf>,
    timeout_sec: Option<u64>,
    allow_non_loopback: bool,
) -> Result<()> {
    let proxy_url = format!("http://{bind}");
    let mut proxy = spawn_proxy(bind, upstream, api_key, provider, store, allow_non_loopback)?;
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
    allow_non_loopback: bool,
) -> Result<Child> {
    let exe = std::env::current_exe().context("could not resolve current executable")?;
    let mut cmd = proxy_command(
        &exe,
        bind,
        upstream,
        api_key,
        provider,
        store,
        allow_non_loopback,
    );
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());
    cmd.spawn().context("failed to start lm-resizer proxy")
}

/// Ligne de commande du proxy lancé par `wrap`. La clé d'API n'y figure jamais : `/proc/<pid>/cmdline`
/// est lisible par tous les comptes locaux, `/proc/<pid>/environ` seulement par le propriétaire.
/// Le fils la reçoit par `LM_RESIZER_API_KEY`, que `serve` lit déjà.
fn proxy_command(
    exe: &Path,
    bind: SocketAddr,
    upstream: Option<String>,
    api_key: Option<String>,
    provider: ProviderKind,
    store: Option<PathBuf>,
    allow_non_loopback: bool,
) -> Command {
    let mut cmd = Command::new(exe);
    cmd.arg("serve").arg("--bind").arg(bind.to_string());
    if allow_non_loopback {
        cmd.arg("--allow-non-loopback");
    }
    if let Some(upstream) = upstream {
        cmd.arg("--upstream").arg(upstream);
    }
    match api_key {
        Some(api_key) => {
            cmd.env("LM_RESIZER_API_KEY", api_key);
        }
        None => {
            cmd.env_remove("LM_RESIZER_API_KEY");
        }
    }
    // Le fichier de clé a déjà été lu par le parent ; le fils n'a pas à le relire.
    cmd.env_remove("LM_RESIZER_API_KEY_FILE");
    cmd.arg("--provider").arg(provider_label(provider));
    if let Some(store) = store {
        cmd.arg("--store").arg(store);
    }
    cmd
}

/// Clé d'API : `--api-key-file` l'emporte sur `--api-key` / `LM_RESIZER_API_KEY`. Le fichier
/// doit rester privé (0600) ; sa première ligne est la clé.
fn resolve_api_key(api_key: Option<String>, file: Option<PathBuf>) -> Result<Option<String>> {
    let Some(file) = file else {
        return Ok(api_key);
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&file)
            .with_context(|| format!("cannot read API key file {}", file.display()))?
            .permissions()
            .mode();
        if mode & 0o077 != 0 {
            anyhow::bail!(
                "API key file {} is accessible to other accounts (mode {:04o}); run `chmod 600 {}`",
                file.display(),
                mode & 0o7777,
                file.display()
            );
        }
    }
    let text = std::fs::read_to_string(&file)
        .with_context(|| format!("cannot read API key file {}", file.display()))?;
    let key = text.lines().next().unwrap_or("").trim().to_string();
    anyhow::ensure!(!key.is_empty(), "API key file {} is empty", file.display());
    Ok(Some(key))
}

/// Une clé donnée par `--api-key` reste dans la ligne de commande de CE processus, donc visible
/// par `ps` : le signaler plutôt que de laisser croire que l'aide masque aussi la ligne de commande.
fn warn_if_api_key_on_command_line() {
    let given = std::env::args_os().skip(1).any(|arg| {
        arg.to_str()
            .is_some_and(|a| a == "--api-key" || a.starts_with("--api-key="))
    });
    if given {
        eprintln!(
            "lm-resizer: --api-key est visible par tous les comptes locaux (ps, /proc). Préférez LM_RESIZER_API_KEY ou --api-key-file."
        );
    }
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
            if candidate.is_file() && program_is_executable(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

fn program_is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        rustix::fs::accessat(
            rustix::fs::CWD,
            path,
            rustix::fs::Access::EXEC_OK,
            rustix::fs::AtFlags::EACCESS,
        )
        .is_ok()
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
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
    allow_non_loopback: bool,
) -> Result<()> {
    let state = Arc::new(AppState {
        store_path: store.unwrap_or(default_store_path()?),
        upstream,
        api_key,
        provider,
        // Jamais de redirection : l'amont choisirait l'hôte qui reçoit `x-api-key`, que
        // reqwest ne retire pas quand l'origine change (audit du 08/10/2026).
        client: Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?,
        dashboard_enabled,
        // Hors boucle locale explicitement voulue, les noms d'hôte légitimes sont inconnus.
        host_guard: (!allow_non_loopback).then_some(bind),
    });
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
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_local_host,
        ))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(bind).await?;
    println!("lm-resizer listening on http://{bind}");
    axum::serve(listener, app).await?;
    Ok(())
}

fn ensure_loopback_bind(bind: SocketAddr, allow_non_loopback: bool) -> Result<()> {
    if bind.ip().is_loopback() || allow_non_loopback {
        return Ok(());
    }
    anyhow::bail!(
        "refusing to listen on {bind}: the proxy has no client authentication, so anyone who can reach it can spend the upstream API key; use a loopback address or pass --allow-non-loopback"
    )
}

/// Nom d'hôte d'un en-tête `Host` (sans port, crochets d'IPv6 retirés).
fn host_name(value: &str) -> &str {
    let value = value.trim();
    if let Some(rest) = value.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(rest);
    }
    value.rsplit_once(':').map_or(value, |(host, port)| {
        if port.chars().all(|c| c.is_ascii_digit()) {
            host
        } else {
            value
        }
    })
}

/// Refuse une requête dont `Host` n'est pas la boucle locale ou l'adresse d'écoute : une page
/// web qui rebinde son nom DNS vers 127.0.0.1 joindrait sinon le proxy depuis le navigateur.
async fn require_local_host(
    State(state): State<Arc<AppState>>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let Some(bind) = state.host_guard else {
        return next.run(request).await;
    };
    let host = request
        .headers()
        .get(axum::http::header::HOST)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .or_else(|| request.uri().authority().map(|a| a.to_string()));
    let allowed = host.as_deref().is_none_or(|host| {
        let name = host_name(host).to_ascii_lowercase();
        name == "localhost" || name == bind.ip().to_string() || {
            name.parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
        }
    });
    if allowed {
        next.run(request).await
    } else {
        (
            axum::http::StatusCode::MISDIRECTED_REQUEST,
            Json(json!({"error": "host not allowed: this proxy only answers on its loopback address"})),
        )
            .into_response()
    }
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
    let (hash, content) = get_ccr_entry(store.as_ref(), &hash)?;
    let _ = record_retrieval_feedback(&hash, content.len(), "http");
    Ok(Json(json!({ "hash": hash, "content": content })))
}

async fn http_stats(State(state): State<Arc<AppState>>) -> Result<Json<Value>, HttpError> {
    let store = open_store(Some(state.store_path.clone()))?;
    Ok(Json(json!({
        "entries": store.len(), "empty": store.is_empty(),
        "exec_history": summarize_exec_history()?,
        "retrieval_feedback": summarize_retrieval_feedback()?,
        "proxy_history": summarize_proxy_history()?,
    })))
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
        .get("tokens_saved")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let legacy_estimate = exec_history
        .get("estimated_tokens_saved")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let unmeasured = exec_history
        .get("unmeasured_commands")
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
<div class="card"><div>Tokens saved (tiktoken-rs/o200k_base; measured)</div><div class="metric">{tokens_saved}</div></div>
<div class="card"><div>Legacy estimated tokens saved (bytes / 4; {unmeasured} unmeasured commands)</div><div class="metric">{legacy_estimate}</div></div>
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
            // Bytes go to the client untouched; usage events are read on the
            // way and recorded when the stream ends — or when the client drops
            // it, marked incomplete.
            let provider_owned = provider_label(state.provider).to_string();
            let path_owned = path.to_string();
            let mut stats_for_record = stats.clone();
            let status_code = status.as_u16();
            let stream = provider_usage::UsageTap::new(
                Box::pin(response.bytes_stream()),
                Box::new(move |usage, _completed| {
                    stats_for_record.provider_usage = usage;
                    let _ = record_proxy_history(
                        &provider_owned,
                        &path_owned,
                        &stats_for_record,
                        Some(status_code),
                    );
                }),
            );
            return builder
                .body(Body::from_stream(stream))
                .map_err(|err| HttpError(anyhow::anyhow!(err)));
        }

        let response_headers = response.headers().clone();
        let response_bytes = response.bytes().await?;
        let decoded_response = decode_http_body(&response_headers, &response_bytes)?;
        let mut value = serde_json::from_slice::<Value>(&decoded_response)?;
        // The provider's own `usage` stays where it is, untouched; a copy goes
        // into our side record, clearly separated from any estimate.
        stats.provider_usage = provider_usage::from_response(&value);
        let _ = record_proxy_history(
            provider_label(state.provider),
            path,
            &stats,
            Some(status.as_u16()),
        );
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                "lm_resizer".to_string(),
                serde_json::to_value(&stats).unwrap_or_else(|_| json!({})),
            );
        }
        if !status.is_success() {
            // Relay the provider's status and error body. Folding every
            // upstream failure into a 400 hid what an agent must react to: a
            // 401 (bad key), 429 (rate limit) or 529 (overloaded) are not the
            // same failure, and none of them is a bad request from the client.
            let code = axum::http::StatusCode::from_u16(status.as_u16())
                .unwrap_or(axum::http::StatusCode::BAD_GATEWAY);
            return Ok((code, Json(value)).into_response());
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
        TungsteniteMessage::Binary(bytes) => Some(WsMessage::Binary(bytes)),
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

// Gregorian years repeat every 400 years. Locate the cycle relative to the
// Unix epoch, then walk its at most 400 years and twelve calendar months.
fn civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let mut year = 1970 + 400 * days_since_epoch.div_euclid(146_097);
    let mut offset = days_since_epoch.rem_euclid(146_097);
    let leap = |y: i64| y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    loop {
        let length = if leap(year) { 366 } else { 365 };
        if offset < length {
            break;
        }
        offset -= length;
        year += 1;
    }
    let months = [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    for (index, length) in months.into_iter().enumerate() {
        if offset < length {
            return (year, index as i64 + 1, offset + 1);
        }
        offset -= length;
    }
    unreachable!("day must belong to a calendar month")
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

#[derive(Default, Debug, Clone, Serialize)]
struct ProxyCompressionStats {
    fields_seen: usize,
    fields_compressed: usize,
    original_bytes: usize,
    compressed_bytes: usize,
    bytes_saved: usize,
    cache_keys: Vec<String>,
    provider_cache_policy: String,
    /// What the provider itself reported in `usage`, verbatim and normalised.
    /// Absent when there was no upstream, or the provider sent no usage.
    /// Never derived from an estimate, never folded into one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provider_usage: Option<provider_usage::ProviderUsage>,
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
    fn powershell_hook_rewrites_the_actual_cmd_field() {
        let event = json!({"tool_name":"exec_command", "tool_input":{"cmd":"git status", "shell":"powershell.exe", "description":"keep"}});
        let result =
            pretooluse_rewrite_json(&event, "C:\\program files\\lm-resizer.exe", "PreToolUse")
                .unwrap();
        let input = &result["hookSpecificOutput"]["updatedInput"];
        assert_eq!(
            input["cmd"],
            "& 'C:\\program files\\lm-resizer.exe' exec -- git status"
        );
        assert!(input.get("command").is_none());
        assert_eq!(input["description"], "keep");
    }
    #[test]
    fn pipeline_guard_requires_stream_provenance_even_without_error_keywords() {
        let before = "ordinary stdout\n[stderr]\nordinary stderr\n";
        let after = "ordinary stdout\nordinary stderr\n";
        assert_eq!(
            first_lost_indispensable_line(before, after, "generic"),
            Some("[stderr]")
        );
    }

    #[test]
    fn filter_git_status_keeps_untracked_file_named_use() {
        let input = "On branch main\nUntracked files:\n  (use \"git add <file>...\" to include in what will be committed)\n\tuse cases.md\n\tuse-me.txt\n\nnothing added to commit but untracked files present (use \"git add\" to track)\n";
        let filtered = command_views::filter(&["git".into(), "status".into()], input)
            .unwrap()
            .1;
        assert!(filtered.contains("use cases.md"));
        assert!(filtered.contains("use-me.txt"));
    }

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
    fn split_shell_words_keeps_empty_quoted_argument() {
        assert_eq!(
            split_shell_words("grep -rn \"\" src").unwrap(),
            vec!["grep", "-rn", "", "src"]
        );
        let report = rewrite_shell_report("grep -rn \"\" src");
        assert!(
            report.rewritten.contains("grep -rn \"\" src"),
            "Rewritten command does not contain empty quotes: {}",
            report.rewritten
        );
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
    fn indispensable_gate_matches_original_scan() {
        // Reference the old scan: trim for listings/search, exact for diff.
        let inputs = [
            "",
            "a\nb\n",
            " a \n\tb\n\n",
            "a\na\nb",
            "é\r\n中\n",
            "+x\n-y\n+++z\n---z\n",
            "ERROR: fail\ncontext\n",
        ];
        for before in inputs {
            for after in inputs {
                for filter in [
                    "listing",
                    "search_results",
                    "diff_summary",
                    "git_show",
                    "none",
                ] {
                    let reference = first_lost_failure_line(before, after).or_else(|| {
                        if matches!(filter, "listing" | "search_results") {
                            before
                                .lines()
                                .filter(|l| !l.trim().is_empty())
                                .find(|l| !after.lines().any(|r| r.trim() == l.trim()))
                        } else if matches!(filter, "diff_summary" | "git_show") {
                            before.lines().find(|l| {
                                ((l.starts_with('+') && !l.starts_with("+++"))
                                    || (l.starts_with('-') && !l.starts_with("---")))
                                    && !after.lines().any(|r| r == *l)
                            })
                        } else {
                            None
                        }
                    });
                    assert_eq!(
                        first_lost_indispensable_line(before, after, filter),
                        reference,
                        "{filter}: {before:?} -> {after:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn exec_generic_filter_preserves_repeated_lines_visibly() {
        let raw = "same\n".repeat(100) + "next\n";
        let filtered = filter_generic(&raw);
        assert_eq!(filtered, raw);
    }

    #[test]
    fn exec_search_filter_groups_matches_by_file() {
        let raw = "src/a.rs:1:match one\nsrc/a.rs:2:match two\nsrc/a.rs:3:match three\nsrc/b.rs:4:match four\n";
        let filtered = file_views::search(raw);
        assert!(filtered.contains("[file] src/a.rs (3)"));
        assert!(filtered.contains("[file] src/b.rs (1)"));
        assert!(filtered.contains("1: match one"));
        assert!(filtered.contains("3: match three"));
        assert!(!filtered.contains("omitted"));
    }

    #[test]
    fn exec_search_filter_context_before() {
        let raw = "g/a.txt-1-before one\ng/a.txt:2:NEEDLE one\n";
        let filtered = file_views::search(raw);

        let pos_before = filtered.find("before one").expect("before one not found");
        let pos_needle = filtered.find("NEEDLE one").expect("needle not found");

        assert!(pos_before < pos_needle, "before should be before needle");

        assert!(filtered.contains("g/a.txt:2:NEEDLE one"));
        assert!(filtered.contains("g/a.txt-1-before one"));
        assert!(filtered.contains("g/a.txt:2:NEEDLE one"));
    }

    #[test]
    fn exec_search_filter_context_with_numeric_filename() {
        let file = "docs/compression-native-2026-09-22.md";
        let raw = format!("{file}-4-before-7-token\n{file}:5:NEEDLE\n{file}-6-after-8-token\n");
        let filtered = file_views::search(&raw);

        assert!(filtered.contains(&format!("{file}:5:NEEDLE")));
        assert!(!filtered.contains("docs/compression-native: 0 matches"));
        let before = filtered.find("-4-before-7-token").expect("before context");
        let hit = filtered.find(":5:NEEDLE").expect("match");
        let after = filtered.find("-6-after-8-token").expect("after context");
        assert!(before < hit && hit < after);
    }

    #[test]
    fn exec_search_filter_context_and_groups() {
        let raw = "g/a.txt:2:NEEDLE one\ng/a.txt-3-after one\n--\ng/b.txt:2:NEEDLE two\ng/b.txt-3-after two\n";
        let filtered = file_views::search(raw);

        let pos_after_one = filtered.find("after one").expect("after one not found");
        let pos_b_header = filtered.find("g/b.txt:").expect("b header not found");
        let pos_after_two = filtered.find("after two").expect("after two not found");

        assert!(
            pos_after_one < pos_b_header,
            "after one should be before b header"
        );
        assert!(
            pos_b_header < pos_after_two,
            "b header should be before after two"
        );

        assert!(filtered.contains("g/a.txt:2:NEEDLE one"));
        assert!(filtered.contains("g/b.txt:2:NEEDLE two"));
        assert!(filtered.contains("-3-after one"));
        assert!(filtered.contains("-3-after two"));
        assert!(filtered.contains("\n--\n"));
        let pos_separator = filtered.find("\n--\n").expect("separator not found");
        assert!(pos_after_one < pos_separator && pos_separator < pos_b_header);
    }

    #[test]
    fn listing_filter_keeps_paths_after_previous_line_limit() {
        let raw = (0..150)
            .map(|n| format!("src/file-{n}.rs\n"))
            .collect::<String>();
        let output = filter_listing(&raw);
        assert!(output.contains("src/file-149.rs"));
        assert_eq!(output.lines().count(), 150);
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
        assert_eq!(filter, "native:git-status");

        let (filter, _text) =
            filter_command_output(&["cargo".into(), "test".into()], "test result: ok\n");
        assert_eq!(filter, "native:cargo-test");

        let (filter, _text) =
            filter_command_output(&["rg".into(), "needle".into()], "src/main.rs:1:needle\n");
        assert_eq!(filter, "native:grep");
    }

    #[test]
    fn rewrite_reports_supported_command() {
        let report = rewrite_command_report(&["git".into(), "status".into()]);
        assert!(report.supported);
        assert_eq!(report.filter, "native:git-status");
        assert_eq!(
            report.rewritten.as_deref(),
            Some("lm-resizer exec -- git status")
        );
    }

    #[test]
    fn rewrite_leaves_generic_command_unsupported() {
        let report = rewrite_command_report(&["unknown-tool".into(), "arg".into()]);
        assert!(!report.supported);
        assert_eq!(report.filter, "lossless:generic");
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
        // Renversé le 2026-10-04. Ce test figeait le défaut : il exigeait
        // `lm-resizer exec -- git status > status.txt`. Le shell redirigeait alors
        // la vue réduite dans le fichier (67 octets au lieu du diff, `git apply` cassé).
        let report = rewrite_shell_report("git status > status.txt");
        assert!(!report.changed, "{}", report.rewritten);
        assert!(report.rewrites.is_empty());
        assert_eq!(report.rewritten, "git status > status.txt");
    }

    #[test]
    fn rewrite_shell_handles_redirect_with_descriptor() {
        // Renversé le 2026-10-04. L'ancien test recollait `2>`, `&>`, `>&` et le tube
        // sur `lm-resizer exec`. Ces formes restent la commande d'origine.
        // `git log &` n'est pas une redirection : la sortie va encore au terminal, il reste réécrit.
        for command in [
            "cargo test 2>&1 | tail -5",
            "git status 2>/dev/null",
            "git status 2>&1",
            "git diff >out.txt 2>&1",
            "git status &> /dev/null",
            "git status >&2",
        ] {
            let report = rewrite_shell_report(command);
            assert_eq!(report.rewritten, command, "ne doit pas réécrire {command}");
            assert!(!report.changed);
        }

        let background = rewrite_shell_report("git log &");
        assert_eq!(background.rewritten, "lm-resizer exec -- git log &");
    }

    #[test]
    fn rewrite_shell_does_not_rewrite_pipe_consumer() {
        // Le producteur non plus : `grep` recevrait la vue réduite. Ancien gel :
        // `lm-resizer exec -- git status | grep modified` (un seul segment réécrit).
        let report = rewrite_shell_report("git status | grep modified");
        assert_eq!(report.rewritten, "git status | grep modified");
        assert!(report.rewrites.is_empty());
        assert!(!report.changed);
    }

    #[test]
    fn rewrite_shell_leaves_captured_or_interactive_output_unchanged() {
        // Doit échouer sur l'ancienne logique (suffixe recollé, producteur de tube réécrit).
        for command in [
            "git diff > p.diff",
            "git diff >> p.diff",
            "git status 2> err.txt",
            "git status 2>> err.txt",
            "git diff &> both.txt",
            "git diff >out.txt 2>&1",
            "git diff | tee p.diff",
            "git diff | tee -a p.diff",
            "git status | head -n 5",
            "git diff |& tee p.diff",
            "cat <<'EOF'",
            "tee note.txt <<'EOF'",
            "git apply <<'PATCH'",
            "git status $(echo --short)",
            "git log -1 --oneline `git rev-parse --short HEAD`",
            "git diff --stat $(echo --numstat)",
            "x=$(git status --short)",
            "psql",
            "psql -h localhost",
            "git rebase -i HEAD",
            "git -C /tmp rebase -i HEAD",
            "git add -p",
            "git add --patch",
            "git commit",
            "git mergetool",
            "vim README",
            "less README",
            "docker exec -it box sh",
            "ssh host",
            "python",
            "node",
            "watch git status",
        ] {
            let report = rewrite_shell_report(command);
            assert_eq!(
                report.rewritten, command,
                "sortie captée ou interactive réécrite: {command} -> {}",
                report.rewritten
            );
            assert!(!report.changed);
            assert!(
                rewrite_command_for_hook(command, "/opt/lm").is_none(),
                "le hook ne partage pas le refus: {command}"
            );
        }

        // Apostrophes : `$(...)` est littéral, mais `shell_join` le remettrait entre
        // guillemets doubles et en ferait une vraie substitution. On ne réécrit pas.
        // Le hook, lui, recolle les octets d'origine : il peut envelopper.
        let literal = rewrite_shell_report("git status '$(echo --short)'");
        assert_eq!(literal.rewritten, "git status '$(echo --short)'");
        assert_eq!(
            rewrite_command_for_hook("git status '$(echo --short)'", "/opt/lm").as_deref(),
            Some("\"/opt/lm\" exec -- git status '$(echo --short)'")
        );

        // Segments sûrs d'une ligne mixte : seule la partie dont la sortie va au modèle.
        let mixed = rewrite_shell_report("git status && git diff > p.diff");
        assert_eq!(
            mixed.rewritten,
            "lm-resizer exec -- git status && git diff > p.diff"
        );
        let mixed_first = rewrite_shell_report("git diff > p.diff && git status");
        assert_eq!(
            mixed_first.rewritten,
            "git diff > p.diff && lm-resizer exec -- git status"
        );

        // psql scripté et commande simple : toujours réécrits. Le hook enveloppe tel quel.
        let scripted = rewrite_shell_report("psql -c 'select 1'");
        // Les octets d'origine sont recollés : plus de re-citation en guillemets doubles.
        assert_eq!(scripted.rewritten, "lm-resizer exec -- psql -c 'select 1'");
        assert_eq!(
            rewrite_command_for_hook("psql -c 'select 1'", "/opt/lm").as_deref(),
            Some("\"/opt/lm\" exec -- psql -c 'select 1'")
        );
        let plain = rewrite_shell_report("git status");
        assert_eq!(plain.rewritten, "lm-resizer exec -- git status");
    }

    #[test]
    fn split_shell_words_respects_quotes() {
        assert_eq!(
            split_shell_words("git commit -m \"hello world\"").unwrap(),
            vec!["git", "commit", "-m", "hello world"]
        );
    }

    #[test]
    fn captured_output_preserves_failure_and_recovers_raw_success() {
        let store = InMemoryCcrStore::default();
        let raw = "PASS [ 0.01s] app::ok\nFAIL [ 0.02s] app::bad\npanicked at src/lib.rs:42:9\nSummary: 1 failed\n";
        let command = vec!["cargo".into(), "nextest".into(), "run".into()];
        let success = process_captured_output(&command, raw, 0, false, "", &store).unwrap();
        assert_eq!(success.filter, "cargo-nextest");
        assert!(success.output.contains("app::bad"));
        assert!(success.output.contains("src/lib.rs:42:9"));
        assert!(success.output.len() <= raw.len());
        if success.output != raw {
            let key = success.cache_keys.last().expect("raw recovery key");
            assert_eq!(store.get(key).as_deref(), Some(raw));
        }
        let failure = process_captured_output(&command, raw, 1, true, "", &store).unwrap();
        assert_eq!(failure.filter, "raw_on_failure");
        assert_eq!(failure.output, raw);
    }

    #[test]
    fn empty_test_filter_cannot_erase_a_shell_failure() {
        let store = InMemoryCcrStore::default();
        let raw = "\n> @typescript/repo@0.0.0 test\n> hereby test\n\nsh: 1: hereby: not found\n";
        let command = vec!["pnpm".into(), "test".into()];
        let report = process_captured_output(&command, raw, 127, false, "", &store).unwrap();
        assert!(report.output.contains("> hereby test"));
        assert!(report.output.contains("sh: 1: hereby: not found"));
        assert_eq!(report.exit_code, 127);
    }

    #[test]
    fn mcp_tool_output_is_listed_and_filters_without_executing() {
        let tools = mcp_tools();
        let names: Vec<&str> = tools
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        assert!(names.contains(&"lm_resizer_tool_output"));

        let temp = tempfile::tempdir().unwrap();
        let response = handle_mcp_tool_call(
            json!(1),
            json!({"name":"lm_resizer_tool_output","arguments":{
                "command":"cargo test","content":"test result: ok. 2 passed; 0 failed; 0 ignored; finished in 0.01s\n"
            }}),
            &temp.path().join("ccr.sqlite"),
        );
        assert!(response.get("error").is_none(), "{response}");
        let payload: Value =
            serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(payload["command"], "cargo test");
        assert!(payload["output"].as_str().unwrap().contains("2 passed"));

        let failed = handle_mcp_tool_call(
            json!(2),
            json!({"name":"lm_resizer_tool_output","arguments":{
                "command":"cargo test","content":"error: compilation failed\n","exit_code":1
            }}),
            &temp.path().join("ccr.sqlite"),
        );
        let failure: Value =
            serde_json::from_str(failed["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert!(failure["output"]
            .as_str()
            .unwrap()
            .starts_with("[FAIL] Command failed (exit code: 1)\nerror: compilation failed\n"));

        let misleading = handle_mcp_tool_call(
            json!(3),
            json!({"name":"lm_resizer_tool_output","arguments":{
                "command":"unknown-tool","content":"progress\nall tests passed\nOK\n","exit_code":7
            }}),
            &temp.path().join("ccr.sqlite"),
        );
        let payload: Value =
            serde_json::from_str(misleading["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(misleading["result"]["isError"], true);
        assert_eq!(payload["exit_code"], 7);
        assert_eq!(payload["filter"], "generic:summary");
        assert!(payload["output"]
            .as_str()
            .unwrap()
            .starts_with("[FAIL] Command failed (exit code: 7)\n"));
    }

    #[test]
    fn image_reencoding_preserves_dimensions_and_high_contrast_marks() {
        use image::GenericImageView;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "lm-resizer-image-test-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source.jpg");
        let output = dir.join("smaller.jpg");
        let mut pixels = image::RgbImage::new(512, 512);
        for y in 0..512 {
            for x in 0..512 {
                let value = if x % 40 < 5 { 0 } else { 255 };
                pixels.put_pixel(x, y, image::Rgb([value, value, value]));
            }
        }
        let mut encoded = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(pixels)
            .write_to(&mut encoded, image::ImageOutputFormat::Jpeg(100))
            .unwrap();
        std::fs::write(&source, encoded.into_inner()).unwrap();
        let smaller = encode_smaller_image(&source, &output, None, Some(80)).unwrap();
        assert!(smaller.is_some());
        let decoded = image::open(&output).unwrap();
        assert_eq!(decoded.dimensions(), (512, 512));
        assert!(decoded.get_pixel(2, 200).0[0] < 70);
        assert!(decoded.get_pixel(20, 200).0[0] > 200);
        assert!(describe_image(&output).unwrap().contains("near-grayscale"));
        assert!(encode_smaller_image(&source, &output, None, Some(80)).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn cmd(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    // Sortie réelle `dotnet test` (SDK 10.0.300, xUnit 2.9.3), abrégée.
    const DOTNET_TEST_ECHEC: &str = "[xUnit.net 00:00:00.21]     Calc.Tests.CalculTests.Somme_grands [FAIL]\n  Failed Calc.Tests.CalculTests.Somme_grands [< 1 ms]\n  Error Message:\n   Assert.Equal() Failure: Values differ\nExpected: 300\nActual:   301\n  Stack Trace:\n     at Calc.Tests.CalculTests.Somme_grands() in /src/CalculTests.cs:line 22\n   at System.Reflection.MethodBaseInvoker.InterpretedInvoke_Method(Object obj, IntPtr* args)\n   at InvokeStub_CalculTests.Somme(Object, Span`1)\n  Passed Calc.Tests.CalculTests.Autre [1 ms]\nFailed!  - Failed:     1, Passed:    43, Skipped:     1, Total:    45\n";

    #[test]
    fn dotnet_test_garde_l_explication_de_l_echec_et_ecarte_les_cadres_du_runtime() {
        let (filter, out) = route_command_filter(&cmd(&["dotnet", "test"]), DOTNET_TEST_ECHEC);
        assert_eq!(filter, "toml:dotnet");
        for attendu in [
            "Expected: 300",
            "Actual:   301",
            "Assert.Equal() Failure",
            "CalculTests.cs:line 22",
            "Failed!  - Failed:     1",
        ] {
            assert!(out.contains(attendu), "{attendu} absent de:\n{out}");
        }
        assert!(!out.contains("System.Reflection"), "{out}");
        assert!(!out.contains("InvokeStub_"), "{out}");
    }

    #[test]
    fn dotnet_build_verbeux_ne_garde_pas_les_proprietes_qui_contiennent_error() {
        let raw = "  TreatWarningsAsErrors = false\n  MSBuildWarningsAsErrors = \n/src/A.cs(12,56): error CS0103: The name 'x' does not exist\nBuild FAILED.\n    0 Warning(s)\n    1 Error(s)\n";
        let (_, out) = route_command_filter(&cmd(&["dotnet", "build", "-v", "n"]), raw);
        assert!(out.contains("error CS0103"));
        assert!(out.contains("1 Error(s)"));
        assert!(!out.contains("TreatWarningsAsErrors"), "{out}");
    }

    #[test]
    fn playwright_garde_les_echecs_et_pas_les_reussites() {
        let raw = "  ✓   1 tests/a.spec.js:4:3 › visible (10ms)\n  ✘   2 tests/a.spec.js:6:1 › total juste (5.0s)\n\n  1) tests/a.spec.js:6:1 › total juste ───\n\n    Error: expect(locator).toHaveText(expected) failed\n\n    Locator:  locator('#total')\n    Expected: \"42,90\"\n    Received: \"41,90\"\n       8 |   await expect(x).toHaveText('42,90');\n        at /p/tests/a.spec.js:8:40\n\n  1 failed\n  1 passed (9.1s)\n";
        let (_, out) = route_command_filter(&cmd(&["playwright", "test"]), raw);
        for attendu in [
            "✘   2 tests/a.spec.js:6:1",
            "Expected: \"42,90\"",
            "Received: \"41,90\"",
            "locator('#total')",
            "a.spec.js:8:40",
            "1 failed",
            "1 passed",
        ] {
            assert!(out.contains(attendu), "{attendu} absent de:\n{out}");
        }
        assert!(!out.contains("✓"), "{out}");
    }

    /// Extrait du TRX réel du 23/09 (SDK 10.0.300, xUnit 2.9.3), pas une
    /// reconstitution : nom, message, Expected/Actual, pile `fichier:ligne`.
    const TRX_REEL: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<TestRun xmlns="http://microsoft.com/schemas/VisualStudio/TeamTest/2010">
  <Results>
    <UnitTestResult testName="Calc.Tests.CalculTests.Passe_01" outcome="Passed" />
    <UnitTestResult testName="Calc.Tests.CalculTests.Liste_contient_element" outcome="Failed">
      <Output><ErrorInfo>
        <Message>Assert.Contains() Failure: Item not found in collection
Collection: ["alpha", "beta"]
Not found:  "gamma"</Message>
        <StackTrace>   at Calc.Tests.CalculTests.Liste_contient_element() in /tmp/work/src/Calc.Tests/CalculTests.cs:line 32
   at System.Reflection.MethodBaseInvoker.InvokeWithNoArgs(Object obj, BindingFlags invokeAttr)</StackTrace>
      </ErrorInfo></Output>
    </UnitTestResult>
    <UnitTestResult testName="Calc.Tests.CalculTests.Somme_theorie(a: 200, b: 1, attendu: 201)" outcome="Failed">
      <Output><ErrorInfo>
        <Message>Assert.Equal() Failure: Values differ
Expected: 201
Actual:   202</Message>
        <StackTrace>   at Calc.Tests.CalculTests.Somme_theorie(Int32 a, Int32 b, Int32 attendu) in /tmp/work/src/Calc.Tests/CalculTests.cs:line 26</StackTrace>
      </ErrorInfo></Output>
    </UnitTestResult>
    <UnitTestResult testName="Calc.Tests.CalculTests.Division_par_zero_leve" outcome="Failed">
      <Output><ErrorInfo>
        <Message>System.DivideByZeroException : Attempted to divide by zero.</Message>
        <StackTrace>   at Calc.Tests.Calcul.Diviser(Int32 a, Int32 b) in /tmp/work/src/Calc.Tests/CalculTests.cs:line 8
   at Calc.Tests.CalculTests.Division_par_zero_leve() in /tmp/work/src/Calc.Tests/CalculTests.cs:line 20</StackTrace>
      </ErrorInfo></Output>
    </UnitTestResult>
  </Results>
  <ResultSummary outcome="Failed">
    <Counters total="18" passed="13" failed="4" />
  </ResultSummary>
</TestRun>"#;

    #[test]
    fn trx_reel_garde_le_nom_le_message_et_la_pile() {
        let (filter, out) = route_command_filter(&cmd(&["dotnet", "test"]), TRX_REEL);
        assert_eq!(filter, "structured:trx");
        for fact in [
            "total=\"18\"",
            "passed=\"13\"",
            "failed=\"4\"",
            "Calc.Tests.CalculTests.Liste_contient_element",
            "Collection: [\"alpha\", \"beta\"]",
            "Not found:  \"gamma\"",
            "CalculTests.cs:line 32",
            "Calc.Tests.CalculTests.Somme_theorie(a: 200, b: 1, attendu: 201)",
            "Expected: 201",
            "Actual:   202",
            "CalculTests.cs:line 26",
            "System.DivideByZeroException : Attempted to divide by zero.",
            "CalculTests.cs:line 8",
            "CalculTests.cs:line 20",
        ] {
            assert!(out.contains(fact), "{fact} absent de:\n{out}");
        }
        assert!(!out.contains("Passe_01"), "{out}");
        assert!(!out.contains("MethodBaseInvoker"), "{out}");
    }

    #[test]
    fn binlog_relie_par_le_chemin_bl_garde_code_et_colonne() {
        let dir = std::env::temp_dir().join(format!("lm-binlog-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("build.binlog");
        let bytes = binlog_minimal_cs0103();
        std::fs::write(&path, &bytes).expect("écrire le binlog");
        let (filter, out) = route_command_filter(
            &cmd(&["dotnet", "build", &format!("-bl:{}", path.display())]),
            "Build FAILED.\n",
        );
        assert_eq!(filter, "structured:binlog");
        for fact in [
            "CS0103",
            "The name 'inconnu' does not exist in the current context",
            "(2,19)",
            "CS0219",
            "(1,5)",
        ] {
            assert!(out.contains(fact), "{fact} absent de:\n{out}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn binlog_minimal_cs0103() -> Vec<u8> {
        fn i32(buf: &mut Vec<u8>, value: i32) {
            buf.extend(value.to_le_bytes());
        }
        fn i7(buf: &mut Vec<u8>, mut value: u32) {
            loop {
                let mut byte = (value & 0x7f) as u8;
                value >>= 7;
                if value != 0 {
                    byte |= 0x80;
                }
                buf.push(byte);
                if value == 0 {
                    break;
                }
            }
        }
        fn string_rec(buf: &mut Vec<u8>, text: &str) {
            i7(buf, 24);
            i7(buf, text.len() as u32);
            buf.extend(text.as_bytes());
        }
        fn diag(
            buf: &mut Vec<u8>,
            kind: u32,
            message: u32,
            code: u32,
            file: u32,
            line: u32,
            col: u32,
        ) {
            let mut body = Vec::new();
            i7(&mut body, 4);
            i7(&mut body, message);
            i7(&mut body, 2);
            i7(&mut body, 1);
            i7(&mut body, code);
            i7(&mut body, file);
            i7(&mut body, 13);
            i7(&mut body, line);
            i7(&mut body, col);
            i7(buf, kind);
            i7(buf, body.len() as u32);
            buf.extend(body);
        }
        let mut buf = Vec::new();
        i32(&mut buf, 25);
        i32(&mut buf, 18);
        string_rec(
            &mut buf,
            "The name 'inconnu' does not exist in the current context",
        );
        string_rec(&mut buf, "CS0103");
        string_rec(&mut buf, "/tmp/work/src/BuildFail/Program.cs");
        string_rec(&mut buf, "/tmp/work/src/BuildFail/BuildFail.csproj");
        string_rec(
            &mut buf,
            "The variable 'jamaisUtilisee' is assigned but its value is never used",
        );
        string_rec(&mut buf, "CS0219");
        diag(&mut buf, 9, 10, 11, 12, 2, 19);
        diag(&mut buf, 10, 14, 15, 12, 1, 5);
        i7(&mut buf, 0);
        buf
    }

    #[test]
    fn dotnet_format_json_garde_le_fichier_et_le_diagnostic() {
        let raw = r#"[{"FilePath":"/tmp/work/src/FormatMe/Program.cs","FileChanges":[{"LineNumber":1,"CharNumber":15,"DiagnosticId":"WHITESPACE","FormatDescription":"Fix whitespace formatting. Insert '\\n'."},{"LineNumber":5,"CharNumber":28,"DiagnosticId":"WHITESPACE","FormatDescription":"Fix whitespace formatting. Delete 1 characters."}]}]"#;
        let (filter, out) =
            route_command_filter(&cmd(&["dotnet", "format", "--verify-no-changes"]), raw);
        assert_eq!(filter, "structured:dotnet-format");
        assert!(out.contains("/tmp/work/src/FormatMe/Program.cs(1,15): error WHITESPACE"));
        assert!(out.contains("Delete 1 characters."));
        assert!(out.contains("(5,28)"));
    }

    #[test]
    fn dotnet_format_console_ecarte_le_bruit_des_analyseurs() {
        let raw = "\
Formatting code files in workspace '/tmp/work/src/FormatMe/FormatMe.csproj'.\n\
Project FormatMe is using configuration from '/opt/dotnet/sdk/10.0.300/Sdks/Microsoft.NET.Sdk/analyzers/build/config/analysislevel_10_default.globalconfig'.\n\
/tmp/work/src/FormatMe/Program.cs(1,15): error WHITESPACE: Fix whitespace formatting. Insert '\\n'. [/tmp/work/src/FormatMe/FormatMe.csproj]\n\
/tmp/work/src/FormatMe/Program.cs(3,24): error WHITESPACE: Fix whitespace formatting. Delete 1 characters. [/tmp/work/src/FormatMe/FormatMe.csproj]\n\
Running 154 analyzers on FormatMe.\n\
Format complete in 2674ms.\n";
        let (filter, out) =
            route_command_filter(&cmd(&["dotnet", "format", "--verify-no-changes"]), raw);
        assert_eq!(filter, "toml:dotnet-format");
        assert!(out.contains("Program.cs(1,15): error WHITESPACE"));
        assert!(out.contains("Delete 1 characters."));
        assert!(!out.contains("Running 154 analyzers"), "{out}");
        assert!(!out.contains("analysislevel"), "{out}");
    }

    #[test]
    fn playwright_json_garde_expected_received_et_le_titre() {
        let raw = r#"{"config":{"version":"1.60.0"},"stats":{"expected":12,"unexpected":2,"skipped":0,"flaky":0,"duration":8602.1},"suites":[{"title":"panier.spec.js","file":"panier.spec.js","specs":[
            {"title":"article 3 visible","ok":true,"tests":[{"results":[{"status":"passed","errors":[]}]}]},
            {"title":"le total est juste","ok":false,"file":"panier.spec.js","tests":[{"results":[{"status":"failed","error":{"message":"Error: expect(locator).toHaveText(expected) failed\nLocator: locator('#total')\nExpected: \"42,90 €\"\nReceived: \"41,90 €\"\n","stack":"at /tmp/pw/tests/panier.spec.js:11:40","location":{"file":"/tmp/pw/tests/panier.spec.js","line":11,"column":40}}}]}]},
            {"title":"le bouton confirmer existe","ok":false,"file":"panier.spec.js","tests":[{"results":[{"status":"failed","error":{"message":"TimeoutError: locator.click: Timeout 1000ms exceeded.\n","location":{"file":"/tmp/pw/tests/panier.spec.js","line":15,"column":36}}}]}]}
        ],"suites":[]}],"errors":[]}"#;
        let (filter, out) =
            route_command_filter(&cmd(&["playwright", "test", "--reporter=json"]), raw);
        assert_eq!(filter, "structured:playwright-json");
        for fact in [
            "\"expected\": 12",
            "\"unexpected\": 2",
            "le total est juste",
            "Expected: \"42,90 €\"",
            "Received: \"41,90 €\"",
            "locator('#total')",
            "panier.spec.js:11:40",
            "le bouton confirmer existe",
            "TimeoutError: locator.click: Timeout 1000ms exceeded.",
            "panier.spec.js:15:36",
        ] {
            assert!(out.contains(fact), "{fact} absent de:\n{out}");
        }
        assert!(!out.contains("article 3 visible"), "{out}");
        assert!(
            out.len() < raw.len() / 2,
            "pas de réduction: {} -> {}",
            raw.len(),
            out.len()
        );
    }

    #[test]
    fn un_budget_de_lignes_ne_se_paie_jamais_avec_les_erreurs_de_fin_de_journal() {
        let mut lines: Vec<String> = (0..300).map(|i| format!("setup step {i}")).collect();
        lines.push("##[error]Process completed with exit code 1.".to_string());
        let out = keep_signal_within(lines, 20, TruncateFrom::Tail, "t");
        assert!(out.iter().any(|l| l.contains("##[error]Process completed")));
        assert!(
            out.iter()
                .any(|l| l.contains("lignes omises par le filtre t")),
            "troncature non annoncée"
        );
        assert!(out.len() <= 23);
    }

    #[test]
    fn gh_run_log_garde_l_erreur_finale() {
        let mut raw: String = (0..400)
            .map(|i| {
                format!(
                    "build\tstep\t2026-09-22T10:00:{:02}Z npm ci line {i}\n",
                    i % 60
                )
            })
            .collect();
        raw.push_str("build\tstep\t2026-09-22T10:07:00Z ##[error]AssertionError: expected [ +0, 1 ] to include null\n");
        let (filter, out) =
            route_command_filter(&cmd(&["gh", "run", "view", "1", "--log-failed"]), &raw);
        assert_eq!(filter, "toml:gh-run-log");
        assert!(out.contains("##[error]AssertionError"), "{out}");
    }

    #[test]
    fn gh_run_log_garde_l_explication_qui_precede_le_error_final() {
        let mut raw: String = (0..240)
            .map(|i| {
                format!(
                    "build\tSetup\t2026-09-22T18:00:{:02}.0Z setup slot {i}\n",
                    i % 60
                )
            })
            .collect();
        for l in [
            "##[group]Run dotnet test --no-restore",
            "  Failed PanierTests.AppliqueCoupon [12 ms]",
            "  Error Message:",
            "   Assert.Equal() Failure: Values differ",
            "Expected: 19,90 €",
            "Actual:   20,00 €",
            "   at Revue.Tests.PanierTests.AppliqueCoupon() in /src/PanierTests.cs:line 44",
            "##[endgroup]",
        ] {
            raw.push_str(&format!("build\tTests\t2026-09-22T18:04:01.0Z {l}\n"));
        }
        raw.push_str("build\tComplete\t2026-09-22T18:06:00.0Z ##[error]Process completed with exit code 1.\n");
        let (_, out) = route_command_filter(&cmd(&["gh", "run", "view", "99", "--log"]), &raw);
        for attendu in [
            "Assert.Equal() Failure",
            "Expected: 19,90 €",
            "Actual:   20,00 €",
            "PanierTests.cs:line 44",
            "##[error]Process completed",
        ] {
            assert!(out.contains(attendu), "{attendu} absent de:\n{out}");
        }
        assert!(!out.contains("setup slot 100"), "bruit gardé:\n{out}");
    }

    #[test]
    fn dotnet_garde_un_cadre_localise_meme_sous_microsoft() {
        let raw = "  Failed Microsoft.Extensions.Logging.Tests.LoggerTests.WritesScope [3 ms]\n  Error Message:\n   Assert.Equal() Failure\nExpected: scope-ouvert\n  Stack Trace:\n     at Microsoft.Extensions.Logging.Tests.LoggerTests.WritesScope() in /src/LoggerTests.cs:line 40\n   at System.RuntimeMethodHandle.InvokeMethod(Object target, Void** arguments)\nFailed!  - Failed:     1, Passed:    0\n";
        let (_, out) = route_command_filter(&cmd(&["dotnet", "test"]), raw);
        assert!(out.contains("LoggerTests.cs:line 40"), "{out}");
        assert!(!out.contains("RuntimeMethodHandle"), "{out}");
    }

    #[test]
    fn la_cle_api_prise_dans_l_environnement_n_apparait_pas_dans_l_aide() {
        use clap::CommandFactory;
        let cli = Cli::command();
        for sub in ["serve", "wrap"] {
            let arg = cli
                .find_subcommand(sub)
                .unwrap()
                .get_arguments()
                .find(|a| a.get_id() == "api_key")
                .unwrap();
            assert!(
                arg.is_hide_env_values_set(),
                "{sub} --help afficherait la clé"
            );
        }
    }

    #[test]
    fn compress_ne_perd_pas_les_erreurs_de_fin_de_journal() {
        let mut raw = String::new();
        for i in 0..400 {
            raw.push_str(&format!(
                "2026-09-22T10:00:00Z INFO step {} cache warm slot ok\n",
                i % 7
            ));
        }
        raw.push_str(
            "2026-09-22T10:07:00Z ##[error]AssertionError: expected [ +0, 1 ] to include null\n",
        );
        for i in 0..50 {
            raw.push_str(&format!(
                "2026-09-22T10:08:00Z INFO cleanup {} done\n",
                i % 5
            ));
        }
        raw.push_str("##[error]Process completed with exit code 1.\n");
        let store = InMemoryCcrStore::new();
        let report = compress_text(&raw, "", &store).unwrap();
        assert!(
            report
                .output
                .contains("##[error]AssertionError: expected [ +0, 1 ] to include null"),
            "{}",
            report.output
        );
        assert!(report
            .output
            .contains("##[error]Process completed with exit code 1."));
        assert!(report.output.len() <= raw.len());
    }

    #[test]
    fn une_sortie_deja_filtree_par_native_n_est_pas_refiltree() {
        let raw = "Failed Tests:\n  Calc.Tests.X\n    Expected: 201\n    Actual:   202\n";
        let (filter, out) = route_command_filter(&cmd(&["lm-resizer", "dotnet", "test"]), raw);
        assert_eq!(filter, "native_owned");
        assert_eq!(out, raw);
    }

    #[test]
    fn la_porte_des_diagnostics_voit_une_ligne_d_echec_perdue() {
        let filtre = "npm ci ok\n##[error]Process completed with exit code 1.\n";
        assert!(first_lost_failure_line(filtre, "npm ci ok\n[1 line omitted]").is_some());
        assert!(first_lost_failure_line(filtre, filtre).is_none());
    }

    #[test]
    fn exec_toml_filter_keeps_matching_lines() {
        let def = TomlFilterDef {
            name: "sample".to_string(),
            match_command: "^sample".to_string(),
            strip_ansi: false,
            strip_lines_matching: Vec::new(),
            keep_lines_matching: vec!["error|warning".to_string()],
            keep_block_after_matching: Vec::new(),
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
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "lm-resizer-filter-init-{}-{nonce}",
            std::process::id()
        ));
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
    fn sensitive_keys_cover_header_spellings_but_not_token_counters() {
        // Audit du 08/10/2026 : `x-api-key` et `x-goog-api-key` restaient en clair.
        for key in [
            "x-api-key",
            "X-Goog-Api-Key",
            "api-key",
            "api_key",
            "apiKey",
            "Authorization",
            "proxy-authorization",
            "x-amz-security-token",
            "access_token",
            "id_token",
            "client_secret",
            "x-webhook-secret",
            "db_password",
            "private_key",
            "openai_api_key",
            "aws_secret_access_key",
            "credentials",
        ] {
            assert!(is_sensitive_key(key), "{key} devrait être masqué");
        }
        // Compteurs et champs ordinaires d'un fixture : à garder lisibles.
        for key in [
            "max_tokens",
            "input_tokens",
            "prompt_tokens",
            "completion_tokens",
            "total_tokens",
            "cache_creation_input_tokens",
            "model",
            "content",
            "role",
            "stop_reason",
            "tokenizer",
            "authors",
        ] {
            assert!(!is_sensitive_key(key), "{key} ne devrait pas être masqué");
        }
    }

    #[test]
    fn sanitize_provider_fixture_redacts_header_style_keys() {
        let root = tempfile::tempdir().unwrap();
        let input = root.path().join("input.json");
        let output = root.path().join("fixture.json");
        std::fs::write(
            &input,
            serde_json::to_string(&json!({
                "headers": {"x-api-key": "sk-fixture-1", "x-goog-api-key": "sk-fixture-2"},
                "max_tokens": 64,
                "usage": {"input_tokens": 12}
            }))
            .unwrap(),
        )
        .unwrap();
        let report =
            sanitize_provider_fixture(ProviderKind::Anthropic, &input, &output, 10).unwrap();
        assert_eq!(report.redacted_fields, 2);
        let text = std::fs::read_to_string(output).unwrap();
        assert!(!text.contains("sk-fixture"), "{text}");
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["max_tokens"], 64);
        assert_eq!(value["usage"]["input_tokens"], 12);
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
        assert_eq!(filter, "native:packages");
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
                "native:go-test",
            ),
            (
                vec!["dotnet", "test"],
                "noise\nTotal tests: 3. Passed: 2. Failed: 1\n",
                "native:dotnet-test",
            ),
            (
                vec!["mvn", "test"],
                "Downloading dependency\n[ERROR] Failed to execute goal\nBUILD FAILURE\n",
                "toml:jvm-build",
            ),
            (
                vec!["uv", "sync"],
                "Resolved 42 packages\nDownloading wheels\nInstalled 42 packages\n",
                "native:packages",
            ),
            (
                vec!["ruff", "check"],
                "src/main.py:1:1: E402 bad\nFound 1 error.\n",
                "native:python-lint",
            ),
            (
                vec!["eslint", "."],
                "file.ts\nError: bad\n",
                "native:eslint",
            ),
            (
                vec!["docker", "logs", "app"],
                "info\nERROR failed\n",
                "native:container-logs",
            ),
            (
                vec!["kubectl", "get", "pods"],
                "NAME READY STATUS\napp 0/1 CrashLoopBackOff\n",
                "native:container-table",
            ),
            (
                vec!["aws", "lambda", "list-functions"],
                "{\"FunctionName\":\"demo\"}\n",
                "aws-json",
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
        let filtered = diagnostic_views::typescript(raw);
        assert!(filtered.contains("TypeScript: 2 errors in 1 files"));
        assert!(filtered.contains("src/a.ts (2 errors)"));
        assert!(filtered.contains("TS2322"));
    }

    #[test]
    fn filter_tsc_keeps_parenthesised_paths() {
        let raw = "app/(auth)/login/page.tsx(3,1): error TS2304: x\napp/(shop)/cart/page.tsx(1,1): error TS2304: y\n";
        let filtered = diagnostic_views::typescript(raw);
        assert!(filtered.contains("app/(auth)/login/page.tsx (1 errors)"));
        assert!(filtered.contains("app/(shop)/cart/page.tsx (1 errors)"));
        assert!(!filtered.contains("app/: "));
    }

    #[test]
    fn exec_cargo_test_filter_summarizes_passes() {
        let raw =
            "Compiling demo\nrunning 4 tests\ntest detailed::module::case_one ... ok\ntest detailed::module::case_two ... ok\ntest detailed::module::case_three ... ok\ntest detailed::module::case_004 ... ok\ntest detailed::module::case_005 ... ok\ntest detailed::module::case_006 ... ok\ntest detailed::module::case_007 ... ok\ntest detailed::module::case_008 ... ok\ntest detailed::module::case_009 ... ok\ntest detailed::module::case_010 ... ok\ntest detailed::module::case_011 ... ok\ntest detailed::module::case_012 ... ok\ntest detailed::module::case_013 ... ok\ntest detailed::module::case_014 ... ok\ntest detailed::module::case_015 ... ok\ntest detailed::module::case_016 ... ok\ntest detailed::module::case_017 ... ok\ntest detailed::module::case_018 ... ok\ntest detailed::module::case_019 ... ok\ntest detailed::module::case_020 ... ok\ntest detailed::module::case_021 ... ok\ntest detailed::module::case_022 ... ok\ntest detailed::module::case_023 ... ok\ntest detailed::module::case_024 ... ok\ntest detailed::module::case_025 ... ok\ntest detailed::module::case_026 ... ok\ntest detailed::module::case_027 ... ok\ntest detailed::module::case_028 ... ok\ntest detailed::module::case_029 ... ok\ntest result: ok. 4 passed; 0 failed\n";
        let filtered = test_views::cargo(raw);
        assert!(filtered.contains("test result: ok"));
        assert!(!filtered.contains("Compiling demo"));
    }

    #[test]
    fn command_runs_js_test_matches_runners_not_search() {
        let v = |args: &[&str]| {
            command_runs_js_test(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
        };
        assert!(v(&["vitest", "run"]));
        assert!(v(&["jest"]));
        assert!(v(&["npx", "vitest", "run"]));
        assert!(v(&["bunx", "jest"]));
        assert!(v(&["pnpm", "exec", "vitest"]));
        assert!(v(&["node_modules/.bin/vitest"]));
        // must NOT misfire when vitest/jest is a search pattern or a config path
        assert!(!v(&["grep", "vitest", "src/"]));
        assert!(!v(&["cat", "jest.config.js"]));
        assert!(!v(&["npm", "run", "build"]));
    }

    #[test]
    fn filter_vitest_collapses_passing_run_to_summary() {
        let raw = " RUN  v4.1.9 /repo\n\n \u{2713} tests/a.test.ts (24 tests) 6ms\n\n Test Files  1 passed (1)\n      Tests  24 passed (24)\n   Start at  20:27:57\n   Duration  132ms (transform 41ms)\n";
        let filtered = filter_vitest(raw);
        assert!(filtered.contains("Tests  24 passed (24)"));
        assert!(filtered.contains("Test Files  1 passed (1)"));
        assert!(!filtered.contains("RUN  v4.1.9"));
        assert!(!filtered.contains("Duration"));
        // hard collapse: dramatically smaller than the raw
        assert!(filtered.len() < raw.len() / 2);
    }

    #[test]
    fn filter_vitest_keeps_failure_signal_drops_noise() {
        // Real vitest v4 failing output shape (captured live).
        let raw = concat!(
            " DEPRECATED  `test.poolOptions` was removed in Vitest 4. See migration guide...\n\n",
            " RUN  v4.1.9 /repo\n\n",
            " \u{276F} tests/x.test.ts (3 tests | 1 failed) 6ms\n",
            "     \u{00D7} fails on purpose 4ms\n\n",
            "\u{23AF}\u{23AF}\u{23AF} Failed Tests 1 \u{23AF}\u{23AF}\u{23AF}\n\n",
            " FAIL  tests/x.test.ts > scratch shape > fails on purpose\n",
            "AssertionError: expected 2 to be 3 // Object.is equality\n\n",
            "- Expected\n+ Received\n\n- 3\n+ 2\n\n",
            " \u{276F} tests/x.test.ts:5:48\n",
            " Test Files  1 failed (1)\n      Tests  1 failed | 2 passed (3)\n",
            "   Start at  20:35:19\n   Duration  117ms\n",
        );
        let filtered = filter_vitest(raw);
        // signal kept
        assert!(filtered.contains("fails on purpose"));
        assert!(filtered.contains("AssertionError: expected 2 to be 3"));
        assert!(filtered.contains("Failed Tests 1"));
        assert!(filtered.contains("tests/x.test.ts:5:48"));
        assert!(filtered.contains("Tests  1 failed | 2 passed (3)"));
        // noise dropped
        assert!(!filtered.contains("DEPRECATED"));
        assert!(!filtered.contains("RUN  v4.1.9"));
        assert!(!filtered.contains("Duration  117ms"));
        // and it is genuinely shorter than the raw
        assert!(filtered.len() < raw.len());
    }

    /// Un `docker build` BuildKit qui se termine bien.
    ///
    /// Forme reproduite depuis la sortie réelle de BuildKit : une étape produit
    /// trois à quatre lignes (en-tête, transfert, `DONE`), et un Dockerfile
    /// ordinaire en compte des dizaines. C'est ce volume-là qui entrait entier
    /// dans le contexte.
    fn docker_build_reussi() -> String {
        let mut out = String::from(
            "#1 [internal] load build definition from Dockerfile\n\
             #1 transferring dockerfile: 1.42kB done\n\
             #1 DONE 0.0s\n\
             \n\
             #2 [internal] load metadata for docker.io/library/node:20-alpine\n\
             #2 DONE 0.4s\n\
             \n\
             #3 [internal] load .dockerignore\n\
             #3 transferring context: 128B done\n\
             #3 DONE 0.0s\n\
             \n",
        );
        for n in 4..40 {
            out.push_str(&format!("#{n} [{}/40] RUN etape numero {n}\n", n - 3));
            out.push_str(&format!("#{n} sha256:c0ffee{n} 32.4MB / 64.8MB 1.2s\n"));
            out.push_str(&format!("#{n} extracting sha256:c0ffee{n} 0.3s done\n"));
            out.push_str(&format!("#{n} DONE 1.{n}s\n\n"));
        }
        out.push_str(
            "#40 [runtime 8/8] RUN npm ci --omit=dev\n\
             #40 12.34 npm warn deprecated inflight@1.0.6: This module is not supported\n\
             #40 45.67 added 1204 packages in 45s\n\
             #40 DONE 46.1s\n\
             \n\
             #41 exporting to image\n\
             #41 exporting layers 2.10s done\n\
             #41 writing image sha256:9f1c2b3a4d5e6f70 done\n\
             #41 naming to docker.io/library/app:latest done\n\
             #41 DONE 2.3s\n",
        );
        out
    }

    /// Le même build, mais l'étape `npm ci` échoue.
    ///
    /// BuildKit répète alors le diagnostic sous trois formes — `#N ERROR:`, le
    /// rappel encadré de l'étape, puis l'extrait du Dockerfile avec la ligne
    /// fautive marquée `>>>`. C'est exactement ce qu'un agent doit recevoir.
    fn docker_build_echoue() -> String {
        let mut out = docker_build_reussi();
        // On remplace la queue « export réussi » par la queue d'échec réelle.
        let coupe = out.find("#41 exporting to image").expect("queue d'export");
        out.truncate(coupe);
        out.push_str(
            "#40 45.31 npm ERR! code ELIFECYCLE\n\
             #40 45.31 npm ERR! errno 1\n\
             #40 ERROR: process \"/bin/sh -c npm ci --omit=dev\" did not complete successfully: exit code 1\n\
             ------\n\
             \u{a0}> [runtime 8/8] RUN npm ci --omit=dev:\n\
             45.31 npm ERR! code ELIFECYCLE\n\
             45.31 npm ERR! errno 1\n\
             ------\n\
             Dockerfile:24\n\
             --------------------\n\
             \u{a0}\u{a0}22 |     COPY package*.json ./\n\
             \u{a0}\u{a0}23 |\n\
             \u{a0}\u{a0}24 | >>> RUN npm ci --omit=dev\n\
             \u{a0}\u{a0}25 |\n\
             --------------------\n\
             ERROR: failed to solve: process \"/bin/sh -c npm ci --omit=dev\" did not complete successfully: exit code 1\n",
        );
        out
    }

    #[test]
    fn docker_build_est_route_sous_ses_quatre_formes() {
        let v = |args: &[&str]| {
            command_runs_docker_build(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
        };
        assert!(v(&["docker", "build", "."]));
        assert!(v(&["podman", "build", "-t", "app", "."]));
        assert!(v(&["docker", "buildx", "build", "--push", "."]));
        assert!(v(&["docker", "compose", "build"]));
        assert!(v(&["docker-compose", "build", "web"]));
        // Un drapeau avant le verbe ne doit pas masquer la sous-commande.
        assert!(v(&["docker", "--context", "distant", "build", "."]));
        // Et surtout, rien ne doit être pris aux commandes docker déjà filtrées
        // ailleurs, ni aux commandes qui parlent seulement de build.
        assert!(!v(&["docker", "ps", "-a"]));
        assert!(!v(&["docker", "logs", "app"]));
        assert!(!v(&["docker", "compose", "up", "-d"]));
        assert!(!v(&["docker", "run", "build"]));
        assert!(!v(&["grep", "build", "Dockerfile"]));
    }

    // Regression coverage of the legacy helper remains explicit. Public exec
    // now uses a reversible view: the helper dropped unclassified RUN output.
    #[test]
    fn docker_public_view_preserves_unclassified_run_output_and_every_stage() {
        for raw in [
            docker_build_reussi(),
            docker_build_echoue(),
            "#1 RUN deploy\ncredential rejected by remote host\n#1 terminated unexpectedly\n"
                .to_string(),
        ] {
            let (name, view) =
                filter_command_output(&["docker", "build", "."].map(String::from), &raw);
            assert_eq!(name, "lossless:containers");
            assert_eq!(lossless_filters::expand(&view).unwrap(), raw);
            assert!(token_metrics::TokenCounts::measure(&raw, &view).tokens_saved >= 0);
        }
    }

    #[test]
    fn docker_build_reussi_est_reduit_sans_perdre_ce_qui_sert() {
        let brut = docker_build_reussi();
        let (filtre, sortie) = route_command_filter(
            &["docker", "build", "-t", "app:latest", "."].map(String::from),
            &brut,
        );
        assert_eq!(filtre, "docker_build");
        // L'identité de l'image produite survit : sans elle, la sortie ne dit
        // plus ce qui a été construit.
        assert!(sortie.contains("writing image sha256:9f1c2b3a4d5e6f70"));
        assert!(sortie.contains("naming to docker.io/library/app:latest"));
        // L'avertissement de dépendance aussi — c'est une information, pas du
        // bruit de progression.
        assert!(sortie.contains("npm warn deprecated inflight@1.0.6"));
        // Le bruit mécanique, lui, disparaît.
        assert!(!sortie.contains("DONE 46.1s"));
        assert!(!sortie.contains("transferring dockerfile"));
        assert!(!sortie.contains("extracting sha256:"));
        // Et le compte des lignes retirées reste visible.
        assert!(sortie.contains("omitted"));
        assert!(
            sortie.len() < brut.len() / 10,
            "reduction insuffisante : {} octets pour {}",
            sortie.len(),
            brut.len()
        );
        // Le banc doit tomber contre l'ancienne logique, sinon il ne prouve
        // rien : avant ce filtre, `docker build` tombait dans le générique, qui
        // garde ses 240 premières lignes — c'est-à-dire tout ce build.
        let ancien = filter_generic(&brut);
        assert_eq!(
            ancien.len(),
            brut.len(),
            "le generique ne rendait pas la sortie entiere : le gain mesure serait faux"
        );
        assert!(sortie.len() * 10 < ancien.len());
    }

    #[test]
    fn docker_build_echoue_conserve_le_diagnostic_entier() {
        let brut = docker_build_echoue();
        let (filtre, sortie) =
            route_command_filter(&["docker", "build", "."].map(String::from), &brut);
        assert_eq!(filtre, "docker_build");
        // Les trois formes du diagnostic doivent être là, en entier : la cause,
        // l'étape fautive, et la ligne du Dockerfile.
        for attendu in [
            "npm ERR! code ELIFECYCLE",
            "npm ERR! errno 1",
            "ERROR: process \"/bin/sh -c npm ci --omit=dev\" did not complete successfully: exit code 1",
            "> [runtime 8/8] RUN npm ci --omit=dev:",
            "Dockerfile:24",
            "24 | >>> RUN npm ci --omit=dev",
            "ERROR: failed to solve:",
        ] {
            assert!(
                sortie.contains(attendu),
                "diagnostic ampute, manque : {attendu:?}"
            );
        }
        // Le bloc encadré garde ses séparateurs : coupés, il devient illisible.
        assert_eq!(
            sortie.matches("------\n").count(),
            brut.matches("------\n").count(),
            "separateurs du bloc d'erreur perdus"
        );
        // Réduction tout de même réelle : les 36 étapes mécaniques d'avant
        // l'échec sont parties.
        assert!(
            sortie.len() < brut.len() / 4,
            "reduction insuffisante : {} octets pour {}",
            sortie.len(),
            brut.len()
        );
    }

    /// Builder classique (`DOCKER_BUILDKIT=0`) : l'échec tient en une phrase
    /// sans le mot « error ». Le filtre ne doit pas la remplacer par un succès.
    #[test]
    fn docker_build_classique_echoue_ne_devient_pas_un_succes() {
        let brut = "\
Sending build context to Docker daemon  3.072kB\n\
Step 1/2 : FROM alpine:3.19\n\
 ---> 6b6e8e0b0c2a\n\
Step 2/2 : RUN false\n\
 ---> Running in 0abc123def45\n\
The command '/bin/sh -c false' returned a non-zero code: 1\n";
        let (filtre, sortie) = route_command_filter(
            &["docker", "build", "--no-cache", "."].map(String::from),
            brut,
        );
        assert_eq!(filtre, "docker_build");
        assert!(
            !sortie.contains("docker build: completed"),
            "faux succes : {sortie:?}"
        );
        assert!(
            sortie.contains("returned a non-zero code: 1"),
            "synthese d'echec perdue : {sortie:?}"
        );
        assert!(
            sortie.contains("Step 2/2 : RUN false"),
            "etape fautive perdue : {sortie:?}"
        );
    }

    /// La sortie du `RUN` précède le résumé classique et n'est pas répétée.
    #[test]
    fn docker_build_classique_conserve_la_sortie_du_run_en_echec() {
        let brut = "\
Step 1/2 : FROM gcc:13\n\
 ---> aaa111\n\
Step 2/2 : RUN cc main.c\n\
 ---> Running in bbb222\n\
/usr/bin/ld: /tmp/ccx.o: undefined reference to `main'\n\
collect2: error: ld returned 1 exit status\n\
The command '/bin/sh -c cc main.c' returned a non-zero code: 1\n";
        let (_, sortie) = route_command_filter(&["docker", "build", "."].map(String::from), brut);
        assert!(
            sortie.contains("undefined reference to `main'"),
            "diagnostic du linker perdu : {sortie:?}"
        );
        assert!(
            sortie.contains("returned a non-zero code: 1"),
            "code de sortie perdu : {sortie:?}"
        );
        assert!(!sortie.contains("docker build: completed"), "{sortie:?}");
    }

    #[test]
    fn docker_build_classique_reussi_garde_l_identite_image() {
        let mut brut = String::new();
        for n in 1..=24 {
            brut.push_str(&format!(
                "Step {n}/24 : RUN echo etape {n}\n ---> deadbeef{n:04}\n"
            ));
        }
        brut.push_str(
            "Successfully built 6b6e8e0b0c2a9f1c\n\
             Successfully tagged registry.example/app:1.2.3\n",
        );
        let (filtre, sortie) = route_command_filter(
            &["docker", "build", "-t", "registry.example/app:1.2.3", "."].map(String::from),
            &brut,
        );
        assert_eq!(filtre, "docker_build");
        assert!(
            sortie.contains("Successfully built 6b6e8e0b0c2a9f1c"),
            "{sortie}"
        );
        assert!(
            sortie.contains("Successfully tagged registry.example/app:1.2.3"),
            "{sortie}"
        );
        assert!(!sortie.contains("docker build: completed"), "{sortie}");
        assert!(
            sortie.len() < brut.len() / 2,
            "{} vs {}",
            sortie.len(),
            brut.len()
        );
    }

    #[test]
    fn docker_build_quiet_garde_le_sha_stdout() {
        let brut = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n";
        let (_, sortie) = route_command_filter(
            &["docker", "build", "-q", "-t", "app", "."].map(String::from),
            brut,
        );
        assert!(
            sortie.contains(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            ),
            "identite -q perdue : {sortie:?}"
        );
        assert!(!sortie.contains("docker build: completed"), "{sortie:?}");
    }

    /// `-q` écrit l'id sur stdout et le progrès sur stderr, réunis par le outil.
    #[test]
    fn docker_build_quiet_conserve_le_sha_meme_si_stderr_est_present() {
        let brut = "\
sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n\
[stderr]\n\
#1 [internal] load build definition from Dockerfile\n\
#1 transferring dockerfile: 12B done\n\
#1 DONE 0.0s\n";
        let (_, sortie) =
            route_command_filter(&["docker", "build", "--quiet", "."].map(String::from), brut);
        assert!(
            sortie.contains(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            ),
            "sha stdout perdu derriere stderr : {sortie:?}"
        );
    }

    #[test]
    fn docker_buildx_push_garde_le_digest_publie() {
        let brut = "\
#11 exporting to image\n\
#11 pushing layers 0.4s done\n\
#11 pushing manifest for docker.io/library/app:1@sha256:9f1c2b3a4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f8 0.2s done\n\
#11 DONE 0.7s\n";
        let (filtre, sortie) = route_command_filter(
            &["docker", "buildx", "build", "--push", "-t", "app:1", "."].map(String::from),
            brut,
        );
        assert_eq!(filtre, "docker_build");
        assert!(
            sortie.contains("pushing manifest for docker.io/library/app:1@sha256:9f1c2b3a4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f8"),
            "digest publie perdu : {sortie:?}"
        );
        assert!(!sortie.contains("docker build: completed"), "{sortie:?}");
    }

    #[test]
    fn docker_build_annulation_n_est_pas_un_succes() {
        let brut = "\
#3 [1/2] RUN sleep 100\n\
#3 CANCELED\n";
        let (_, sortie) = route_command_filter(&["docker", "build", "."].map(String::from), brut);
        assert!(
            !sortie.contains("docker build: completed"),
            "annulation presentee comme un succes : {sortie:?}"
        );
        assert!(sortie.contains("CANCELED"), "{sortie:?}");
    }

    #[test]
    fn docker_build_garde_une_deprecation_sans_le_mot_deprecated() {
        let brut = "\
#4 [1/1] RUN true\n\
#4 DEPRECATION NOTICE: the legacy builder frontend is going away\n\
#4 DONE 0.1s\n";
        let (_, sortie) = route_command_filter(&["docker", "build", "."].map(String::from), brut);
        assert!(
            sortie.contains("DEPRECATION NOTICE"),
            "avertissement de depreciation perdu : {sortie:?}"
        );
    }

    #[test]
    fn podman_build_reussi_garde_le_tag() {
        let brut = "\
STEP 1/1: FROM alpine\n\
COMMIT localhost/app:latest\n\
--> 9f1c2b3a4d5e\n\
Successfully tagged localhost/app:latest\n";
        let (filtre, sortie) = route_command_filter(
            &["podman", "build", "-t", "localhost/app:latest", "."].map(String::from),
            brut,
        );
        assert_eq!(filtre, "docker_build");
        assert!(
            sortie.contains("Successfully tagged localhost/app:latest"),
            "{sortie}"
        );
        assert!(sortie.contains("COMMIT localhost/app:latest"), "{sortie}");
        assert!(!sortie.contains("docker build: completed"), "{sortie:?}");
    }

    #[test]
    fn docker_build_route_bake_et_refuse_compose_up() {
        let v = |args: &[&str]| {
            command_runs_docker_build(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
        };
        assert!(v(&[
            "docker",
            "buildx",
            "bake",
            "-f",
            "docker-bake.hcl",
            "web"
        ]));
        assert!(v(&["docker", "bake", "web"]));
        assert!(v(&[
            "docker",
            "--context",
            "distant",
            "buildx",
            "build",
            "--push",
            "."
        ]));
        // `up --build` mélange des logs de services : ce n'est pas un build seul.
        assert!(!v(&["docker", "compose", "up", "--build", "-d"]));
        assert!(!v(&["docker", "buildx", "du"]));
        assert!(!v(&["bash", "-lc", "docker build ."]));
    }

    #[test]
    fn docker_build_ne_confond_pas_la_valeur_d_un_drapeau_avec_le_verbe() {
        let v = |args: &[&str]| {
            command_runs_docker_build(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
        };
        // Un contexte nommé « build » : la valeur du drapeau global n'est pas
        // la sous-commande. Ces quatre commandes ne construisent rien.
        assert!(!v(&["docker", "--context", "build", "ps"]));
        assert!(!v(&["docker", "-c", "build", "images"]));
        assert!(!v(&["docker", "--context", "build", "logs", "app"]));
        assert!(!v(&["podman", "--connection", "build", "ps", "-a"]));
        // La forme collée était déjà correcte : elle doit le rester.
        assert!(!v(&["docker", "--context=build", "ps"]));
        // Mais construire *depuis* ce contexte reste un build.
        assert!(v(&["docker", "--context", "build", "build", "."]));
        assert!(v(&["docker", "--context", "build", "buildx", "build", "."]));
        // Et un drapeau booléen ne doit pas avaler le verbe qui le suit :
        // sauter aveuglément l'argument d'après casserait ces deux cas.
        assert!(v(&["docker", "--debug", "build", "."]));
        assert!(v(&["docker", "--tls", "build", "."]));
        assert!(v(&["docker", "--context", "distant", "build", "."]));
    }

    #[test]
    fn docker_build_mal_route_ne_transformerait_pas_un_ps_en_build() {
        // Le test de routage ne prouve pas le câblage : on passe ici la chaîne
        // entière, avec une sortie `docker ps` assez longue pour que le filtre
        // `docker_build` la détruise au lieu d'être borné par la non-croissance.
        let mut brut = String::from(
            "CONTAINER ID   IMAGE          COMMAND      CREATED       STATUS       PORTS     NAMES\n",
        );
        for i in 0..40 {
            brut.push_str(&format!(
                "c0ffee{i:06}   app:latest     \"/bin/sh\"    2 hours ago   Up 2 hours   8080/tcp  service-{i}\n"
            ));
        }
        let (filtre, sortie) = filter_command_output(
            &["docker", "--context", "build", "ps"].map(String::from),
            &brut,
        );
        assert_ne!(
            filtre, "docker_build",
            "un contexte nomme « build » ne doit pas router un `ps` vers le filtre de build"
        );
        // Ce que produisait l'ancienne logique, et qu'on n'accepte plus : une
        // liste de conteneurs résumée en succès de construction.
        assert!(!sortie.contains("docker build: completed"));
        assert!(sortie.contains("service-0"));
        assert!(sortie.contains("service-39"));
    }

    #[test]
    fn filter_command_output_routes_vitest_via_npx() {
        let command: Vec<String> = ["npx", "vitest", "run"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let raw = " Test Files  1 passed (1)\n      Tests  3 passed (3)\n   Duration  10ms\n";
        let (name, filtered) = filter_command_output(&command, raw);
        assert_eq!(name, "lossless:npm-test");
        assert!(filtered.contains("Tests  3 passed (3)"));
        assert!(filtered.contains("Duration"));
    }

    #[test]
    fn generic_filter_preserves_every_literal_line() {
        let mut raw = (0..250)
            .map(|n| format!("progress line {n}\n"))
            .collect::<String>();
        raw.push_str("error: src/main.rs:42: missing value\n");
        let command = vec!["unknown-command".to_string()];
        let (name, output) = filter_command_output(&command, &raw);
        assert_eq!(name, "lossless:generic");
        assert_eq!(output, raw);
    }

    #[test]
    fn curl_transfer_header_does_not_trigger_diagnostic_guard() {
        let raw = include_str!("../fixtures/parity/captured/curl.txt");
        let command = vec!["curl".to_string(), "-i".to_string()];
        let (name, output) = filter_command_output(&command, raw);
        assert_eq!(name, "curl");
        assert!(output.len() < raw.len());
        assert!(output.contains("HTTP/1.1 503 Service Unavailable"));
        assert!(output.contains("database unavailable"));
    }

    #[test]
    fn diff_filter_keeps_every_changed_line_beyond_old_eight_line_limit() {
        let mut raw = "diff --git a/src/main.rs b/src/main.rs\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,1 +1,12 @@\n context\n".to_string();
        for number in 0..12 {
            raw.push_str(&format!("+changed line {number}\n"));
        }
        let (name, output) = filter_command_output(&["diff".into(), "-u".into()], &raw);
        assert_eq!(name, "diff_summary");
        for number in 0..12 {
            assert!(output.contains(&format!("+changed line {number}")));
        }
    }

    #[test]
    fn git_diff_capture_keeps_complete_patch_including_file_headers() {
        let raw = include_str!("../bench/corpus/git_diff.txt");
        let (name, output) = legacy_lossless_filter(&["git".into(), "diff".into()], raw);
        assert_eq!(name, "lossless:git");
        assert!(
            output.contains("diff --git"),
            "patch file headers must survive"
        );
        for fact in [
            "--- a/src/billing.py",
            "+++ b/src/billing.py",
            "@@ -1,4 +1,4 @@",
            "-    return subtotal + tax",
            "+    return round(subtotal + tax, 2)",
            "--- a/tests/test_billing.py",
            "+++ b/tests/test_billing.py",
            "+def test_rounding():",
            "+    assert total([1.005]) == 1.21",
        ] {
            assert!(output.contains(fact), "lost {fact}");
        }
        assert!(first_lost_indispensable_line(raw, &output, &name).is_none());
    }

    #[test]
    fn diff_filter_keeps_header_when_patch_has_no_file_markers() {
        let raw =
            "diff --git a/logo.png b/logo.png\nBinary files a/logo.png and b/logo.png differ\n";
        let (_, output) = legacy_lossless_filter(&["git".into(), "diff".into()], raw);
        assert!(output.contains("diff --git a/logo.png b/logo.png"));
    }

    #[test]
    fn git_show_keeps_commit_author_and_subject() {
        let raw = "commit abc123\nAuthor: Example <example@example.org>\nDate:   Mon Sep 28 2026\n\n    Repair error handling\n\ndiff --git a/src/main.rs b/src/main.rs\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1 +1 @@\n-old\n+new\n";
        let (name, output) = filter_command_output(&["git".into(), "show".into()], raw);
        assert_eq!(name, "native:git-show");
        for fact in [
            "commit abc123",
            "Author: Example",
            "Repair error handling",
            "-old",
            "+new",
        ] {
            assert!(output.contains(fact), "lost {fact}");
        }
    }

    #[test]
    fn pretooluse_rewrite_wraps_supported_command_at_exe_path() {
        let ev = serde_json::json!({
            "hook_event_name": "PreToolUse",
            "tool_name": "Bash",
            "tool_input": {"command": "vitest run", "description": "run tests"}
        });
        let out =
            pretooluse_rewrite_json(&ev, "/opt/lm-resizer", "PreToolUse").expect("should rewrite");
        let cmd = out
            .pointer("/hookSpecificOutput/updatedInput/command")
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(cmd, "\"/opt/lm-resizer\" exec -- vitest run");
        // preserves other tool_input fields (description)
        assert_eq!(
            out.pointer("/hookSpecificOutput/updatedInput/description")
                .and_then(|v| v.as_str()),
            Some("run tests")
        );
        assert_eq!(
            out.pointer("/hookSpecificOutput/hookEventName")
                .and_then(|v| v.as_str()),
            Some("PreToolUse")
        );
    }

    #[test]
    fn pretooluse_rewrite_skips_unsupported_and_self() {
        // generic/unsupported command → run raw (no rewrite)
        let ev = serde_json::json!({"tool_input": {"command": "echo hello"}});
        assert!(pretooluse_rewrite_json(&ev, "/opt/lm-resizer", "PreToolUse").is_none());
        // our own exec invocation → never re-wrapped (anti-recursion)
        let ev2 =
            serde_json::json!({"tool_input": {"command": "/opt/lm-resizer exec -- git status"}});
        assert!(pretooluse_rewrite_json(&ev2, "/opt/lm-resizer", "PreToolUse").is_none());
    }

    #[test]
    fn hook_rewrite_preserves_quoting_verbatim() {
        // Regression: a split-and-rejoin dropped the backslash inside double quotes, turning a
        // grep BRE alternation `"\|"` into a literal `|` (0 matches). The verbatim wrap must keep
        // the exact original bytes so bash re-parses the pattern identically.
        let cmd = r#"grep -rn "export function\|export const" src/x.ts"#;
        let out = rewrite_command_for_hook(cmd, "/opt/lm").unwrap();
        assert_eq!(
            out,
            r#""/opt/lm" exec -- grep -rn "export function\|export const" src/x.ts"#
        );
    }

    #[test]
    fn hook_rewrite_skips_compound_redirect_and_unsupported() {
        // compound (operators) → run raw, never re-quote a pipeline
        assert!(rewrite_command_for_hook("cd x && vitest run", "/opt/lm").is_none());
        assert!(rewrite_command_for_hook("git log | head", "/opt/lm").is_none());
        // redirect → output goes to a file, not the model → don't wrap
        assert!(rewrite_command_for_hook("grep x foo > out.txt", "/opt/lm").is_none());
        // même refus que rewrite-shell : substitution, here-doc, interactif
        assert!(rewrite_command_for_hook("git status $(echo --short)", "/opt/lm").is_none());
        assert!(rewrite_command_for_hook("cat <<'EOF'", "/opt/lm").is_none());
        assert!(rewrite_command_for_hook("psql", "/opt/lm").is_none());
        assert!(rewrite_command_for_hook("git diff | tee p.diff", "/opt/lm").is_none());
        // unsupported program → run raw
        assert!(rewrite_command_for_hook("echo hello", "/opt/lm").is_none());
        // supported single command → wrapped
        assert!(rewrite_command_for_hook("vitest run", "/opt/lm").is_some());
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
{"output":"Compiling demo\nrunning 4 tests\ntest detailed::module::case_one ... ok\ntest detailed::module::case_two ... ok\ntest detailed::module::case_three ... ok\ntest detailed::module::case_004 ... ok\ntest detailed::module::case_005 ... ok\ntest detailed::module::case_006 ... ok\ntest detailed::module::case_007 ... ok\ntest detailed::module::case_008 ... ok\ntest detailed::module::case_009 ... ok\ntest detailed::module::case_010 ... ok\ntest detailed::module::case_011 ... ok\ntest detailed::module::case_012 ... ok\ntest detailed::module::case_013 ... ok\ntest detailed::module::case_014 ... ok\ntest detailed::module::case_015 ... ok\ntest detailed::module::case_016 ... ok\ntest detailed::module::case_017 ... ok\ntest detailed::module::case_018 ... ok\ntest detailed::module::case_019 ... ok\ntest detailed::module::case_020 ... ok\ntest detailed::module::case_021 ... ok\ntest detailed::module::case_022 ... ok\ntest detailed::module::case_023 ... ok\ntest detailed::module::case_024 ... ok\ntest detailed::module::case_025 ... ok\ntest detailed::module::case_026 ... ok\ntest detailed::module::case_027 ... ok\ntest detailed::module::case_028 ... ok\ntest detailed::module::case_029 ... ok\ntest result: ok. 4 passed; 0 failed\n"}
"#;
        let report = discover_in_content(content, "session.jsonl");
        assert_eq!(report.command_outputs, 1);
        assert_eq!(report.rewritable_commands, 1);
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(report.candidates[0].filter, "native:cargo-test");
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
        assert_eq!(report.candidates[0].filter, "native:git-status");
    }

    #[test]
    fn discover_extracts_codex_style_arguments() {
        let content = r#"{"tool_name":"exec_command","arguments":"{\"command\":\"cargo test\"}"}
{"tool_output":"Compiling demo\nrunning 4 tests\ntest detailed::module::case_one ... ok\ntest detailed::module::case_two ... ok\ntest detailed::module::case_three ... ok\ntest detailed::module::case_004 ... ok\ntest detailed::module::case_005 ... ok\ntest detailed::module::case_006 ... ok\ntest detailed::module::case_007 ... ok\ntest detailed::module::case_008 ... ok\ntest detailed::module::case_009 ... ok\ntest detailed::module::case_010 ... ok\ntest detailed::module::case_011 ... ok\ntest detailed::module::case_012 ... ok\ntest detailed::module::case_013 ... ok\ntest detailed::module::case_014 ... ok\ntest detailed::module::case_015 ... ok\ntest detailed::module::case_016 ... ok\ntest detailed::module::case_017 ... ok\ntest detailed::module::case_018 ... ok\ntest detailed::module::case_019 ... ok\ntest detailed::module::case_020 ... ok\ntest detailed::module::case_021 ... ok\ntest detailed::module::case_022 ... ok\ntest detailed::module::case_023 ... ok\ntest detailed::module::case_024 ... ok\ntest detailed::module::case_025 ... ok\ntest detailed::module::case_026 ... ok\ntest detailed::module::case_027 ... ok\ntest detailed::module::case_028 ... ok\ntest detailed::module::case_029 ... ok\ntest result: ok. 4 passed; 0 failed\n"}
"#;
        let report = discover_in_content(content, "codex.jsonl");
        assert_eq!(report.command_outputs, 1);
        assert_eq!(report.rewritable_commands, 1);
        assert_eq!(report.candidates[0].filter, "native:cargo-test");
    }

    #[test]
    fn discover_markdown_summarizes_candidates() {
        let content = r#"{"command":"cargo test"}
{"output":"Compiling demo\nrunning 4 tests\ntest detailed::module::case_one ... ok\ntest detailed::module::case_two ... ok\ntest detailed::module::case_three ... ok\ntest detailed::module::case_004 ... ok\ntest detailed::module::case_005 ... ok\ntest detailed::module::case_006 ... ok\ntest detailed::module::case_007 ... ok\ntest detailed::module::case_008 ... ok\ntest detailed::module::case_009 ... ok\ntest detailed::module::case_010 ... ok\ntest detailed::module::case_011 ... ok\ntest detailed::module::case_012 ... ok\ntest detailed::module::case_013 ... ok\ntest detailed::module::case_014 ... ok\ntest detailed::module::case_015 ... ok\ntest detailed::module::case_016 ... ok\ntest detailed::module::case_017 ... ok\ntest detailed::module::case_018 ... ok\ntest detailed::module::case_019 ... ok\ntest detailed::module::case_020 ... ok\ntest detailed::module::case_021 ... ok\ntest detailed::module::case_022 ... ok\ntest detailed::module::case_023 ... ok\ntest detailed::module::case_024 ... ok\ntest detailed::module::case_025 ... ok\ntest detailed::module::case_026 ... ok\ntest detailed::module::case_027 ... ok\ntest detailed::module::case_028 ... ok\ntest detailed::module::case_029 ... ok\ntest result: ok. 4 passed; 0 failed\n"}
"#;
        let mut report = discover_in_content(content, "session.jsonl");
        report.files_scanned = 1;
        report.estimated_bytes_saved = report.original_bytes - report.filtered_bytes;
        report.estimated_tokens_saved = report.tokens.tokens_saved;
        let markdown = format_discover_markdown(&report);
        assert!(markdown.contains("# lm-resizer Discover Audit"));
        assert!(markdown.contains("| `cargo test` | `native:cargo-test` |"));
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
                tokens: TokenCounts {
                    tokens_saved: 15,
                    ..TokenCounts::default()
                },
                candidates: Vec::new(),
            },
        };
        let markdown = format_discover_sessions_markdown(&report);
        assert!(markdown.contains("Agent: codex"));
        assert!(markdown.contains("Paths scanned: 1"));
        assert!(markdown.contains(".codex/sessions"));
        assert!(markdown.contains("exact text count): 15"));
    }

    #[test]
    fn learn_recommends_rewrite_for_compressible_sessions() {
        let content = r#"{"command":"cargo test"}
{"output":"Compiling demo\nrunning 4 tests\ntest detailed::module::case_one ... ok\ntest detailed::module::case_two ... ok\ntest detailed::module::case_three ... ok\ntest detailed::module::case_004 ... ok\ntest detailed::module::case_005 ... ok\ntest detailed::module::case_006 ... ok\ntest detailed::module::case_007 ... ok\ntest detailed::module::case_008 ... ok\ntest detailed::module::case_009 ... ok\ntest detailed::module::case_010 ... ok\ntest detailed::module::case_011 ... ok\ntest detailed::module::case_012 ... ok\ntest detailed::module::case_013 ... ok\ntest detailed::module::case_014 ... ok\ntest detailed::module::case_015 ... ok\ntest detailed::module::case_016 ... ok\ntest detailed::module::case_017 ... ok\ntest detailed::module::case_018 ... ok\ntest detailed::module::case_019 ... ok\ntest detailed::module::case_020 ... ok\ntest detailed::module::case_021 ... ok\ntest detailed::module::case_022 ... ok\ntest detailed::module::case_023 ... ok\ntest detailed::module::case_024 ... ok\ntest detailed::module::case_025 ... ok\ntest detailed::module::case_026 ... ok\ntest detailed::module::case_027 ... ok\ntest detailed::module::case_028 ... ok\ntest detailed::module::case_029 ... ok\ntest result: ok. 4 passed; 0 failed\n"}
"#;
        let mut discover = discover_in_content(content, "session.jsonl");
        discover.files_scanned = 1;
        discover.estimated_bytes_saved = discover.original_bytes - discover.filtered_bytes;
        discover.estimated_tokens_saved = discover.tokens.tokens_saved;

        let recommendations = build_learn_recommendations(&discover, &json!({"commands": 0}));
        assert!(recommendations
            .iter()
            .any(|rec| rec.instruction.contains("lm-resizer rewrite-shell")));
        let markdown = format_learn_markdown(&recommendations, &discover, &json!({"commands": 0}));
        assert!(markdown.contains("# lm-resizer Learned Agent Guidance"));
        assert!(markdown.contains("Route noisy commands"));
    }

    #[test]
    fn discover_refuses_missing_explicit_paths() {
        let root = tempfile::tempdir().unwrap();
        let valid = root.path().join("session.jsonl");
        std::fs::write(&valid, "{}\n").unwrap();
        let missing = root.path().join("missing");
        assert!(collect_discover_files(std::slice::from_ref(&missing), true).is_err());
        assert!(collect_discover_files(&[valid, missing], true).is_err());
    }

    #[test]
    fn compress_json_recovers_exact_input_before_minification() {
        let input = serde_json::to_string_pretty(
            &(0..150)
                .map(|id| json!({"id": id, "status": "ok", "score": 100}))
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let store = InMemoryCcrStore::default();
        let report = compress_text(&input, "", &store).unwrap();
        assert!(report.bytes_saved > 0);
        let key = report.cache_keys.first().expect("original recovery key");
        assert_eq!(store.get(key).as_deref(), Some(input.as_str()));
    }

    #[test]
    fn compress_source_persists_the_original_advertised_by_its_banner() {
        let input = include_str!("main.rs").to_string();
        let store = InMemoryCcrStore::default();
        let report = compress_text(&input, "", &store).unwrap();
        assert!(report.bytes_saved > 0);
        let key = lm_resizer_core::ccr::compute_key(input.as_bytes());
        assert!(report.cache_keys.contains(&key));
        assert_eq!(store.get(&key).as_deref(), Some(input.as_str()));
    }

    #[test]
    fn eval_report_summarizes_discover_fixture() {
        let root = std::env::temp_dir().join(format!("lm-resizer-eval-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("session.jsonl");
        std::fs::write(
            &path,
            r#"{"command":"cargo test"}
{"output":"Compiling demo\nrunning 4 tests\ntest detailed::module::case_one ... ok\ntest detailed::module::case_two ... ok\ntest detailed::module::case_three ... ok\ntest detailed::module::case_004 ... ok\ntest detailed::module::case_005 ... ok\ntest detailed::module::case_006 ... ok\ntest detailed::module::case_007 ... ok\ntest detailed::module::case_008 ... ok\ntest detailed::module::case_009 ... ok\ntest detailed::module::case_010 ... ok\ntest detailed::module::case_011 ... ok\ntest detailed::module::case_012 ... ok\ntest detailed::module::case_013 ... ok\ntest detailed::module::case_014 ... ok\ntest detailed::module::case_015 ... ok\ntest detailed::module::case_016 ... ok\ntest detailed::module::case_017 ... ok\ntest detailed::module::case_018 ... ok\ntest detailed::module::case_019 ... ok\ntest detailed::module::case_020 ... ok\ntest detailed::module::case_021 ... ok\ntest detailed::module::case_022 ... ok\ntest detailed::module::case_023 ... ok\ntest detailed::module::case_024 ... ok\ntest detailed::module::case_025 ... ok\ntest detailed::module::case_026 ... ok\ntest detailed::module::case_027 ... ok\ntest detailed::module::case_028 ... ok\ntest detailed::module::case_029 ... ok\ntest result: ok. 4 passed; 0 failed\n"}
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
        assert!(markdown.contains("| `cargo_test` | 2 | 100 | 25 | 0 |"));
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
    fn native_hook_configs_target_codex_and_claude_pre_and_post_tool_use() {
        let codex = codex_native_hooks_json("lm-resizer").unwrap();
        assert!(codex.contains("PreToolUse"));
        assert!(codex.contains("PostToolUse"));
        assert!(codex.contains("\"matcher\": \"Bash|exec_command\""));
        assert!(codex.contains("hook --client codex --event PreToolUse"));
        assert!(codex.contains("hook --client codex --event PostToolUse"));

        let claude = claude_native_hooks_json("lm-resizer").unwrap();
        assert!(claude.contains("PreToolUse"));
        assert!(claude.contains("PostToolUse"));
        assert!(claude.contains("\"matcher\": \"Bash\""));
        assert!(claude.contains("hook --client claude --event PreToolUse"));
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

    #[cfg(unix)]
    #[test]
    fn generated_hook_helper_is_executable() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        init_hook_helpers(Some(root.path().to_path_buf()), false).unwrap();
        let script = root.path().join(".lm-resizer/hooks/rewrite.sh");
        assert_ne!(
            std::fs::metadata(script).unwrap().permissions().mode() & 0o111,
            0
        );
    }

    #[cfg(unix)]
    #[test]
    fn generated_command_shims_are_executable() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let report = init_command_shims(Some(root.path().to_path_buf()), false).unwrap();
        assert!(
            !report.files.is_empty(),
            "test requires a supported command on PATH"
        );
        for path in report.files {
            let permissions = std::fs::metadata(&path).unwrap().permissions();
            assert_ne!(
                permissions.mode() & 0o111,
                0,
                "shim must be executable: {path}"
            );
        }
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
    fn json_table_preserves_every_nested_value() {
        let rows: Vec<Value> = (0..20).map(|id| json!({
            "id": id, "amount": id * 3,
            "meta": {"region": "test", "reason": if id == 7 { "limit_exceeded" } else { "none" }},
            "state": if id == 7 { "rejected" } else { "ok" }
        })).collect();
        let raw =
            serde_json::to_string_pretty(&json!({"schema":"orders-v3", "rows":rows})).unwrap();
        let compact: Value = serde_json::from_str(&compact_json_rows(&raw).unwrap()).unwrap();
        let columns: Vec<&str> = compact["columns"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect();
        let restored: Vec<Value> = compact["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                Value::Object(
                    columns
                        .iter()
                        .zip(row.as_array().unwrap())
                        .map(|(key, cell)| ((*key).to_string(), cell.clone()))
                        .collect(),
                )
            })
            .collect();
        let original: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(restored, original["rows"].as_array().unwrap().clone());
    }

    #[test]
    fn command_filters_keep_failure_facts_and_collapse_repeated_success() {
        let pytest = include_str!("../bench/corpus/pytest_fail.txt");
        let (_, filtered) = legacy_lossless_filter(&["pytest".into()], pytest);
        assert!(filtered.contains("test_reject_zero"));
        assert!(filtered.contains("status=422"));
        assert!(filtered.contains("status=200"));
        assert!(filtered.contains("1 failed"));
        assert!(filtered.contains("test_case 010")); // Unknown success syntax stays visible.

        let cargo = include_str!("../bench/corpus/cargo_ok.txt");
        let (_, filtered) = legacy_lossless_filter(&["cargo".into(), "test".into()], cargo);
        assert!(filtered.contains("80 passed; 0 failed"));
        assert!(filtered.contains("0 ignored"));
        assert!(filtered.contains("parse::case 000")); // Not Cargo result syntax.

        let with_warning = "warning: unused variable at src/lib.rs:7\n\
test result: ok. 80 passed; 0 failed; 0 ignored; finished in 0.08s\n";
        let (_, filtered) = legacy_lossless_filter(&["cargo".into(), "test".into()], with_warning);
        assert!(filtered.contains("warning: unused variable"));
        assert!(filtered.contains("0 ignored"));
        let (_, filtered) = legacy_lossless_filter(
            &["cargo".into(), "test".into(), "--workspace".into()],
            with_warning,
        );
        assert!(filtered.contains("warning: unused variable"));
    }

    #[test]
    fn tee_trailer_only_spends_tokens_on_substantial_savings() {
        let hint = "[raw: 012345abcdef]";
        let raw = "word ".repeat(100);
        let mut small_gain = "word ".repeat(90);
        let before = small_gain.clone();
        append_recovery_instruction(&mut small_gain, hint, &raw);
        assert_eq!(small_gain, before);
        let mut large_gain = "summary\n".to_string();
        append_recovery_instruction(&mut large_gain, hint, &raw);
        assert!(large_gain.ends_with("[tee:012345abcdef]\n"));
        assert!(TokenCounts::measure(&raw, &large_gain).tokens_saved > 0);
        let mut expansion = "word ".repeat(120);
        let before = expansion.clone();
        append_recovery_instruction(&mut expansion, hint, &raw);
        assert_eq!(expansion, before);
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
        for entry in WalkDir::new(&root).into_iter().filter_entry(|entry| {
            !matches!(
                entry.file_name().to_str(),
                Some("target" | "_qa" | "node_modules" | ".git")
            )
        }) {
            let entry = entry.expect("repo walk should succeed");
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            // Python is a benchmark dependency, never a product runtime dependency.
            // The real-token benchmark explicitly requires Python tiktoken.
            if path.starts_with(root.join("bench/real")) {
                continue;
            }
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

    const RSPEC_RAW: &str = include_str!("../fixtures/exec/ruby-prisma/rspec_raw.txt");
    const RSPEC_RB: &str = include_str!("../fixtures/exec/ruby-prisma/rspec_test.rb");
    const MINITEST_RAW: &str = include_str!("../fixtures/exec/ruby-prisma/minitest_raw.txt");
    const MINITEST_RB: &str = include_str!("../fixtures/exec/ruby-prisma/minitest_test.rb");
    const PRISMA_RAW: &str = include_str!("../fixtures/exec/ruby-prisma/prisma_raw.txt");

    struct VueExec {
        filtre: String,
        texte_filtre: String,
        sortie: String,
        etapes: String,
    }

    fn vue_exec(command: &[&str], raw: &str) -> VueExec {
        let cmd: Vec<String> = command.iter().map(|arg| (*arg).to_string()).collect();
        let (filtre, texte_filtre) = filter_command_output(&cmd, raw);
        let store = InMemoryCcrStore::default();
        let compresse = compress_text_with_pipeline_gate(
            &texte_filtre,
            "",
            &store,
            &build_pipeline(),
            None,
            false,
        )
        .expect("compression de la sortie filtrée");
        let sortie = if first_lost_failure_line(&texte_filtre, &compresse.output).is_some() {
            texte_filtre.clone()
        } else {
            compresse.output
        };
        VueExec {
            filtre,
            texte_filtre,
            sortie,
            etapes: compresse.steps_applied.join(","),
        }
    }

    fn bilan(filtre: &str, attendu: &str, sortie: &str, faits: &[&str], bruits: &[&str]) -> String {
        let mut problemes = Vec::new();
        if filtre != attendu {
            problemes.push(format!("filtre={filtre}, attendu {attendu}"));
        }
        for fait in faits {
            if !sortie.contains(fait) {
                problemes.push(format!("fait perdu: {fait}"));
            }
        }
        for bruit in bruits {
            if sortie.contains(bruit) {
                problemes.push(format!("bruit gardé: {bruit}"));
            }
        }
        if problemes.is_empty() {
            String::new()
        } else {
            format!("{} \n---\n{sortie}", problemes.join(" | "))
        }
    }

    #[test]
    fn ruby_prisma_rspec_reel_garde_les_faits_et_ecarte_le_bruit() {
        let ligne = RSPEC_RB.lines().nth(4).expect("ligne 5");
        assert!(
            ligne.contains("eq(3)"),
            "le fichier de test ne place pas eq(3) à la ligne 5 : {ligne}"
        );
        let vue = vue_exec(&["rspec", "rspec_test.rb"], RSPEC_RAW);
        let faits = [
            "1) Math adds numbers",
            "Failure/Error: expect(1 + 1).to eq(3)",
            "expected: 3",
            "got: 2",
            "./rspec_test.rb:5",
            "2) Math subtracts numbers",
            "Failure/Error: expect(2 - 1).to eq(2)",
            "expected: 2",
            "got: 1",
            "./rspec_test.rb:9",
            "2 examples, 2 failures",
            "rspec ./rspec_test.rb:4 # Math adds numbers",
            "rspec ./rspec_test.rb:8 # Math subtracts numbers",
        ];
        let bruits = ["files took 0.12175 seconds to load"];
        let probleme = bilan(&vue.filtre, "toml:rspec", &vue.sortie, &faits, &bruits);
        assert!(probleme.is_empty(), "ASSERT {probleme}");
        assert!(
            !vue.sortie.lines().any(|ligne| ligne.trim() == "FF"),
            "bruit gardé: FF\n{}",
            vue.sortie
        );
        assert!(
            vue.texte_filtre.len() < RSPEC_RAW.len(),
            "ASSERT octets rspec brut={} filtre={}",
            RSPEC_RAW.len(),
            vue.texte_filtre.len()
        );
        println!(
            "ASSERT octets rspec brut={} filtre={} apres_porte={} etapes={}",
            RSPEC_RAW.len(),
            vue.texte_filtre.len(),
            vue.sortie.len(),
            vue.etapes
        );
    }

    #[test]
    fn ruby_prisma_minitest_reel_garde_les_faits_et_ecarte_le_bruit() {
        let addition = MINITEST_RB.lines().nth(4).expect("ligne 5");
        let soustraction = MINITEST_RB.lines().nth(8).expect("ligne 9");
        assert!(addition.contains("assert_equal 3, 1 + 1"), "{addition}");
        assert!(
            soustraction.contains("assert_equal 2, 2 - 1"),
            "{soustraction}"
        );
        let vue = vue_exec(&["ruby", "minitest_test.rb"], MINITEST_RAW);
        let faits = [
            "Run options: --seed 64567",
            "1) Failure:",
            "TestMath#test_subtraction [minitest_test.rb:9]:",
            "Expected: 2",
            "Actual: 1",
            "2) Failure:",
            "TestMath#test_addition [minitest_test.rb:5]:",
            "Expected: 3",
            "Actual: 2",
            "2 runs, 2 assertions, 2 failures, 0 errors, 0 skips",
        ];
        let bruits = ["# Running:", "Finished in 0.005124s", "390.3469 runs/s"];
        let probleme = bilan(&vue.filtre, "toml:minitest", &vue.sortie, &faits, &bruits);
        assert!(probleme.is_empty(), "ASSERT {probleme}");
        assert!(
            !vue.sortie.lines().any(|ligne| ligne.trim() == "FF"),
            "bruit gardé: FF\n{}",
            vue.sortie
        );
        assert!(
            vue.texte_filtre.len() < MINITEST_RAW.len(),
            "ASSERT octets minitest brut={} filtre={}",
            MINITEST_RAW.len(),
            vue.texte_filtre.len()
        );
        println!(
            "ASSERT octets minitest brut={} filtre={} apres_porte={} etapes={}",
            MINITEST_RAW.len(),
            vue.texte_filtre.len(),
            vue.sortie.len(),
            vue.etapes
        );
    }

    #[test]
    fn ruby_prisma_prisma_validate_reel_garde_p1012_et_le_message() {
        let vue = vue_exec(&["prisma", "validate"], PRISMA_RAW);
        let faits = [
            "Error: Prisma schema validation - (validate wasm)",
            "Error code: P1012",
            "Error validating field `posts` in model `User`",
            "missing an opposite relation field on the model `Post`",
            "prisma/schema.prisma:15",
            "posts Post[]",
            "deliberate error: missing relation fields",
            "Validation Error Count: 1",
            "[Context: validate]",
        ];
        let bruits = [
            "Environment variables loaded from .env",
            "Prisma schema loaded from prisma/schema.prisma",
            "Prisma CLI Version : 5.15.0",
        ];
        let probleme = bilan(&vue.filtre, "toml:prisma", &vue.sortie, &faits, &bruits);
        assert!(probleme.is_empty(), "ASSERT {probleme}");
        assert!(
            !vue.sortie.contains('\u{1b}'),
            "bruit gardé: séquence ANSI\n{}",
            vue.sortie
        );
        assert!(
            vue.texte_filtre.len() < PRISMA_RAW.len(),
            "ASSERT octets prisma brut={} filtre={}",
            PRISMA_RAW.len(),
            vue.texte_filtre.len()
        );
        println!(
            "ASSERT octets prisma brut={} filtre={} apres_porte={} etapes={}",
            PRISMA_RAW.len(),
            vue.texte_filtre.len(),
            vue.sortie.len(),
            vue.etapes
        );
    }

    /// Forme d'un échec `migrate`, absente du corpus capturé (seulement
    /// `prisma validate` / P1012). Ce n'est pas une sortie réelle.
    #[test]
    fn ruby_prisma_p3009_synthetique_garde_lexplication() {
        let raw = "\
Environment variables loaded from .env
Error: P3009

migrate found failed migrations in the target database, new migrations will not be applied.
The `20240101120000_init` migration started at 2024-01-01 failed

Prisma CLI Version : 5.15.0
";
        let vue = vue_exec(&["prisma", "migrate", "dev"], raw);
        let faits = [
            "Error: P3009",
            "migrate found failed migrations in the target database, new migrations will not be applied.",
            "The `20240101120000_init` migration started at 2024-01-01 failed",
        ];
        let bruits = [
            "Environment variables loaded from .env",
            "Prisma CLI Version : 5.15.0",
        ];
        let probleme = bilan(&vue.filtre, "prisma-migrate", &vue.sortie, &faits, &bruits);
        assert!(probleme.is_empty(), "ASSERT {probleme}");
    }

    #[test]
    fn ruby_prisma_ne_filtre_pas_une_commande_etrangere() {
        let (filtre, sortie) =
            filter_command_output(&["cat".into(), "prisma/schema.prisma".into()], PRISMA_RAW);
        assert_ne!(filtre, "toml:prisma", "{filtre}");
        assert!(
            sortie.contains("Environment variables loaded from .env"),
            "le brut d'une commande étrangère ne doit pas être filtré comme prisma : {sortie}"
        );
        let (filtre, sortie) =
            filter_command_output(&["ruby".into(), "script.rb".into()], RSPEC_RAW);
        assert_ne!(filtre, "toml:rspec", "{filtre}");
        assert!(
            sortie.contains("files took 0.12175 seconds to load"),
            "bruit encore là sans le filtre rspec, filtre={filtre}"
        );
    }

    #[test]
    fn ruby_prisma_routage_des_lanceurs() {
        let cas = [
            (vec!["rspec", "rspec_test.rb"], "toml:rspec"),
            (vec!["bundle", "exec", "rspec", "spec"], "toml:rspec"),
            (vec!["ruby", "-S", "rspec"], "toml:rspec"),
            (vec!["ruby", "minitest_test.rb"], "toml:minitest"),
            (
                vec!["bundle", "exec", "ruby", "minitest_test.rb"],
                "toml:minitest",
            ),
            (vec!["rake", "test"], "toml:minitest"),
            (vec!["rails", "test"], "toml:minitest"),
            (vec!["prisma", "validate"], "toml:prisma"),
            (vec!["prisma", "migrate", "dev"], "prisma-migrate"),
            (vec!["prisma", "generate"], "toml:prisma"),
            (vec!["npx", "prisma", "validate"], "toml:prisma"),
            (
                vec!["npx", "--yes", "prisma", "migrate", "dev"],
                "prisma-migrate",
            ),
            (vec!["yarn", "prisma", "generate"], "toml:prisma"),
            (vec!["pnpm", "exec", "prisma", "validate"], "toml:prisma"),
        ];
        for (args, attendu) in cas {
            let cmd: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
            let (filtre, _) = filter_command_output(&cmd, "");
            assert_eq!(filtre, attendu, "commande {args:?}");
        }
    }

    #[test]
    fn ruby_prisma_sans_filtre_generique_le_bruit_reste() {
        let rspec = filter_generic(RSPEC_RAW);
        assert!(
            rspec.contains("files took 0.12175 seconds to load"),
            "ASSERT bruit gardé absent du générique rspec"
        );
        assert!(rspec.contains("got: 2"));
        let minitest = filter_generic(MINITEST_RAW);
        assert!(
            minitest.contains("Finished in 0.005124s"),
            "ASSERT bruit gardé absent du générique minitest"
        );
        assert!(minitest.contains("TestMath#test_subtraction"));
        let prisma = filter_generic(PRISMA_RAW);
        assert!(
            prisma.contains("Environment variables loaded from .env"),
            "ASSERT bruit gardé absent du générique prisma"
        );
        assert!(prisma.contains("Error code: P1012"));
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
// Historical lossless-codec regressions remain covered independently of the
// explicitly requested native view. native parity has its own external binary oracle.
#[cfg(test)]
fn legacy_lossless_filter(command: &[String], raw: &str) -> (String, String) {
    if let Some((name, out)) = lossless_filters::filter(command, raw) {
        (format!("lossless:{name}"), out)
    } else {
        route_command_filter(command, raw)
    }
}

#[test]
fn pytest_filter_keeps_failing_statement_and_test_header() {
    let raw = "=== FAILURES ===\n___ test_x ___\n\n    def test_x():\n        data = load()\n>       assert check(data)\nE       AssertionError\n\ntests/test_a.py:9: AssertionError\n=== short test summary info ===\nFAILED tests/test_a.py::test_x\n1 failed in 0.1s\n";
    let filtered = legacy_lossless_filter(&["pytest".into()], raw).1;
    assert!(filtered.contains(">       assert check(data)"));
    assert!(filtered.contains("___ test_x ___"));
    assert!(filtered.contains("tests/test_a.py:9"));
    assert!(filtered.contains("data = load()"));
}

#[test]
fn calendar_cycles_include_leap_centuries_and_pre_epoch_days() {
    for (days, date) in [
        (0, (1970, 1, 1)),
        (-1, (1969, 12, 31)),
        (11016, (2000, 2, 29)),
        (-25509, (1900, 2, 28)),
        (146097, (2370, 1, 1)),
        (-146097, (1570, 1, 1)),
    ] {
        assert_eq!(civil_from_days(days), date);
    }
}
