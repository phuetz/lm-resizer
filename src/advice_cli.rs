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

/// Ask `code-explorer lm-resizer-advice <file>` about the file being compressed.
///
/// The path handed over is canonical, so the producer reads the same file
/// whatever its working directory. It still reads it on its own: if the file
/// changes between its read and ours, the hashes differ and the advice is
/// refused as stale — the race is detected, not avoided.
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
    let started = Instant::now();
    let mut child = Command::new(&bin)
        .arg("lm-resizer-advice")
        .arg(&canonical)
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
    match RetentionAdvice::from_json(text.trim()) {
        Some(advice) => Ok((advice, elapsed)),
        None => {
            let mut report = AdviceReport::failure(
                "advice-invalid",
                &source,
                "producer stdout is not a RetentionAdvice JSON document",
            );
            report.producer_ms = Some(elapsed);
            Err(report)
        }
    }
}
