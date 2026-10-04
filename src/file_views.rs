//! Text-only filesystem views. Every displayed name and match is a source slice.
use std::sync::LazyLock;

enum Entry<'a> {
    Path(&'a str),
    Hit { number: &'a str, text: &'a str },
}
struct Record<'a> {
    group: &'a str,
    entry: Entry<'a>,
}

// Stable sorting of a flat record stream preserves occurrence order inside
// each group. Both filesystem presentations share this layout, not a parser.
fn grouped_page(mut records: Vec<Record<'_>>, search: bool) -> String {
    records.sort_by_key(|record| record.group);
    let groups: Vec<_> = records.chunk_by(|a, b| a.group == b.group).collect();
    let mut fragments = vec![if search {
        format!("{} matches in {}F:\n\n", records.len(), groups.len())
    } else {
        format!("{} files in {} dirs:\n\n", records.len(), groups.len())
    }];
    let limit = if search { groups.len() } else { 20 };
    for group in &groups[..groups.len().min(limit)] {
        let name = group[0].group;
        fragments.push(if search {
            format!("[file] {name} ({}):\n", group.len())
        } else {
            format!("{name}/  ({})\n", group.len())
        });
        let (page, remaining) = group.split_at(group.len().min(10));
        fragments.extend(page.iter().map(|record| match record.entry {
            Entry::Path(name) => format!("  {name}\n"),
            Entry::Hit { number, text } => format!("  {number:>4}: {}\n", text.trim()),
        }));
        match (remaining.len(), search) {
            (0, false) => {}
            (0, true) => fragments.push("\n".into()),
            (n, false) => fragments.push(format!("  +{n}\n")),
            (n, true) => fragments.push(format!("  +{n}\n\n")),
        }
    }
    if groups.len() > limit {
        fragments.push(format!("\n+{} more dirs\n", groups.len() - limit));
    }
    fragments.concat()
}

pub fn search(raw: &str) -> String {
    static MATCH: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^(.+?):([0-9]+):(.*)$").unwrap());
    let parsed: Option<Vec<Record<'_>>> = raw
        .lines()
        .filter(|s| {
            !s.is_empty()
                && *s != "[stderr]"
                && !(s.starts_with("grep: ") && s.ends_with(": binary file matches"))
        })
        .map(|line| {
            let captures = MATCH.captures(line)?;
            let (path, number, text) = (
                captures.get(1)?.as_str(),
                captures.get(2)?.as_str(),
                captures.get(3)?.as_str(),
            );
            if MATCH.is_match(text) || path.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            Some(Record {
                group: path,
                entry: Entry::Hit { number, text },
            })
        })
        .collect();
    match parsed {
        Some(records) if !records.is_empty() => grouped_page(records, true),
        _ => raw.into(),
    }
}

pub fn paths(raw: &str) -> String {
    let parsed: Option<Vec<Record<'_>>> = raw
        .lines()
        .filter(|s| !s.is_empty())
        .map(|path| {
            if path.contains('\0') || path.starts_with("find:") || path == "[stderr]" {
                return None;
            }
            let (group, name) = path.rsplit_once('/').unwrap_or((".", path));
            Some(Record {
                group,
                entry: Entry::Path(name),
            })
        })
        .collect();
    match parsed {
        Some(records) if !records.is_empty() => grouped_page(records, false),
        _ => raw.into(),
    }
}

pub fn tree(raw: &str) -> String {
    static SUMMARY: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^\d+ director(?:y|ies), \d+ files?$").unwrap());
    let mut rows: Vec<_> = raw.lines().collect();
    while rows.last().is_some_and(|s| s.is_empty()) {
        rows.pop();
    }
    if rows.last().is_some_and(|s| SUMMARY.is_match(s)) {
        rows.pop();
    }
    while rows.last().is_some_and(|s| s.is_empty()) {
        rows.pop();
    }
    if rows.is_empty() {
        raw.to_owned()
    } else {
        rows.join("\n") + "\n"
    }
}

pub fn listing(command: &[String], raw: &str) -> String {
    static ROW: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"^([dl-][rwxStTs-]{9})[+@.]?\s+\d+\s+.+?\s+(\d+)\s+(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\s+\d{1,2}\s+(?:\d{2}:\d{2}|\d{4})\s(.*)$").unwrap()
    });
    let detailed = command
        .iter()
        .skip(1)
        .any(|s| s.starts_with('-') && !s.starts_with("--") && s.contains(['l', 'n', 'g', 'o']));
    let all = command.iter().skip(1).any(|s| {
        s == "--all"
            || s == "--almost-all"
            || (s.starts_with('-') && !s.starts_with("--") && s.contains(['a', 'A']))
    });
    let mut hidden = 0;
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for line in raw.lines() {
        if line.starts_with("total ") || line.is_empty() {
            continue;
        }
        let Some(caps) = ROW.captures(line) else {
            return raw.to_owned();
        };
        let mode = caps[1].as_bytes();
        let name = &caps[3];
        if name == "." || name == ".." {
            continue;
        }
        let noise = ".DS_Store .cache .eggs .git .idea .mypy_cache .next .nyc_output .pytest_cache .tox .turbo .venv .vercel .vs .vscode *.egg-info Thumbs.db __pycache__ build coverage dist env node_modules target venv";
        if !all && noise.split_whitespace().any(|entry| entry == name) {
            hidden += 1;
            continue;
        }
        // Extended permissions cannot be represented faithfully in three digits.
        if mode.iter().any(|c| matches!(c, b's' | b'S' | b't' | b'T')) {
            return raw.to_owned();
        }
        let mut prefix = String::new();
        if detailed {
            for chunk in mode[1..].chunks(3) {
                let digit = chunk
                    .iter()
                    .zip([4, 2, 1])
                    .filter(|(c, _)| **c != b'-')
                    .map(|(_, n)| n)
                    .sum::<u8>();
                prefix.push(char::from(b'0' + digit));
            }
            prefix.push_str("  ");
        }
        if mode[0] == b'd' {
            dirs.push(format!("{prefix}{name}/\n"));
        } else {
            let Ok(size) = caps[2].parse::<u64>() else {
                return raw.to_owned();
            };
            let unit = (size.checked_ilog2().unwrap_or(0) / 10).min(3);
            let shown = match unit {
                0 => format!("{size}B"),
                _ => format!(
                    "{:.1}{}",
                    size as f64 / (1u64 << (10 * unit)) as f64,
                    ["B", "K", "M", "G"][unit as usize]
                ),
            };
            files.push(format!("{prefix}{name}  {shown}\n"));
        }
    }
    if dirs.is_empty() && files.is_empty() && hidden == 0 {
        return raw.to_owned();
    }
    dirs.extend(files);
    let extra = dirs.len().saturating_sub(50);
    dirs.truncate(50);
    let mut output = if dirs.is_empty() {
        "(empty)\n".to_string()
    } else {
        dirs.concat()
    };
    match (extra, hidden) {
        (0, 0) => {}
        (0, n) => output.push_str(&format!("... ({n} filtered)\n")),
        (n, 0) => output.push_str(&format!("... ({n} more)\n")),
        (n, h) => output.push_str(&format!("... ({n} more, {h} filtered)\n")),
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_keeps_the_literal_line_number() {
        let raw = (179..199)
            .map(|n| format!("source.rs:{n}:error_{n}()\n"))
            .collect::<String>();
        let view = search(&raw);
        assert!(view.starts_with("20 matches in 1F:\n"));
        assert!(view.contains("179: error_179()"));
        assert!(view.contains("+10"));
    }
    #[test]
    fn search_retains_diagnostics_context_and_ambiguous_colons() {
        for raw in [
            "a:179:ok\ngrep: forbidden\n",
            "a:179:some:path:99:ambiguous\n",
            "a-178-before\na:179:match\n",
        ] {
            assert_eq!(search(raw), raw);
        }
    }
    #[test]
    fn binary_notices_do_not_prevent_grouping_text_matches() {
        let raw = "a.rs:179:match\n\n[stderr]\ngrep: a.bin: binary file matches\n";
        assert_eq!(
            search(raw),
            "1 matches in 1F:\n\n[file] a.rs (1):\n   179: match\n\n"
        );
        let error = "a.rs:179:match\n\n[stderr]\ngrep: secret: Permission denied\n";
        assert_eq!(search(error), error);
    }
    #[test]
    fn names_that_look_like_dates_are_not_reparsed() {
        let raw = "-rw-r--r-- 1 user user 12030 Oct  3 08:45 Nov 10 2025 report.txt\n";
        assert_eq!(
            listing(&["ls".into(), "-ln".into()], raw),
            "644  Nov 10 2025 report.txt  11.7K\n"
        );
    }
    #[test]
    fn tree_removes_only_the_final_count() {
        let raw = "src\n└── directories and files.txt\n\n1 directory, 1 file\n";
        assert_eq!(tree(raw), "src\n└── directories and files.txt\n");
    }
    #[test]
    fn listing_bounds_rows_and_respects_all() {
        let raw = "drwxr-xr-x 1 user user 4096 Oct  3 08:45 node_modules\n".to_string()
            + &(0..75)
                .map(|n| format!("-rw-r--r-- 1 user user 42 Oct  3 08:45 file_{n}\n"))
                .collect::<String>();
        let view = listing(&["ls".into(), "-l".into()], &raw);
        assert!(view.contains("... (25 more, 1 filtered)"));
        assert!(!view.contains("node_modules"));
        let all = listing(&["ls".into(), "-la".into()], &raw);
        assert!(all.contains("node_modules/"));
        assert!(all.contains("... (26 more)"));
    }
    #[test]
    fn recursive_ls_keeps_section_paths_and_errors() {
        let raw = ".:\nfile\n\n./sub:\nfile\nls: forbidden\n";
        assert_eq!(listing(&["ls".into(), "-R".into()], raw), raw);
    }
}
