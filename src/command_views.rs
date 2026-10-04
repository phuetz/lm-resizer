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
    struct Excerpt {
        rows: Vec<String>,
        hidden: usize,
    }
    let mut excerpts = Vec::new();
    for record in raw.split("---END---").take(50) {
        let mut excerpt = Excerpt {
            rows: Vec::new(),
            hidden: 0,
        };
        for row in record.trim().lines() {
            if excerpt.rows.is_empty() {
                excerpt.rows.push(clipped(row, 80));
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
            match excerpt.rows.len() {
                0..=3 => excerpt.rows.push(format!("  {}", clipped(text, 80))),
                _ => excerpt.hidden += 1,
            }
        }
        if excerpt.hidden != 0 {
            excerpt
                .rows
                .push(format!("  [+{} lines omitted]", excerpt.hidden));
        }
        excerpts.extend(excerpt.rows);
    }
    excerpts.join("\n")
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
