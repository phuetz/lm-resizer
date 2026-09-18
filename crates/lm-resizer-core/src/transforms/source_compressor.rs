//! Syntax-aware and conservative source-code compressor.
//!
//! Preserves signatures, public exports, type structures, entry points, and
//! leading doc-comments for Rust, TypeScript/JavaScript, and Python.
//! Summarizes repetitive blocks, long imports, and function bodies with explicit
//! line omission notes and CCR retrieval markers.
//!
//! Falls back gracefully to conservative line-by-line comment/blank stripping
//! if the language is unsupported or syntax analysis fails.

use regex::Regex;
use std::sync::LazyLock;

use crate::ccr::{compute_key, CcrStore};
use crate::transforms::retention_advice::{RetentionAdvice, RetentionRange};

/// Supported target languages for structural compression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceLanguage {
    Rust,
    Python,
    TypeScript,
    JavaScript,
}

impl SourceLanguage {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::TypeScript => "typescript",
            Self::JavaScript => "javascript",
        }
    }
}

/// AST Symbol extracted via runtime query to Code Explorer or external tool.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct AstSymbol {
    pub name: String,
    pub label: String,
    pub start_line: usize,
    pub end_line: usize,
}

/// Mode of Code Explorer runtime integration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CodeExplorerMode {
    /// Automatically check if `code-explorer` is present at runtime and try it;
    /// falls back to embedded regex/brace counter if absent or on query failure.
    #[default]
    Auto,
    /// Force CLI Cypher query against `code-explorer`.
    Cli,
    /// Force MCP stdio JSON-RPC query against `code-explorer`.
    Mcp,
    /// Disable external Code Explorer; always use embedded lightweight analysis.
    Disabled,
}

impl CodeExplorerMode {
    pub fn from_env() -> Self {
        if let Ok(v) = std::env::var("LM_RESIZER_NO_CODE_EXPLORER") {
            if v == "1" || v.eq_ignore_ascii_case("true") {
                return Self::Disabled;
            }
        }
        if let Ok(v) = std::env::var("LM_RESIZER_CODE_EXPLORER_MODE") {
            match v.to_ascii_lowercase().as_str() {
                "disabled" | "off" | "none" => return Self::Disabled,
                "cli" => return Self::Cli,
                "mcp" => return Self::Mcp,
                "auto" => return Self::Auto,
                _ => {}
            }
        }
        Self::Auto
    }
}

/// Result of source code compression with telemetry metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceCompressionResult {
    pub original: String,
    pub compressed: String,
    pub removed_comment_lines: usize,
    pub removed_blank_lines: usize,
    pub omitted_body_lines: usize,
    pub omitted_import_lines: usize,
    pub omitted_functions: usize,
    pub language: Option<SourceLanguage>,
    pub ccr_key: Option<String>,
    pub is_approximate: bool,
    pub engine_used: &'static str,
}

#[derive(Debug, Clone)]
pub struct SourceCompressor {
    max_blank_run: usize,
    min_lines_for_structural: usize,
    code_explorer_mode: CodeExplorerMode,
    custom_bin_path: Option<String>,
}

impl Default for SourceCompressor {
    fn default() -> Self {
        Self {
            max_blank_run: 1,
            min_lines_for_structural: 6,
            code_explorer_mode: CodeExplorerMode::from_env(),
            custom_bin_path: None,
        }
    }
}

impl SourceCompressor {
    pub fn new(max_blank_run: usize, min_lines_for_structural: usize) -> Self {
        Self {
            max_blank_run,
            min_lines_for_structural,
            code_explorer_mode: CodeExplorerMode::from_env(),
            custom_bin_path: None,
        }
    }

    pub fn embedded_only() -> Self {
        Self {
            max_blank_run: 1,
            min_lines_for_structural: 6,
            code_explorer_mode: CodeExplorerMode::Disabled,
            custom_bin_path: None,
        }
    }

    pub fn with_code_explorer_mode(mut self, mode: CodeExplorerMode) -> Self {
        self.code_explorer_mode = mode;
        self
    }

    pub fn with_custom_bin(mut self, bin_path: impl Into<String>) -> Self {
        self.custom_bin_path = Some(bin_path.into());
        self
    }

    /// Compress source code with optional CCR store persistence.
    pub fn compress_with_store(
        &self,
        input: &str,
        store: Option<&dyn CcrStore>,
    ) -> SourceCompressionResult {
        self.compress_path_with_store(input, None, None, store)
    }

    /// Standard compression without pre-wired store.
    pub fn compress(&self, input: &str) -> SourceCompressionResult {
        self.compress_with_store(input, None)
    }

    /// Compress source code with optional file/repository context for Code Explorer runtime integration.
    pub fn compress_path_with_store(
        &self,
        input: &str,
        file_path: Option<&str>,
        repo_path: Option<&str>,
        store: Option<&dyn CcrStore>,
    ) -> SourceCompressionResult {
        if input.is_empty() {
            return SourceCompressionResult {
                original: String::new(),
                compressed: String::new(),
                removed_comment_lines: 0,
                removed_blank_lines: 0,
                omitted_body_lines: 0,
                omitted_import_lines: 0,
                omitted_functions: 0,
                language: None,
                ccr_key: None,
                is_approximate: true,
                engine_used: "noop",
            };
        }

        let total_input_lines = input.lines().count();

        // Attempt structural compression if file has sufficient lines
        if total_input_lines >= self.min_lines_for_structural {
            if let Some(lang) = detect_language(input) {
                let mut structural_opt: Option<(StructuralOutput, bool, &'static str)> = None;

                // 1. Try Code Explorer runtime analysis if enabled and file path provided
                if self.code_explorer_mode != CodeExplorerMode::Disabled {
                    if let Some(fp) = file_path {
                        let bin = resolve_code_explorer_bin(self.custom_bin_path.as_deref());
                        let symbols = match self.code_explorer_mode {
                            CodeExplorerMode::Mcp => query_symbols_mcp(&bin, fp, repo_path),
                            _ => query_symbols_cli(&bin, fp, repo_path),
                        };

                        if let Some(syms) = symbols {
                            if let Some(res) = compress_with_ast_symbols(input, lang, &syms) {
                                let st = build_structural_output(input, lang, res, false);
                                structural_opt = Some((st, false, "code-explorer"));
                            }
                        }
                    }
                }

                // 2. Fall back to embedded lightweight analysis (regex + brace counting)
                if structural_opt.is_none() {
                    if let Some(res) = match lang {
                        SourceLanguage::Rust => compress_rust(input),
                        SourceLanguage::Python => compress_python(input),
                        SourceLanguage::TypeScript => compress_ts_js(input, true),
                        SourceLanguage::JavaScript => compress_ts_js(input, false),
                    } {
                        if res.omitted_body_lines > 0 || res.omitted_import_lines > 0 {
                            let st = build_structural_output(input, lang, res, true);
                            structural_opt = Some((st, true, "embedded-regex-braces"));
                        }
                    }
                }

                if let Some((structural, is_approximate, engine_used)) = structural_opt {
                    if structural.omitted_body_lines > 0
                        || structural.omitted_import_lines > 0
                        || structural.compressed.len() < input.len()
                    {
                        let ccr_key = compute_key(input.as_bytes());
                        if let Some(s) = store {
                            s.put(&ccr_key, input);
                        }
                        return SourceCompressionResult {
                            original: input.to_string(),
                            compressed: structural.compressed,
                            removed_comment_lines: 0,
                            removed_blank_lines: 0,
                            omitted_body_lines: structural.omitted_body_lines,
                            omitted_import_lines: structural.omitted_import_lines,
                            omitted_functions: structural.omitted_functions,
                            language: Some(lang),
                            ccr_key: Some(ccr_key),
                            is_approximate,
                            engine_used,
                        };
                    }
                }
            }
        }

        // Fallback: conservative line-by-line comment and blank line stripping
        self.compress_conservative(input)
    }

    /// Compress using externally provided AST symbols directly.
    pub fn compress_with_symbols(
        &self,
        input: &str,
        lang: SourceLanguage,
        symbols: &[AstSymbol],
        store: Option<&dyn CcrStore>,
    ) -> Option<SourceCompressionResult> {
        let res = compress_with_ast_symbols(input, lang, symbols)?;
        let structural = build_structural_output(input, lang, res, false);
        let ccr_key = compute_key(input.as_bytes());
        if let Some(s) = store {
            s.put(&ccr_key, input);
        }
        Some(SourceCompressionResult {
            original: input.to_string(),
            compressed: structural.compressed,
            removed_comment_lines: 0,
            removed_blank_lines: 0,
            omitted_body_lines: structural.omitted_body_lines,
            omitted_import_lines: structural.omitted_import_lines,
            omitted_functions: structural.omitted_functions,
            language: Some(lang),
            ccr_key: Some(ccr_key),
            is_approximate: false,
            engine_used: "code-explorer",
        })
    }

    /// Conservative line-by-line compression fallback.
    /// Line-by-line compression that never touches a line an advisor protects.
    ///
    /// This is the point of the whole advice mechanism. Cutting by lines keeps
    /// the beginning of a file, which is rarely the part that matters; with
    /// advice, the parts that matter survive wherever they sit in the file.
    ///
    /// Advice only ever protects. A line nobody mentioned is compressed by the
    /// ordinary rules, never dropped *because* it was unmentioned — an advisor
    /// that says nothing about a range is an advisor that does not know, and a
    /// call graph is known to miss calls through interfaces and dynamic
    /// dispatch. With empty advice this behaves exactly like
    /// [`Self::compress_conservative`].
    pub fn compress_with_advice(
        &self,
        input: &str,
        advice: &RetentionAdvice,
        store: Option<&dyn CcrStore>,
    ) -> SourceCompressionResult {
        if advice.ranges.is_empty() {
            return self.compress_conservative(input);
        }

        let mut output = String::with_capacity(input.len());
        let mut blank_run = 0usize;
        let mut removed_comment_lines = 0usize;
        let mut removed_blank_lines = 0usize;
        let mut protected_lines = 0usize;

        for (index, line) in input.lines().enumerate() {
            // Les numéros de ligne d'un conseiller commencent à 1, comme ceux
            // d'un éditeur et d'un compilateur.
            if advice.protects(index + 1) {
                protected_lines += 1;
                blank_run = 0;
                output.push_str(line);
                output.push('\n');
                continue;
            }

            let trimmed = line.trim_start();

            if is_full_line_comment(trimmed) {
                removed_comment_lines += 1;
                continue;
            }

            if trimmed.is_empty() {
                blank_run += 1;
                if blank_run > self.max_blank_run {
                    removed_blank_lines += 1;
                    continue;
                }
            } else {
                blank_run = 0;
            }

            output.push_str(line);
            output.push('\n');
        }

        if !input.ends_with('\n') && output.ends_with('\n') {
            output.pop();
        }

        // Une compression qui grossit n'est pas une compression. On rend
        // l'original, qui est toujours correct.
        if output.len() >= input.len() {
            output.clear();
            output.push_str(input);
        }

        // Ce qu'on retire doit rester récupérable, sans quoi aucun taux de
        // compression n'est défendable.
        let ccr_key = if output.len() < input.len() {
            let key = compute_key(input.as_bytes());
            if let Some(s) = store {
                s.put(&key, input);
            }
            Some(key)
        } else {
            None
        };

        let _ = protected_lines;
        SourceCompressionResult {
            original: input.to_string(),
            compressed: output,
            removed_comment_lines,
            removed_blank_lines,
            omitted_body_lines: 0,
            omitted_import_lines: 0,
            omitted_functions: 0,
            language: detect_language(input),
            ccr_key,
            is_approximate: true,
            engine_used: "advice-guided",
        }
    }

    fn compress_conservative(&self, input: &str) -> SourceCompressionResult {
        let mut output = String::with_capacity(input.len());
        let mut blank_run = 0usize;
        let mut removed_comment_lines = 0usize;
        let mut removed_blank_lines = 0usize;

        for line in input.lines() {
            let trimmed = line.trim_start();

            if is_full_line_comment(trimmed) {
                removed_comment_lines += 1;
                continue;
            }

            if trimmed.is_empty() {
                blank_run += 1;
                if blank_run > self.max_blank_run {
                    removed_blank_lines += 1;
                    continue;
                }
            } else {
                blank_run = 0;
            }

            output.push_str(line);
            output.push('\n');
        }

        if !input.ends_with('\n') && output.ends_with('\n') {
            output.pop();
        }

        if output.len() >= input.len() {
            output.clear();
            output.push_str(input);
        }

        SourceCompressionResult {
            original: input.to_string(),
            compressed: output,
            removed_comment_lines,
            removed_blank_lines,
            omitted_body_lines: 0,
            omitted_import_lines: 0,
            omitted_functions: 0,
            language: None,
            ccr_key: None,
            is_approximate: true,
            engine_used: "conservative-fallback",
        }
    }
}

/// Turn Code Explorer symbols into advice — one producer among several.
///
/// Deliberately a free function on the *output* type rather than a method on
/// the compressor: it proves the interface is not Code Explorer's. `ctags`,
/// tree-sitter or a symbol grep produce the same document, and the compressor
/// cannot tell which one wrote it.
///
/// Weighting is by span length, which is a poor proxy for importance and is
/// meant to be replaced. The honest signal would be in-degree and out-degree
/// from the graph, which `code-explorer` does not expose in machine-readable
/// form today: at 0.2.1 `--json` is offered by `doctor`, `hotspots`, `coupling`
/// and `ownership`, and not by `impact`, which is the one that carries it.
pub fn advice_from_symbols(symbols: &[AstSymbol]) -> RetentionAdvice {
    RetentionAdvice {
        advisor: Some("code-explorer".to_string()),
        ranges: symbols
            .iter()
            .filter(|s| s.start_line >= 1 && s.end_line >= s.start_line)
            .map(|s| RetentionRange {
                start_line: s.start_line,
                end_line: s.end_line,
                weight: (s.end_line - s.start_line + 1) as f64,
                label: Some(s.name.clone()),
            })
            .collect(),
    }
}

pub fn resolve_code_explorer_bin(custom: Option<&str>) -> String {
    if let Some(c) = custom {
        return c.to_string();
    }
    if let Ok(b) = std::env::var("LM_RESIZER_CODE_EXPLORER_PATH") {
        if !b.trim().is_empty() {
            return b;
        }
    }
    if let Ok(b) = std::env::var("CODE_EXPLORER_BIN") {
        if !b.trim().is_empty() {
            return b;
        }
    }
    "code-explorer".to_string()
}

pub fn is_code_explorer_available(custom_bin: Option<&str>) -> bool {
    if CodeExplorerMode::from_env() == CodeExplorerMode::Disabled {
        return false;
    }
    let bin = resolve_code_explorer_bin(custom_bin);
    std::process::Command::new(&bin)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn query_symbols_cli(
    bin: &str,
    file_path: &str,
    repo_path: Option<&str>,
) -> Option<Vec<AstSymbol>> {
    let mut cmd = std::process::Command::new(bin);
    cmd.arg("cypher");
    if let Some(rp) = repo_path {
        cmd.arg("--repo").arg(rp);
    }
    let norm_path = file_path.replace('\\', "/");
    let query = format!(
        "MATCH (n) WHERE n.filePath = '{0}' OR n.filePath ENDS WITH '{0}' RETURN n.name, n.startLine, n.endLine, n._label",
        norm_path
    );
    cmd.arg(query);
    let output = cmd.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    parse_cypher_symbols(&stdout)
}

pub fn parse_cypher_symbols(stdout: &str) -> Option<Vec<AstSymbol>> {
    let json_start = stdout.find('[')?;
    let json_end = stdout.rfind(']')? + 1;
    let json_str = &stdout[json_start..json_end];

    #[derive(serde::Deserialize)]
    struct Row {
        #[serde(rename = "n.name")]
        name: Option<String>,
        #[serde(rename = "n._label")]
        label: Option<String>,
        #[serde(rename = "n.startLine")]
        start_line: Option<usize>,
        #[serde(rename = "n.endLine")]
        end_line: Option<usize>,
    }

    let rows: Vec<Row> = serde_json::from_str(json_str).ok()?;
    let mut symbols = Vec::new();
    for r in rows {
        if let (Some(name), Some(label), Some(start_line), Some(end_line)) =
            (r.name, r.label, r.start_line, r.end_line)
        {
            symbols.push(AstSymbol {
                name,
                label,
                start_line,
                end_line,
            });
        }
    }
    if symbols.is_empty() {
        None
    } else {
        Some(symbols)
    }
}

pub fn query_symbols_mcp(
    bin: &str,
    file_path: &str,
    repo_path: Option<&str>,
) -> Option<Vec<AstSymbol>> {
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};

    let mut child = Command::new(bin)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let mut stdin = child.stdin.take()?;
    let stdout = child.stdout.take()?;
    let mut reader = BufReader::new(stdout);

    // 1. Initialize
    let init_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "lm-resizer", "version": "0.2.2"}
        }
    });
    writeln!(stdin, "{}", serde_json::to_string(&init_req).ok()?).ok()?;
    stdin.flush().ok()?;

    let mut line1 = String::new();
    reader.read_line(&mut line1).ok()?;

    // 2. Call read_file
    let call_req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "read_file",
            "arguments": {
                "path": file_path,
                "repo": repo_path.unwrap_or(".")
            }
        }
    });
    writeln!(stdin, "{}", serde_json::to_string(&call_req).ok()?).ok()?;
    stdin.flush().ok()?;

    let mut line2 = String::new();
    reader.read_line(&mut line2).ok()?;

    let _ = child.kill();
    let _ = child.wait();

    parse_mcp_symbols(&line2)
}

pub fn parse_mcp_symbols(stdout_line: &str) -> Option<Vec<AstSymbol>> {
    #[derive(serde::Deserialize)]
    struct McpSym {
        name: Option<String>,
        label: Option<String>,
        #[serde(rename = "startLine")]
        start_line: Option<usize>,
        #[serde(rename = "endLine")]
        end_line: Option<usize>,
    }

    #[derive(serde::Deserialize)]
    struct McpMeta {
        symbols: Option<Vec<McpSym>>,
    }

    #[derive(serde::Deserialize)]
    struct McpResult {
        #[serde(rename = "_meta")]
        meta: Option<McpMeta>,
    }

    #[derive(serde::Deserialize)]
    struct McpResponse {
        result: Option<McpResult>,
    }

    let resp: McpResponse = serde_json::from_str(stdout_line.trim()).ok()?;
    let syms = resp.result?.meta?.symbols?;
    let mut symbols = Vec::new();
    for s in syms {
        if let (Some(name), Some(label), Some(start_line), Some(end_line)) =
            (s.name, s.label, s.start_line, s.end_line)
        {
            symbols.push(AstSymbol {
                name,
                label,
                start_line,
                end_line,
            });
        }
    }
    if symbols.is_empty() {
        None
    } else {
        Some(symbols)
    }
}

fn compress_with_ast_symbols(
    input: &str,
    lang: SourceLanguage,
    symbols: &[AstSymbol],
) -> Option<InternalStructural> {
    let lines: Vec<&str> = input.lines().collect();
    let mut output: Vec<String> = Vec::with_capacity(lines.len());
    let mut omitted_body_lines = 0usize;
    let mut omitted_import_lines = 0usize;
    let mut omitted_functions = 0usize;

    let mut target_symbols: Vec<&AstSymbol> = symbols
        .iter()
        .filter(|s| {
            (s.label == "Function" || s.label == "Method")
                && s.start_line > 0
                && s.end_line > s.start_line
                && s.end_line <= lines.len()
        })
        .collect();
    target_symbols.sort_by_key(|s| s.start_line);

    let mut i = 0;
    let mut sym_idx = 0;

    while i < lines.len() {
        let line_num = i + 1;
        let line = lines[i];
        let trimmed = line.trim();

        // Imports folding (> 3 lines)
        let is_import_start = match lang {
            SourceLanguage::Rust => trimmed.starts_with("use ") || trimmed.starts_with("pub use "),
            SourceLanguage::Python => {
                trimmed.starts_with("import ") || trimmed.starts_with("from ")
            }
            SourceLanguage::TypeScript | SourceLanguage::JavaScript => {
                trimmed.starts_with("import ")
                    || (trimmed.starts_with("const ") && trimmed.contains("= require("))
            }
        };

        if is_import_start && sym_idx == 0 {
            let mut j = i;
            let mut import_lines = 0usize;
            while j < lines.len() {
                let lt = lines[j].trim();
                let is_import = match lang {
                    SourceLanguage::Rust => {
                        lt.starts_with("use ")
                            || lt.starts_with("pub use ")
                            || (import_lines > 0
                                && (lt.starts_with('{')
                                    || lt.ends_with(';')
                                    || lt.starts_with("};")))
                    }
                    SourceLanguage::Python => {
                        lt.starts_with("import ")
                            || lt.starts_with("from ")
                            || (import_lines > 0
                                && (lt.starts_with('(') || lt.ends_with(')') || lt.ends_with(',')))
                    }
                    SourceLanguage::TypeScript | SourceLanguage::JavaScript => {
                        lt.starts_with("import ")
                            || (lt.starts_with("const ") && lt.contains("= require("))
                            || (import_lines > 0
                                && (lt.starts_with('{')
                                    || lt.ends_with(';')
                                    || lt.starts_with("};")))
                    }
                };
                if is_import {
                    import_lines += 1;
                    j += 1;
                    if (lang != SourceLanguage::Python && lt.ends_with(';'))
                        || (lang == SourceLanguage::Python
                            && !lt.ends_with('\\')
                            && !lt.starts_with('('))
                    {
                        if j < lines.len() {
                            let next_t = lines[j].trim();
                            let is_next_import = match lang {
                                SourceLanguage::Rust => {
                                    next_t.starts_with("use ") || next_t.starts_with("pub use ")
                                }
                                SourceLanguage::Python => {
                                    next_t.starts_with("import ") || next_t.starts_with("from ")
                                }
                                SourceLanguage::TypeScript | SourceLanguage::JavaScript => {
                                    next_t.starts_with("import ")
                                        || (next_t.starts_with("const ")
                                            && next_t.contains("= require("))
                                }
                            };
                            if is_next_import {
                                continue;
                            }
                        }
                        break;
                    }
                } else {
                    break;
                }
            }

            if import_lines > 3 {
                output.push(lines[i].to_string());
                output.push(lines[i + 1].to_string());
                let omitted = import_lines - 2;
                let comment = match lang {
                    SourceLanguage::Python => {
                        format!("# ... [{} import lines omitted] ...", omitted)
                    }
                    _ => format!("// ... [{} import lines omitted] ...", omitted),
                };
                output.push(comment);
                omitted_import_lines += omitted;
                i = j;
                continue;
            }
        }

        // Check if current line starts a function/method AST symbol
        if sym_idx < target_symbols.len() && line_num == target_symbols[sym_idx].start_line {
            let sym = target_symbols[sym_idx];
            sym_idx += 1;

            let start_idx = sym.start_line - 1;
            let end_idx = sym.end_line - 1;

            let fn_indent = lines[start_idx]
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect::<String>();

            match lang {
                SourceLanguage::Rust | SourceLanguage::TypeScript | SourceLanguage::JavaScript => {
                    let mut open_brace_idx = start_idx;
                    while open_brace_idx <= end_idx {
                        if lines[open_brace_idx].contains('{') {
                            break;
                        }
                        open_brace_idx += 1;
                    }

                    if open_brace_idx <= end_idx && open_brace_idx < end_idx {
                        for line_ref in &lines[start_idx..=open_brace_idx] {
                            output.push((*line_ref).to_string());
                        }
                        let body_lines_count = end_idx - open_brace_idx - 1;
                        if body_lines_count > 0 {
                            output.push(format!(
                                "{}    /* ... [{} lines omitted: function body] ... */",
                                fn_indent, body_lines_count
                            ));
                            omitted_body_lines += body_lines_count;
                            omitted_functions += 1;
                        }
                        output.push(lines[end_idx].to_string());
                        i = end_idx + 1;
                        continue;
                    }
                }
                SourceLanguage::Python => {
                    let mut colon_idx = start_idx;
                    while colon_idx <= end_idx {
                        if lines[colon_idx].trim_end().ends_with(':') {
                            break;
                        }
                        colon_idx += 1;
                    }

                    if colon_idx <= end_idx && colon_idx < end_idx {
                        for line_ref in &lines[start_idx..=colon_idx] {
                            output.push((*line_ref).to_string());
                        }

                        let mut body_start = colon_idx + 1;
                        if body_start < lines.len() {
                            let next_t = lines[body_start].trim();
                            if next_t.starts_with("\"\"\"") || next_t.starts_with("'''") {
                                let quote = if next_t.starts_with("\"\"\"") {
                                    "\"\"\""
                                } else {
                                    "'''"
                                };
                                output.push(lines[body_start].to_string());
                                if !(next_t.len() > 3 && next_t[3..].contains(quote)) {
                                    body_start += 1;
                                    while body_start <= end_idx {
                                        let dl = lines[body_start];
                                        output.push(dl.to_string());
                                        if dl.contains(quote) {
                                            body_start += 1;
                                            break;
                                        }
                                        body_start += 1;
                                    }
                                } else {
                                    body_start += 1;
                                }
                            }
                        }

                        if body_start <= end_idx {
                            let body_lines_count = end_idx - body_start + 1;
                            output.push(format!(
                                "{}    # ... [{} lines omitted: function body] ...",
                                fn_indent, body_lines_count
                            ));
                            output.push(format!("{}    ...", fn_indent));
                            omitted_body_lines += body_lines_count;
                            omitted_functions += 1;
                        }
                        i = end_idx + 1;
                        continue;
                    }
                }
            }
        }

        // Regular line
        if is_full_line_comment(trimmed)
            && !trimmed.starts_with("///")
            && !trimmed.starts_with("//!")
            && !trimmed.starts_with("/**")
        {
            i += 1;
            continue;
        }

        output.push(line.to_string());
        i += 1;
    }

    if omitted_body_lines == 0 && omitted_import_lines == 0 {
        return None;
    }

    Some(InternalStructural {
        lines: output,
        omitted_body_lines,
        omitted_import_lines,
        omitted_functions,
    })
}

fn build_structural_output(
    input: &str,
    lang: SourceLanguage,
    res: InternalStructural,
    is_approximate: bool,
) -> StructuralOutput {
    let ccr_key = compute_key(input.as_bytes());
    let orig_lines = input.lines().count();
    let comp_lines = res.lines.len() + 1; // +1 for banner line
    let total_omitted = res.omitted_body_lines + res.omitted_import_lines;

    let mut parts = Vec::new();
    if res.omitted_functions > 0 {
        parts.push(format!(
            "{} function {}",
            res.omitted_functions,
            if res.omitted_functions == 1 {
                "body"
            } else {
                "bodies"
            }
        ));
    }
    if res.omitted_import_lines > 0 {
        parts.push(format!("{} import lines", res.omitted_import_lines));
    }
    let detail = parts.join(", ");

    let banner = match lang {
        SourceLanguage::Rust | SourceLanguage::TypeScript | SourceLanguage::JavaScript => {
            if is_approximate {
                format!(
                    "// [Structure approximative: {} lines -> {} lines ({} lines omitted: {}). Retrieve full source: hash={}]",
                    orig_lines, comp_lines, total_omitted, detail, ccr_key
                )
            } else {
                format!(
                    "// [Structure syntaxique (Code Explorer): {} lines -> {} lines ({} lines omitted: {}). Retrieve full source: hash={}]",
                    orig_lines, comp_lines, total_omitted, detail, ccr_key
                )
            }
        }
        SourceLanguage::Python => {
            if is_approximate {
                format!(
                    "# [Structure approximative: {} lines -> {} lines ({} lines omitted: {}). Retrieve full source: hash={}]",
                    orig_lines, comp_lines, total_omitted, detail, ccr_key
                )
            } else {
                format!(
                    "# [Structure syntaxique (Code Explorer): {} lines -> {} lines ({} lines omitted: {}). Retrieve full source: hash={}]",
                    orig_lines, comp_lines, total_omitted, detail, ccr_key
                )
            }
        }
    };

    let mut final_output = String::with_capacity(input.len());
    let mut lines_iter = res.lines.into_iter();

    if let Some(first) = lines_iter.next() {
        if first.starts_with("#!") {
            final_output.push_str(&first);
            final_output.push('\n');
            final_output.push_str(&banner);
            final_output.push('\n');
        } else {
            final_output.push_str(&banner);
            final_output.push('\n');
            final_output.push_str(&first);
            final_output.push('\n');
        }
    } else {
        final_output.push_str(&banner);
        final_output.push('\n');
    }

    for l in lines_iter {
        final_output.push_str(&l);
        final_output.push('\n');
    }

    if !input.ends_with('\n') && final_output.ends_with('\n') {
        final_output.pop();
    }

    StructuralOutput {
        compressed: final_output,
        omitted_body_lines: res.omitted_body_lines,
        omitted_import_lines: res.omitted_import_lines,
        omitted_functions: res.omitted_functions,
    }
}

pub(crate) struct InternalStructural {
    lines: Vec<String>,
    omitted_body_lines: usize,
    omitted_import_lines: usize,
    omitted_functions: usize,
}

struct StructuralOutput {
    compressed: String,
    omitted_body_lines: usize,
    omitted_import_lines: usize,
    omitted_functions: usize,
}

fn is_full_line_comment(trimmed: &str) -> bool {
    if trimmed.starts_with("#!") {
        return false;
    }

    trimmed.starts_with("//")
        || trimmed.starts_with("///")
        || trimmed.starts_with("//!")
        || trimmed.starts_with('#')
        || trimmed.starts_with("--")
        || trimmed.starts_with(';')
        || trimmed.starts_with("<!--")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.starts_with("*/")
}

// ─── Language Detection ────────────────────────────────────────────────

pub fn detect_language(input: &str) -> Option<SourceLanguage> {
    if input.trim().is_empty() {
        return None;
    }

    if let Some(first_line) = input.lines().next() {
        if first_line.starts_with("#!") {
            let lower = first_line.to_lowercase();
            if lower.contains("python") {
                return Some(SourceLanguage::Python);
            }
            if lower.contains("node") || lower.contains("deno") || lower.contains("bun") {
                if input.lines().take(50).any(is_ts_specific_line) {
                    return Some(SourceLanguage::TypeScript);
                }
                return Some(SourceLanguage::JavaScript);
            }
        }
    }

    let mut rust_score = 0usize;
    let mut py_score = 0usize;
    let mut ts_score = 0usize;
    let mut js_score = 0usize;

    for line in input.lines().take(100) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("pub fn ")
            || trimmed.starts_with("fn ")
            || trimmed.starts_with("pub(crate) fn ")
            || trimmed.starts_with("pub struct ")
            || trimmed.starts_with("struct ")
            || trimmed.starts_with("pub enum ")
            || trimmed.starts_with("enum ")
            || trimmed.starts_with("impl ")
            || trimmed.starts_with("impl<")
            || trimmed.starts_with("trait ")
            || trimmed.starts_with("pub trait ")
            || trimmed.starts_with("use ")
            || trimmed.starts_with("pub use ")
            || trimmed.starts_with("#[derive")
            || trimmed.starts_with("#[")
            || trimmed.starts_with("//!")
            || trimmed.starts_with("///")
            || trimmed.contains("println!")
            || trimmed.contains("format!")
            || trimmed.contains("let mut ")
            || (trimmed.starts_with("let ") && trimmed.contains(';'))
        {
            rust_score += 2;
        }

        if trimmed.starts_with("def ")
            || trimmed.starts_with("async def ")
            || (trimmed.starts_with("class ") && trimmed.ends_with(':'))
            || (trimmed.starts_with("from ") && trimmed.contains(" import "))
            || (trimmed.starts_with("import ") && !trimmed.contains(';'))
            || (trimmed.starts_with('@') && !trimmed.starts_with("#["))
            || trimmed.starts_with("if __name__ ==")
            || trimmed.starts_with("elif ")
            || trimmed.starts_with("\"\"\"")
            || trimmed.starts_with("'''")
            || (trimmed.starts_with('#')
                && !trimmed.starts_with("#!")
                && !trimmed.starts_with("#["))
        {
            py_score += 2;
        }

        if is_ts_specific_line(trimmed) {
            ts_score += 3;
        }

        if trimmed.starts_with("export function ")
            || trimmed.starts_with("function ")
            || trimmed.starts_with("export const ")
            || (trimmed.starts_with("const ") && trimmed.contains(" = "))
            || (trimmed.starts_with("let ") && trimmed.contains(" = "))
            || trimmed.starts_with("export default ")
            || (trimmed.starts_with("import ") && trimmed.contains(" from "))
            || trimmed.contains("console.log")
            || trimmed.contains("module.exports")
            || trimmed.contains("require(")
        {
            js_score += 2;
        }
    }

    let max_score = rust_score.max(py_score).max(ts_score).max(js_score);
    if max_score < 3 {
        return None;
    }

    if rust_score == max_score
        && rust_score > py_score
        && rust_score > js_score
        && rust_score > ts_score
    {
        Some(SourceLanguage::Rust)
    } else if py_score == max_score
        && py_score > rust_score
        && py_score > js_score
        && py_score > ts_score
    {
        Some(SourceLanguage::Python)
    } else if ts_score > 0 && (ts_score >= js_score || (js_score == max_score && ts_score >= 2)) {
        Some(SourceLanguage::TypeScript)
    } else if js_score == max_score && js_score > rust_score && js_score > py_score {
        Some(SourceLanguage::JavaScript)
    } else {
        None
    }
}

fn is_ts_specific_line(trimmed: &str) -> bool {
    trimmed.starts_with("interface ")
        || trimmed.starts_with("export interface ")
        || (trimmed.starts_with("type ") && trimmed.contains('='))
        || (trimmed.starts_with("export type ") && trimmed.contains('='))
        || trimmed.starts_with("declare ")
        || trimmed.contains(": string")
        || trimmed.contains(": number")
        || trimmed.contains(": boolean")
        || trimmed.contains(": any")
        || trimmed.contains(": void")
        || trimmed.contains(": Promise<")
        || trimmed.contains("as const")
}

// ─── Rust Compression ──────────────────────────────────────────────────

static RUST_FN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^(?:pub(?:\s*\([^)]*\))?\s+)?(?:async\s+)?(?:const\s+)?(?:unsafe\s+)?(?:extern(?:\s+"[^"]+")?\s+)?fn\s+[A-Za-z0-9_]+"#).unwrap()
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RustLexState {
    Normal,
    InBlockComment(usize),
    InString { escaped: bool },
    InRawString { hashes: usize },
    InChar { escaped: bool },
}

fn scan_rust_tokens(line: &str, state: &mut RustLexState) -> (isize, bool) {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut net_braces: isize = 0;
    let mut has_close = false;

    while i < len {
        match *state {
            RustLexState::Normal => {
                if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
                    break;
                }
                if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                    *state = RustLexState::InBlockComment(1);
                    i += 2;
                    continue;
                }
                if bytes[i] == b'"' {
                    *state = RustLexState::InString { escaped: false };
                    i += 1;
                    continue;
                }
                if bytes[i] == b'r' && i + 1 < len {
                    let mut h = 0;
                    let mut j = i + 1;
                    while j < len && bytes[j] == b'#' {
                        h += 1;
                        j += 1;
                    }
                    if j < len && bytes[j] == b'"' {
                        *state = RustLexState::InRawString { hashes: h };
                        i = j + 1;
                        continue;
                    }
                }
                if bytes[i] == b'\'' {
                    if i + 2 < len && bytes[i + 1] != b'\\' && bytes[i + 2] == b'\'' {
                        i += 3;
                        continue;
                    } else if i + 1 < len && bytes[i + 1] == b'\\' {
                        *state = RustLexState::InChar { escaped: true };
                        i += 2;
                        continue;
                    }
                }
                if bytes[i] == b'{' {
                    net_braces += 1;
                } else if bytes[i] == b'}' {
                    net_braces -= 1;
                    has_close = true;
                }
                i += 1;
            }
            RustLexState::InBlockComment(ref mut depth) => {
                if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                    *depth += 1;
                    i += 2;
                } else if i + 1 < len && bytes[i] == b'*' && bytes[i + 1] == b'/' {
                    *depth -= 1;
                    if *depth == 0 {
                        *state = RustLexState::Normal;
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            }
            RustLexState::InString { ref mut escaped } => {
                if *escaped {
                    *escaped = false;
                    i += 1;
                } else if bytes[i] == b'\\' {
                    *escaped = true;
                    i += 1;
                } else if bytes[i] == b'"' {
                    *state = RustLexState::Normal;
                    i += 1;
                } else {
                    i += 1;
                }
            }
            RustLexState::InRawString { hashes } => {
                if bytes[i] == b'"' {
                    let mut matches = true;
                    if i + 1 + hashes <= len {
                        for k in 0..hashes {
                            if bytes[i + 1 + k] != b'#' {
                                matches = false;
                                break;
                            }
                        }
                    } else {
                        matches = false;
                    }
                    if matches {
                        *state = RustLexState::Normal;
                        i += 1 + hashes;
                        continue;
                    }
                }
                i += 1;
            }
            RustLexState::InChar { ref mut escaped } => {
                if *escaped {
                    *escaped = false;
                    i += 1;
                } else if bytes[i] == b'\\' {
                    *escaped = true;
                    i += 1;
                } else if bytes[i] == b'\'' {
                    *state = RustLexState::Normal;
                    i += 1;
                } else {
                    i += 1;
                }
            }
        }
    }

    (net_braces, has_close)
}

fn compress_rust(input: &str) -> Option<InternalStructural> {
    let lines: Vec<&str> = input.lines().collect();
    let mut output: Vec<String> = Vec::with_capacity(lines.len());
    let mut omitted_body_lines = 0usize;
    let mut omitted_import_lines = 0usize;
    let mut omitted_functions = 0usize;

    let mut i = 0;
    let mut global_braces: isize = 0;
    let mut lex_state = RustLexState::Normal;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Check for import block at top-level
        if global_braces == 0 && (trimmed.starts_with("use ") || trimmed.starts_with("pub use ")) {
            let mut import_lines = 0usize;
            let mut j = i;
            while j < lines.len() {
                let lt = lines[j].trim();
                if lt.starts_with("use ")
                    || lt.starts_with("pub use ")
                    || (import_lines > 0
                        && (lt.starts_with('{') || lt.ends_with(';') || lt.starts_with("};")))
                {
                    import_lines += 1;
                    j += 1;
                    // continue until semicolon ends the import
                    if lt.ends_with(';') {
                        // check if next line is another import
                        if j < lines.len() {
                            let next_t = lines[j].trim();
                            if next_t.starts_with("use ") || next_t.starts_with("pub use ") {
                                continue;
                            }
                        }
                        break;
                    }
                } else {
                    break;
                }
            }

            if import_lines > 3 {
                // Keep first 2 lines
                output.push(lines[i].to_string());
                output.push(lines[i + 1].to_string());
                let omitted = import_lines - 2;
                output.push(format!("// ... [{} import lines omitted] ...", omitted));
                omitted_import_lines += omitted;
                i = j;
                continue;
            }
        }

        // Keep doc comments & inner attributes verbatim
        if trimmed.starts_with("///")
            || trimmed.starts_with("//!")
            || trimmed.starts_with("#[")
            || trimmed.starts_with("#![")
        {
            output.push(line.to_string());
            i += 1;
            continue;
        }

        // Check if function definition starts here
        if !trimmed.starts_with("type ")
            && !trimmed.starts_with("pub type ")
            && RUST_FN_RE.is_match(trimmed)
        {
            let fn_indent = line
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect::<String>();
            // Collect function signature lines until `{` or `;`
            let mut sig_lines = Vec::new();
            let mut has_body = false;
            let mut sig_idx = i;

            while sig_idx < lines.len() {
                let s_line = lines[sig_idx];
                let (net, _) = scan_rust_tokens(s_line, &mut lex_state);
                sig_lines.push(s_line);
                if s_line.contains('{') && net > 0 {
                    has_body = true;
                    sig_idx += 1;
                    break;
                }
                if s_line.trim().ends_with(';') {
                    has_body = false;
                    sig_idx += 1;
                    break;
                }
                sig_idx += 1;
            }

            if !has_body {
                for s in sig_lines {
                    output.push(s.to_string());
                }
                i = sig_idx;
                continue;
            }

            // Has body: push signature lines
            for s in sig_lines {
                output.push(s.to_string());
            }

            // Now scan body until depth returns to 0
            let mut body_depth: usize = 1;
            let mut body_line_count = 0usize;
            let mut close_indent = fn_indent.clone();

            let mut body_idx = sig_idx;
            while body_idx < lines.len() && body_depth > 0 {
                let b_line = lines[body_idx];
                let (net, has_close) = scan_rust_tokens(b_line, &mut lex_state);
                let new_depth = (body_depth as isize + net).max(0) as usize;
                if new_depth == 0 {
                    // Closed on this line
                    if has_close {
                        close_indent = b_line.chars().take_while(|c| c.is_whitespace()).collect();
                    }
                    body_depth = 0;
                    body_idx += 1;
                    break;
                }
                body_depth = new_depth;
                body_line_count += 1;
                body_idx += 1;
            }

            if body_depth != 0 {
                // Unclosed function brace -> syntax error
                return None;
            }

            if body_line_count > 0 {
                output.push(format!(
                    "{}    /* ... [{} lines omitted: function body] ... */",
                    fn_indent, body_line_count
                ));
                omitted_body_lines += body_line_count;
                omitted_functions += 1;
            }
            output.push(format!("{}}}", close_indent));

            i = body_idx;
            continue;
        }

        // Regular line
        let (net, _) = scan_rust_tokens(line, &mut lex_state);
        global_braces = (global_braces + net).max(0);

        // Discard non-doc full line comments to maximize compactness
        if is_full_line_comment(trimmed)
            && !trimmed.starts_with("///")
            && !trimmed.starts_with("//!")
        {
            i += 1;
            continue;
        }

        output.push(line.to_string());
        i += 1;
    }

    if global_braces != 0 || lex_state != RustLexState::Normal {
        return None;
    }

    Some(InternalStructural {
        lines: output,
        omitted_body_lines,
        omitted_import_lines,
        omitted_functions,
    })
}

// ─── Python Compression ────────────────────────────────────────────────

static PY_FN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^(\s*)(?:async\s+)?def\s+([A-Za-z0-9_]+)\s*\("#).unwrap());

fn compress_python(input: &str) -> Option<InternalStructural> {
    let lines: Vec<&str> = input.lines().collect();
    let mut output: Vec<String> = Vec::with_capacity(lines.len());
    let mut omitted_body_lines = 0usize;
    let mut omitted_import_lines = 0usize;
    let mut omitted_functions = 0usize;

    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // 1. Imports block
        if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
            let mut j = i;
            let mut import_lines = 0usize;
            while j < lines.len() {
                let lt = lines[j].trim();
                if lt.starts_with("import ")
                    || lt.starts_with("from ")
                    || (import_lines > 0
                        && (lt.starts_with('(') || lt.ends_with(')') || lt.ends_with(',')))
                {
                    import_lines += 1;
                    j += 1;
                } else {
                    break;
                }
            }
            if import_lines > 3 {
                output.push(lines[i].to_string());
                output.push(lines[i + 1].to_string());
                let omitted = import_lines - 2;
                output.push(format!("# ... [{} import lines omitted] ...", omitted));
                omitted_import_lines += omitted;
                i = j;
                continue;
            }
        }

        // 2. Module / Class docstring
        if trimmed.starts_with("\"\"\"") || trimmed.starts_with("'''") {
            let quote = if trimmed.starts_with("\"\"\"") {
                "\"\"\""
            } else {
                "'''"
            };
            output.push(line.to_string());
            if trimmed.len() > 3 && trimmed[3..].contains(quote) {
                // One-line docstring
                i += 1;
                continue;
            }
            i += 1;
            while i < lines.len() {
                let dl = lines[i];
                output.push(dl.to_string());
                if dl.contains(quote) {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }

        // 3. Functions / methods
        if let Some(caps) = PY_FN_RE.captures(line) {
            let indent = caps.get(1).unwrap().as_str();
            let indent_len = indent.len();

            // Collect full signature lines until line ends with `:`
            let mut sig_idx = i;
            while sig_idx < lines.len() {
                let sl = lines[sig_idx];
                output.push(sl.to_string());
                let st = sl.trim_end();
                if st.ends_with(':') {
                    sig_idx += 1;
                    break;
                }
                sig_idx += 1;
            }

            // Check if immediately followed by docstring
            if sig_idx < lines.len() {
                let next_t = lines[sig_idx].trim();
                if next_t.starts_with("\"\"\"") || next_t.starts_with("'''") {
                    let quote = if next_t.starts_with("\"\"\"") {
                        "\"\"\""
                    } else {
                        "'''"
                    };
                    output.push(lines[sig_idx].to_string());
                    if !(next_t.len() > 3 && next_t[3..].contains(quote)) {
                        sig_idx += 1;
                        while sig_idx < lines.len() {
                            let dl = lines[sig_idx];
                            output.push(dl.to_string());
                            if dl.contains(quote) {
                                sig_idx += 1;
                                break;
                            }
                            sig_idx += 1;
                        }
                    } else {
                        sig_idx += 1;
                    }
                }
            }

            // Now consume all body lines (indent > indent_len or empty)
            let mut body_lines = 0usize;
            let mut body_idx = sig_idx;
            while body_idx < lines.len() {
                let bl = lines[body_idx];
                let bt = bl.trim();
                if bt.is_empty() {
                    body_idx += 1;
                    continue;
                }
                let line_indent = bl.chars().take_while(|c| c.is_whitespace()).count();
                if line_indent > indent_len {
                    body_lines += 1;
                    body_idx += 1;
                } else {
                    break;
                }
            }

            if body_lines > 0 {
                output.push(format!(
                    "{}    # ... [{} lines omitted: function body] ...",
                    indent, body_lines
                ));
                output.push(format!("{}    ...", indent));
                omitted_body_lines += body_lines;
                omitted_functions += 1;
            }

            i = body_idx;
            continue;
        }

        // 4. `if __name__ == "__main__":` entrypoint
        if trimmed.starts_with("if __name__ ==") && trimmed.ends_with(':') {
            output.push(line.to_string());
            let indent = line
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect::<String>();
            let indent_len = indent.len();
            let mut main_idx = i + 1;
            let mut main_lines = 0usize;
            while main_idx < lines.len() {
                let ml = lines[main_idx];
                let mt = ml.trim();
                if mt.is_empty() {
                    main_idx += 1;
                    continue;
                }
                let l_indent = ml.chars().take_while(|c| c.is_whitespace()).count();
                if l_indent > indent_len {
                    main_lines += 1;
                    main_idx += 1;
                } else {
                    break;
                }
            }

            if main_lines > 2 {
                output.push(format!(
                    "{}    # ... [{} lines omitted: main execution block] ...",
                    indent, main_lines
                ));
                output.push(format!("{}    ...", indent));
                omitted_body_lines += main_lines;
                omitted_functions += 1;
                i = main_idx;
                continue;
            }
        }

        output.push(line.to_string());
        i += 1;
    }

    Some(InternalStructural {
        lines: output,
        omitted_body_lines,
        omitted_import_lines,
        omitted_functions,
    })
}

// ─── TypeScript / JavaScript Compression ───────────────────────────────

static JS_FN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^(?:export\s+)?(?:default\s+)?(?:async\s+)?function(?:\s+[A-Za-z0-9_$]+|\s*\()"#)
        .unwrap()
});
static JS_METHOD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^\s*(?:public\s+|private\s+|protected\s+|static\s+|async\s+|get\s+|set\s+)*(?:constructor|[A-Za-z0-9_$]+)\s*\([^)]*\)\s*(?::\s*[^{]+)?\{"#).unwrap()
});
static JS_ARROW_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^(?:export\s+)?(?:const|let|var)\s+[A-Za-z0-9_$]+\s*=\s*(?:async\s*)?(?:\([^)]*\)|[A-Za-z0-9_$]+)\s*(?::\s*[^{=]+)?=>\s*\{"#).unwrap()
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JsLexState {
    Normal,
    InBlockComment,
    InString { quote: u8, escaped: bool },
    InTemplateString { escaped: bool },
}

fn scan_js_tokens(line: &str, state: &mut JsLexState) -> (isize, bool) {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut net_braces: isize = 0;
    let mut has_close = false;

    while i < len {
        match *state {
            JsLexState::Normal => {
                if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'/' {
                    break;
                }
                if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                    *state = JsLexState::InBlockComment;
                    i += 2;
                    continue;
                }
                if bytes[i] == b'"' || bytes[i] == b'\'' {
                    *state = JsLexState::InString {
                        quote: bytes[i],
                        escaped: false,
                    };
                    i += 1;
                    continue;
                }
                if bytes[i] == b'`' {
                    *state = JsLexState::InTemplateString { escaped: false };
                    i += 1;
                    continue;
                }
                if bytes[i] == b'{' {
                    net_braces += 1;
                } else if bytes[i] == b'}' {
                    net_braces -= 1;
                    has_close = true;
                }
                i += 1;
            }
            JsLexState::InBlockComment => {
                if i + 1 < len && bytes[i] == b'*' && bytes[i + 1] == b'/' {
                    *state = JsLexState::Normal;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            JsLexState::InString {
                quote,
                ref mut escaped,
            } => {
                if *escaped {
                    *escaped = false;
                    i += 1;
                } else if bytes[i] == b'\\' {
                    *escaped = true;
                    i += 1;
                } else if bytes[i] == quote {
                    *state = JsLexState::Normal;
                    i += 1;
                } else {
                    i += 1;
                }
            }
            JsLexState::InTemplateString { ref mut escaped } => {
                if *escaped {
                    *escaped = false;
                    i += 1;
                } else if bytes[i] == b'\\' {
                    *escaped = true;
                    i += 1;
                } else if bytes[i] == b'`' {
                    *state = JsLexState::Normal;
                    i += 1;
                } else {
                    i += 1;
                }
            }
        }
    }

    (net_braces, has_close)
}

fn compress_ts_js(input: &str, _is_ts: bool) -> Option<InternalStructural> {
    let lines: Vec<&str> = input.lines().collect();
    let mut output: Vec<String> = Vec::with_capacity(lines.len());
    let mut omitted_body_lines = 0usize;
    let mut omitted_import_lines = 0usize;
    let mut omitted_functions = 0usize;

    let mut i = 0;
    let mut global_braces: isize = 0;
    let mut lex_state = JsLexState::Normal;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // 1. Imports block
        if global_braces == 0
            && (trimmed.starts_with("import ")
                || (trimmed.starts_with("const ") && trimmed.contains("= require(")))
        {
            let mut j = i;
            let mut import_lines = 0usize;
            while j < lines.len() {
                let lt = lines[j].trim();
                if lt.starts_with("import ")
                    || (lt.starts_with("const ") && lt.contains("= require("))
                    || (import_lines > 0
                        && (lt.starts_with('{') || lt.ends_with(';') || lt.starts_with("};")))
                {
                    import_lines += 1;
                    j += 1;
                    if lt.ends_with(';') {
                        if j < lines.len() {
                            let next_t = lines[j].trim();
                            if next_t.starts_with("import ")
                                || (next_t.starts_with("const ") && next_t.contains("= require("))
                            {
                                continue;
                            }
                        }
                        break;
                    }
                } else {
                    break;
                }
            }

            if import_lines > 3 {
                output.push(lines[i].to_string());
                output.push(lines[i + 1].to_string());
                let omitted = import_lines - 2;
                output.push(format!("// ... [{} import lines omitted] ...", omitted));
                omitted_import_lines += omitted;
                i = j;
                continue;
            }
        }

        // 2. Preserve JSDoc comment blocks
        if trimmed.starts_with("/**") {
            output.push(line.to_string());
            if !trimmed.ends_with("*/") || trimmed.len() <= 4 {
                i += 1;
                while i < lines.len() {
                    let jl = lines[i];
                    output.push(jl.to_string());
                    if jl.trim().ends_with("*/") {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                continue;
            }
            i += 1;
            continue;
        }

        // 3. Preserve TypeScript interfaces / type aliases / enums
        if trimmed.starts_with("interface ")
            || trimmed.starts_with("export interface ")
            || (trimmed.starts_with("type ") && trimmed.contains('='))
            || (trimmed.starts_with("export type ") && trimmed.contains('='))
            || trimmed.starts_with("enum ")
            || trimmed.starts_with("export enum ")
        {
            let (net, _) = scan_js_tokens(line, &mut lex_state);
            global_braces = (global_braces + net).max(0);
            output.push(line.to_string());
            i += 1;
            continue;
        }

        // 4. Function or method definitions
        let is_fn = JS_FN_RE.is_match(trimmed)
            || JS_ARROW_RE.is_match(trimmed)
            || (global_braces > 0 && JS_METHOD_RE.is_match(trimmed));
        if is_fn {
            let fn_indent = line
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect::<String>();
            let mut sig_lines = Vec::new();
            let mut has_body = false;
            let mut sig_idx = i;

            while sig_idx < lines.len() {
                let sl = lines[sig_idx];
                let (net, _) = scan_js_tokens(sl, &mut lex_state);
                sig_lines.push(sl);
                if sl.contains('{') && net > 0 {
                    has_body = true;
                    sig_idx += 1;
                    break;
                }
                if sl.trim().ends_with(';') {
                    has_body = false;
                    sig_idx += 1;
                    break;
                }
                sig_idx += 1;
            }

            if !has_body {
                for s in sig_lines {
                    output.push(s.to_string());
                }
                i = sig_idx;
                continue;
            }

            for s in sig_lines {
                output.push(s.to_string());
            }

            // Scan body until depth returns to 0
            let mut body_depth: usize = 1;
            let mut body_line_count = 0usize;
            let mut close_indent = fn_indent.clone();

            let mut body_idx = sig_idx;
            while body_idx < lines.len() && body_depth > 0 {
                let b_line = lines[body_idx];
                let (net, has_close) = scan_js_tokens(b_line, &mut lex_state);
                let new_depth = (body_depth as isize + net).max(0) as usize;
                if new_depth == 0 {
                    if has_close {
                        close_indent = b_line.chars().take_while(|c| c.is_whitespace()).collect();
                    }
                    body_depth = 0;
                    body_idx += 1;
                    break;
                }
                body_depth = new_depth;
                body_line_count += 1;
                body_idx += 1;
            }

            if body_depth != 0 {
                return None;
            }

            if body_line_count > 0 {
                output.push(format!(
                    "{}    /* ... [{} lines omitted: function body] ... */",
                    fn_indent, body_line_count
                ));
                omitted_body_lines += body_line_count;
                omitted_functions += 1;
            }
            output.push(format!("{}}}", close_indent));

            i = body_idx;
            continue;
        }

        // Regular line
        let (net, _) = scan_js_tokens(line, &mut lex_state);
        global_braces = (global_braces + net).max(0);

        if is_full_line_comment(trimmed) && !trimmed.starts_with("/**") && !trimmed.starts_with('*')
        {
            i += 1;
            continue;
        }

        output.push(line.to_string());
        i += 1;
    }

    if global_braces != 0 || lex_state != JsLexState::Normal {
        return None;
    }

    Some(InternalStructural {
        lines: output,
        omitted_body_lines,
        omitted_import_lines,
        omitted_functions,
    })
}

// ─── ReformatTransform impl ────────────────────────────────────────────

impl crate::transforms::pipeline::ReformatTransform for SourceCompressor {
    fn name(&self) -> &'static str {
        "source_compressor"
    }

    fn applies_to(&self) -> &[crate::transforms::ContentType] {
        &[crate::transforms::ContentType::SourceCode]
    }

    fn apply(
        &self,
        content: &str,
    ) -> Result<
        crate::transforms::pipeline::ReformatOutput,
        crate::transforms::pipeline::TransformError,
    > {
        let result = self.compress(content);
        if result.compressed == result.original {
            return Err(crate::transforms::pipeline::TransformError::skipped(
                self.name(),
                "source already compact",
            ));
        }

        Ok(crate::transforms::pipeline::ReformatOutput::from_lengths(
            content.len(),
            result.compressed,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_only_full_line_comments_and_extra_blank_lines() {
        let input =
            "#!/usr/bin/env bash\n# setup\n\n\nvalue=1 # keep inline\n// js note\nprintln(value)\n";
        let result = SourceCompressor::default().compress(input);

        assert_eq!(result.removed_comment_lines, 2);
        assert_eq!(result.removed_blank_lines, 1);
        assert!(result.compressed.contains("#!/usr/bin/env bash"));
        assert!(result.compressed.contains("value=1 # keep inline"));
        assert!(!result.compressed.contains("# setup"));
        assert!(!result.compressed.contains("// js note"));
        assert!(result.is_approximate);
    }

    #[test]
    fn structural_rust_compression_approximate_preserves_signatures_and_doc() {
        let input = r#"
//! Module level documentation.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::io::Result as IoResult;

/// Calculate something important.
pub fn calculate_score(items: &[u32], multiplier: u32) -> u64 {
    let mut total = 0u64;
    for &item in items {
        total += (item as u64) * (multiplier as u64);
    }
    total
}

pub struct Config {
    pub name: String,
    pub retries: u32,
}

impl Config {
    /// Create new config instance.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            retries: 3,
        }
    }
}
"#;
        let result = SourceCompressor::embedded_only().compress(input);
        assert_eq!(result.language, Some(SourceLanguage::Rust));
        assert!(result.omitted_body_lines > 0);
        assert!(result.omitted_functions >= 2);
        assert!(result.is_approximate);
        assert_eq!(result.engine_used, "embedded-regex-braces");
        assert!(result.compressed.contains("Structure approximative"));
        assert!(result.compressed.contains("Retrieve full source: hash="));
        assert!(result
            .compressed
            .contains("calculate_score(items: &[u32], multiplier: u32) -> u64"));
        assert!(result.compressed.contains("Calculate something important."));
        assert!(result.compressed.contains("pub struct Config"));
        assert!(result.compressed.contains("pub fn new(name: &str) -> Self"));
        assert!(result.compressed.contains("lines omitted: function body"));
        assert!(result.compressed.contains("import lines omitted"));
    }

    #[test]
    fn structural_python_compression_approximate_preserves_signatures_and_doc() {
        let input = r#"
"""User service module."""

import os
import sys
import logging
from typing import Optional, List, Dict

logger = logging.getLogger(__name__)

class UserService:
    """Service handling users."""

    def __init__(self, db_url: str):
        """Initialize database."""
        self.db_url = db_url
        self.connected = True
        self._cache = {}

    def get_user(self, user_id: int) -> Optional[dict]:
        """Fetch user by id."""
        if user_id in self._cache:
            return self._cache[user_id]
        data = {"id": user_id, "name": "alice"}
        self._cache[user_id] = data
        return data

if __name__ == "__main__":
    svc = UserService("sqlite:///:memory:")
    print(svc.get_user(1))
"#;
        let result = SourceCompressor::embedded_only().compress(input);
        assert_eq!(result.language, Some(SourceLanguage::Python));
        assert!(result.omitted_body_lines > 0);
        assert!(result.omitted_functions >= 2);
        assert!(result.is_approximate);
        assert_eq!(result.engine_used, "embedded-regex-braces");
        assert!(result.compressed.contains("Structure approximative"));
        assert!(result
            .compressed
            .contains("def __init__(self, db_url: str):"));
        assert!(result
            .compressed
            .contains("def get_user(self, user_id: int) -> Optional[dict]:"));
        assert!(result.compressed.contains("Initialize database."));
        assert!(result.compressed.contains("Fetch user by id."));
        assert!(result.compressed.contains("if __name__ == \"__main__\":"));
    }

    #[test]
    fn structural_typescript_compression_approximate_preserves_signatures_and_types() {
        let input = r#"
/**
 * API client module.
 */
import { Request, Response } from "express";
import { User, Role } from "./types";
import { Database } from "./db";
import { Logger } from "./logger";

export interface ClientOptions {
    timeoutMs: number;
    retries: number;
}

export class ApiClient {
    private readonly options: ClientOptions;

    constructor(options: ClientOptions) {
        this.options = options;
        this.init();
    }

    /**
     * Send authenticated request.
     */
    public async fetchUser(id: string): Promise<User> {
        const response = await fetch(`/api/users/${id}`);
        if (!response.ok) {
            throw new Error("Failed");
        }
        return await response.json();
    }
}
"#;
        let result = SourceCompressor::embedded_only().compress(input);
        assert_eq!(result.language, Some(SourceLanguage::TypeScript));
        assert!(result.omitted_body_lines > 0);
        assert!(result.omitted_functions >= 2);
        assert!(result.is_approximate);
        assert_eq!(result.engine_used, "embedded-regex-braces");
        assert!(result.compressed.contains("Structure approximative"));
        assert!(result.compressed.contains("export interface ClientOptions"));
        assert!(result.compressed.contains("export class ApiClient"));
        assert!(result
            .compressed
            .contains("public async fetchUser(id: string): Promise<User>"));
        assert!(result.compressed.contains("Send authenticated request."));
    }

    #[test]
    fn structural_javascript_compression_approximate() {
        let input = r#"
// CommonJS utility module
const fs = require("fs");
const path = require("path");
const http = require("http");
const url = require("url");

function loadConfiguration(filePath) {
    const raw = fs.readFileSync(filePath, "utf-8");
    const parsed = JSON.parse(raw);
    return parsed;
}

const formatResponse = (code, message) => {
    const payload = { code, message, timestamp: Date.now() };
    return JSON.stringify(payload);
};

module.exports = { loadConfiguration, formatResponse };
"#;
        let result = SourceCompressor::embedded_only().compress(input);
        assert_eq!(result.language, Some(SourceLanguage::JavaScript));
        assert!(result.omitted_body_lines > 0);
        assert!(result.is_approximate);
        assert!(result.compressed.contains("Structure approximative"));
        assert!(result
            .compressed
            .contains("function loadConfiguration(filePath)"));
        assert!(result
            .compressed
            .contains("formatResponse = (code, message) =>"));
    }

    #[test]
    fn fallback_on_unbalanced_rust_syntax() {
        let input = r#"
pub fn broken() {
    let a = 1;
    let b = 2;
// missing closing brace
"#;
        let result = SourceCompressor::default().compress(input);
        assert_eq!(result.language, None);
        assert_eq!(result.omitted_body_lines, 0);
        assert!(!result.compressed.contains("Structure approximative"));
        assert!(!result.compressed.contains("Structure syntaxique"));
    }

    #[test]
    fn fallback_on_unknown_language() {
        let input = r#"
# Makefile sample
all: build test

build:
	@echo "building..."
	cargo build --release

test:
	@echo "testing..."
	cargo test
"#;
        let result = SourceCompressor::default().compress(input);
        assert_eq!(result.language, None);
        assert_eq!(result.omitted_body_lines, 0);
    }

    #[test]
    fn handles_empty_and_single_long_line() {
        let empty = SourceCompressor::default().compress("");
        assert_eq!(empty.compressed, "");

        let long_line = format!("let val = \"{}\";", "x".repeat(10_000));
        let res = SourceCompressor::default().compress(&long_line);
        assert!(!res.compressed.is_empty());
    }

    #[test]
    fn ccr_persistence_stores_original_payload() {
        use crate::ccr::InMemoryCcrStore;
        let store = InMemoryCcrStore::default();
        let input = r#"
pub fn compute_hash(data: &[u8]) -> u64 {
    let mut h = 0u64;
    for b in data {
        h = h.wrapping_add(*b as u64);
    }
    h
}
pub fn verify_hash(data: &[u8], expected: u64) -> bool {
    let h = compute_hash(data);
    h == expected
}
"#;
        let result = SourceCompressor::default().compress_with_store(input, Some(&store));
        assert!(result.ccr_key.is_some());
        let key = result.ccr_key.unwrap();
        assert_eq!(store.get(&key), Some(input.to_string()));
        assert!(result.is_approximate);
        assert!(result.compressed.contains(&format!("hash={}", key)));
    }

    #[test]
    fn embedded_mode_explicitly_announces_structure_approximative_in_banner() {
        let input = r#"
pub fn run_job(id: u64) -> bool {
    let mut state = true;
    for i in 0..10 {
        state ^= (i % 2 == 0);
    }
    state
}
pub fn cancel_job(id: u64) {
    let mut reason = "timeout";
    let _ = reason;
}
"#;
        let result = SourceCompressor::embedded_only().compress(input);
        assert!(result.is_approximate);
        assert!(result.compressed.contains("// [Structure approximative:"));
        assert!(!result.compressed.contains("Code Explorer"));
    }

    #[test]
    fn code_explorer_disabled_mode_guarantees_embedded_fallback() {
        let compressor =
            SourceCompressor::new(1, 4).with_code_explorer_mode(CodeExplorerMode::Disabled);
        let input = r#"
def calculate_vat(price: float) -> float:
    vat_rate = 0.20
    return price * (1.0 + vat_rate)

def calculate_discount(price: float, discount: float) -> float:
    final_price = price - discount
    return max(0.0, final_price)
"#;
        let result = compressor.compress_path_with_store(input, Some("src/calc.py"), None, None);
        assert!(result.is_approximate);
        assert_eq!(result.engine_used, "embedded-regex-braces");
        assert!(result.compressed.contains("# [Structure approximative:"));
    }

    #[test]
    fn ast_symbols_rust_exact_compression_via_code_explorer() {
        let input = r#"use std::sync::Arc;
use std::collections::HashMap;
use std::time::Instant;
use std::path::PathBuf;

pub fn process_order(id: u64) -> Result<(), String> {
    let start = Instant::now();
    let map = HashMap::new();
    println!("processing {}", id);
    Ok(())
}

pub fn validate_order(id: u64) -> bool {
    id > 0
}
"#;
        let symbols = vec![
            AstSymbol {
                name: "process_order".to_string(),
                label: "Function".to_string(),
                start_line: 6,
                end_line: 11,
            },
            AstSymbol {
                name: "validate_order".to_string(),
                label: "Function".to_string(),
                start_line: 13,
                end_line: 15,
            },
        ];

        let compressor = SourceCompressor::default();
        let result = compressor
            .compress_with_symbols(input, SourceLanguage::Rust, &symbols, None)
            .unwrap();

        assert!(!result.is_approximate);
        assert_eq!(result.engine_used, "code-explorer");
        assert!(result
            .compressed
            .contains("// [Structure syntaxique (Code Explorer):"));
        assert!(!result.compressed.contains("Structure approximative"));
        assert!(result
            .compressed
            .contains("process_order(id: u64) -> Result<(), String>"));
        assert!(result
            .compressed
            .contains("/* ... [4 lines omitted: function body] ... */"));
        assert!(result
            .compressed
            .contains("validate_order(id: u64) -> bool"));
        assert!(result.omitted_body_lines >= 4);
        assert!(result.omitted_import_lines >= 2);
    }

    #[test]
    fn ast_symbols_python_exact_compression_via_code_explorer() {
        let input = r#"from os import path
from sys import argv
import math
import logging

def complex_algorithm(data: list) -> int:
    """Compute something with math."""
    total = 0
    for x in data:
        total += math.isqrt(x)
    return total

def simple_helper():
    pass
"#;
        let symbols = vec![
            AstSymbol {
                name: "complex_algorithm".to_string(),
                label: "Function".to_string(),
                start_line: 6,
                end_line: 11,
            },
            AstSymbol {
                name: "simple_helper".to_string(),
                label: "Function".to_string(),
                start_line: 13,
                end_line: 14,
            },
        ];

        let compressor = SourceCompressor::default();
        let result = compressor
            .compress_with_symbols(input, SourceLanguage::Python, &symbols, None)
            .unwrap();

        assert!(!result.is_approximate);
        assert_eq!(result.engine_used, "code-explorer");
        assert!(result
            .compressed
            .contains("# [Structure syntaxique (Code Explorer):"));
        assert!(!result.compressed.contains("Structure approximative"));
        assert!(result
            .compressed
            .contains("def complex_algorithm(data: list) -> int:"));
        assert!(result.compressed.contains("Compute something with math."));
        assert!(result
            .compressed
            .contains("# ... [4 lines omitted: function body] ..."));
    }

    #[test]
    fn ast_symbols_typescript_exact_compression_via_code_explorer() {
        let input = r#"import { A } from "./a";
import { B } from "./b";
import { C } from "./c";
import { D } from "./d";

export class DataService {
    public async fetchData(id: string): Promise<Record<string, any>> {
        const url = `/api/${id}`;
        const resp = await fetch(url);
        return await resp.json();
    }
}
"#;
        let symbols = vec![
            AstSymbol {
                name: "DataService".to_string(),
                label: "Class".to_string(),
                start_line: 6,
                end_line: 12,
            },
            AstSymbol {
                name: "fetchData".to_string(),
                label: "Method".to_string(),
                start_line: 7,
                end_line: 11,
            },
        ];

        let compressor = SourceCompressor::default();
        let result = compressor
            .compress_with_symbols(input, SourceLanguage::TypeScript, &symbols, None)
            .unwrap();

        assert!(!result.is_approximate);
        assert_eq!(result.engine_used, "code-explorer");
        assert!(result
            .compressed
            .contains("// [Structure syntaxique (Code Explorer):"));
        assert!(result.compressed.contains("export class DataService"));
        assert!(result
            .compressed
            .contains("public async fetchData(id: string): Promise<Record<string, any>>"));
        assert!(result
            .compressed
            .contains("/* ... [3 lines omitted: function body] ... */"));
    }

    #[test]
    fn code_explorer_query_fallback_on_invalid_binary() {
        let compressor =
            SourceCompressor::new(1, 4).with_custom_bin("/nonexistent/code-explorer-fake-bin");
        let input = r#"
pub fn add(a: i32, b: i32) -> i32 {
    let mut sum = a;
    sum += b;
    sum
}
pub fn sub(a: i32, b: i32) -> i32 {
    a - b
}
"#;
        // Even when pointing to an invalid binary, it should gracefully fall back to embedded
        let result = compressor.compress_path_with_store(input, Some("src/math.rs"), None, None);
        assert!(result.is_approximate);
        assert_eq!(result.engine_used, "embedded-regex-braces");
        assert!(result.compressed.contains("Structure approximative"));
        assert!(!result.compressed.is_empty());
    }

    #[test]
    fn parse_cypher_symbols_handles_valid_and_malformed_json() {
        let valid = r#"OK 2 results

[
  {
    "n._label": "Function",
    "n.endLine": 20,
    "n.name": "hello",
    "n.startLine": 10
  }
]"#;
        let symbols = parse_cypher_symbols(valid).unwrap();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "hello");
        assert_eq!(symbols[0].label, "Function");
        assert_eq!(symbols[0].start_line, 10);
        assert_eq!(symbols[0].end_line, 20);

        let invalid = "ERROR: connection lost";
        assert!(parse_cypher_symbols(invalid).is_none());

        let empty = "OK 0 results\n[]";
        assert!(parse_cypher_symbols(empty).is_none());
    }

    #[test]
    fn parse_mcp_symbols_handles_valid_and_malformed_json() {
        let valid = r#"{"jsonrpc":"2.0","id":2,"result":{"_meta":{"symbols":[{"name":"test","label":"Method","startLine":5,"endLine":15}]}}}"#;
        let symbols = parse_mcp_symbols(valid).unwrap();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "test");
        assert_eq!(symbols[0].label, "Method");
        assert_eq!(symbols[0].start_line, 5);
        assert_eq!(symbols[0].end_line, 15);

        let invalid = r#"{"error":{"code":-32600}}"#;
        assert!(parse_mcp_symbols(invalid).is_none());
    }
}
