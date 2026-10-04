//! Explicit inspection commands for arbitrary producers and local manifests.
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};

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
    let mut keep = vec![false; lines.len()];
    let traceback_start = lines.iter().position(|line| {
        line.trim_start()
            .starts_with("Traceback (most recent call last):")
    });
    if let Some(start) = traceback_start {
        // A traceback is a single diagnostic: the intermediate frames and
        // source lines explain the final exception even without keywords.
        keep[start..].fill(true);
    }
    for (i, line) in lines.iter().enumerate() {
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
            || file_location(line)
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
            if (file_location(line) || lower.starts_with("file ")) && i + 2 < lines.len() {
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
    // Preserve final numeric totals even if the producer uses unfamiliar wording.
    for i in (0..lines.len()).rev().take(10) {
        if lines[i].chars().any(|c| c.is_ascii_digit()) {
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
    fn schema_has_every_heterogeneous_variant() {
        let out = json_schema(r#"{"items":[{"a":1},{"b":true},null]}"#).unwrap();
        assert!(out.contains("number"));
        assert!(out.contains("boolean"));
        assert!(out.contains("null"));
        assert!(out.contains("\"length\":3"));
    }
}
