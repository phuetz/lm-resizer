//! Command-aware views with no line budgets. Prefix factoring is reversible;
//! test filters remove only explicitly recognized successful result/progress
//! rows outside diagnostic blocks. Unknown output is retained verbatim.

pub fn filter(command: &[String], raw: &str) -> Option<(&'static str, String)> {
    let program = std::path::Path::new(command.first()?)
        .file_stem()?
        .to_str()?;
    let sub = command.get(1).map(String::as_str).unwrap_or("");
    let (name, text) = match (program, sub) {
        // Search routing awaits the independently bisected regression fix.
        ("grep" | "rg", _) => return None,
        ("find" | "fd", _) => ("paths", raw.to_string()),
        ("ls", _) => ("listing", raw.to_string()),
        ("cat" | "head" | "tail", _) => ("file", raw.to_string()),
        ("git", "log" | "diff" | "show" | "status") => ("git", raw.to_string()),
        ("cargo", "test") => ("cargo", test_view(raw, "cargo")),
        ("pytest", _) => ("pytest", test_view(raw, "pytest")),
        ("npm" | "pnpm" | "yarn", "test") | ("vitest" | "jest", _) => {
            ("npm-test", test_view(raw, "npm"))
        }
        _ => return None,
    };
    let compact = factor_lines(&text);
    // Count the view plus its recovery marker, not bytes/4. A reversible
    // representation can still cost more tokens than the original text.
    let measured = crate::token_metrics::TokenCounts::measure(raw, &compact);
    let compact = if measured.tokens_saved > 20 {
        compact
    } else {
        raw.to_string()
    };
    Some((name, compact))
}

fn test_view(raw: &str, runner: &str) -> String {
    let mut output = String::new();
    let mut diagnostics = false;
    for line in raw.split_inclusive('\n') {
        let row = line.trim();
        // Once diagnostics begin, keep the entire remaining transcript. No
        // keyword-based traceback selection, length limits or deduplication.
        if row == "failures:"
            || row.starts_with("---- ")
            || row.contains(" FAILURES ")
            || row.contains(" ERRORS ")
            || row.starts_with("warning:")
            || row.starts_with("error:")
            || row.starts_with("FAIL ")
            || row.starts_with("ERROR ")
            || row.starts_with("FAILED ")
        {
            diagnostics = true;
        }
        let success = !diagnostics
            && match runner {
                "cargo" => row.starts_with("test ") && row.ends_with(" ... ok"),
                "pytest" => {
                    // Pytest -q dots, possibly followed by a percentage. Never
                    // consume source/stack rows inside a failure block.
                    let dots = row.split_once('[').map_or(row, |(dots, _)| dots.trim());
                    let percentage = row.split_once('[').is_none_or(|(_, percent)| {
                        percent.strip_suffix("%]").is_some_and(|n| {
                            !n.trim().is_empty() && n.trim().bytes().all(|b| b.is_ascii_digit())
                        })
                    });
                    !dots.is_empty()
                        && dots
                            .bytes()
                            .all(|b| matches!(b, b'.' | b's' | b'F' | b'x' | b'X' | b'E'))
                        && percentage
                }
                // npm can invoke arbitrary scripts: retain all unknown output.
                _ => false,
            };
        if !success {
            output.push_str(line);
        }
    }
    // Without a result counter, even "test ... ok" may be application text.
    let has_summary = match runner {
        "cargo" => raw.lines().any(|l| l.starts_with("test result:")),
        "pytest" => raw
            .lines()
            .any(|l| l.contains(" passed") || l.contains(" failed") || l.contains(" errors")),
        _ => false,
    };
    if has_summary {
        output
    } else {
        raw.to_string()
    }
}

/// LMR-LINES/2: @JSON-prefix, literal suffix rows (escape @, &, =, ! and backslash with backslash), =N additional occurrences of
/// the preceding decoded row, &N an earlier decoded row (zero-based),
/// !1/!0 final newline flag. Line order is exact.
/// Only consecutive lines share prefixes; no hashing, truncation or sorting.
pub fn factor_lines(raw: &str) -> String {
    if raw.is_empty() || raw.starts_with("LMR-LINES/2\n") {
        return raw.to_string(); // Avoid treating an application's codec text as ours.
    }
    let lines: Vec<&str> = raw.split_terminator('\n').collect();
    let mut output = String::from("LMR-LINES/2\n");
    let mut current_prefix = "";
    let mut seen = std::collections::HashMap::<&str, usize>::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.len() >= 40 {
            if let Some(index) = seen.get(line) {
                output.push_str(&format!("&{index}\n"));
                i += 1;
                continue;
            }
        }
        let mut end = i + 1;
        while end < lines.len() && lines[end] == line {
            end += 1;
        }
        if end > i + 1 {
            if !current_prefix.is_empty() {
                output.push_str("@\"\"\n");
                current_prefix = "";
            }
            seen.entry(line).or_insert(i);
            write_literal_row(&mut output, line);
            output.push_str(&format!("={}\n", end - i - 1));
            i = end;
            continue;
        }
        let prefix = if i + 1 < lines.len() {
            common_prefix(line, lines[i + 1])
        } else {
            ""
        };
        let mut end = i + 1;
        if prefix.len() >= 8 {
            while end < lines.len() && lines[end].starts_with(prefix) {
                end += 1;
            }
        }
        // Account for prefix record + quotes. Tiny prefix groups cost more.
        let prefix = if prefix.len() * (end - i - 1) > prefix.len() + 12 {
            prefix
        } else {
            end = i + 1;
            ""
        };
        if prefix != current_prefix {
            output.push('@');
            write_json_row(&mut output, prefix);
            current_prefix = prefix;
        }
        for (offset, line) in lines[i..end].iter().enumerate() {
            seen.entry(line).or_insert(i + offset);
            write_literal_row(&mut output, &line[prefix.len()..]);
        }
        i = end;
    }
    output.push_str(if raw.ends_with('\n') { "!1\n" } else { "!0\n" });
    if output.len() < raw.len() {
        output
    } else {
        raw.to_string()
    }
}

fn common_prefix<'a>(a: &'a str, b: &str) -> &'a str {
    let mut size = a.bytes().zip(b.bytes()).take_while(|(a, b)| a == b).count();
    while !a.is_char_boundary(size) {
        size -= 1;
    }
    &a[..size]
}

fn write_literal_row(output: &mut String, row: &str) {
    if row.starts_with(['@', '&', '=', '!', '\\']) {
        output.push('\\');
    }
    output.push_str(row);
    output.push('\n');
}

fn write_json_row(output: &mut String, row: &str) {
    output.push_str(&serde_json::to_string(row).expect("string JSON"));
    output.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(view: &str) -> String {
        if !view.starts_with("LMR-LINES/2\n") {
            return view.to_string();
        }
        let mut prefix = String::new();
        let mut rows = Vec::<String>::new();
        for row in view.split_terminator('\n').skip(1) {
            if let Some(value) = row.strip_prefix('@') {
                prefix = serde_json::from_str(value).unwrap();
            } else if let Some(value) = row.strip_prefix('=') {
                for _ in 0..value.parse::<usize>().unwrap() {
                    rows.push(rows.last().unwrap().clone());
                }
            } else if let Some(index) = row.strip_prefix('&') {
                rows.push(rows[index.parse::<usize>().unwrap()].clone());
            } else if row.starts_with('!') {
                return rows.join("\n") + if row == "!1" { "\n" } else { "" };
            } else {
                let suffix = row.strip_prefix('\\').unwrap_or(row);
                rows.push(prefix.clone() + suffix);
            }
        }
        panic!("missing end record");
    }

    #[test]
    fn exact_roundtrip_with_controls_unicode_duplicates_and_final_newline() {
        for ending in ["", "\n"] {
            let raw = format!(
                "{}\n\n\n@\"!1\"\r\n{}{}",
                "répertoire/long/avec/des/espaces/nom\nrépertoire/long/avec/des/espaces/autre\n"
                    .repeat(30),
                "long duplicate row\n".repeat(50),
                ending
            );
            assert_eq!(decode(&factor_lines(&raw)), raw);
        }
        for raw in ["", "\n", "\r\n", "=2", "LMR-LINES/2\n@\"x\"\n"] {
            assert_eq!(factor_lines(raw), raw);
        }
    }

    #[test]
    fn reversible_rows_preserve_references_and_non_newline_controls() {
        let atoms = [
            "@prefix",
            "&123",
            "=4",
            "!0",
            "\\escape",
            "a\rb",
            "a\tb",
            "é漢字",
            "",
            "with \"quotes\"",
            "space at end ",
        ];
        for offset in 0..atoms.len() {
            let mut raw = String::new();
            for index in 0..300 {
                raw.push_str("directory/with/a/long/common/prefix/");
                raw.push_str(atoms[(index + offset) % atoms.len()]);
                raw.push('\n');
            }
            raw.push_str(atoms[offset]);
            let view = factor_lines(&raw);
            assert!(view.len() < raw.len());
            assert_eq!(decode(&view), raw);
        }
    }

    #[test]
    fn long_diagnostics_and_unknown_npm_output_are_never_cut() {
        let diagnostic = format!(
            "failures:\n---- fails stdout ----\n{}test result: FAILED. 1 failed\n",
            "stack with values and locations\n".repeat(350)
        );
        let raw = format!("test passes ... ok\n{diagnostic}");
        assert_eq!(decode(&factor_lines(&test_view(&raw, "cargo"))), diagnostic);
        let npm = "sh: 1: hereby: not found\n";
        assert_eq!(filter(&["npm".into(), "test".into()], npm).unwrap().1, npm);
        let pytest = format!(
            "... [ 50%]\n=== ERRORS ===\n{}350 errors in 1.0s\n",
            ".\n".repeat(350)
        );
        let out = decode(&factor_lines(&test_view(&pytest, "pytest")));
        assert_eq!(out, pytest.strip_prefix("... [ 50%]\n").unwrap());
    }
}
