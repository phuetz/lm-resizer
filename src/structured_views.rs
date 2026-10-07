//! Additional structured views for producers outside native's command registry.
//! Independent Rust implementation: typed JSON cells and exact run lengths;
//! no semantic similarity model, row sampling or fabricated error counts.
use serde_json::{json, Value};

pub fn compress(raw: &str) -> Option<(&'static str, String)> {
    let candidate = json_table(raw)
        .map(|s| ("json-table", s))
        .or_else(|| json_compact(raw).map(|s| ("json-compact", s)))
        .or_else(|| log_runs(raw).map(|s| ("log-runs", s)))
        .or_else(|| diff_metadata(raw).map(|s| ("diff-metadata", s)))
        .or_else(|| crate::reversible_views::fold(raw, true).map(|s| ("match-fold", s)))
        .or_else(|| crate::reversible_views::fold(raw, false).map(|s| ("path-fold", s)))
        .or_else(|| rust_outline(raw).map(|s| ("code-outline:rust", s)))?;
    let counts = crate::token_metrics::TokenCounts::measure(raw, &candidate.1);
    (candidate.1.len() < raw.len() && counts.tokens_saved > 0).then_some(candidate)
}

fn json_table(raw: &str) -> Option<String> {
    let value: Value = serde_json::from_str(raw).ok()?;
    let items = value.as_array()?;
    if items.len() < 5 {
        return None;
    }
    let columns: Vec<_> = items.first()?.as_object()?.keys().cloned().collect();
    if columns.is_empty() {
        return None;
    }
    let rows: Option<Vec<Vec<&Value>>> = items
        .iter()
        .map(|item| {
            let object = item.as_object()?;
            if object.len() != columns.len() {
                return None;
            }
            columns.iter().map(|key| object.get(key)).collect()
        })
        .collect();
    let rows = rows?;
    let table = format!(
        "JSON table (all {} rows; cells follow columns):\n{}\n",
        items.len(),
        json!({"columns": columns, "rows": rows})
    );
    // CSV with a typed schema, independently
    // implemented here. Explicit all-string schema prevents coercion of
    // identifiers, null-like strings and padded numbers. Mixed cells retain
    // the typed JSON table. RFC 4180 quoting preserves commas/quotes/newlines.
    if rows.iter().flatten().all(|cell| cell.is_string()) {
        let mut csv = format!(
            "CSV strings[{}]\n{}\n",
            rows.len(),
            columns
                .iter()
                .map(|s| csv_cell(s))
                .collect::<Vec<_>>()
                .join(",")
        );
        for row in &rows {
            csv.push_str(
                &row.iter()
                    .map(|v| csv_cell(v.as_str().unwrap()))
                    .collect::<Vec<_>>()
                    .join(","),
            );
            csv.push('\n');
        }
        if crate::token_metrics::TokenCounts::measure(&table, &csv).tokens_saved > 0 {
            return Some(csv);
        }
    }
    Some(table)
}

fn csv_cell(cell: &str) -> String {
    if cell.contains([',', '"', '\r', '\n']) || cell.is_empty() {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.to_owned()
    }
}

// Lossless document compaction, restricted to whitespace removal.
// Keep original string escapes, numeric lexemes and even duplicate object keys;
// parsing validates the grammar, but serialization never reconstructs facts.
fn json_compact(raw: &str) -> Option<String> {
    let _: Value = serde_json::from_str(raw).ok()?;
    let mut output = String::with_capacity(raw.len());
    let (mut quoted, mut escaped) = (false, false);
    for ch in raw.chars() {
        if quoted {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
        } else if ch == '"' {
            quoted = true;
            output.push(ch);
        } else if !matches!(ch, ' ' | '\t' | '\r' | '\n') {
            output.push(ch);
        }
    }
    (output.len() < raw.len()).then_some(output)
}

fn log_runs(raw: &str) -> Option<String> {
    let (prefix, body) = raw
        .strip_prefix("[stderr]\n")
        .map_or(("", raw), |body| ("[stderr]\n", body));
    let lines: Vec<_> = body.lines().collect();
    if !body.ends_with('\n')
        || lines.len() < 20
        || !lines.iter().all(|line| {
            [
                "INFO ",
                "DEBUG ",
                "WARN ",
                "WARNING ",
                "ERROR ",
                "CRITICAL ",
            ]
            .iter()
            .any(|prefix| line.starts_with(prefix))
        })
    {
        return None;
    }
    let mut out = prefix.to_string();
    let mut i = 0;
    while i < lines.len() {
        let end = (i + 1..lines.len())
            .find(|&j| lines[j] != lines[i])
            .unwrap_or(lines.len());
        out.push_str(lines[i]);
        out.push('\n');
        if end - i > 1 {
            out.push_str(&format!("[repeat {} more]\n", end - i - 1));
        }
        i = end;
    }
    (crate::reversible_views::expand_runs(&out).ok()? == raw).then_some(out)
}

// AST outline strategy, implemented with Rust's native syn parser instead
// of invoking Code Explorer's external tree-sitter index. Only measured Rust
// inputs are enabled. Signatures are original slices, never reconstructed.
pub(crate) fn rust_outline(raw: &str) -> Option<String> {
    use syn::visit::Visit;
    if raw.lines().count() < 50 {
        return None;
    }
    struct Bodies(Vec<(usize, usize, usize)>);
    impl Bodies {
        fn block(&mut self, block: &syn::Block) {
            let open = block.brace_token.span.open();
            let close = block.brace_token.span.close();
            if close.start().line.saturating_sub(open.end().line) > 3 {
                self.0.push((
                    open.byte_range().end,
                    close.byte_range().start,
                    close.start().line - open.end().line,
                ));
            }
        }
    }
    impl<'ast> Visit<'ast> for Bodies {
        fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
            self.block(&item.block);
        }
        fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
            self.block(&item.block);
        }
        fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
            if let Some(block) = &item.default {
                self.block(block);
            }
        }
    }
    let file = syn::parse_file(raw).ok()?;
    let mut bodies = Bodies(Vec::new());
    bodies.visit_file(&file);
    bodies.0.sort_unstable();
    if bodies.0.is_empty() {
        return None;
    }
    let mut out = "Rust outline (function bodies omitted; signatures unchanged):\n".to_string();
    let mut offset = 0;
    for (start, end, _) in bodies.0 {
        // Reject unexpected or overlapping source spans, rather than guessing.
        if start < offset
            || start > end
            || end > raw.len()
            || !raw.is_char_boundary(start)
            || !raw.is_char_boundary(end)
        {
            return None;
        }
        out.push_str(&raw[offset..start]);
        out.push_str(" /* body omitted */ ");
        offset = end;
    }
    out.push_str(&raw[offset..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn table_preserves_typed_cells_and_key_value_associations() {
        let rows: Vec<_> = (0..20)
            .map(|i| {
                json!({"path":"a,\"b\\c", "line":i,
            "failed":i==11,"value":null,"nested":{"name":"error"}})
            })
            .collect();
        let raw = serde_json::to_string_pretty(&rows).unwrap();
        let (_, view) = compress(&raw).unwrap();
        let table: Value = serde_json::from_str(view.split_once('\n').unwrap().1).unwrap();
        for (expected, cells) in rows.iter().zip(table["rows"].as_array().unwrap()) {
            for (key, value) in table["columns"]
                .as_array()
                .unwrap()
                .iter()
                .zip(cells.as_array().unwrap())
            {
                assert_eq!(&expected[key.as_str().unwrap()], value);
            }
        }
        assert_eq!(table["rows"].as_array().unwrap().len(), rows.len());
    }
    #[test]
    fn rust_ast_keeps_unicode_signatures_and_ignores_braces_in_strings() {
        let mut raw =
            "// Préface é\nconst KEEP: &str = \"{ fact }\";\npub fn café(x: &str) -> usize {\n"
                .to_string();
        raw.push_str(&"    let s = r#\"} {\"#;\n".repeat(60));
        raw.push_str("    x.len()\n}\n");
        let view = rust_outline(&raw).unwrap();
        assert!(view.contains("const KEEP: &str = \"{ fact }\";"));
        assert!(view.contains("pub fn café(x: &str) -> usize { /* body omitted */ }"));
        assert!(!view.contains("x.len()"));
        assert!(rust_outline(&(raw + "invalid syntax !")).is_none());
    }

    #[test]
    fn heterogeneous_json_is_not_tabularized() {
        assert!(json_table(r#"[{"a":1},{"b":2},{"a":3},{"a":4},{"a":5}]"#).is_none());
    }
    #[test]
    fn log_runs_keep_error_multiplicity_and_order() {
        let raw =
            "INFO healthy\n".repeat(30) + &"ERROR request 42 failed\n".repeat(2) + "WARN slow\n";
        let (_, view) = compress(&raw).unwrap();
        assert_eq!(
            view,
            "INFO healthy\n[repeat 29 more]\nERROR request 42 failed\n[repeat 1 more]\nWARN slow\n"
        );
    }
    #[test]
    fn code_and_unknown_diagnostics_are_unchanged() {
        assert!(compress("fn foo() {}\n").is_none());
        assert!(compress(&("INFO healthy\n".repeat(30) + "  stack trace\n")).is_none());
    }
}

#[cfg(test)]
mod extra_tests {
    use super::*;
    #[test]
    fn json_whitespace_preserves_escapes_numbers_and_duplicate_keys() {
        let raw = r#" { "same": 1, "same": 2, "n": 1.2300e+42,
            "path": "C:\\file\\a b", "s": "é \" a\n", "nil": null } "#;
        let compact = json_compact(raw).unwrap();
        assert!(compact.contains(r#""same":1,"same":2,"n":1.2300e+42"#));
        assert!(compact.contains(r#""path":"C:\\file\\a b""#));
        assert_eq!(
            serde_json::from_str::<Value>(raw).unwrap(),
            serde_json::from_str::<Value>(&compact).unwrap()
        );
        assert!(json_compact("{bad}").is_none());
        assert!(json_compact("INFO service started").is_none());
    }
    #[test]
    fn csv_quoting_preserves_all_string_cells() {
        assert_eq!(csv_cell("001"), "001");
        assert_eq!(csv_cell("null"), "null");
        assert_eq!(csv_cell(""), "\"\"");
        assert_eq!(csv_cell("a,\"b\r\n"), "\"a,\"\"b\r\n\"");
        let rows: Vec<_> = (0..50)
            .map(|i| json!({"id":format!("{i:03}"),"state":"false"}))
            .collect();
        let raw = serde_json::to_string_pretty(&rows).unwrap();
        let (_, view) = compress(&raw).unwrap();
        assert!(view.starts_with("CSV strings[50]\nid,state\n000,false\n"));
    }
}

#[cfg(test)]
mod mixed_table_tests {
    use super::*;
    #[test]
    fn mixed_cells_reconstruct_including_null_quotes_and_nested_objects() {
        let items:Vec<Value>=(0..20).map(|n|json!({"id":n,"name":format!("invoice-{n}"),"status":"pending","note":if n==1 {"one, \"quoted\" note"}else{"fine"},"value":if n==0 {Value::Null}else{json!({"amount":n})}})).collect();
        let raw = serde_json::to_string_pretty(&items).unwrap();
        let view = json_table(&raw).unwrap();
        let table: Value = serde_json::from_str(view.split_once('\n').unwrap().1).unwrap();
        let columns = table["columns"].as_array().unwrap();
        let rebuilt: Vec<Value> = table["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                Value::Object(
                    columns
                        .iter()
                        .zip(row.as_array().unwrap())
                        .map(|(k, v)| (k.as_str().unwrap().into(), v.clone()))
                        .collect(),
                )
            })
            .collect();
        assert_eq!(rebuilt, items);
        assert!(crate::token_metrics::TokenCounts::measure(&raw, &view).tokens_saved > 0);
    }
}

// Only metadata rows before a hunk may disappear. The complete document is
// archived by the caller; this view is explicitly a summary, not an inline codec.
fn diff_metadata(raw: &str) -> Option<String> {
    if !raw.starts_with("diff --git ") {
        return None;
    }
    let index = regex::Regex::new(r"^index [0-9a-f]+\.\.[0-9a-f]+(?: [0-7]{6})?$").ok()?;
    let mut hunk = false;
    let mut omitted = 0;
    let mut out = String::from("Diff (blob IDs in tee):\n");
    for row in raw.split_inclusive('\n') {
        if row.starts_with("diff --git ") {
            hunk = false;
        }
        if row.starts_with("@@") {
            hunk = true;
        }
        if !hunk && index.is_match(row.trim_end_matches(['\r', '\n'])) {
            omitted += 1;
        } else {
            out.push_str(row);
        }
    }
    (omitted > 0).then_some(out)
}
#[cfg(test)]
mod diff_metadata_tests {
    use super::*;
    #[test]
    fn index_text_inside_a_hunk_is_not_metadata() {
        let raw="diff --git a/a b/a\nindex abc..def 100644\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-index abc..123\n+index def..456\n";
        assert_eq!(diff_metadata(raw), Some("Diff (blob IDs in tee):\ndiff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-index abc..123\n+index def..456\n".into()));
        let only_hunk = raw.replace("index abc..def 100644\n", "");
        assert_eq!(diff_metadata(&only_hunk), None);
    }
}
