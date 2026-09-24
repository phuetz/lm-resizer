//! Lectures structurées que le filtre ligne à ligne ne sait pas faire :
//! TRX vstest, binlog MSBuild, rapport `dotnet format`, JSON Playwright.
//!
//! Le binlog réel (format 25, SDK 10.0.300) écrit certains index de chaîne
//! `2` suivis de l'index véritable. Sans cette indirection, le code `CS0103`
//! est lu comme un chemin et la colonne 19 devient un autre champ.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use serde_json::Value;

use crate::command_basename;

const RECORD_END_OF_FILE: u32 = 0;
const RECORD_ERROR: u32 = 9;
const RECORD_WARNING: u32 = 10;
const RECORD_STRING: u32 = 24;

const FLAG_CONTEXT: u32 = 1 << 0;
const FLAG_MESSAGE: u32 = 1 << 2;
const FLAG_TIMESTAMP: u32 = 1 << 5;
const FLAG_ARGUMENTS: u32 = 1 << 14;
const FLAG_IMPORTANCE: u32 = 1 << 15;
const FLAG_EXTENDED: u32 = 1 << 16;
const STRING_INDEX_BASE: u32 = 10;

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn rest(&self) -> usize {
        self.bytes.len().saturating_sub(self.pos)
    }

    fn u8(&mut self) -> Option<u8> {
        let b = *self.bytes.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }

    fn exact(&mut self, len: usize) -> Option<&'a [u8]> {
        if self.rest() < len {
            return None;
        }
        let start = self.pos;
        self.pos += len;
        Some(&self.bytes[start..self.pos])
    }

    fn i32_le(&mut self) -> Option<i32> {
        let b = self.exact(4)?;
        Some(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn i64_le(&mut self) -> Option<i64> {
        let b = self.exact(8)?;
        Some(i64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    fn i7(&mut self) -> Option<u32> {
        let mut value: u32 = 0;
        let mut shift = 0;
        loop {
            let byte = self.u8()?;
            value |= u32::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Some(value);
            }
            shift += 7;
            if shift >= 35 {
                return None;
            }
        }
    }

    fn dotnet_string(&mut self) -> Option<String> {
        let len = self.i7()? as usize;
        let bytes = self.exact(len)?;
        String::from_utf8(bytes.to_vec()).ok()
    }
}

struct Issue {
    kind: &'static str,
    code: String,
    file: String,
    line: u32,
    column: u32,
    message: String,
}

pub fn summarize_for_command(command: &[String], raw: &str) -> Option<(String, String)> {
    if let Some(kind) = dotnet_verb(command) {
        if kind == "test" {
            if let Some(summary) = trx_from_command(command, raw) {
                return Some(("trx".to_string(), summary));
            }
        }
        if kind == "build" || kind == "test" {
            if let Some(summary) = binlog_from_command(command) {
                return Some(("binlog".to_string(), summary));
            }
        }
        if kind == "format" {
            if let Some(summary) = format_from_command(command, raw) {
                return Some(("dotnet-format".to_string(), summary));
            }
        }
        return None;
    }
    if command_mentions(command, "playwright") {
        if let Some(summary) = summarize_playwright_json(raw) {
            return Some(("playwright-json".to_string(), summary));
        }
    }
    None
}

fn dotnet_verb(command: &[String]) -> Option<&str> {
    let program = command.first()?;
    let base = command_basename(program);
    if base == "dotnet-format" {
        return Some("format");
    }
    if base != "dotnet" {
        return None;
    }
    command
        .iter()
        .skip(1)
        .find(|arg| !arg.starts_with('-'))
        .map(String::as_str)
}

fn command_mentions(command: &[String], needle: &str) -> bool {
    command.iter().any(|arg| arg_mentions(arg, needle))
}

fn arg_mentions(arg: &str, needle: &str) -> bool {
    if command_basename(arg) == needle {
        return true;
    }
    arg.split(['/', '\\']).any(|part| {
        let bare = part.trim_start_matches('@').to_ascii_lowercase();
        bare == needle
            || bare.starts_with(&format!("{needle}."))
            || bare.starts_with(&format!("{needle}-"))
    })
}

fn trx_from_command(command: &[String], raw: &str) -> Option<String> {
    if looks_like_trx(raw) {
        return summarize_trx(raw);
    }
    let paths = execution_trx_paths(command, raw);
    if paths.is_empty() {
        return None;
    }
    let (reports, unreadable) = collect_trx_reports(&paths);
    if reports.is_empty() && unreadable.is_empty() {
        return None;
    }
    Some(format_trx_reports(&reports, &unreadable))
}

/// Les lignes « Results File » sont les TRX de cette exécution. Sans elles,
/// on ne garde que la cohorte de la fin la plus récente : un trou de plus
/// de trois heures sépare une exécution précédente.
const TRX_COHORT_GAP_MS: i64 = 3 * 60 * 60 * 1000;

fn execution_trx_paths(command: &[String], raw: &str) -> Vec<PathBuf> {
    let listed = results_file_paths(raw);
    let mut paths = if !listed.is_empty() {
        listed
    } else {
        bound_to_latest_cohort(trx_paths(command))
    };
    paths.sort();
    dedup_paths(paths)
}

fn results_file_paths(raw: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for line in raw.lines() {
        let Some(rest) = results_file_rest(line) else {
            continue;
        };
        let path = rest.trim().trim_matches('"');
        if path.to_ascii_lowercase().ends_with(".trx") {
            paths.push(PathBuf::from(path));
        }
    }
    paths
}

fn results_file_rest(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let lower = trimmed.to_ascii_lowercase();
    if let Some(idx) = lower.find("results file:") {
        return Some(trimmed[idx + "results file:".len()..].trim());
    }
    if let Some(idx) = lower.find("fichier de résultats") {
        return Some(
            trimmed[idx + "fichier de résultats".len()..]
                .trim()
                .trim_start_matches(':')
                .trim(),
        );
    }
    None
}

fn dedup_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for path in paths {
        let key = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if seen.insert(key) {
            out.push(path);
        }
    }
    out
}

fn bound_to_latest_cohort(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    if paths.len() <= 1 {
        return paths;
    }
    let mut dated = Vec::new();
    let mut undated = Vec::new();
    for path in paths {
        match trx_stamp(&path) {
            Some(when) => dated.push((when, path)),
            None => undated.push(path),
        }
    }
    if dated.is_empty() {
        return undated;
    }
    dated.sort_by_key(|(when, _)| std::cmp::Reverse(*when));
    let mut cursor = dated[0].0;
    let mut kept = Vec::new();
    for (when, path) in dated {
        if cursor.saturating_sub(when) <= TRX_COHORT_GAP_MS {
            cursor = when;
            kept.push(path);
        }
    }
    kept.extend(undated);
    kept
}

fn trx_stamp(path: &Path) -> Option<i64> {
    if let Ok(bytes) = std::fs::read(path) {
        if let Ok(text) = String::from_utf8(bytes) {
            if let Some(ms) =
                times_attr_millis(&text, "finish").or_else(|| times_attr_millis(&text, "creation"))
            {
                return Some(ms);
            }
        }
    }
    mtime_millis(path)
}

fn times_attr_millis(xml: &str, name: &str) -> Option<i64> {
    let times_at = xml.find("<Times")?;
    let tag = xml[times_at..].split('>').next()?;
    let key = format!("{name}=\"");
    let idx = tag.find(&key)?;
    let value = tag[idx + key.len()..].split('"').next()?;
    parse_rfc3339_millis(value)
}

fn mtime_millis(path: &Path) -> Option<i64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let dur = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(dur.as_millis() as i64)
}

fn parse_rfc3339_millis(text: &str) -> Option<i64> {
    if text.len() < 19 || text.as_bytes().get(10) != Some(&b'T') {
        return None;
    }
    let year: i32 = text[0..4].parse().ok()?;
    let month: u32 = text[5..7].parse().ok()?;
    let day: u32 = text[8..10].parse().ok()?;
    let hour: u32 = text[11..13].parse().ok()?;
    let min: u32 = text[14..16].parse().ok()?;
    let sec: u32 = text[17..19].parse().ok()?;
    let mut rest = &text[19..];
    let mut millis: i64 = 0;
    if let Some(stripped) = rest.strip_prefix('.') {
        let digits: String = stripped
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        rest = &stripped[digits.len()..];
        let mut ms = digits;
        while ms.len() < 3 {
            ms.push('0');
        }
        millis = ms[..3].parse().ok()?;
    }
    let offset_min: i64 = if rest.is_empty() || rest == "Z" {
        0
    } else if rest.starts_with('+') || rest.starts_with('-') {
        let sign: i64 = if rest.starts_with('-') { -1 } else { 1 };
        let hh: i64 = rest.get(1..3)?.parse().ok()?;
        let mm: i64 = if rest.len() >= 6 {
            rest.get(4..6)?.parse().ok()?
        } else {
            0
        };
        sign * (hh * 60 + mm)
    } else {
        return None;
    };
    let days = days_from_civil(year, month, day)?;
    let unix_sec = days * 86400 + i64::from(hour) * 3600 + i64::from(min) * 60 + i64::from(sec)
        - offset_min * 60;
    Some(unix_sec * 1000 + millis)
}

/// Jours écoulés depuis 1970-01-01 (algorithme de Howard Hinnant).
fn days_from_civil(year: i32, month: u32, day: u32) -> Option<i64> {
    if !(1..=12).contains(&month) || day == 0 || day > 31 {
        return None;
    }
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + u64::from(doy);
    Some(i64::from(era) * 146097 + doe as i64 - 719468)
}

fn collect_trx_reports(paths: &[PathBuf]) -> (Vec<TrxReport>, Vec<String>) {
    let mut reports = Vec::new();
    let mut unreadable = Vec::new();
    for path in paths {
        match read_trx_report(path) {
            TrxRead::Ok(report) => reports.push(report),
            TrxRead::Unreadable => unreadable.push(path.display().to_string()),
        }
    }
    (reports, unreadable)
}

enum TrxRead {
    Ok(TrxReport),
    Unreadable,
}

fn read_trx_report(path: &Path) -> TrxRead {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return TrxRead::Unreadable,
    };
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => return TrxRead::Unreadable,
    };
    match parse_trx(&text) {
        Some(report) => TrxRead::Ok(report),
        None => TrxRead::Unreadable,
    }
}

struct TrxReport {
    total: u32,
    passed: u32,
    failed: u32,
    failures: Vec<Failure>,
}

fn format_trx_reports(reports: &[TrxReport], unreadable: &[String]) -> String {
    let total: u32 = reports.iter().map(|report| report.total).sum();
    let passed: u32 = reports.iter().map(|report| report.passed).sum();
    let failed: u32 = reports.iter().map(|report| report.failed).sum();
    let mut out = if reports.len() > 1 {
        format!(
            "dotnet trx: total=\"{total}\" passed=\"{passed}\" failed=\"{failed}\" projects=\"{}\"\n",
            reports.len()
        )
    } else {
        format!("dotnet trx: total=\"{total}\" passed=\"{passed}\" failed=\"{failed}\"\n")
    };
    for path in unreadable {
        out.push_str("unreadable trx: ");
        out.push_str(path);
        out.push('\n');
    }
    let failures: Vec<&Failure> = reports
        .iter()
        .flat_map(|report| report.failures.iter())
        .collect();
    if failures.is_empty() && failed == 0 && unreadable.is_empty() {
        out.push_str("no failed tests\n");
        return out;
    }
    for failure in failures {
        out.push_str("Failed ");
        out.push_str(failure.name.trim());
        if let Some(project) = &failure.project {
            out.push_str(" [");
            out.push_str(project);
            out.push(']');
        }
        out.push('\n');
        let message = failure.message.trim();
        if !message.is_empty() {
            out.push_str(message);
            out.push('\n');
        }
        let stack = useful_stack(&failure.stack);
        if !stack.is_empty() {
            out.push_str(&stack);
            out.push('\n');
        }
    }
    out
}

fn looks_like_trx(raw: &str) -> bool {
    let head = raw.trim_start_matches('\u{feff}');
    head.contains("<TestRun") && head.contains("UnitTestResult")
}

fn trx_paths(command: &[String]) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut results_dir: Option<PathBuf> = None;
    let mut log_name: Option<String> = None;
    let args: Vec<&str> = command.iter().map(String::as_str).collect();
    let mut i = 0;
    while i < args.len() {
        let arg = args[i];
        if let Some(dir) = flag_value(arg, &["--results-directory", "/ResultsDirectory"]) {
            results_dir = Some(PathBuf::from(dir));
        } else if arg == "--results-directory" || arg == "--resultsDirectory" {
            if let Some(next) = args.get(i + 1) {
                results_dir = Some(PathBuf::from(next));
                i += 1;
            }
        }
        if let Some(name) = arg.split("LogFileName=").nth(1) {
            let name = name.trim_matches(|c| c == '"' || c == ';');
            if !name.is_empty() {
                log_name = Some(name.to_string());
            }
        }
        if arg.ends_with(".trx") {
            paths.push(PathBuf::from(arg));
        }
        i += 1;
    }
    if let Some(dir) = results_dir {
        if let Some(name) = &log_name {
            paths.push(dir.join(name));
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            let mut found: Vec<PathBuf> = entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.extension()
                        .and_then(|ext| ext.to_str())
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("trx"))
                })
                .collect();
            found.sort();
            paths.extend(found);
        }
    }
    paths
}

fn flag_value<'a>(arg: &'a str, flags: &[&str]) -> Option<&'a str> {
    for flag in flags {
        if let Some(rest) = arg.strip_prefix(flag) {
            let rest = rest.trim_start_matches(['=', ':']);
            if !rest.is_empty() && rest != *flag {
                return Some(rest);
            }
        }
    }
    None
}

fn binlog_from_command(command: &[String]) -> Option<String> {
    let path = binlog_path(command)?;
    let bytes = std::fs::read(&path).ok()?;
    let summary = summarize_binlog(&bytes)?;
    if summary_has_issue(&summary) {
        Some(summary)
    } else {
        None
    }
}

fn summary_has_issue(summary: &str) -> bool {
    summary.contains("error ") || summary.contains("warning ") || summary.contains("Failed ")
}

fn binlog_path(command: &[String]) -> Option<PathBuf> {
    for arg in command {
        if let Some(path) = arg.strip_prefix("-bl:") {
            return Some(PathBuf::from(path));
        }
        if let Some(path) = arg.strip_prefix("/bl:") {
            return Some(PathBuf::from(path));
        }
        if let Some(path) = arg.strip_prefix("-binaryLogger:") {
            return Some(PathBuf::from(path));
        }
        if let Some(path) = arg.strip_prefix("--binaryLogger:") {
            return Some(PathBuf::from(path));
        }
        if arg.ends_with(".binlog") {
            return Some(PathBuf::from(arg.as_str()));
        }
    }
    if command.iter().any(|arg| arg == "-bl" || arg == "/bl") {
        return Some(PathBuf::from("msbuild.binlog"));
    }
    None
}

fn format_from_command(command: &[String], raw: &str) -> Option<String> {
    if let Some(summary) = summarize_format_report(raw) {
        return Some(summary);
    }
    for path in format_report_paths(command) {
        if path.is_dir() {
            let direct = path.join("format-report.json");
            if let Some(summary) = read_format_file(&direct) {
                return Some(summary);
            }
            if let Ok(entries) = std::fs::read_dir(&path) {
                for entry in entries.filter_map(Result::ok) {
                    let candidate = entry.path();
                    if candidate.extension().and_then(|ext| ext.to_str()) == Some("json") {
                        if let Some(summary) = read_format_file(&candidate) {
                            return Some(summary);
                        }
                    }
                }
            }
        } else if let Some(summary) = read_format_file(&path) {
            return Some(summary);
        }
    }
    None
}

fn read_format_file(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    summarize_format_report(&text)
}

fn format_report_paths(command: &[String]) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let args: Vec<&str> = command.iter().map(String::as_str).collect();
    let mut i = 0;
    while i < args.len() {
        let arg = args[i];
        if let Some(path) = arg.strip_prefix("--report=") {
            paths.push(PathBuf::from(path));
        } else if arg == "--report" {
            if let Some(next) = args.get(i + 1) {
                paths.push(PathBuf::from(next));
                i += 1;
            }
        }
        i += 1;
    }
    paths
}

pub fn summarize_trx(raw: &str) -> Option<String> {
    let report = parse_trx(raw)?;
    Some(format_trx_reports(std::slice::from_ref(&report), &[]))
}

fn parse_trx(raw: &str) -> Option<TrxReport> {
    let xml = raw.trim_start_matches('\u{feff}');
    if !looks_like_trx(xml) {
        return None;
    }
    let mut events = XmlEvents::new(xml);
    let mut total: Option<u32> = None;
    let mut passed: Option<u32> = None;
    let mut failed: Option<u32> = None;
    let mut failures: Vec<Failure> = Vec::new();
    let mut passed_n = 0usize;
    let mut failed_n = 0usize;
    let mut other_n = 0usize;
    let mut current: Option<Failure> = None;
    let mut in_error = false;
    let mut capture: Option<Capture> = None;
    let mut projects: HashMap<String, String> = HashMap::new();
    let mut assemblies: HashSet<String> = HashSet::new();
    let mut current_test_id: Option<String> = None;

    while let Some(ev) = events.next() {
        match ev {
            XmlEv::Start { name, empty, attrs } if name == "Counters" => {
                total = attr(&attrs, "total").and_then(|v| v.parse().ok());
                passed = attr(&attrs, "passed").and_then(|v| v.parse().ok());
                failed = attr(&attrs, "failed").and_then(|v| v.parse().ok());
                if empty {
                    continue;
                }
            }
            XmlEv::Start { name, empty, attrs } if name == "UnitTestResult" => {
                let outcome = attr(&attrs, "outcome").unwrap_or_default();
                match outcome.as_str() {
                    "Failed" => {
                        failed_n += 1;
                        current = Some(Failure {
                            name: attr(&attrs, "testName").unwrap_or_else(|| "unknown".into()),
                            message: String::new(),
                            stack: String::new(),
                            project: None,
                            test_id: attr(&attrs, "testId"),
                        });
                    }
                    "Passed" => passed_n += 1,
                    _ => other_n += 1,
                }
                if empty {
                    if let Some(done) = current.take() {
                        failures.push(done);
                    }
                }
            }
            XmlEv::End { name } if name == "UnitTestResult" => {
                if let Some(done) = current.take() {
                    failures.push(done);
                }
                in_error = false;
                capture = None;
            }
            XmlEv::Start { name, empty, .. } if name == "ErrorInfo" && current.is_some() => {
                in_error = !empty;
            }
            XmlEv::End { name } if name == "ErrorInfo" => {
                in_error = false;
                capture = None;
            }
            XmlEv::Start { name, empty, .. }
                if in_error && (name == "Message" || name == "StackTrace") =>
            {
                if !empty {
                    capture = Some(if name == "Message" {
                        Capture::Message
                    } else {
                        Capture::Stack
                    });
                }
            }
            XmlEv::End { name } if name == "Message" || name == "StackTrace" => {
                capture = None;
            }
            XmlEv::Start { name, empty, attrs } if name == "UnitTest" => {
                current_test_id = attr(&attrs, "id");
                if let (Some(id), Some(storage)) = (&current_test_id, attr(&attrs, "storage")) {
                    if let Some(stem) = assembly_stem(&storage) {
                        assemblies.insert(stem.clone());
                        projects.entry(id.clone()).or_insert(stem);
                    }
                }
                if empty {
                    current_test_id = None;
                }
            }
            XmlEv::End { name } if name == "UnitTest" => {
                current_test_id = None;
            }
            XmlEv::Start { name, attrs, .. } if name == "TestMethod" => {
                if let Some(stem) = attr(&attrs, "codeBase").and_then(|path| assembly_stem(&path)) {
                    assemblies.insert(stem.clone());
                    if let Some(id) = &current_test_id {
                        projects.insert(id.clone(), stem);
                    }
                }
            }
            XmlEv::Text(text) => {
                if let Some(failure) = current.as_mut() {
                    match capture {
                        Some(Capture::Message) => failure.message.push_str(&text),
                        Some(Capture::Stack) => failure.stack.push_str(&text),
                        None => {}
                    }
                }
            }
            _ => {}
        }
    }

    if total.is_none() && passed_n + failed_n + other_n == 0 {
        return None;
    }
    let (total, passed, failed) = match (total, passed, failed) {
        (Some(total), Some(passed), Some(failed)) => (total, passed, failed),
        _ => (
            (passed_n + failed_n + other_n) as u32,
            passed_n as u32,
            failed_n as u32,
        ),
    };
    for failure in &mut failures {
        if let Some(id) = &failure.test_id {
            if let Some(project) = projects.get(id) {
                failure.project = Some(project.clone());
            }
        }
        if failure.project.is_none() && assemblies.len() == 1 {
            failure.project = assemblies.iter().next().cloned();
        }
    }
    Some(TrxReport {
        total,
        passed,
        failed,
        failures,
    })
}

fn assembly_stem(path: &str) -> Option<String> {
    let name = path
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())?;
    let dot = name.rfind('.')?;
    let ext = &name[dot + 1..];
    if !ext.eq_ignore_ascii_case("dll") && !ext.eq_ignore_ascii_case("exe") {
        return None;
    }
    let stem = &name[..dot];
    if stem.is_empty() {
        None
    } else {
        Some(stem.to_string())
    }
}

struct Failure {
    name: String,
    message: String,
    stack: String,
    project: Option<String>,
    test_id: Option<String>,
}

enum Capture {
    Message,
    Stack,
}

fn useful_stack(stack: &str) -> String {
    stack
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| line.contains(":line ") || line.contains(".cs"))
        .filter(|line| {
            !line.contains("System.Reflection")
                && !line.contains("InvokeStub_")
                && !line.contains("MethodBaseInvoker")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn attr(attrs: &[(String, String)], key: &str) -> Option<String> {
    attrs
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.clone())
}

enum XmlEv {
    Start {
        name: String,
        empty: bool,
        attrs: Vec<(String, String)>,
    },
    End {
        name: String,
    },
    Text(String),
}

struct XmlEvents<'a> {
    src: &'a str,
    i: usize,
}

impl<'a> XmlEvents<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, i: 0 }
    }

    fn next(&mut self) -> Option<XmlEv> {
        let bytes = self.src.as_bytes();
        if self.i >= bytes.len() {
            return None;
        }
        if bytes[self.i] != b'<' {
            let start = self.i;
            while self.i < bytes.len() && bytes[self.i] != b'<' {
                self.i += 1;
            }
            let text = unescape(&self.src[start..self.i]);
            if text.trim().is_empty() {
                return self.next();
            }
            return Some(XmlEv::Text(text));
        }
        if self.src[self.i..].starts_with("</") {
            self.i += 2;
            let name = self.read_name();
            self.skip_until(b'>');
            return Some(XmlEv::End { name });
        }
        if self.src[self.i..].starts_with("<?") {
            self.skip_until_str("?>");
            return self.next();
        }
        if self.src[self.i..].starts_with("<!--") {
            self.skip_until_str("-->");
            return self.next();
        }
        if self.src[self.i..].starts_with("<!") {
            self.skip_until(b'>');
            return self.next();
        }
        self.i += 1;
        let name = self.read_name();
        let attrs = self.read_attrs();
        let empty = self.src[..self.i].ends_with("/>")
            || (self.i > 0 && self.src.as_bytes().get(self.i - 1) == Some(&b'/'));
        // read_attrs leaves i on the closing '>' or after '/>'.
        if self.i < bytes.len() && bytes[self.i - 1] != b'>' {
            self.skip_until(b'>');
        }
        Some(XmlEv::Start { name, empty, attrs })
    }

    fn read_name(&mut self) -> String {
        let start = self.i;
        let bytes = self.src.as_bytes();
        while self.i < bytes.len() && is_name(bytes[self.i]) {
            self.i += 1;
        }
        let raw = &self.src[start..self.i];
        raw.rsplit(':').next().unwrap_or(raw).to_string()
    }

    fn read_attrs(&mut self) -> Vec<(String, String)> {
        let mut attrs = Vec::new();
        let bytes = self.src.as_bytes();
        loop {
            self.skip_ws();
            if self.i >= bytes.len() {
                break;
            }
            let c = bytes[self.i];
            if c == b'>' {
                self.i += 1;
                break;
            }
            if c == b'/' {
                self.i += 1;
                if self.i < bytes.len() && bytes[self.i] == b'>' {
                    self.i += 1;
                }
                break;
            }
            let name = self.read_name();
            self.skip_ws();
            if self.i >= bytes.len() || bytes[self.i] != b'=' {
                continue;
            }
            self.i += 1;
            self.skip_ws();
            if self.i >= bytes.len() {
                break;
            }
            let quote = bytes[self.i];
            if quote != b'"' && quote != b'\'' {
                continue;
            }
            self.i += 1;
            let start = self.i;
            while self.i < bytes.len() && bytes[self.i] != quote {
                self.i += 1;
            }
            let value = unescape(&self.src[start..self.i]);
            if self.i < bytes.len() {
                self.i += 1;
            }
            attrs.push((name, value));
        }
        attrs
    }

    fn skip_ws(&mut self) {
        let bytes = self.src.as_bytes();
        while self.i < bytes.len() && bytes[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn skip_until(&mut self, byte: u8) {
        let bytes = self.src.as_bytes();
        while self.i < bytes.len() && bytes[self.i] != byte {
            self.i += 1;
        }
        if self.i < bytes.len() {
            self.i += 1;
        }
    }

    fn skip_until_str(&mut self, marker: &str) {
        if let Some(rel) = self.src[self.i..].find(marker) {
            self.i += rel + marker.len();
        } else {
            self.i = self.src.len();
        }
    }
}

fn is_name(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b':' | b'-' | b'.')
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        if let Some(end) = rest.find(';') {
            let entity = &rest[1..end];
            let ch = match entity {
                "lt" => Some('<'),
                "gt" => Some('>'),
                "amp" => Some('&'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ if entity.starts_with('#') => decode_numeric(&entity[1..]),
                _ => None,
            };
            if let Some(ch) = ch {
                out.push(ch);
                rest = &rest[end + 1..];
                continue;
            }
        }
        out.push('&');
        rest = &rest[1..];
    }
    out.push_str(rest);
    out
}

fn decode_numeric(body: &str) -> Option<char> {
    let code = if let Some(hex) = body.strip_prefix('x').or_else(|| body.strip_prefix('X')) {
        u32::from_str_radix(hex, 16).ok()?
    } else {
        body.parse().ok()?
    };
    char::from_u32(code)
}

pub fn summarize_binlog(bytes: &[u8]) -> Option<String> {
    let payload = inflate_binlog(bytes)?;
    let mut cursor = Cursor::new(&payload);
    let version = cursor.i32_le()?;
    let _min_reader = cursor.i32_le()?;
    if version < 18 {
        return None;
    }
    let mut strings: Vec<String> = Vec::new();
    let mut issues: Vec<Issue> = Vec::new();
    while cursor.rest() > 0 {
        let Some(kind) = cursor.i7() else { break };
        if kind == RECORD_END_OF_FILE {
            break;
        }
        if kind == RECORD_STRING {
            let Some(text) = cursor.dotnet_string() else {
                break;
            };
            strings.push(text);
            continue;
        }
        let Some(len) = cursor.i7() else { break };
        let Some(record) = cursor.exact(len as usize) else {
            break;
        };
        if kind == RECORD_ERROR || kind == RECORD_WARNING {
            if let Some(issue) = read_issue(kind, record, version, &strings) {
                issues.push(issue);
            }
        }
    }
    if issues.is_empty() {
        return None;
    }
    let errors = issues.iter().filter(|issue| issue.kind == "error").count();
    let warnings = issues.len() - errors;
    let mut out = format!("binlog: {errors} error, {warnings} warning\n");
    for issue in issues {
        if issue.file.is_empty() {
            out.push_str(&format!(
                "{} {}: {}\n",
                issue.kind, issue.code, issue.message
            ));
        } else {
            out.push_str(&format!(
                "{}({},{}): {} {}: {}\n",
                issue.file, issue.line, issue.column, issue.kind, issue.code, issue.message
            ));
        }
    }
    Some(out)
}

fn inflate_binlog(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut decoder = GzDecoder::new(bytes);
        let mut payload = Vec::new();
        decoder.read_to_end(&mut payload).ok()?;
        Some(payload)
    } else if bytes.len() >= 8 {
        Some(bytes.to_vec())
    } else {
        None
    }
}

fn read_issue(kind: u32, record: &[u8], version: i32, strings: &[String]) -> Option<Issue> {
    let mut cursor = Cursor::new(record);
    let message = read_event_fields(&mut cursor, version, strings)?;
    let _subcategory = read_string(&mut cursor, strings)?;
    let code = read_string(&mut cursor, strings)?.unwrap_or_default();
    let file = read_string(&mut cursor, strings)?.unwrap_or_default();
    let _project = read_string(&mut cursor, strings)?;
    let line = cursor.i7().unwrap_or(0);
    let column = cursor.i7().unwrap_or(0);
    if code.is_empty() && message.is_none() {
        return None;
    }
    Some(Issue {
        kind: if kind == RECORD_ERROR {
            "error"
        } else {
            "warning"
        },
        code,
        file,
        line,
        column,
        message: message.unwrap_or_default(),
    })
}

fn read_event_fields(
    cursor: &mut Cursor<'_>,
    version: i32,
    strings: &[String],
) -> Option<Option<String>> {
    let flags = cursor.i7()?;
    let mut message = None;
    if flags & FLAG_MESSAGE != 0 {
        message = read_string(cursor, strings)?;
    }
    if flags & FLAG_CONTEXT != 0 {
        let count = if version > 1 { 7 } else { 6 };
        for _ in 0..count {
            cursor.i7()?;
        }
    }
    if flags & FLAG_TIMESTAMP != 0 {
        cursor.i64_le()?;
        cursor.i7()?;
    }
    if flags & FLAG_EXTENDED != 0 {
        let _ = read_string(cursor, strings)?;
        cursor.i7()?;
        let _ = read_string(cursor, strings)?;
    }
    if flags & FLAG_ARGUMENTS != 0 {
        let count = cursor.i7()? as usize;
        for _ in 0..count {
            let _ = read_string(cursor, strings)?;
        }
    }
    // Importance is a message-record field. Format 25 diagnostics do not set
    // the flag; reading a spare integer here would swallow the error code.
    if version < 13 || flags & FLAG_IMPORTANCE != 0 {
        cursor.i7()?;
    }
    Some(message)
}

/// `None` means the index itself could not be read. `Some(None)` is a null string.
fn read_string(cursor: &mut Cursor<'_>, strings: &[String]) -> Option<Option<String>> {
    read_string_depth(cursor, strings, 0)
}

fn read_string_depth(
    cursor: &mut Cursor<'_>,
    strings: &[String],
    depth: u8,
) -> Option<Option<String>> {
    let index = cursor.i7()?;
    if index == 0 {
        return Some(None);
    }
    if index == 1 {
        return Some(Some(String::new()));
    }
    // Format 25: index 2 is followed by the real string index.
    if index == 2 {
        if depth > 4 {
            return Some(None);
        }
        return read_string_depth(cursor, strings, depth + 1);
    }
    if index < STRING_INDEX_BASE {
        return Some(None);
    }
    let slot = (index - STRING_INDEX_BASE) as usize;
    Some(strings.get(slot).cloned())
}

pub fn summarize_format_report(raw: &str) -> Option<String> {
    let value = first_json(raw)?;
    let entries = value.as_array()?;
    if entries.is_empty() {
        return None;
    }
    let mut lines = Vec::new();
    let mut files = 0usize;
    for entry in entries {
        let changes = json_field(entry, &["FileChanges", "fileChanges"])?.as_array()?;
        if changes.is_empty() {
            continue;
        }
        files += 1;
        let path = json_str(entry, &["FilePath", "filePath"]).unwrap_or_else(|| "?".into());
        for change in changes {
            let line = json_u32(change, &["LineNumber", "lineNumber"]).unwrap_or(0);
            let column = json_u32(change, &["CharNumber", "charNumber"]).unwrap_or(0);
            let id = json_str(change, &["DiagnosticId", "diagnosticId"]).unwrap_or_default();
            let description =
                json_str(change, &["FormatDescription", "formatDescription"]).unwrap_or_default();
            if id.is_empty() && description.is_empty() {
                continue;
            }
            lines.push(format!(
                "{path}({line},{column}): error {id}: {description}"
            ));
        }
    }
    if lines.is_empty() {
        return None;
    }
    let mut out = format!(
        "dotnet format --verify-no-changes: {files} file, {} change\n",
        lines.len()
    );
    for line in lines {
        out.push_str(&line);
        out.push('\n');
    }
    Some(out)
}

pub fn summarize_playwright_json(raw: &str) -> Option<String> {
    let value = first_json(raw)?;
    let stats = value.get("stats")?;
    let suites = value.get("suites")?.as_array()?;
    let expected = json_u32(stats, &["expected"]).unwrap_or(0);
    let unexpected = json_u32(stats, &["unexpected"]).unwrap_or(0);
    let skipped = json_u32(stats, &["skipped"]).unwrap_or(0);
    let flaky = json_u32(stats, &["flaky"]).unwrap_or(0);
    let mut failures = Vec::new();
    for suite in suites {
        collect_playwright_failures(suite, "", &mut failures);
    }
    if failures.is_empty() && unexpected == 0 && expected == 0 {
        return None;
    }
    let mut out = format!(
        "playwright json: \"expected\": {expected}, \"unexpected\": {unexpected}, \"skipped\": {skipped}, \"flaky\": {flaky}\n"
    );
    for failure in failures {
        out.push_str("Failed ");
        out.push_str(&failure.title);
        out.push('\n');
        if !failure.detail.is_empty() {
            out.push_str(&failure.detail);
            if !failure.detail.ends_with('\n') {
                out.push('\n');
            }
        }
    }
    Some(out)
}

struct PwFailure {
    title: String,
    detail: String,
}

fn collect_playwright_failures(suite: &Value, prefix: &str, out: &mut Vec<PwFailure>) {
    let title = suite.get("title").and_then(Value::as_str).unwrap_or("");
    let file = suite.get("file").and_then(Value::as_str).unwrap_or("");
    let here = if prefix.is_empty() {
        if !file.is_empty() && !title.is_empty() {
            format!("{file} › {title}")
        } else if !file.is_empty() {
            file.to_string()
        } else {
            title.to_string()
        }
    } else if title.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix} › {title}")
    };
    if let Some(specs) = suite.get("specs").and_then(Value::as_array) {
        for spec in specs {
            let ok = spec.get("ok").and_then(Value::as_bool).unwrap_or(true);
            if ok {
                continue;
            }
            let spec_title = spec.get("title").and_then(Value::as_str).unwrap_or("");
            let spec_file = spec.get("file").and_then(Value::as_str).unwrap_or(file);
            let full = if !spec_file.is_empty() && !spec_title.is_empty() {
                format!("{spec_file} › {spec_title}")
            } else if spec_title.is_empty() {
                here.clone()
            } else {
                format!("{here} › {spec_title}")
            };
            out.push(PwFailure {
                title: full,
                detail: playwright_detail(spec),
            });
        }
    }
    if let Some(nested) = suite.get("suites").and_then(Value::as_array) {
        for child in nested {
            collect_playwright_failures(child, &here, out);
        }
    }
}

fn playwright_detail(spec: &Value) -> String {
    let mut lines = Vec::new();
    let tests = spec.get("tests").and_then(Value::as_array);
    let Some(tests) = tests else {
        return String::new();
    };
    for test in tests {
        let results = test.get("results").and_then(Value::as_array);
        let Some(results) = results else {
            continue;
        };
        for result in results {
            let status = result.get("status").and_then(Value::as_str).unwrap_or("");
            if status == "passed" || status == "skipped" {
                continue;
            }
            if let Some(error) = result.get("error") {
                push_playwright_error(&mut lines, error);
            }
            if let Some(errors) = result.get("errors").and_then(Value::as_array) {
                for error in errors {
                    push_playwright_error(&mut lines, error);
                }
            }
        }
    }
    dedup_keep_order(lines).join("\n")
}

fn push_playwright_error(lines: &mut Vec<String>, error: &Value) {
    if let Some(message) = error.get("message").and_then(Value::as_str) {
        let clean = strip_ansi(message);
        for line in clean.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            // Playwright repeats the same poll dozens of times. The first
            // `waiting for locator(...)` names the target; the repeats do not.
            if trimmed.starts_with("- Expect ")
                || trimmed.contains("× locator resolved")
                || trimmed.starts_with("- unexpected value")
            {
                continue;
            }
            lines.push(trimmed.to_string());
        }
    }
    if let Some(stack) = error.get("stack").and_then(Value::as_str) {
        let clean = strip_ansi(stack);
        for line in clean.lines() {
            let trimmed = line.trim();
            if trimmed.contains(".spec.") || trimmed.contains(".test.") {
                lines.push(trimmed.to_string());
            }
        }
    }
    if let Some(location) = error.get("location") {
        let file = location.get("file").and_then(Value::as_str).unwrap_or("");
        let line = location.get("line").and_then(Value::as_u64).unwrap_or(0);
        let column = location.get("column").and_then(Value::as_u64).unwrap_or(0);
        if !file.is_empty() && line > 0 {
            lines.push(format!("at {file}:{line}:{column}"));
        }
    }
}

fn dedup_keep_order(lines: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for line in lines {
        if !out.iter().any(|seen: &String| seen == &line) {
            out.push(line);
        }
    }
    out
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('\u{1b}') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let bytes = rest.as_bytes();
        if bytes.len() >= 2 && bytes[1] == b'[' {
            let mut i = 2;
            while i < bytes.len() && !(bytes[i].is_ascii_alphabetic() || bytes[i] == b'@') {
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            rest = rest.get(i..).unwrap_or("");
        } else {
            out.push('\u{1b}');
            rest = rest.get(1..).unwrap_or("");
        }
    }
    out.push_str(rest);
    out
}

fn first_json(raw: &str) -> Option<Value> {
    let start = raw.find(['{', '['])?;
    let mut iter = serde_json::Deserializer::from_str(&raw[start..]).into_iter::<Value>();
    iter.next()?.ok()
}

fn json_field<'a>(value: &'a Value, names: &[&str]) -> Option<&'a Value> {
    let obj = value.as_object()?;
    names.iter().find_map(|name| obj.get(*name))
}

fn json_str(value: &Value, names: &[&str]) -> Option<String> {
    json_field(value, names)
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn json_u32(value: &Value, names: &[&str]) -> Option<u32> {
    let field = json_field(value, names)?;
    field
        .as_u64()
        .or_else(|| field.as_f64().map(|n| n as u64))
        .map(|n| n as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRX: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<TestRun xmlns="http://microsoft.com/schemas/VisualStudio/TeamTest/2010">
  <Results>
    <UnitTestResult testName="Calc.Tests.CalculTests.Passe_01" outcome="Passed" />
    <UnitTestResult testName="Calc.Tests.CalculTests.Somme_grands_nombres_echoue" outcome="Failed">
      <Output>
        <ErrorInfo>
          <Message>Assert.Equal() Failure: Values differ
Expected: 300
Actual:   301</Message>
          <StackTrace>   at Calc.Tests.CalculTests.Somme_grands_nombres_echoue() in /tmp/work/src/Calc.Tests/CalculTests.cs:line 17
   at System.Reflection.MethodBaseInvoker.InvokeWithNoArgs(Object obj, BindingFlags invokeAttr)</StackTrace>
        </ErrorInfo>
      </Output>
    </UnitTestResult>
    <UnitTestResult outcome="Failed" testName="Calc.Tests.CalculTests.Division_par_zero_leve">
      <Output>
        <ErrorInfo>
          <Message>System.DivideByZeroException : Attempted to divide by zero.</Message>
          <StackTrace>   at Calc.Tests.Calcul.Diviser(Int32 a, Int32 b) in /tmp/work/src/Calc.Tests/CalculTests.cs:line 8
   at Calc.Tests.CalculTests.Division_par_zero_leve() in /tmp/work/src/Calc.Tests/CalculTests.cs:line 20</StackTrace>
        </ErrorInfo>
      </Output>
    </UnitTestResult>
  </Results>
  <ResultSummary outcome="Failed">
    <Counters total="18" executed="17" passed="13" failed="4" notExecuted="0" />
  </ResultSummary>
</TestRun>"#;

    #[test]
    fn trx_garde_message_expected_et_pile_utile() {
        let out = summarize_trx(TRX).expect("trx");
        for fact in [
            "total=\"18\"",
            "passed=\"13\"",
            "failed=\"4\"",
            "Calc.Tests.CalculTests.Somme_grands_nombres_echoue",
            "Expected: 300",
            "Actual:   301",
            "CalculTests.cs:line 17",
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
    fn binlog_format_25_lit_code_fichier_ligne_colonne() {
        // Same layout as the SDK 10.0.300 build.binlog: flags 4 (message
        // only) would skip the indirection test, so the body starts with the
        // index-2 indirection observed on the real error record.
        let mut payload = Vec::new();
        push_i32(&mut payload, 25);
        push_i32(&mut payload, 18);
        push_string(
            &mut payload,
            "The name 'inconnu' does not exist in the current context",
        );
        push_string(&mut payload, "CS0103");
        push_string(&mut payload, "/tmp/work/src/BuildFail/Program.cs");
        push_string(&mut payload, "/tmp/work/src/BuildFail/BuildFail.csproj");
        push_string(
            &mut payload,
            "The variable 'jamaisUtilisee' is assigned but its value is never used",
        );
        push_string(&mut payload, "CS0219");
        push_diagnostic(&mut payload, 9, 10, 11, 12, 13, 2, 19);
        push_diagnostic(&mut payload, 10, 14, 15, 12, 13, 1, 5);
        push_i7(&mut payload, 0);
        let out = summarize_binlog(&payload).expect("binlog");
        for fact in [
            "CS0103",
            "The name 'inconnu' does not exist in the current context",
            "/tmp/work/src/BuildFail/Program.cs(2,19)",
            "CS0219",
            "The variable 'jamaisUtilisee' is assigned but its value is never used",
            "/tmp/work/src/BuildFail/Program.cs(1,5)",
        ] {
            assert!(out.contains(fact), "{fact} absent de:\n{out}");
        }
    }

    fn push_i32(buf: &mut Vec<u8>, value: i32) {
        buf.extend(value.to_le_bytes());
    }

    fn push_i7(buf: &mut Vec<u8>, mut value: u32) {
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

    fn push_string(buf: &mut Vec<u8>, text: &str) {
        push_i7(buf, RECORD_STRING);
        push_i7(buf, text.len() as u32);
        buf.extend(text.as_bytes());
    }

    fn push_diagnostic(
        buf: &mut Vec<u8>,
        kind: u32,
        message: u32,
        code: u32,
        file: u32,
        project: u32,
        line: u32,
        column: u32,
    ) {
        let mut body = Vec::new();
        push_i7(&mut body, FLAG_MESSAGE);
        push_i7(&mut body, message);
        // Index 2, then 1: empty subcategory written the way format 25 does.
        push_i7(&mut body, 2);
        push_i7(&mut body, 1);
        push_i7(&mut body, code);
        push_i7(&mut body, file);
        push_i7(&mut body, project);
        push_i7(&mut body, line);
        push_i7(&mut body, column);
        push_i7(&mut body, 0);
        push_i7(&mut body, 0);
        push_i7(buf, kind);
        push_i7(buf, body.len() as u32);
        buf.extend(body);
    }

    #[test]
    fn format_json_garde_fichier_ligne_et_diagnostic() {
        let raw = r#"[{"FilePath":"/tmp/work/src/FormatMe/Program.cs","FileChanges":[{"LineNumber":1,"CharNumber":15,"DiagnosticId":"WHITESPACE","FormatDescription":"Fix whitespace formatting. Insert '\\n'."},{"LineNumber":3,"CharNumber":24,"DiagnosticId":"WHITESPACE","FormatDescription":"Fix whitespace formatting. Delete 1 characters."}]}]"#;
        let out = summarize_format_report(raw).expect("format");
        assert!(out.contains("/tmp/work/src/FormatMe/Program.cs(1,15): error WHITESPACE: Fix whitespace formatting. Insert '\\n'."));
        assert!(out
            .contains("(3,24): error WHITESPACE: Fix whitespace formatting. Delete 1 characters."));
        assert!(!out.contains("DocumentId"), "{out}");
    }

    #[test]
    fn playwright_json_garde_les_echecs_pas_les_articles() {
        let raw = r#"{"stats":{"expected":12,"unexpected":2,"skipped":0,"flaky":0},"suites":[{"title":"panier.spec.js","file":"panier.spec.js","specs":[
            {"title":"article 1 visible","ok":true,"tests":[{"results":[{"status":"passed","errors":[]}]}]},
            {"title":"le total est juste","ok":false,"file":"panier.spec.js","tests":[{"results":[{"status":"failed","error":{"message":"Error: expect(locator).toHaveText(expected) failed\n\nLocator: locator('#total')\nExpected: \"42,90 €\"\nReceived: \"41,90 €\"\n","stack":"Error: failed\n    at /tmp/pw/tests/panier.spec.js:11:40","location":{"file":"/tmp/pw/tests/panier.spec.js","line":11,"column":40}}}]}]},
            {"title":"le bouton confirmer existe","ok":false,"file":"panier.spec.js","tests":[{"results":[{"status":"failed","error":{"message":"TimeoutError: locator.click: Timeout 1000ms exceeded.\nCall log:\n  - waiting for locator('#confirmer')\n","location":{"file":"/tmp/pw/tests/panier.spec.js","line":15,"column":36}}}]}]}
        ]}]}"#;
        let out = summarize_playwright_json(raw).expect("pw");
        for fact in [
            "\"expected\": 12",
            "\"unexpected\": 2",
            "le total est juste",
            "Expected: \"42,90 €\"",
            "Received: \"41,90 €\"",
            "locator('#total')",
            "/tmp/pw/tests/panier.spec.js:11:40",
            "le bouton confirmer existe",
            "TimeoutError: locator.click: Timeout 1000ms exceeded.",
            "locator('#confirmer')",
            "/tmp/pw/tests/panier.spec.js:15:36",
        ] {
            assert!(out.contains(fact), "{fact} absent de:\n{out}");
        }
        assert!(!out.contains("article 1 visible"), "{out}");
    }

    /// Trois TRX d'une même exécution : le premier trié passe, deux échouent.
    /// Un quatrième, plus ancien dans le tri, n'est pas cité par « Results File ».
    #[test]
    fn trx_solution_additionne_les_compteurs_et_nomme_le_projet() {
        let dir = std::env::temp_dir().join(format!(
            "lm-trx-solution-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let passe = dir.join("aaa-passe.trx");
        let beta = dir.join("bbb-beta.trx");
        let gamma = dir.join("ccc-gamma.trx");
        let ancien = dir.join("zzz-ancien.trx");
        let finish = "2026-09-23T16:12:33.0000000Z";
        std::fs::write(
            &passe,
            mini_trx(
                "AlphaPasse",
                finish,
                &[
                    ("AlphaPasse.AlphaTests.Deux_plus_deux", "Passed", ""),
                    ("AlphaPasse.AlphaTests.Chaine_stable", "Passed", ""),
                ],
                (2, 2, 0),
            ),
        )
        .unwrap();
        std::fs::write(
            &beta,
            mini_trx(
                "BetaEchoue",
                finish,
                &[(
                    "BetaEchoue.BetaTests.Attendu_quarante_deux",
                    "Failed",
                    "Assert.Equal() Failure: Values differ\nExpected: 42\nActual:   43",
                )],
                (1, 0, 1),
            ),
        )
        .unwrap();
        std::fs::write(
            &gamma,
            mini_trx(
                "GammaEchoue",
                finish,
                &[(
                    "GammaEchoue.GammaTests.Attendu_sept",
                    "Failed",
                    "Assert.Equal() Failure: Values differ\nExpected: 7\nActual:   9",
                )],
                (1, 0, 1),
            ),
        )
        .unwrap();
        std::fs::write(
            &ancien,
            mini_trx(
                "Ancien",
                finish,
                &[(
                    "Ancien.Tests.AncienHorsExecution",
                    "Failed",
                    "ANCIEN_HORS_EXECUTION",
                )],
                (1, 0, 1),
            ),
        )
        .unwrap();
        let raw = format!(
            "Passed! Alpha\nResults File: {}\nFailed! Beta\nResults File: {}\nFailed! Gamma\nResults File: {}\n",
            passe.display(),
            beta.display(),
            gamma.display()
        );
        let command = vec![
            "dotnet".to_string(),
            "test".to_string(),
            "Trois.sln".to_string(),
            "--results-directory".to_string(),
            dir.display().to_string(),
        ];
        let (_kind, out) = summarize_for_command(&command, &raw).expect("résumé trx");
        for fact in [
            "total=\"4\"",
            "passed=\"2\"",
            "failed=\"2\"",
            "BetaEchoue.BetaTests.Attendu_quarante_deux",
            "[BetaEchoue]",
            "Expected: 42",
            "Actual:   43",
            "BetaEchoue.cs:line 3",
            "GammaEchoue.GammaTests.Attendu_sept",
            "[GammaEchoue]",
            "Expected: 7",
            "Actual:   9",
            "GammaEchoue.cs:line 3",
        ] {
            assert!(out.contains(fact), "{fact} absent de:\n{out}");
        }
        assert!(!out.contains("no failed tests"), "{out}");
        assert!(!out.contains("AncienHorsExecution"), "{out}");
        assert!(!out.contains("ANCIEN_HORS_EXECUTION"), "{out}");
        assert!(!out.contains("Deux_plus_deux"), "{out}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Sans « Results File », la date de fin écarte un TRX d'une exécution précédente.
    #[test]
    fn trx_vieux_hors_cohorte_nest_pas_agrege() {
        let dir = std::env::temp_dir().join(format!(
            "lm-trx-cohorte-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ancien = dir.join("000-ancien.trx");
        let hors = dir.join("111-hors.trx");
        let passe = dir.join("222-passe.trx");
        let echec = dir.join("333-echec.trx");
        std::fs::write(
            &ancien,
            mini_trx(
                "Ancien",
                "2020-01-01T00:00:00.0000000Z",
                &[(
                    "Ancien.Tests.AncienHorsExecution",
                    "Failed",
                    "ANCIEN_HORS_EXECUTION",
                )],
                (1, 0, 1),
            ),
        )
        .unwrap();
        std::fs::write(
            &hors,
            mini_trx(
                "Hors",
                "2026-09-23T12:00:00.0000000Z",
                &[("Hors.Tests.HorsCohorte", "Failed", "HORS_COHORTE")],
                (1, 0, 1),
            ),
        )
        .unwrap();
        std::fs::write(
            &passe,
            mini_trx(
                "RecentPasse",
                "2026-09-23T16:00:00.0000000Z",
                &[("RecentPasse.Tests.Ok", "Passed", "")],
                (1, 1, 0),
            ),
        )
        .unwrap();
        std::fs::write(
            &echec,
            mini_trx(
                "RecentEchec",
                "2026-09-23T16:05:00.0000000Z",
                &[(
                    "RecentEchec.Tests.Boom",
                    "Failed",
                    "RECENT_ECHEC Expected: 1\nActual:   2",
                )],
                (1, 0, 1),
            ),
        )
        .unwrap();
        let command = vec![
            "dotnet".to_string(),
            "test".to_string(),
            "--results-directory".to_string(),
            dir.display().to_string(),
        ];
        let (_kind, out) = summarize_for_command(&command, "Passed!  - Failed: 0\n").expect("trx");
        assert!(out.contains("total=\"2\""), "{out}");
        assert!(out.contains("passed=\"1\""), "{out}");
        assert!(out.contains("failed=\"1\""), "{out}");
        assert!(out.contains("RecentEchec.Tests.Boom"), "{out}");
        assert!(out.contains("[RecentEchec]"), "{out}");
        assert!(out.contains("RECENT_ECHEC"), "{out}");
        assert!(!out.contains("AncienHorsExecution"), "{out}");
        assert!(!out.contains("ANCIEN_HORS_EXECUTION"), "{out}");
        assert!(!out.contains("HorsCohorte"), "{out}");
        assert!(!out.contains("HORS_COHORTE"), "{out}");
        assert!(!out.contains("no failed tests"), "{out}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Un TRX illisible ne doit pas laisser conclure « no failed tests ».
    #[test]
    fn trx_illisible_empeche_de_conclure_aucun_echec() {
        let dir = std::env::temp_dir().join(format!(
            "lm-trx-illisible-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let passe = dir.join("aaa-passe.trx");
        let casse = dir.join("bbb-casse.trx");
        std::fs::write(
            &passe,
            mini_trx(
                "SeulPasse",
                "2026-09-23T16:12:33.0000000Z",
                &[("SeulPasse.Tests.Ok", "Passed", "")],
                (1, 1, 0),
            ),
        )
        .unwrap();
        std::fs::write(&casse, [0xff, 0xfe, b'<', b'T']).unwrap();
        let raw = format!(
            "Results File: {}\nResults File: {}\n",
            passe.display(),
            casse.display()
        );
        let command = vec![
            "dotnet".to_string(),
            "test".to_string(),
            "--results-directory".to_string(),
            dir.display().to_string(),
        ];
        let (_kind, out) = summarize_for_command(&command, &raw).expect("trx");
        assert!(out.contains("unreadable trx:"), "{out}");
        assert!(out.contains(casse.display().to_string().as_str()), "{out}");
        assert!(!out.contains("no failed tests"), "{out}");
        assert!(out.contains("total=\"1\""), "{out}");
        assert!(out.contains("passed=\"1\""), "{out}");
        assert!(out.contains("failed=\"0\""), "{out}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn mini_trx(
        project: &str,
        finish: &str,
        cases: &[(&str, &str, &str)],
        counters: (u32, u32, u32),
    ) -> String {
        let (total, passed, failed) = counters;
        let mut results = String::new();
        let mut defs = String::new();
        for (i, (name, outcome, message)) in cases.iter().enumerate() {
            let id = format!("id-{project}-{i}");
            if *outcome == "Failed" {
                results.push_str(&format!(
                    "<UnitTestResult testId=\"{id}\" testName=\"{name}\" outcome=\"Failed\"><Output><ErrorInfo><Message>{message}</Message><StackTrace>   at {name}() in /tmp/{project}/{project}.cs:line 3</StackTrace></ErrorInfo></Output></UnitTestResult>"
                ));
            } else {
                results.push_str(&format!(
                    "<UnitTestResult testId=\"{id}\" testName=\"{name}\" outcome=\"{outcome}\" />"
                ));
            }
            defs.push_str(&format!(
                "<UnitTest name=\"{name}\" storage=\"/tmp/{project}/bin/{project}.dll\" id=\"{id}\"><TestMethod codeBase=\"/tmp/{project}/bin/{project}.dll\" className=\"{project}.Tests\" name=\"{name}\" /></UnitTest>"
            ));
        }
        format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<TestRun xmlns=\"http://microsoft.com/schemas/VisualStudio/TeamTest/2010\">\n  <Times creation=\"{finish}\" finish=\"{finish}\" />\n  <Results>{results}</Results>\n  <TestDefinitions>{defs}</TestDefinitions>\n  <ResultSummary outcome=\"Failed\">\n    <Counters total=\"{total}\" passed=\"{passed}\" failed=\"{failed}\" />\n  </ResultSummary>\n</TestRun>\n"
        )
    }
}
