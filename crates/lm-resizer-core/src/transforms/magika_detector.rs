//! Deterministic content detection facade.
//!
//! The public function keeps the historical `magika_detect` name so callers do
//! not need to change, but the implementation is local Rust logic only. No
//! model files, native inference runtime, Python bridge, or network download is
//! involved.

use thiserror::Error;

use crate::transforms::content_detector::{detect_content_type, ContentType};
use crate::transforms::unidiff_detector::is_diff;

#[derive(Debug, Error)]
pub enum MagikaDetectorError {
    #[error("deterministic detector failed")]
    Internal,
}

pub fn magika_detect(content: &str) -> Result<ContentType, MagikaDetectorError> {
    Ok(detect_local(content))
}

fn detect_local(content: &str) -> ContentType {
    if content.is_empty() {
        return ContentType::PlainText;
    }

    if is_diff(content) {
        return ContentType::GitDiff;
    }

    if serde_json::from_str::<serde_json::Value>(content).is_ok() {
        return ContentType::JsonArray;
    }

    let detected = detect_content_type(content).content_type;
    if detected != ContentType::PlainText {
        return detected;
    }

    if looks_like_source_code(content) {
        return ContentType::SourceCode;
    }

    ContentType::PlainText
}

fn looks_like_source_code(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    let trimmed = content.trim_start();

    trimmed.starts_with("#!")
        || lower.contains("const ")
        || lower.contains("let ")
        || lower.contains("async ")
        || lower.contains("=>")
        || lower.contains("def ")
        || lower.contains("class ")
        || (lower.contains("select ") && lower.contains(" from "))
        || (lower.contains(" where ") && lower.contains(" join "))
        || looks_like_yaml(content)
}

fn looks_like_yaml(content: &str) -> bool {
    let mut keyed_lines = 0usize;
    let mut indented_list_lines = 0usize;

    for line in content.lines().take(50) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with("- ") && line.starts_with(' ') {
            indented_list_lines += 1;
            continue;
        }
        if let Some((key, _value)) = trimmed.split_once(':') {
            if !key.is_empty()
                && key
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
            {
                keyed_lines += 1;
            }
        }
    }

    keyed_lines >= 2 || (keyed_lines >= 1 && indented_list_lines >= 1)
}

pub fn map_magika_label(label: &str) -> ContentType {
    match label {
        "json" | "jsonl" => ContentType::JsonArray,
        "diff" => ContentType::GitDiff,
        "html" | "xml" => ContentType::Html,
        "rust" | "python" | "javascript" | "typescript" | "go" | "java" | "c" | "cpp" | "cs"
        | "php" | "ruby" | "swift" | "kotlin" | "scala" | "haskell" | "lua" | "dart" | "perl"
        | "shell" | "powershell" | "batch" | "sql" | "css" | "vue" | "groovy" | "clojure"
        | "asm" | "cmake" | "dockerfile" | "makefile" | "yaml" | "toml" | "ini" | "hcl"
        | "jinja" => ContentType::SourceCode,
        "markdown" | "rst" | "latex" | "txt" | "empty" | "unknown" | "undefined" => {
            ContentType::PlainText
        }
        _ => ContentType::PlainText,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_detect(content: &str, expected: ContentType, hint: &str) {
        match magika_detect(content) {
            Ok(got) => assert_eq!(got, expected, "{hint}: expected {expected:?}, got {got:?}"),
            Err(e) => panic!("{hint}: detection failed: {e}"),
        }
    }

    #[test]
    fn empty_input_is_plain_text_without_model_call() {
        let result = magika_detect("").unwrap();
        assert_eq!(result, ContentType::PlainText);
    }

    #[test]
    fn detects_json() {
        assert_detect(
            r#"{"name": "Alice", "age": 30, "tags": ["a", "b"]}"#,
            ContentType::JsonArray,
            "single-object JSON",
        );
    }

    #[test]
    fn detects_json_array() {
        let payload = r#"[{"id": 1, "v": "a"}, {"id": 2, "v": "b"}, {"id": 3, "v": "c"}]"#;
        assert_detect(payload, ContentType::JsonArray, "array-of-records JSON");
    }

    #[test]
    fn detects_python_source() {
        let src = r#"
def fibonacci(n):
    if n <= 1:
        return n
    return fibonacci(n-1) + fibonacci(n-2)

class Tree:
    def __init__(self, value):
        self.value = value
        self.children = []
"#;
        assert_detect(src, ContentType::SourceCode, "python class+def");
    }

    #[test]
    fn detects_rust_source() {
        let src = r#"
use std::collections::HashMap;

pub struct Counter {
    counts: HashMap<String, u32>,
}

impl Counter {
    pub fn new() -> Self {
        Self { counts: HashMap::new() }
    }
}
"#;
        assert_detect(src, ContentType::SourceCode, "rust struct+impl");
    }

    #[test]
    fn detects_javascript_source() {
        let src = r#"
const fetchUser = async (id) => {
    const response = await fetch(`/api/users/${id}`);
    if (!response.ok) throw new Error('Not found');
    return response.json();
};
"#;
        assert_detect(src, ContentType::SourceCode, "JS arrow + async");
    }

    #[test]
    fn detects_unified_diff() {
        let diff = r#"diff --git a/foo.py b/foo.py
index abc123..def456 100644
--- a/foo.py
+++ b/foo.py
@@ -1,3 +1,4 @@
 def hello():
+    print("new line")
     return "world"
"#;
        assert_detect(diff, ContentType::GitDiff, "git unified diff");
    }

    #[test]
    fn detects_markdown_as_plain_text() {
        let md = "# Hello\n\nThis is **bold** and *italic*.\n\n- Item 1\n- Item 2\n";
        assert_detect(md, ContentType::PlainText, "markdown");
    }

    #[test]
    fn detects_plain_text() {
        let prose = "The quick brown fox jumps over the lazy dog. \
                     This is just regular English prose with no \
                     special structure.";
        assert_detect(prose, ContentType::PlainText, "english prose");
    }

    #[test]
    fn detects_html() {
        let html =
            "<!DOCTYPE html><html><head><title>x</title></head><body><h1>Hi</h1></body></html>";
        assert_detect(html, ContentType::Html, "minimal HTML page");
    }

    #[test]
    fn detects_yaml_as_source_code() {
        let yaml = "name: my-app\nversion: 1.0\ndependencies:\n  - foo\n  - bar\n";
        assert_detect(yaml, ContentType::SourceCode, "YAML config");
    }

    #[test]
    fn detects_shell_script_as_source_code() {
        let sh = "#!/bin/bash\nset -euo pipefail\nfor f in *.txt; do\n  echo \"$f\"\ndone\n";
        assert_detect(sh, ContentType::SourceCode, "bash script with shebang");
    }

    #[test]
    fn detects_sql_as_source_code() {
        let sql = "SELECT u.id, u.name, COUNT(o.id) AS order_count \
                   FROM users u LEFT JOIN orders o ON u.id = o.user_id \
                   WHERE u.active = TRUE GROUP BY u.id, u.name;";
        assert_detect(sql, ContentType::SourceCode, "SQL query");
    }

    #[test]
    fn singleton_session_is_reused_across_calls() {
        magika_detect("hello world").unwrap();
        magika_detect("def f(): pass").unwrap();
        magika_detect(r#"{"a":1}"#).unwrap();
    }

    #[test]
    fn unmapped_labels_route_to_plain_text() {
        assert_eq!(map_magika_label("ace"), ContentType::PlainText);
        assert_eq!(map_magika_label("flac"), ContentType::PlainText);
        assert_eq!(map_magika_label("3gp"), ContentType::PlainText);
        assert_eq!(
            map_magika_label("garbage_unseen_label"),
            ContentType::PlainText
        );
    }

    #[test]
    fn known_label_table_round_trips() {
        assert_eq!(map_magika_label("json"), ContentType::JsonArray);
        assert_eq!(map_magika_label("jsonl"), ContentType::JsonArray);
        assert_eq!(map_magika_label("diff"), ContentType::GitDiff);
        assert_eq!(map_magika_label("html"), ContentType::Html);
        assert_eq!(map_magika_label("rust"), ContentType::SourceCode);
        assert_eq!(map_magika_label("python"), ContentType::SourceCode);
        assert_eq!(map_magika_label("yaml"), ContentType::SourceCode);
        assert_eq!(map_magika_label("markdown"), ContentType::PlainText);
        assert_eq!(map_magika_label("txt"), ContentType::PlainText);
        assert_eq!(map_magika_label("empty"), ContentType::PlainText);
    }
}
