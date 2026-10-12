//! Readable command-aware views with no line budgets or implicit dictionaries.
//! Unknown output is retained verbatim. Test runners are recognized only by
//! `test_views::runner`; none of these routes reduces a test transcript.

pub fn filter(command: &[String], raw: &str) -> Option<(&'static str, String)> {
    // Inspect wrapper arguments without changing execution or environment.
    let command = unwrap_runner(command);
    let program = std::path::Path::new(command.first()?)
        .file_stem()?
        .to_str()?;
    let sub = command.get(1).map(String::as_str).unwrap_or("");
    let (name, text) = match (program, sub) {
        // bb85e73 mistook source matches for diagnostics; retain every match.
        ("grep" | "rg", _) => ("search", raw.to_string()),
        ("find" | "fd", _) => ("paths", raw.to_string()),
        ("ls" | "dir" | "tree", _) => ("listing", raw.to_string()),
        ("cat" | "head" | "tail" | "nl", _) => ("file", raw.to_string()),
        ("git", "log" | "diff" | "show" | "status") => ("git", raw.to_string()),
        ("cargo", "build" | "check" | "clippy" | "rustc") => ("cargo", raw.to_string()),
        ("docker" | "podman" | "docker-compose", _) => ("containers", raw.to_string()),
        ("tsc" | "eslint", _) => ("linter", raw.to_string()),
        // Other ecosystems: literal facts first.
        // Dedicated success grammars can be added without discarding unknown rows.
        ("php" | "phpunit" | "phpstan" | "pest" | "paratest" | "ecs" | "pint" | "phpt", _) => {
            ("php", raw.to_string())
        }
        ("deno" | "bun", _) => ("runtime", raw.to_string()),
        ("sbt", _) => ("scala", raw.to_string()),
        ("ctest", _) => ("ctest", raw.to_string()),
        ("sqlfluff", _) => ("sqlfluff", raw.to_string()),
        ("oc", _) => ("openshift", raw.to_string()),
        ("glab", _) => ("gitlab", raw.to_string()),
        ("wget", _) => ("download", raw.to_string()),
        ("prettier" | "black", _) => ("formatter", raw.to_string()),
        _ => return None,
    };
    // A reversible codec is not a readable view. Keep every literal field and
    // every diagnostic visible; only explicitly recognized success progress
    // may be summarized. Legacy codecs below exist solely for archive decoding.
    if text == raw {
        return Some((name, text));
    }
    let measured = crate::token_metrics::TokenCounts::measure(raw, &text);
    Some((
        name,
        if measured.tokens_saved > 64 {
            text
        } else {
            raw.to_string()
        },
    ))
}

fn unwrap_runner(mut command: &[String]) -> &[String] {
    loop {
        let Some(first) = command.first() else {
            return command;
        };
        let program = first
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(first)
            .trim_end_matches(".exe");
        let skip = match program {
            "npx" | "bunx" => {
                let mut index = 1;
                while command
                    .get(index)
                    .is_some_and(|arg| matches!(arg.as_str(), "--yes" | "-y" | "--"))
                {
                    index += 1;
                }
                index
            }
            "bundle" | "npm" | "pnpm" | "yarn"
                if command.get(1).is_some_and(|arg| arg == "exec") =>
            {
                if command.get(2).is_some_and(|s| s == "--") {
                    3
                } else {
                    2
                }
            }
            "uv" if command.get(1).is_some_and(|arg| arg == "run") => 2,
            "python" | "python3" if command.get(1).is_some_and(|arg| arg == "-m") => 2,
            _ => return command,
        };
        if skip >= command.len() || command[skip].starts_with('-') {
            return command;
        }
        command = &command[skip..];
    }
}

/// LMR-LINES/2: @JSON-prefix, literal suffix rows (escape @, &, =, ! and backslash with backslash), =N additional occurrences of
/// the preceding decoded row, &N an earlier decoded row (zero-based),
/// !1/!0 final newline flag. Line order is exact.
/// Only consecutive lines share prefixes; no hashing, truncation or sorting.
#[cfg(test)]
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

/// Version 3 adds readable prefix dictionaries and references to complete blocks.
/// `@N` selects a previously defined prefix; `@N:JSON` defines an extension.
/// `&N:C` copies C already decoded rows starting at N (no forward references).
/// References preserve order, multiplicity, whitespace and final LF.
#[cfg(test)]
fn factor_blocks(raw: &str) -> String {
    use std::collections::HashMap;
    if raw.is_empty() || raw.starts_with("LMR-LINES/") {
        return raw.to_string();
    }
    let lines: Vec<&str> = raw.split_terminator('\n').collect();
    let mut out = String::from("LMR-LINES/3\n");
    let mut seen = HashMap::<&str, usize>::new();
    let mut prefixes = HashMap::<&str, usize>::new();
    let mut prefix = "";
    let mut group_end = 0;
    let mut i = 0;
    while i < lines.len() {
        let row = lines[i];
        if let Some(&start) = seen.get(row) {
            let mut count = 1;
            // Disjoint source/destination blocks make expansion unambiguous.
            while start + count < i
                && i + count < lines.len()
                && lines[start + count] == lines[i + count]
            {
                count += 1;
            }
            if count >= 2 || row.len() >= 24 {
                let record = if count == 1 {
                    format!("&{start}\n")
                } else {
                    format!("&{start}:{count}\n")
                };
                let literal_bytes: usize = lines[i..i + count].iter().map(|s| s.len() + 1).sum();
                if record.len() < literal_bytes {
                    out.push_str(&record);
                    // Keep the first occurrence: stable references are easier to read.
                    for offset in 0..count {
                        seen.entry(lines[i + offset]).or_insert(i + offset);
                    }
                    i += count;
                    continue;
                }
            }
        }
        if i >= group_end || !row.starts_with(prefix) {
            let mut best = "";
            let mut best_end = i + 1;
            let mut best_score = 0;
            let mut common = row;
            for (end, next) in lines.iter().enumerate().take(i + 64).skip(i + 1) {
                common = common_prefix(common, next);
                if common.len() < 4 {
                    break;
                }
                let score = common.len() * (end - i);
                if score > best_score + 12 {
                    best_score = score;
                    best = common;
                    best_end = end + 1;
                }
            }
            if best != prefix {
                if let Some(id) = prefixes.get(best) {
                    out.push_str(&format!("@{id}\n"));
                } else {
                    // Reuse the longest dictionary ancestor, without a linear
                    // scan of every prefix in a large directory listing.
                    let parent = best
                        .char_indices()
                        .rev()
                        .find_map(|(end, _)| prefixes.get(&best[..end]).map(|id| (*id, end)));
                    if let Some((id, end)) = parent.filter(|(_, end)| *end >= 8) {
                        out.push_str(&format!("@{id}:"));
                        write_json_row(&mut out, &best[end..]);
                    } else {
                        out.push('@');
                        write_json_row(&mut out, best);
                    }
                    prefixes.insert(best, prefixes.len());
                }
                prefix = best;
            }
            group_end = best_end;
        }
        seen.entry(row).or_insert(i);
        write_literal_row(&mut out, &row[prefix.len()..]);
        i += 1;
    }
    out.push_str(if raw.ends_with('\n') { "!1\n" } else { "!0\n" });
    if out.len() < raw.len() {
        out
    } else {
        raw.to_string()
    }
}

/// A visible dictionary for repeated long identifiers and URL prefixes. Each
/// two-character ~a reference expands to the literal printed in the header;
/// ~~ is a literal tilde. No hash, abbreviation, or externally stored fact.
#[cfg(test)]
fn factor_text(raw: &str) -> String {
    use std::collections::{BTreeMap, HashMap};
    use std::sync::LazyLock;
    static WORD: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"https?://[A-Za-z0-9./_-]+/|[A-Za-z_][A-Za-z_0-9]{9,}").unwrap()
    });
    if raw.len() < 4096 || raw.starts_with("LMR-") {
        return raw.to_string();
    }
    let mut counts = HashMap::<&str, usize>::new();
    for word in WORD.find_iter(raw) {
        *counts.entry(word.as_str()).or_default() += 1;
    }
    let mut ranked: Vec<_> = counts
        .into_iter()
        .filter_map(|(word, count)| {
            if count < 8 {
                return None;
            }
            let tokens =
                crate::token_metrics::TokenCounts::measure(word, "").original_tokens as i64;
            let score = count as i64 * (tokens - 2) - tokens - 6;
            (score > 0).then_some((score, word))
        })
        .collect();
    ranked.sort_unstable_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
    let aliases: HashMap<&str, char> = ranked
        .iter()
        .take(52)
        .zip((b'a'..=b'z').chain(b'A'..=b'Z'))
        .map(|((_, word), alias)| (*word, char::from(alias)))
        .collect();
    if aliases.is_empty() {
        return raw.to_string();
    }
    let dictionary: BTreeMap<char, &str> = aliases
        .iter()
        .map(|(word, alias)| (*alias, *word))
        .collect();
    let mut out = String::from("LMR-TEXT/1\n");
    out.push_str(&serde_json::to_string(&dictionary).expect("literal dictionary"));
    out.push('\n');
    let mut offset = 0;
    for word in WORD.find_iter(raw) {
        out.push_str(&raw[offset..word.start()].replace('~', "~~"));
        if let Some(alias) = aliases.get(word.as_str()) {
            out.push('~');
            out.push(*alias);
        } else {
            out.push_str(word.as_str());
        }
        offset = word.end();
    }
    out.push_str(&raw[offset..].replace('~', "~~"));
    if !raw.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(if raw.ends_with('\n') { "!1\n" } else { "!0\n" });
    out
}

fn expand_text(view: &str) -> anyhow::Result<String> {
    use anyhow::{bail, Context};
    let mut records = view.split_terminator('\n').skip(1);
    let dictionary: std::collections::BTreeMap<char, String> =
        serde_json::from_str(records.next().context("missing text dictionary")?)?;
    if dictionary.len() > 52 || dictionary.keys().any(|c| !c.is_ascii_alphabetic()) {
        bail!("invalid text dictionary keys");
    }
    let mut rows: Vec<_> = records.collect();
    if rows
        .last()
        .is_some_and(|s| s.starts_with("[raw: ") && s.ends_with(']'))
    {
        rows.pop();
    }
    let final_lf = match rows.pop() {
        Some("!0") => false,
        Some("!1") => true,
        _ => bail!("missing text end record"),
    };
    let payload = rows.join("\n");
    let mut chars = payload.chars();
    let mut result = String::new();
    while let Some(c) = chars.next() {
        if c == '~' {
            let key = chars.next().context("incomplete text reference")?;
            if key == '~' {
                result.push('~');
            } else {
                let word = dictionary.get(&key).context("unknown text reference")?;
                if result.len().saturating_add(word.len()) > 512 * 1024 * 1024 {
                    bail!("expanded text too large");
                }
                result.push_str(word);
            }
        } else {
            result.push(c);
        }
        if result.len() > 512 * 1024 * 1024 {
            bail!("expanded text too large");
        }
    }
    if final_lf {
        result.push('\n');
    }
    Ok(result)
}

/// Expand a reversible view without needing tee or a running original tool.
/// Reference validation and an output limit prevent malformed input amplification.
pub fn expand(view: &str) -> anyhow::Result<String> {
    if view.starts_with("Patch v1\n") || view.starts_with("Patch v2\n") {
        return crate::patch_view::expand(view);
    }
    if serde_json::from_str::<serde_json::Value>(view)
        .ok()
        .is_some_and(|v| v["format"] == "conversation-fold-v1")
    {
        return crate::conversation_views::expand(view);
    }

    use anyhow::{bail, Context};
    if view.starts_with("Paths v1\n")
        || view.starts_with("Matches v1\n")
        || view.contains("\n[repeat ")
    {
        return crate::reversible_views::expand(view);
    }
    if view.starts_with("LMR-TEXT/1\n") {
        return expand_text(view);
    }
    if !view.starts_with("LMR-LINES/2\n") && !view.starts_with("LMR-LINES/3\n") {
        return Ok(view.to_string());
    }
    const LIMIT: usize = 512 * 1024 * 1024;
    let mut prefix = String::new();
    let mut prefixes = Vec::<String>::new();
    let version3 = view.starts_with("LMR-LINES/3\n");
    let mut rows: Vec<String> = Vec::new();
    let mut bytes = 0usize;
    let mut records = view.split_terminator('\n').skip(1);
    while let Some(record) = records.next() {
        if let Some(json) = record.strip_prefix('@') {
            if version3 && !json.starts_with('"') {
                if let Some((id, tail)) = json.split_once(':') {
                    let id: usize = id.parse().context("invalid prefix reference")?;
                    prefix = prefixes
                        .get(id)
                        .context("prefix reference out of range")?
                        .clone()
                        + &serde_json::from_str::<String>(tail)
                            .context("invalid prefix extension")?;
                    prefixes.push(prefix.clone());
                } else {
                    let id: usize = json.parse().context("invalid prefix reference")?;
                    prefix = prefixes
                        .get(id)
                        .context("prefix reference out of range")?
                        .clone();
                }
            } else {
                prefix = serde_json::from_str(json).context("invalid prefix record")?;
                prefixes.push(prefix.clone());
            }
            if prefix.len() > LIMIT {
                bail!("prefix exceeds output limit");
            }
            continue;
        }
        if record == "!0" || record == "!1" {
            let rest: Vec<_> = records.collect();
            if !(rest.is_empty()
                || rest.len() == 1 && rest[0].starts_with("[raw: ") && rest[0].ends_with(']'))
            {
                bail!("unexpected records after end of reversible view");
            }
            return Ok(rows.join("\n") + if record == "!1" { "\n" } else { "" });
        }
        if version3 {
            if let Some((start, count)) = record.strip_prefix('&').and_then(|r| r.split_once(':')) {
                let start: usize = start.parse().context("invalid block start")?;
                let count: usize = count.parse().context("invalid block count")?;
                let end = start.checked_add(count).context("block overflow")?;
                if count == 0 || end > rows.len() || rows.len().saturating_add(count) > 1_000_000 {
                    bail!("block reference out of range");
                }
                let extra: usize = rows[start..end].iter().map(|s| s.len() + 1).sum();
                bytes = bytes
                    .checked_add(extra)
                    .context("expanded view too large")?;
                if bytes > LIMIT {
                    bail!("expanded view exceeds 512 MiB");
                }
                rows.extend_from_within(start..end);
                continue;
            }
        }
        let (row, repetitions) = if let Some(index) = record.strip_prefix('&') {
            let index: usize = index.parse().context("invalid line reference")?;
            (
                rows.get(index)
                    .context("line reference out of range")?
                    .clone(),
                1,
            )
        } else if let Some(count) = record.strip_prefix('=') {
            (
                rows.last().context("repeat without previous line")?.clone(),
                count.parse::<usize>().context("invalid repeat count")?,
            )
        } else {
            (
                prefix.clone() + record.strip_prefix('\\').unwrap_or(record),
                1,
            )
        };
        bytes = (row.len() + 1)
            .checked_mul(repetitions)
            .and_then(|n| bytes.checked_add(n))
            .context("expanded view too large")?;
        if bytes > LIMIT {
            bail!("expanded view exceeds 512 MiB");
        }
        if rows
            .len()
            .checked_add(repetitions)
            .is_none_or(|n| n > 1_000_000)
        {
            bail!("expanded view exceeds one million lines");
        }
        rows.extend(std::iter::repeat_n(row, repetitions));
    }
    bail!("missing reversible view end record")
}

#[cfg(test)]
fn common_prefix<'a>(a: &'a str, b: &str) -> &'a str {
    let mut size = a.bytes().zip(b.bytes()).take_while(|(a, b)| a == b).count();
    while !a.is_char_boundary(size) {
        size -= 1;
    }
    &a[..size]
}

#[cfg(test)]
fn write_literal_row(output: &mut String, row: &str) {
    if row.starts_with(['@', '&', '=', '!', '\\']) {
        output.push('\\');
    }
    output.push_str(row);
    output.push('\n');
}

#[cfg(test)]
fn write_json_row(output: &mut String, row: &str) {
    output.push_str(&serde_json::to_string(row).expect("string JSON"));
    output.push('\n');
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn decode(view: &str) -> String {
        if view.starts_with("LMR-LINES/3\n") || view.starts_with("LMR-TEXT/1\n") {
            return expand(view).unwrap();
        }
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
    fn text_dictionary_preserves_urls_unicode_crlf_tildes_and_final_lf() {
        for ending in ["", "\n"] {
            let raw = format!(
                "{}literal~a~~é漢字\r{ending}",
                "See https://example.invalid/long/path/42 and veryLongRepeatedIdentifierName\r\n"
                    .repeat(100)
            );
            let view = factor_text(&raw);
            assert!(view.starts_with("LMR-TEXT/1\n"));
            assert_eq!(expand(&view).unwrap(), raw);
            assert_eq!(expand(&(view + "[raw: abc123]\n")).unwrap(), raw);
        }
        for view in [
            "LMR-TEXT/1\n{}\n~a\n!0\n",
            "LMR-TEXT/1\n{}\n~\n!0\n",
            "LMR-TEXT/1\n{}\nx\n",
        ] {
            assert!(expand(view).is_err());
        }
    }

    #[test]
    fn block_dictionary_roundtrip_and_invalid_references() {
        for ending in ["", "\n"] {
            let mut raw = String::new();
            for i in 0..80 {
                raw.push_str(&format!(
                    "directory/long/prefix/{i}/file.rs:42:  exact match\n"
                ));
                raw.push_str("error: exact expected value\r\n  actual value\n");
            }
            raw.pop();
            raw.push_str(ending);
            let encoded = factor_blocks(&raw);
            assert!(encoded.starts_with("LMR-LINES/3\n"));
            assert_eq!(expand(&encoded).unwrap(), raw);
        }
        for text in ["@0", "@4:\"tail\"", "&0:1", "x\n&0:2", "x\n&0:0"] {
            assert!(expand(&format!("LMR-LINES/3\n{text}\n!0\n")).is_err());
        }
        assert_eq!(expand("LMR-LINES/3\nx\n!0\n").unwrap(), "x");
        assert_eq!(expand("LMR-LINES/3\nx\n!1\n").unwrap(), "x\n");
    }

    #[test]
    fn public_decoder_rejects_invalid_and_excessive_references() {
        for text in [
            "LMR-LINES/2\n&0\n!1\n",
            "LMR-LINES/2\nx\n=9999999999999999\n!1\n",
            "LMR-LINES/2\n=3\n!1\n",
            "LMR-LINES/2\nx\n",
            "LMR-LINES/2\nx\n!1\nlost text\n",
        ] {
            assert!(expand(text).is_err());
        }
        let raw = "large repeated diagnostic with paths and values\r\n".repeat(200);
        let view = factor_lines(&raw);
        assert_eq!(expand(&(view + "[raw: abc123]\n")).unwrap(), raw);
    }

    #[test]
    fn missing_ecosystems_and_wrappers_preserve_all_diagnostic_rows() {
        let raw = format!(
            "{}unique failure: path/to/file:92 unexpected value\n",
            "repeated diagnostic with complete expected and actual values\n".repeat(150)
        );
        for command in [
            "tsc --noEmit",
            "eslint .",
            "npx eslint .",
            "docker ps -a",
            "docker images --digests",
            "docker compose logs",
            "docker build .",
            "php artisan test",
            "phpunit",
            "phpstan analyze",
            "pest",
            "paratest",
            "ecs check",
            "pint",
            "phpt",
            "deno test",
            "bun test",
            "sbt test",
            "ctest",
            "sqlfluff lint",
            "oc get pods",
            "glab mr list",
            "wget --server-response example.invalid",
            "prettier --check .",
            "black --check .",
        ] {
            let command: Vec<String> = command.split_whitespace().map(str::to_string).collect();
            let (_, compact) = filter(&command, &raw).expect("dedicated lossless route");
            assert_eq!(decode(&compact), raw);
            assert_eq!(compact, raw);
        }
        let unsupported = ["uv", "run", "--project", "elsewhere", "pytest"].map(str::to_string);
        assert!(filter(&unsupported, &raw).is_none());
        // Les lanceurs de tests ne passent plus par ces routes : `test_views::runner` les reconnaît.
        for command in [
            "npx --yes jest",
            "npm exec vitest",
            "npm run test",
            "yarn test",
            "uv run pytest",
            "python3 -m pytest",
            "cargo test",
            "pytest",
        ] {
            let command: Vec<String> = command.split_whitespace().map(str::to_string).collect();
            assert!(filter(&command, &raw).is_none(), "{command:?}");
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
    fn cargo_build_preserves_warnings_locations_and_unknown_messages() {
        let raw = format!("{}warning: unused import\n --> src/lib.rs:7:3\nerror[E0308]: expected u32, found str\n[stderr]\nfinished with 1 error\n", "    Checking dependency v1.2.3\n".repeat(80));
        for sub in ["build", "check", "clippy", "rustc"] {
            let (_, view) = filter(&["cargo".into(), sub.into()], &raw).unwrap();
            assert_eq!(expand(&view).unwrap(), raw);
        }
    }
}
