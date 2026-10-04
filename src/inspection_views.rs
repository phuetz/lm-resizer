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
    let candidate = match mode {
        Mode::Raw => return raw.into(),
        Mode::Tests if raw.contains("test result:") => crate::test_views::cargo(raw),
        Mode::Tests
            if raw.contains("test session starts") || raw.contains("short test summary") =>
        {
            crate::test_views::pytest(raw)
        }
        Mode::Tests => crate::test_views::javascript(raw),
        Mode::Errors if exit == 0 => raw.into(),
        Mode::Errors | Mode::Summary => {
            // Keep complete diagnostic paragraphs, including their context.
            // If no known signal is found, retain everything instead of
            // turning an unfamiliar failed command into an empty success.
            let blocks: Vec<_> = raw.split("\n\n").collect();
            let selected: Vec<_> = blocks
                .iter()
                .copied()
                .filter(|s| {
                    let s = s.to_ascii_lowercase();
                    [
                        "error",
                        "failed",
                        "failure",
                        "panic",
                        "fatal",
                        "exception",
                        "denied",
                        "not found",
                    ]
                    .iter()
                    .any(|k| s.contains(k))
                })
                .collect();
            if exit != 0 && !selected.is_empty() && selected.len() < blocks.len() {
                format!("exit {exit}\n{}", selected.join("\n\n"))
            } else {
                crate::container_views::logs(raw)
            }
        }
    };
    if crate::token_metrics::TokenCounts::measure(raw, &candidate).tokens_saved > 0 {
        candidate
    } else {
        raw.into()
    }
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
        assert!(out.contains("exit 7"));
        assert_eq!(
            summarize(Mode::Errors, "unusual diagnostic", 7),
            "unusual diagnostic"
        );
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
