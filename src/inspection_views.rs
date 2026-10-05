//! Explicit inspection commands for arbitrary producers and local manifests.
use anyhow::{Context, Result};
use regex::Regex;
use serde_json::{json, Value};
use std::{borrow::Cow, collections::BTreeMap, path::Path, sync::LazyLock};

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum Mode {
    Raw,
    Errors,
    Tests,
    Summary,
}

pub fn summarize(mode: Mode, raw: &str, exit: i32) -> String {
    if matches!(mode, Mode::Raw) {
        return raw.into();
    }
    let lines: Vec<&str> = raw.lines().collect();
    let plain: Vec<Cow<'_, str>> = lines.iter().map(|line| strip_ansi(line)).collect();
    let mut keep = vec![false; lines.len()];
    let traceback_start = plain.iter().position(|line| {
        line.trim_start()
            .starts_with("Traceback (most recent call last):")
    });
    if let Some(start) = traceback_start {
        // Keep every frame through the terminal exception (including chained
        // tracebacks), without treating unrelated progress after it as a frame.
        let end = plain
            .iter()
            .enumerate()
            .skip(start)
            .rev()
            .find_map(|(i, line)| python_exception_line(line).then_some(i));
        keep[start..=end.unwrap_or(lines.len() - 1)].fill(true);
    }
    for (i, line) in plain.iter().enumerate() {
        let lower = line.to_ascii_lowercase();
        let diagnostic = [
            "error",
            "fatal",
            "panic",
            "traceback",
            "exception",
            "fail",
            "denied",
            "expected",
            "received",
            "assert",
            "not found",
            "missing",
            "warning",
            "symbol:",
            "location:",
        ]
        .iter()
        .any(|word| lower.contains(word))
            || rtk_error_pattern(line)
            || diagnostic_marker(line)
            || file_location(line)
            || structured_fact(line)
            || line.starts_with("[non-UTF-8 capture:")
            || line.trim_start().starts_with('^');
        let test_signal = diagnostic
            || [
                "assert",
                "expected",
                "received",
                "test result:",
                "tests:",
                "test suites:",
                "test files",
                "short test summary",
                "= failures =",
            ]
            .iter()
            .any(|word| lower.contains(word));
        let selected = match mode {
            Mode::Errors => diagnostic,
            Mode::Tests => test_signal,
            Mode::Summary => diagnostic || test_signal,
            Mode::Raw => false,
        };
        if selected {
            keep[i] = true;
            if i > 0 {
                keep[i - 1] = true;
            }
            if i + 1 < lines.len() {
                keep[i + 1] = true;
            }
            // Compilers place source and caret lines after the location.
            if (file_location(line) || line.trim_start().starts_with("File "))
                && i + 2 < lines.len()
            {
                keep[i + 2] = true;
            }
            // Git's fatal diagnostics can carry a short usage example.
            if lower.contains("fatal") {
                for slot in keep.iter_mut().take((i + 5).min(lines.len())).skip(i) {
                    *slot = true;
                }
            }
        }
    }
    // RTK's error stream keeps every indented continuation in an open
    // diagnostic block, including the first blank line. Its independently
    // recognizable frames can also reopen a block far from the first error.
    let mut in_block = false;
    let mut blanks = 0;
    for (i, line) in plain.iter().enumerate() {
        if rtk_error_pattern(line) {
            keep[i] = true;
            in_block = true;
            blanks = 0;
        } else if in_block && line.trim().is_empty() {
            blanks += 1;
            if blanks == 1 {
                keep[i] = true;
            } else {
                in_block = false;
            }
        } else if in_block && (line.starts_with(' ') || line.starts_with('\t')) {
            keep[i] = true;
            blanks = 0;
        } else {
            in_block = false;
        }
    }
    // A true numeric total can precede a long trailing log. Its syntax,
    // rather than its distance from the end, distinguishes it from progress.
    for i in 0..lines.len() {
        if numeric_summary(&plain[i]) {
            keep[i] = true;
        }
    }
    let candidate = lines
        .iter()
        .zip(keep)
        .filter_map(|(line, selected)| selected.then_some(*line))
        .collect::<Vec<_>>()
        .join("\n");
    let body = if candidate.is_empty()
        || crate::token_metrics::TokenCounts::measure(raw, &candidate).tokens_saved <= 0
    {
        raw.to_owned()
    } else {
        format!("{candidate}\n")
    };
    if exit != 0 {
        format!("[FAIL] Command failed (exit code: {exit})\n{body}")
    } else {
        body
    }
}

fn strip_ansi(line: &str) -> Cow<'_, str> {
    static ANSI: LazyLock<Regex> = LazyLock::new(|| {
        // CSI colour/cursor controls and OSC title/hyperlink controls.
        Regex::new(r"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07\x1b]*(?:\x07|\x1b\\))").unwrap()
    });
    ANSI.replace_all(line, "")
}

fn diagnostic_marker(line: &str) -> bool {
    // Tool prefixes, diagnostic annotations and process signals are common
    // across ecosystems. Keep these lines even when their wording has no
    // English "error" or "fail" substring.
    static MARKER: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?ix)
            ^\s*(?:
                (?:[\w.-]+\s+)?(?:err!|warn!|error:|warning:|fatal:|panic:|note:|hint:|help:|caused\s+by:|reason:)
                |(?:segmentation\s+fault|bus\s+error|illegal\s+instruction|floating\s+point\s+exception|aborted|core\s+dumped|killed)(?:\s|\(|$)
                |(?:signal\s+\d+|signal\s+[a-z][\w-]*)(?:\s|$)
                |goroutine\s+\d+\s+\[[^]]+\]:
            )
        ").unwrap()
    });
    MARKER.is_match(line)
}

fn numeric_summary(line: &str) -> bool {
    static TOTAL: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?ix)
            \b(?:\d+\s+(?:tests?|files?|packages?|records?|jobs?|suites?|passed|failed|errors?|warnings?|skipped|ignored|compiled|processed|added)|
                 (?:total|found|finished|results?|summary|tests?\s+run)\s*[:=]?\s*\d+|
                 \d+(?:\.\d+)?%\s+tests?\s+passed|
                 total\s+test\s+time\s*(?:\([^)]*\))?\s*=\s*\d+)\b
        ").unwrap()
    });
    TOTAL.is_match(line)
}

fn rtk_error_pattern(line: &str) -> bool {
    static GENERIC: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)(?:error[\s:\[]|\berr\b|warning[\s:\[]|\bwarn\b|failed|failure|exception|panic)",
        )
        .unwrap()
    });
    static LOCATION: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"^(?:\s*--> .*:\d+:\d+$|Traceback.*$|\s*File ".*", line \d+.*$|\s*at .*:\d+:\d+.*$|.*\.go:\d+:.*$)"#).unwrap()
    });
    GENERIC.is_match(line) || LOCATION.is_match(line)
}

fn structured_fact(line: &str) -> bool {
    static FACT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?ix)^(?:
            \s*[^|\r\n]+\|\s*(?:\d+(?:\s|$)|Bin\s+\d+\s*->\s*\d+\s+bytes\b) # git diff --stat file row
            |\s*[a-z][a-z0-9-]*:\s*\d+(?:\s|$) # numeric HTTP header
            |HTTP/\d(?:\.\d)?\s+\d{3}(?:\s|$) # HTTP status
            |\s*Start\s+\d+\s*:           # CTest test start
            |\s*\d+/\d+\s+Test\s+\x23\d+: # CTest test result
            |\s*(?:not\s+ok|ok)\s+\d+(?:\s|$) # TAP result
        )",
        )
        .unwrap()
    });
    FACT.is_match(line)
}

fn python_exception_line(line: &str) -> bool {
    static EXCEPTION: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^[A-Za-z_][\w.]*(?:Error|Exception|Warning|Interrupt|Exit):(?:\s|$)").unwrap()
    });
    EXCEPTION.is_match(line)
}

fn file_location(line: &str) -> bool {
    let bytes = line.as_bytes();
    bytes.windows(2).enumerate().any(|(i, pair)| {
        pair[0] == b':' && pair[1].is_ascii_digit() && i > 0 && bytes[i - 1] != b'/'
    })
}

pub fn json_schema(raw: &str) -> Result<String> {
    fn schema(v: &Value) -> Value {
        match v {
            Value::Object(map) => {
                Value::Object(map.iter().map(|(k, v)| (k.clone(), schema(v))).collect())
            }
            Value::Array(items) => {
                let mut types = Vec::new();
                for item in items {
                    let s = schema(item);
                    if !types.contains(&s) {
                        types.push(s);
                    }
                }
                json!({"length":items.len(),"items":types})
            }
            Value::Null => json!("null"),
            Value::Bool(_) => json!("boolean"),
            Value::Number(_) => json!("number"),
            Value::String(_) => json!("string"),
        }
    }
    let value: Value = serde_json::from_str(raw).context("invalid JSON input")?;
    Ok(format!(
        "JSON schema (values in tee):\n{}\n",
        serde_json::to_string(&schema(&value))?
    ))
}

pub fn dependencies(path: &Path) -> Result<String> {
    let files = if path.is_file() {
        vec![path.to_path_buf()]
    } else {
        [
            "Cargo.toml",
            "package.json",
            "requirements.txt",
            "pyproject.toml",
        ]
        .iter()
        .map(|p| path.join(p))
        .filter(|p| p.is_file())
        .collect()
    };
    anyhow::ensure!(!files.is_empty(), "no supported dependency manifest found");
    let mut out = String::new();
    for file in files {
        let raw = std::fs::read_to_string(&file)?;
        out.push_str(&format!(
            "{}\n",
            file.file_name().unwrap().to_string_lossy()
        ));
        match file.file_name().and_then(|s| s.to_str()).unwrap_or("") {
            "package.json" => {
                let value: Value = serde_json::from_str(&raw)?;
                for group in [
                    "dependencies",
                    "devDependencies",
                    "peerDependencies",
                    "optionalDependencies",
                ] {
                    if let Some(deps) = value[group].as_object() {
                        for (name, version) in deps {
                            out.push_str(&format!("  {group}: {name} {version}\n"));
                        }
                    }
                }
            }
            "requirements.txt" => {
                for line in raw
                    .lines()
                    .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
                {
                    out.push_str(&format!("  {line}\n"));
                }
            }
            _ => {
                let value: toml::Value = toml::from_str(&raw)?;
                fn visit(value: &toml::Value, prefix: &str, out: &mut String) {
                    if let Some(table) = value.as_table() {
                        for (key, value) in table {
                            let path = if prefix.is_empty() {
                                key.clone()
                            } else {
                                format!("{prefix}.{key}")
                            };
                            if key.ends_with("dependencies") || key == "optional-dependencies" {
                                out.push_str(&format!("  {path}: {value}\n"));
                            } else {
                                visit(value, &path, out);
                            }
                        }
                    }
                }
                visit(&value, "", &mut out);
            }
        }
    }
    Ok(out)
}

pub fn environment(
    vars: impl IntoIterator<Item = (String, String)>,
    filter: Option<&str>,
) -> String {
    let sorted: BTreeMap<_, _> = vars.into_iter().collect();
    let mut out = String::new();
    for (key, value) in sorted {
        if filter.is_some_and(|s| !key.contains(s)) {
            continue;
        }
        let upper = key.to_ascii_uppercase();
        let secret = [
            "TOKEN",
            "SECRET",
            "PASSWORD",
            "PASSWD",
            "KEY",
            "CREDENTIAL",
            "AUTH",
            "COOKIE",
        ]
        .iter()
        .any(|s| upper.contains(s))
            || upper
                .split(|c: char| !c.is_ascii_alphanumeric())
                .any(|part| part == "PASS")
            || upper.contains("PASSPHRASE")
            || value.contains("://") && value.contains('@');
        let value = if secret {
            "[redacted]".into()
        } else if value.chars().count() > 120 {
            value.chars().take(117).collect::<String>() + "..."
        } else {
            value
        };
        out.push_str(&format!(
            "{key}={}\n",
            value.replace('\n', "\\n").replace('\r', "\\r")
        ));
    }
    out
}
// Only unambiguous, single-file line counts; byte counts and multi-file headers
// must keep their original command semantics.
pub fn head_read(command: &[String]) -> Option<(String, usize)> {
    if command.first().map(String::as_str) != Some("head") {
        return None;
    }
    match &command[1..] {
        [flag, n, file] if flag == "-n" && !file.starts_with('-') => {
            Some((file.clone(), n.parse().ok()?))
        }
        [flag, file] if flag.starts_with('-') && !file.starts_with('-') => {
            Some((file.clone(), flag[1..].parse().ok()?))
        }
        [file] if !file.starts_with('-') => Some((file.clone(), 10)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn environment_never_prints_credentials() {
        let out = environment(
            [
                ("API_TOKEN".into(), "secret value".into()),
                ("DATABASE_URL".into(), "postgres://user:password@db".into()),
                ("LANG".into(), "C".into()),
                ("DB_PASS".into(), "hidden".into()),
                ("PASS".into(), "hidden too".into()),
            ],
            None,
        );
        assert_eq!(
            out,
            "API_TOKEN=[redacted]\nDATABASE_URL=[redacted]\nDB_PASS=[redacted]\nLANG=C\nPASS=[redacted]\n"
        );
    }
    #[test]
    fn generic_failure_keeps_its_paragraph_and_status() {
        let raw="progress successful\n\nERROR invoice failed\nExpected 42, got 43\n\ncompleted progress\n";
        let out = summarize(Mode::Errors, raw, 7);
        assert!(out.contains("Expected 42, got 43"));
        assert!(out.starts_with("[FAIL] Command failed (exit code: 7)"));
        assert_eq!(
            summarize(Mode::Errors, "unusual diagnostic", 7),
            "[FAIL] Command failed (exit code: 7)\nunusual diagnostic"
        );
    }
    #[test]
    fn generic_errors_keep_source_carets_and_full_traceback() {
        let traceback = "noise\nTraceback (most recent call last):\n  File \"app.py\", line 8, in a\n    a()\n  File \"app.py\", line 5, in b\n    return c()\n  File \"app.py\", line 2, in c\n    raise ValueError('deep boom')\nValueError: deep boom\n";
        let view = summarize(Mode::Errors, traceback, 1);
        for line in [
            "    a()",
            "    return c()",
            "    raise ValueError('deep boom')",
            "ValueError: deep boom",
        ] {
            assert!(view.contains(line), "missing traceback line: {line}");
        }
        let syntax = "noise\n  File \"bad.py\", line 1\n    def broken(:\n               ^\nSyntaxError: invalid syntax\n";
        let view = summarize(Mode::Errors, syntax, 1);
        assert!(view.contains("    def broken(:"));
        assert!(view.contains("               ^"));
    }
    #[test]
    fn generic_errors_keep_missing_header_and_compiler_details() {
        let raw = "progress\nerror: first problem\nat step compile\nmissing header stdio.h\n12 files checked\n";
        assert!(summarize(Mode::Errors, raw, 1).contains("missing header stdio.h"));
        let java = "Bad.java:1: error: cannot find symbol\n  int x = missing;\n          ^\n  symbol:   variable missing\n  location: class Bad\n1 error\n";
        let view = summarize(Mode::Errors, java, 1);
        for line in [
            "          ^",
            "symbol:   variable missing",
            "location: class Bad",
        ] {
            assert!(view.contains(line), "missing compiler detail: {line}");
        }
    }
    #[test]
    fn generic_views_keep_prefixed_diagnostics_signals_and_distant_advice() {
        let noise = "progress step\n".repeat(30);
        for (line, exit) in [
            ("npm ERR! code ELIFECYCLE", 1),
            ("yarn WARN! deprecated package", 0),
            ("Segmentation fault (core dumped)", 139),
            ("Bus error (core dumped)", 135),
            ("Illegal instruction (core dumped)", 132),
            ("note: this binding is intentionally distant", 0),
            ("hint: inspect the dependency graph", 1),
            ("help: consider changing this to a borrow", 1),
            ("Caused by: inaccessible registry", 1),
            ("goroutine 1 [running]:", 2),
        ] {
            let raw = format!("{noise}{line}\n{noise}done\n");
            for mode in [Mode::Errors, Mode::Tests, Mode::Summary] {
                let out = summarize(mode, &raw, exit);
                assert!(out.contains(line), "lost {line}");
                if exit != 0 {
                    assert!(out.starts_with(&format!("[FAIL] Command failed (exit code: {exit})")));
                }
            }
        }
    }
    #[test]
    fn final_totals_do_not_pull_in_numbered_progress() {
        let raw = format!(
            "warning: check configuration\n{}12 files checked\n",
            (1..=30)
                .map(|n| format!("progress phase {n}: preparing files\n"))
                .collect::<String>()
        );
        let out = summarize(Mode::Summary, &raw, 0);
        assert!(out.contains("warning: check configuration"));
        assert!(out.contains("12 files checked"));
        assert!(!out.contains("progress phase 30:"));
    }
    #[test]
    fn traceback_stops_at_last_exception_and_keeps_chained_frames() {
        let raw = format!(
            "Traceback (most recent call last):\n  File \"a.py\", line 1, in a\n    b()\nValueError: root\n\nDuring handling of the above exception, another exception occurred:\n\nTraceback (most recent call last):\n  File \"a.py\", line 5, in b\n    raise RuntimeError('wrap')\nRuntimeError: wrap\n{}",
            "progress after crash\n".repeat(30)
        );
        let out = summarize(Mode::Errors, &raw, 1);
        assert!(out.contains("ValueError: root"));
        assert!(out.contains("RuntimeError: wrap"));
        assert!(out.contains("    b()"));
        assert_eq!(out.matches("progress after crash").count(), 1);
    }
    #[test]
    fn generic_views_keep_locations_test_totals_file_stats_and_numeric_headers() {
        let noise = "progress phase\n".repeat(25);
        for (line, mode) in [
            ("  File \"bad.py\", line 17, in <module>", Mode::Errors),
            ("100% tests passed, 0 tests failed out of 1", Mode::Tests),
            ("Total Test time (real) =   0.12 sec", Mode::Summary),
            (" tracked.txt | 2 +-", Mode::Summary),
            (" logo.png | Bin 1234 -> 5678 bytes", Mode::Summary),
            ("Content-Length: 31", Mode::Summary),
            ("1/2 Test #1: alpha ... Passed", Mode::Summary),
            ("not ok 2 - total", Mode::Tests),
            ("23 records processed", Mode::Summary),
        ] {
            let raw = format!("{noise}{line}\n{noise}");
            assert!(summarize(mode, &raw, 0).contains(line), "lost {line}");
        }
    }
    #[test]
    fn ansi_decoration_does_not_change_diagnostic_or_fact_selection() {
        let noise = "progress phase\n".repeat(25);
        for (line, fact, mode) in [
            (
                "\x1b[0;32m100% tests passed\x1b[0;0m out of 1",
                "100% tests passed out of 1",
                Mode::Tests,
            ),
            (
                "\x1b[33m logo.png | Bin 1234 -> 5678 bytes\x1b[0m",
                " logo.png | Bin 1234 -> 5678 bytes",
                Mode::Summary,
            ),
            (
                "\x1b[31m  File \"bad.py\", line 7\x1b[0m",
                "  File \"bad.py\", line 7",
                Mode::Errors,
            ),
            (
                "\x1b[33mContent-Length: 31\x1b[0m",
                "Content-Length: 31",
                Mode::Summary,
            ),
            (
                "\x1b[33mwarning: unused value\x1b[0m",
                "warning: unused value",
                Mode::Errors,
            ),
        ] {
            let raw = format!("{noise}{line}\n{noise}");
            let shown = summarize(mode, &raw, 0);
            assert!(strip_ansi(&shown).contains(fact), "lost {fact}");
        }
    }
    #[test]
    fn generic_error_blocks_keep_indented_continuations_until_second_blank() {
        let raw = format!(
            "error: invalid value\n{}\n  help: inspect it\n\n\n  unrelated tail\n",
            "  context without a keyword\n".repeat(8)
        );
        let out = summarize(Mode::Errors, &raw, 1);
        assert!(out.contains("  help: inspect it"));
        assert!(out.contains("  context without a keyword"));
        assert!(!out.contains("  unrelated tail"));
    }
    #[test]
    fn schema_has_every_heterogeneous_variant() {
        let out = json_schema(r#"{"items":[{"a":1},{"b":true},null]}"#).unwrap();
        assert!(out.contains("number"));
        assert!(out.contains("boolean"));
        assert!(out.contains("null"));
        assert!(out.contains("\"length\":3"));
    }
}
