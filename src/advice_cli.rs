//! `lm-resizer compress --advice` / `--advice-from-code-explorer`.
//!
//! Getting a structural advisor's opinion is opt-in and never blocking: every
//! way it can fail — no binary, a crash, a timeout, a malformed document,
//! advice about other bytes — ends in the ordinary pipeline, with the reason
//! reported. The only thing advice can do is let callable bodies be elided;
//! see `lm_resizer_core::transforms::advice_structural`.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use lm_resizer_core::transforms::retention_advice::RetentionAdvice;
use serde::Serialize;

/// Where advice came from and what became of it, for `--json` and stderr.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AdviceReport {
    /// `applied`, `stale`, `unknown-freshness`, `no-callable-kinds`,
    /// `unsupported-language`, `no-gain`, or a loading failure:
    /// `advice-unreadable`, `advice-invalid`, `producer-missing`,
    /// `producer-failed`, `producer-timeout`.
    pub status: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub elided_bodies: usize,
    pub elided_lines: usize,
    pub guard_rejects: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub focused: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub producer_ms: Option<u128>,
}

impl AdviceReport {
    pub fn failure(status: &str, source: &str, detail: impl Into<String>) -> Self {
        Self {
            status: status.to_string(),
            source: source.to_string(),
            advisor: None,
            detail: Some(detail.into()),
            elided_bodies: 0,
            elided_lines: 0,
            guard_rejects: 0,
            focused: Vec::new(),
            producer_ms: None,
        }
    }
}

/// Advice from a file written earlier by any producer.
#[allow(clippy::result_large_err)] // Reports are returned rarely and serialized by callers.
pub fn load_advice_file(path: &Path) -> Result<RetentionAdvice, AdviceReport> {
    let source = format!("file:{}", path.display());
    let raw = std::fs::read_to_string(path)
        .map_err(|e| AdviceReport::failure("advice-unreadable", &source, e.to_string()))?;
    RetentionAdvice::from_json(&raw).ok_or_else(|| {
        AdviceReport::failure(
            "advice-invalid",
            &source,
            "not a RetentionAdvice JSON document",
        )
    })
}

/// Binary used for `--advice-from-code-explorer`.
pub fn code_explorer_bin() -> String {
    std::env::var("LM_RESIZER_CODE_EXPLORER_BIN")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "code-explorer".to_string())
}

fn producer_timeout() -> Duration {
    std::env::var("LM_RESIZER_ADVICE_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or(Duration::from_secs(20))
}

/// Read Code Explorer's indexed tree-sitter symbols through its stable `cypher`
/// CLI. No Code Explorer code is linked into the Apache-2.0 binary: its source
/// is BUSL-1.1. An absent or stale index leaves the ordinary compressor active.
///
/// The path handed over is canonical, so the producer reads the same file
/// whatever its working directory. It still reads it on its own: if the file
/// changes between its read and ours, the hashes differ and the advice is
/// refused as stale — the race is detected, not avoided.
#[allow(clippy::result_large_err)] // Keep the same report type for producer failures.
pub fn advice_from_code_explorer(input: &Path) -> Result<(RetentionAdvice, u128), AdviceReport> {
    let bin = code_explorer_bin();
    let source = format!("code-explorer:{bin}");
    let canonical = std::fs::canonicalize(input).map_err(|e| {
        AdviceReport::failure(
            "advice-unreadable",
            &source,
            format!("{}: {e}", input.display()),
        )
    })?;
    let repo = canonical
        .ancestors()
        .find(|p| p.join(".codeexplorer/graph.bin").is_file())
        .ok_or_else(|| {
            AdviceReport::failure("producer-missing", &source, "no Code Explorer index")
        })?;
    let relative = canonical
        .strip_prefix(repo)
        .expect("ancestor")
        .to_string_lossy()
        .replace('\\', "/");
    // Avoid passing path text into a Cypher string literal when it contains a
    // quote. Failing closed is cheaper than guessing the query escaping rules.
    if relative.contains('\'') {
        return Err(AdviceReport::failure(
            "advice-invalid",
            &source,
            "quoted path",
        ));
    }
    let graph = repo.join(".codeexplorer/graph.bin");
    let input_time = std::fs::metadata(&canonical).and_then(|m| m.modified());
    let index_time = std::fs::metadata(&graph).and_then(|m| m.modified());
    if !matches!((input_time, index_time), (Ok(file), Ok(index)) if file <= index) {
        return Err(AdviceReport::failure(
            "advice-stale",
            &source,
            "Code Explorer index predates file",
        ));
    }
    let query = format!(
        "MATCH (n) WHERE n.filePath = '{relative}' RETURN n.name, n.startLine, n.endLine, n._label"
    );
    let started = Instant::now();
    let mut child = Command::new(&bin)
        .arg("cypher")
        .arg("--repo")
        .arg(repo)
        .arg(query)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AdviceReport::failure("producer-missing", &source, e.to_string()))?;

    // Drain both pipes on threads so a chatty producer cannot block on a full
    // pipe while we wait for it.
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let out_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf);
        buf
    });

    let timeout = producer_timeout();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AdviceReport::failure(
                    "producer-timeout",
                    &source,
                    format!("no answer after {} ms", timeout.as_millis()),
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => {
                return Err(AdviceReport::failure(
                    "producer-failed",
                    &source,
                    e.to_string(),
                ))
            }
        }
    };
    let elapsed = started.elapsed().as_millis();
    let out = out_reader.join().unwrap_or_default();
    let err = err_reader.join().unwrap_or_default();

    if !status.success() {
        let first = String::from_utf8_lossy(&err)
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .to_string();
        let mut report = AdviceReport::failure(
            "producer-failed",
            &source,
            format!(
                "exit {}: {first}",
                status
                    .code()
                    .map_or("signal".to_string(), |c| c.to_string())
            ),
        );
        report.producer_ms = Some(elapsed);
        return Err(report);
    }
    let text = String::from_utf8_lossy(&out);
    let symbols = lm_resizer_core::transforms::source_compressor::parse_cypher_symbols(&text);
    match symbols {
        Some(mut symbols) => {
            let current = std::fs::read(&canonical)
                .map_err(|e| AdviceReport::failure("advice-unreadable", &source, e.to_string()))?;
            let current = String::from_utf8(current)
                .map_err(|e| AdviceReport::failure("advice-invalid", &source, e.to_string()))?;
            if matches!(language_from_path(&relative), Some("c" | "cpp")) {
                for symbol in &mut symbols {
                    if symbol.label == "Function" && symbol.end_line == symbol.start_line {
                        if let Some(end) =
                            lm_resizer_core::transforms::advice_structural::end_line_for_inline_body(
                                &current,
                                symbol.start_line,
                            )
                        {
                            symbol.end_line = end;
                        }
                    }
                }
            }
            let mut advice = lm_resizer_core::transforms::source_compressor::advice_from_symbols(
                &symbols, &current,
            );
            advice.source_path = Some(relative.clone());
            advice.language = language_from_path(&relative).map(str::to_string);
            Ok((advice, elapsed))
        }
        None => {
            let mut report = AdviceReport::failure(
                "advice-invalid",
                &source,
                "Code Explorer did not return indexed symbols",
            );
            report.producer_ms = Some(elapsed);
            Err(report)
        }
    }
}

fn language_from_path(path: &str) -> Option<&'static str> {
    match path.rsplit('.').next()?.to_ascii_lowercase().as_str() {
        "cs" => Some("csharp"),
        "go" => Some("go"),
        "java" => Some("java"),
        "c" | "h" => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" | "hh" => Some("cpp"),
        "rs" => Some("rust"),
        "ts" | "tsx" => Some("typescript"),
        "js" | "jsx" => Some("javascript"),
        _ => None,
    }
}

pub fn supports_structural_path(path: &Path) -> bool {
    language_from_path(&path.to_string_lossy()).is_some()
}
