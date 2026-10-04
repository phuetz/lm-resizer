//! Explicit callable outline: indexed scopes, original signature bytes and
//! syntax-validated Python body ranges. Missing/stale analysis returns the input.
use lm_resizer_core::ccr::CcrStore;
use lm_resizer_core::transforms::retention_advice::RetentionAdvice;
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

pub fn outline(path: &Path, raw: &str, store: &dyn CcrStore) -> String {
    if path.extension().is_some_and(|s| s == "rs") {
        return crate::structured_views::rust_outline(raw).unwrap_or_else(|| raw.into());
    }
    let Ok((advice, _)) = crate::advice_cli::advice_from_code_explorer(path) else {
        return raw.into();
    };
    if advice.applies_to(raw).is_none() {
        return raw.into();
    }
    if path.extension().is_some_and(|s| s == "py") {
        return python(raw, &advice).unwrap_or_else(|| raw.into());
    }
    lm_resizer_core::transforms::advice_structural::elide_bodies_with_advice(
        raw,
        &advice,
        "",
        Some(store),
    )
    .output
}

fn python(raw: &str, advice: &RetentionAdvice) -> Option<String> {
    // CPython validates the complete document; indexed tree-sitter ranges
    // authorize which callables may be outlined. No regular-expression parser.
    let script = r#"import ast,json,sys
try:
    tree=ast.parse(sys.stdin.read())
    spans=[]
    for n in ast.walk(tree):
        if isinstance(n,(ast.FunctionDef,ast.AsyncFunctionDef)):
            body=n.body
            if body and isinstance(body[0],ast.Expr) and isinstance(body[0].value,ast.Constant) and isinstance(body[0].value.value,str):
                body=body[1:]
            if body and body[0].lineno>n.lineno and n.end_lineno-body[0].lineno>=4:
                spans.append([n.lineno,body[0].lineno,n.end_lineno])
    print(json.dumps(spans))
except SyntaxError:
    sys.exit(1)
"#;
    let mut child = Command::new("python3")
        .args(["-I", "-c", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.take()?.write_all(raw.as_bytes()).ok()?;
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    let mut spans: Vec<[usize; 3]> = serde_json::from_slice(&output.stdout).ok()?;
    spans.retain(|[start, _, end]| {
        advice.ranges.iter().any(|r| {
            r.start_line <= *start
                && r.end_line == *end
                && matches!(r.kind.as_deref(), Some("Function" | "Method"))
        })
    });
    spans.sort_by_key(|s| (s[0], std::cmp::Reverse(s[2])));
    let lines: Vec<_> = raw.split_inclusive('\n').collect();
    let mut out = String::new();
    let mut position = 0;
    for [_, start, end] in spans {
        let start = start.checked_sub(1)?;
        if start < position || end > lines.len() {
            continue;
        }
        for line in &lines[position..start] {
            out.push_str(line);
        }
        let indent: String = lines[start]
            .chars()
            .take_while(|c| matches!(c, ' ' | '\t'))
            .collect();
        out.push_str(&format!("{indent}# body omitted\n{indent}pass\n"));
        position = end;
    }
    for line in &lines[position..] {
        out.push_str(line);
    }
    (crate::token_metrics::TokenCounts::measure(raw, &out).tokens_saved > 0).then_some(out)
}
