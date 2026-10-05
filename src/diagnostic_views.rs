//! Structured compiler diagnostics. Locations are copied, never shortened.
use regex::Regex;
use std::collections::BTreeMap;
use std::sync::LazyLock;

pub fn typescript(raw: &str) -> String {
    static DIAG: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?:(.+)\(([0-9]+),[0-9]+\): )?error (TS[0-9]+): (.*)$").unwrap()
    });
    struct Diagnostic<'a> {
        line: &'a str,
        code: &'a str,
        message: &'a str,
        context: Vec<&'a str>,
    }
    let mut groups = BTreeMap::<&str, Vec<Diagnostic<'_>>>::new();
    let mut codes = BTreeMap::<&str, usize>::new();
    let mut last = None;
    for row in raw.lines() {
        if let Some(c) = DIAG.captures(row) {
            let file = c.get(1).map_or("", |m| m.as_str());
            let code = c.get(3).unwrap().as_str();
            groups.entry(file).or_default().push(Diagnostic {
                line: c.get(2).map_or("", |m| m.as_str()),
                code,
                message: c.get(4).unwrap().as_str(),
                context: Vec::new(),
            });
            *codes.entry(code).or_default() += 1;
            last = Some(file);
        } else if row.starts_with("  ") || row.starts_with('\t') {
            let Some(file) = last else {
                return raw.into();
            };
            groups
                .get_mut(file)
                .unwrap()
                .last_mut()
                .unwrap()
                .context
                .push(row.trim());
        } else if !(row.trim().is_empty() || row.starts_with("Found ") && row.contains(" errors")) {
            return raw.into();
        }
    }
    if groups.is_empty() {
        return raw.into();
    }
    let count: usize = codes.values().sum();
    let files = groups.keys().filter(|k| !k.is_empty()).count();
    let mut out = if files > 0 {
        format!("TypeScript: {count} errors in {files} files\n")
    } else {
        format!("TypeScript: {count} errors\n")
    };
    let mut codes: Vec<_> = codes.into_iter().collect();
    codes.sort_by_key(|(code, n)| (std::cmp::Reverse(*n), *code));
    if codes.len() > 1 {
        out.push_str(&format!(
            "Top codes: {}\n\n",
            codes
                .iter()
                .take(5)
                .map(|(c, n)| format!("{c} ({n}x)"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let mut groups: Vec<_> = groups.into_iter().collect();
    groups.sort_by_key(|(file, rows)| {
        (
            if file.is_empty() { 0 } else { 1 },
            std::cmp::Reverse(rows.len()),
            *file,
        )
    });
    for (file, rows) in groups {
        out.push_str(&format!(
            "{} ({} errors)\n",
            if file.is_empty() { "global" } else { file },
            rows.len()
        ));
        for d in rows {
            let location = if d.line.is_empty() {
                String::new()
            } else {
                format!("L{}: ", d.line)
            };
            out.push_str(&format!("  {location}{} {}\n", d.code, d.message));
            for c in d.context {
                out.push_str(&format!("    {c}\n"));
            }
        }
        out.push('\n');
    }
    out.trim_end().into()
}
pub fn cargo_build(raw: &str, sub: &str) -> String {
    if raw.lines().any(|row| row.starts_with("error[E"))
        && raw.lines().any(|row| row.trim_start().starts_with("--> "))
        && raw
            .lines()
            .any(|row| row.starts_with("error: could not compile "))
    {
        // Keep the diagnostic itself verbatim. Only remove the compiler's
        // navigation boilerplate and empty gutters; these carry no source text.
        return raw
            .split_inclusive('\n')
            .filter(|row| {
                let text = row.trim();
                text != "|"
                    && !(text.starts_with(
                        "For more information about this error, try `rustc --explain ",
                    ) && text.ends_with("`."))
            })
            .collect();
    }
    let mut compiled = 0;
    let mut finished = None;
    for row in raw.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if row.starts_with("Compiling ") {
            compiled += 1;
        } else if row.starts_with("Finished ") {
            finished = Some(row);
        } else {
            return raw.into();
        }
    }
    match finished {
        Some(line) => format!("cargo {sub} ({compiled} crates compiled)\n{line}\n"),
        None => raw.into(),
    }
}

pub fn python_lint(raw: &str) -> String {
    // Structural whitespace reduction only for explicit file:line:column
    // diagnostics. All locations, rule codes and messages stay intact.
    static LINE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(.+:[0-9]+(?::[0-9]+)?:) +(.+)$").unwrap());
    let mut changed = false;
    let rows: Vec<_> = raw
        .lines()
        .map(|s| {
            if let Some(c) = LINE.captures(s) {
                let row = format!("{} {}", &c[1], &c[2]);
                changed |= row != s;
                row
            } else {
                s.into()
            }
        })
        .collect();
    if changed {
        rows.join("\n") + if raw.ends_with('\n') { "\n" } else { "" }
    } else {
        raw.into()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn locations_are_literal_and_unknown_lines_force_fallback() {
        let raw="folder/a.ts(179,7): error TS2304: Missing a.\nfolder/a.ts(181,7): error TS2304: Missing b.\n";
        let out = typescript(raw);
        assert!(out.contains("L179:"));
        assert!(out.contains("L181:"));
        assert!(out.contains("folder/a.ts"));
        let mixed = raw.to_owned() + "permission denied\n";
        assert_eq!(typescript(&mixed), mixed);
    }
    #[test]
    fn long_typescript_error_is_never_cut() {
        let message = format!("Cannot find name '{}'.", "invoice_identifier".repeat(12));
        let raw = format!("numbers.ts(181,20): error TS2304: {message}\n");
        assert!(typescript(&raw).contains(&message));
    }
    #[test]
    fn builds_never_hide_warnings_or_compile_failures() {
        for line in [
            "warning: unused variable",
            "error[E001]: invalid",
            "unknown message",
        ] {
            let raw = format!("Compiling crate\n{line}\nFinished dev\n");
            assert_eq!(cargo_build(&raw, "build"), raw);
        }
    }
    #[test]
    fn compile_diagnostics_keep_source_labels_and_failure_count() {
        let raw = include_str!("../bench/corpus/compile_error.txt");
        let compact = cargo_build(raw, "build");
        assert!(compact.len() < raw.len());
        for row in raw.lines().filter(|row| {
            !row.trim().is_empty() && row.trim() != "|" && !row.starts_with("For more information")
        }) {
            assert!(compact.lines().any(|kept| kept == row), "{row}");
        }
        let warning = format!("{raw}warning: check another crate\n");
        assert!(cargo_build(&warning, "build").ends_with("warning: check another crate\n"));
    }
    #[test]
    fn python_lint_retains_failure_context() {
        let raw = "src/a.py:179:7:    F401 unused import\n  import os\n  ^^^^^^^\nFound 1 error.\n";
        assert_eq!(
            python_lint(raw),
            "src/a.py:179:7: F401 unused import\n  import os\n  ^^^^^^^\nFound 1 error.\n"
        );
    }
}
