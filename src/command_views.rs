//! Native command-view registry. Renderers consume captured text; they never
//! launch a second tool or depend on an external executable's filter engine.
use crate::patch_view::render as git_diff;

pub fn direct_args(command: &[String]) -> Option<Vec<String>> {
    let name = super::command_basename(command.first()?);
    match name.as_str() {
        "git" | "ls" | "tree" | "find" | "grep" | "rg" | "cat" | "cargo" | "pytest" | "jest"
        | "vitest" | "go" | "npm" | "pnpm" | "uv" | "pip" | "docker" | "kubectl" | "tsc"
        | "eslint" | "ctest" => Some(command.to_vec()),
        _ if pipe_command(&name).is_some() => Some(command.to_vec()),
        _ => None,
    }
}

pub fn pipe_command(name: &str) -> Option<Vec<String>> {
    let words: &[&str] = match name {
        "git-log" => &["git", "log"],
        "git-diff" => &["git", "diff"],
        "git-status" => &["git", "status"],
        "grep" | "rg" => &["rg"],
        "find" | "fd" => &["find"],
        "cargo" | "cargo-test" => &["cargo", "test"],
        "pytest" => &["pytest"],
        "go-test" => &["go", "test"],
        "go-build" => &["go", "build"],
        "ctest" => &["ctest"],
        "tsc" => &["tsc"],
        "vitest" => &["vitest"],
        "ruff-check" => &["ruff", "check"],
        "ruff-format" => &["ruff", "format"],
        "mypy" => &["mypy"],
        "sqlfluff-lint" => &["sqlfluff", "lint"],
        "prettier" => &["prettier"],
        "log" => &["pipe-raw"],
        "phpunit" | "pest" | "paratest" | "ecs" | "phpstan" | "pint" => &[name],
        "php-test" => &["pest"],
        _ => return None,
    };
    Some(words.iter().map(|s| s.to_string()).collect())
}

pub fn filter(command: &[String], raw: &str) -> Option<(String, String)> {
    let name = super::command_basename(command.first()?);
    let sub = command.get(1).map(String::as_str).unwrap_or("");
    let (kind, candidate) = match (name.as_str(), sub) {
        ("git", "log") => ("git-log", git_log(raw)),
        ("git", "diff") => ("git-diff", git_diff(raw)),
        ("git", "status") => ("git-status", git_status(raw)),
        ("git", "show") => (
            "git-show",
            if raw.lines().any(|row| row.starts_with("diff --git ")) {
                git_diff(raw)
            } else {
                raw.to_owned()
            },
        ),
        ("rg" | "grep", _) => ("grep", crate::file_views::search(raw)),
        ("find", _) => ("find", crate::file_views::paths(raw)),
        ("ls", _) => ("ls", crate::file_views::listing(command, raw)),
        ("tree", _) => ("tree", crate::file_views::tree(raw)),
        ("cargo", "test") => ("cargo-test", crate::test_views::cargo(raw)),
        ("dotnet", "test") => ("dotnet-test", crate::test_views::dotnet(raw)),
        ("pytest", _) => ("pytest", crate::test_views::pytest(raw)),
        ("go", "test") => ("go-test", crate::test_views::go(raw)),
        ("jest" | "vitest", _) => ("js-test", crate::test_views::javascript(raw)),
        ("go", "build") => ("go-build", raw.to_owned()),
        ("npm" | "pnpm" | "pip" | "uv", _) if sub != "exec" => {
            ("packages", crate::package_views::filter(&name, sub, raw))
        }
        ("docker" | "kubectl", "logs") => ("container-logs", crate::container_views::logs(raw)),
        ("docker", "ps") | ("kubectl", "get") => {
            ("container-table", crate::container_views::table(raw))
        }
        ("cargo", "build" | "check") => (
            "cargo-build",
            crate::diagnostic_views::cargo_build(raw, sub),
        ),
        ("tsc", _) => ("typescript", crate::diagnostic_views::typescript(raw)),
        ("eslint", _) => ("eslint", raw.to_owned() + "\n\n"),
        ("ruff", "check") | ("mypy", _) => {
            ("python-lint", crate::diagnostic_views::python_lint(raw))
        }
        _ => return None,
    };
    // A renderer may cost more on tiny inputs. Retain the producer's text in
    // that case; measurements remain exact and tee stores the original bytes.
    let output = if name == "git"
        || candidate == raw
        || crate::token_metrics::TokenCounts::measure(raw, &candidate).tokens_saved >= 0
    {
        candidate
    } else {
        raw.to_owned()
    };
    Some((format!("native:{kind}"), output))
}

fn clipped(s: &str, width: usize) -> String {
    let mut boundaries = s.char_indices();
    let cutoff = boundaries.nth(width.saturating_sub(3)).map(|(i, _)| i);
    match (cutoff, boundaries.nth(2)) {
        (Some(byte), Some(_)) => format!("{}...", &s[..byte]),
        _ => s.to_owned(),
    }
}

fn git_log(raw: &str) -> String {
    // A patch is a distinct record type, never message prose.
    if raw.lines().any(|s| s.starts_with("diff --")) {
        return git_diff(raw);
    }
    // Records are delimited by the `---END---` sentinel of hook-generated
    // formats or, for Git's default/medium/--stat output, by `commit <hash>`
    // header lines. Any other format (for example `--oneline`) has no record
    // boundary we can trust and stays literal.
    let records: Vec<String> = if raw.contains("---END---") {
        raw.split("---END---").map(str::to_owned).collect()
    } else {
        let mut records: Vec<String> = Vec::new();
        for row in raw.lines() {
            if is_commit_header(row) || records.is_empty() {
                records.push(String::new());
            }
            let last = records.last_mut().expect("a record was just pushed");
            last.push_str(row);
            last.push('\n');
        }
        if !records
            .first()
            .is_some_and(|r| is_commit_header(r.lines().next().unwrap_or("")))
        {
            return raw.to_owned();
        }
        records
    };
    let total = records.iter().filter(|r| !r.trim().is_empty()).count();
    let mut excerpts = Vec::new();
    for record in records
        .iter()
        .filter(|r| !r.trim().is_empty())
        .take(GIT_LOG_RECORDS)
    {
        let mut rows: Vec<String> = Vec::new();
        let mut hidden = 0usize;
        let mut subject = false;
        for row in record.trim().lines() {
            if rows.is_empty() {
                rows.push(clipped(row, 80));
                continue;
            }
            let text = row.trim();
            if text.is_empty()
                || ["Signed-off-by:", "Co-authored-by:"]
                    .iter()
                    .any(|prefix| text.starts_with(prefix))
            {
                continue;
            }
            // Header fields, the subject line and `--stat` rows identify the
            // commit; they are never folded into the omitted-line count.
            let header = [
                "Merge:",
                "Author:",
                "Date:",
                "AuthorDate:",
                "Commit:",
                "CommitDate:",
            ]
            .iter()
            .any(|prefix| row.starts_with(prefix));
            if header {
                // `Date:   Tue` alignment padding carries no information.
                let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
                rows.push(format!("  {}", clipped(&compact, 80)));
            } else if is_stat_row(row) {
                rows.push(format!("  {}", clipped(text, 80)));
            } else if !subject {
                subject = true;
                rows.push(format!("  {}", clipped(text, 80)));
            } else {
                hidden += 1;
            }
        }
        if hidden != 0 {
            rows.push(format!("  [+{hidden} lines omitted]"));
        }
        excerpts.extend(rows);
    }
    if total > GIT_LOG_RECORDS {
        excerpts.push(format!("[+{} commits omitted]", total - GIT_LOG_RECORDS));
    }
    excerpts.join("\n")
}

const GIT_LOG_RECORDS: usize = 50;

fn is_commit_header(row: &str) -> bool {
    row.strip_prefix("commit ").is_some_and(|rest| {
        let hash = rest.split_whitespace().next().unwrap_or("");
        // Git never abbreviates below four hex digits.
        hash.len() >= 4 && hash.chars().all(|c| c.is_ascii_hexdigit())
    })
}

fn is_stat_row(row: &str) -> bool {
    static STAT: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(
            r"^ \S.*\s\|\s+(\d+( [+-]+)?|Bin .*)$|^ \d+ files? changed(, \d+ insertions?\(\+\))?(, \d+ deletions?\(-\))?$",
        )
        .unwrap()
    });
    STAT.is_match(row)
}

fn git_status(raw: &str) -> String {
    let mut lines: Vec<_> = raw
        .lines()
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned)
        .collect();
    if lines.is_empty() {
        return "Clean working tree".into();
    }
    if let Some(branch) = lines[0].strip_prefix("On branch ") {
        if lines.last().map(String::as_str) == Some("nothing to commit, working tree clean") {
            if lines.len() == 2 {
                return format!("{branch}; clean");
            }
            if lines.len() == 3 {
                if let Some(upstream) = lines[1]
                    .strip_prefix("Your branch is up to date with '")
                    .and_then(|s| s.strip_suffix("'."))
                {
                    return format!("{branch} = {upstream}; clean");
                }
            }
        }
    }
    if let Some(branch) = lines[0].strip_prefix("## ") {
        lines[0] = format!("* {branch}");
        if lines.len() == 1 {
            lines.push("clean — nothing to commit".into());
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clean_status_preserves_branch_and_tracking_state() {
        assert_eq!(git_status("On branch topic\nYour branch is up to date with 'origin/topic'.\n\nnothing to commit, working tree clean\n"), "topic = origin/topic; clean");
        assert_eq!(
            git_status("On branch topic\nnothing to commit, working tree clean\n"),
            "topic; clean"
        );
        for raw in ["On branch topic\nYour branch is ahead of 'origin/topic' by 2 commits.\nnothing to commit, working tree clean", "On branch topic\nChanges not staged for commit:\n modified: file"] {
            assert_eq!(git_status(raw), raw);
        }
    }
    #[test]
    fn git_author_stays_with_its_commit() {
        let raw = "commit aaaa\nAuthor: Alice <a@example.test>\nDate: today\n\n  title\n---END---\ncommit bbbb\nAuthor: Bob <b@example.test>\nDate: yesterday\n\n  autre\n";
        assert_eq!(git_log(raw), "commit aaaa\n  Author: Alice <a@example.test>\n  Date: today\n  title\ncommit bbbb\n  Author: Bob <b@example.test>\n  Date: yesterday\n  autre");
    }
    #[test]
    fn default_git_log_keeps_every_commit_hash_and_subject() {
        let raw = "commit 280ebcb6edac3aa4cdc545dbff8a26c5ac4861fe\nAuthor: David <d@example.test>\nDate:   Tue Jun 23 20:02:34 2026 -0700\n\n    Update upload-artifact\n\n    Long body line one\n    Long body line two\n\ncommit 82f83d8667e4f6c3aaa62140ab33ec9436a3196f\nMerge: 8953020 b041087\nAuthor: Eve <e@example.test>\nDate:   Sat Jun 20 15:19:45 2026 -0700\n\n    Merge branch 'stable'\n";
        let view = git_log(raw);
        assert!(view.contains("commit 280ebcb6edac3aa4cdc545dbff8a26c5ac4861fe"));
        assert!(view.contains("commit 82f83d8667e4f6c3aaa62140ab33ec9436a3196f"));
        assert!(view.contains("  Update upload-artifact"));
        assert!(view.contains("  Merge: 8953020 b041087"));
        assert!(view.contains("  Date: Tue Jun 23 20:02:34 2026 -0700"));
        assert!(view.contains("  Merge branch 'stable'"), "{view}");
        assert!(view.contains("  [+2 lines omitted]"));
        assert!(!view.contains("Long body line one"));
    }
    #[test]
    fn git_log_stat_rows_are_kept() {
        let raw = "commit 2c18cc482244f4bb9cc65003b07426c18a79a190\nAuthor: A <a@example.test>\nDate:   Mon May 18 16:11:12 2026 +0200\n\n    Resolve lint\n\n    body\n---\n tests/test_version_req.rs | 4 ++--\n 1 file changed, 2 insertions(+), 2 deletions(-)\n";
        let view = git_log(raw);
        assert!(
            view.contains("tests/test_version_req.rs | 4 ++--"),
            "{view}"
        );
        assert!(view.contains("1 file changed, 2 insertions(+), 2 deletions(-)"));
        assert!(view.contains("  Resolve lint"));
    }
    #[test]
    fn git_log_without_record_headers_stays_literal() {
        let raw = "280ebcb Update actions\n82f83d8 Update checkout\n2c18cc4 Resolve lint\n7625c7a Release\nfd404d0 Merge\n";
        assert_eq!(git_log(raw), raw);
    }
    #[test]
    fn all_git_patch_rows_remain_recoverable() {
        let raw = "diff --git a/source b/source\n--- a/source\n+++ b/source\n@@ -179,1 +181,1 @@ fn total\n---source\n+++source\n";
        let view = git_diff(raw);
        assert!(view.contains("@@ -179,1 +181,1 @@ fn total"));
        assert!(view.contains("---source\n+++source"));
    }
    #[test]
    fn unicode_truncation_never_splits_a_character() {
        assert_eq!(clipped(&"é".repeat(100), 80), "é".repeat(77) + "...");
    }
}
