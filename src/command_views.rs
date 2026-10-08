//! Native command-view registry. Renderers consume captured text; they never
//! launch a second tool or depend on an external executable's filter engine.
use crate::patch_view;

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
                patch_view::render(raw)
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

fn git_diff(raw: &str) -> String {
    let rows: Vec<&str> = raw.lines().collect();
    // Large Git patches can dwarf every other command in an agent transcript.
    // Keep the opening patch records through the reference tool's visible
    // window and make the complete producer output recoverable through tee.
    if rows.len() > 1000 && rows.first().is_some_and(|r| r.starts_with("diff --git ")) {
        let visible = rows.len().min(350);
        return format!(
            "{}\n[{} further patch lines; retrieve the complete output with tee]\n",
            rows[..visible].join("\n"),
            rows.len() - visible
        );
    }
    // A short ordinary one-file diff has no need for a transport codec:
    // its file name, hunk and every changed/context line fit in a plain view.
    if rows.len() <= 30
        && rows.iter().filter(|r| r.starts_with("diff --git ")).count() == 1
        && rows.iter().any(|r| r.starts_with("--- a/"))
        && rows.iter().any(|r| r.starts_with("+++ b/"))
        && rows.iter().any(|r| r.starts_with("@@ "))
    {
        let mut view = Vec::new();
        let mut added = 0;
        let mut removed = 0;
        for row in &rows {
            if let Some(path) = row.strip_prefix("+++ b/") {
                view.push(path.to_string());
            } else if row.starts_with("diff --git ")
                || row.starts_with("index ")
                || row.starts_with("--- a/")
            {
                continue;
            } else {
                if row.starts_with('+') {
                    added += 1;
                } else if row.starts_with('-') {
                    removed += 1;
                }
                view.push((*row).to_string());
            }
        }
        view.push(format!("  +{added} -{removed}"));
        return view.join("\n");
    }
    patch_view::render(raw)
}

fn clipped(s: &str, width: usize) -> String {
    let mut boundaries = s.char_indices();
    let cutoff = boundaries.nth(width.saturating_sub(3)).map(|(i, _)| i);
    match (cutoff, boundaries.nth(2)) {
        (Some(byte), Some(_)) => format!("{}...", &s[..byte]),
        _ => s.to_owned(),
    }
}

/// Une ligne `git log --stat` : ` chemin | 12 ++++---` ou ` image.png | Bin 0 -> 512 bytes`.
fn stat_row(row: &str) -> Option<String> {
    let (path, rest) = row.split_once(" | ")?;
    let path = path.trim();
    // Une ligne de statistique commence par exactement une espace ; le corps du
    // message est indenté de quatre.
    if path.is_empty() || !row.starts_with(' ') || row.starts_with("  ") {
        return None;
    }
    let rest = rest.trim();
    let count = rest.trim_end_matches(['+', '-']).trim_end();
    let digits = count.split_whitespace().next()?;
    (rest.starts_with("Bin ") || digits.chars().all(|c| c.is_ascii_digit()))
        .then(|| format!("  {path} | {count}"))
}

fn is_commit_header(row: &str) -> bool {
    row.strip_prefix("commit ").is_some_and(|rest| {
        rest.split_whitespace()
            .next()
            .is_some_and(|hash| hash.len() >= 7 && hash.chars().all(|c| c.is_ascii_hexdigit()))
    })
}

/// `git log --stat` sans sentinelle : un enregistrement par commit, aucun commit perdu.
/// Les barres de proportion `+++---` disparaissent (le nombre de lignes reste), ainsi que
/// les lignes vides et les pieds `Signed-off-by` / `Co-authored-by`. Les trois premières
/// lignes de corps suivent le titre ; le reste est compté et récupérable dans tee.
fn git_log_stat(raw: &str) -> Option<String> {
    // Le bilan commence par exactement une espace ; le corps d'un message est
    // indenté de quatre et ne doit pas suffire à déclencher cette vue.
    let has_summary = raw.lines().any(|row| {
        if !row.starts_with(' ') || row.starts_with("  ") {
            return false;
        }
        let row = row.trim_start();
        row.split_once(" file").is_some_and(|(n, rest)| {
            !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) && rest.contains("changed")
        })
    });
    if !has_summary || !raw.lines().any(is_commit_header) {
        return None;
    }
    let mut out: Vec<String> = Vec::new();
    let mut message_rows = 0usize;
    let mut hidden = 0usize;
    let flush = |out: &mut Vec<String>, hidden: &mut usize| {
        if *hidden != 0 {
            out.push(format!("  [+{hidden} message lines omitted]"));
            *hidden = 0;
        }
    };
    for row in raw.lines() {
        if is_commit_header(row) {
            flush(&mut out, &mut hidden);
            message_rows = 0;
            out.push(clipped(row, 120));
            continue;
        }
        let text = row.trim();
        if text.is_empty() {
            continue;
        }
        if let Some(stat) = stat_row(row) {
            flush(&mut out, &mut hidden);
            out.push(stat);
        } else if row.starts_with("    ") {
            if ["Signed-off-by:", "Co-authored-by:", "Co-Authored-By:"]
                .iter()
                .any(|prefix| text.starts_with(prefix))
            {
                continue;
            }
            // Titre + trois lignes de corps au plus.
            if message_rows < 4 {
                out.push(format!("  {}", clipped(text, 100)));
                message_rows += 1;
            } else {
                hidden += 1;
            }
        } else {
            // Author:, Date:, Merge:, bilan « N files changed » : conservés tels quels.
            flush(&mut out, &mut hidden);
            out.push(format!("  {}", clipped(text, 120)));
        }
    }
    flush(&mut out, &mut hidden);
    Some(out.join("\n"))
}

fn git_log(raw: &str) -> String {
    // A patch is a distinct record type, never message prose.
    if raw.lines().any(|s| s.starts_with("diff --")) {
        return git_diff(raw);
    }
    if !raw.contains("---END---") {
        if let Some(view) = git_log_stat(raw) {
            return view;
        }
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
    fn stat_log() -> String {
        let mut raw = String::new();
        for (hash, who, title, files) in [
            (
                "a".repeat(40),
                "Alice",
                "Premier titre",
                vec![
                    ("src/lib.rs", 12, "++++++-----"),
                    ("docs/guide.md", 3, "+++"),
                ],
            ),
            (
                "b".repeat(40),
                "Bob",
                "Second titre",
                vec![(
                    "src/main.rs",
                    40,
                    "++++++++++++++++++++----------------------",
                )],
            ),
            (
                "c".repeat(40),
                "Carol",
                "Troisième titre",
                vec![("img/logo.png", 0, "")],
            ),
        ] {
            raw.push_str(&format!("commit {hash}\nAuthor: {who} <{who}@example.test>\nDate:   Mon Oct 5 12:00:00 2026 +0200\n\n    {title}\n\n    Corps de {who} : tableau a | 5 colonnes.\n\n    Co-Authored-By: Autre <autre@example.test>\n\n"));
            for (path, n, bars) in &files {
                if *n == 0 {
                    raw.push_str(&format!(" {path} | Bin 0 -> 512 bytes\n"));
                } else {
                    raw.push_str(&format!(" {path} | {n} {bars}\n"));
                }
            }
            raw.push_str(&format!(
                " {} files changed, 5 insertions(+), 2 deletions(-)\n\n",
                files.len()
            ));
        }
        raw
    }
    #[test]
    fn stat_log_keeps_every_commit_title_and_file() {
        let raw = stat_log();
        let view = git_log(&raw);
        for fact in [
            "commit aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "commit bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "commit cccccccccccccccccccccccccccccccccccccccc",
            "Author: Bob <Bob@example.test>",
            "Troisième titre",
            "Corps de Alice : tableau a | 5 colonnes.",
            "src/lib.rs | 12",
            "docs/guide.md | 3",
            "src/main.rs | 40",
            "img/logo.png | Bin 0 -> 512 bytes",
            "2 files changed, 5 insertions(+), 2 deletions(-)",
        ] {
            assert!(view.contains(fact), "manque {fact:?} dans\n{view}");
        }
        assert!(
            !view.contains("  tableau a | 5"),
            "une ligne de corps n'est pas un fichier\n{view}"
        );
        assert!(
            !view.contains("+++"),
            "les barres de proportion sont retirées\n{view}"
        );
        assert!(!view.contains("Co-Authored-By"));
        assert!(view.len() < raw.len());
    }
    #[test]
    fn body_line_resembling_a_stat_summary_does_not_switch_views() {
        let raw = "commit aaaaaaa\nAuthor: Alice <a@example.test>\nDate: today\n\n    refactor\n\n    5 files changed in this refactor\n\ncommit bbbbbbb\nAuthor: Bob <b@example.test>\nDate: yesterday\n\n    autre\n";
        assert!(git_log_stat(raw).is_none());
        assert!(git_log(raw).contains("lines omitted"));
    }
    #[test]
    fn plain_log_without_stat_keeps_the_historical_view() {
        // Comportement historique conservé tel quel : le second commit est replié
        // dans le décompte « omitted » (le brut reste dans tee).
        let raw = "commit aaaa\nAuthor: Alice <a@example.test>\nDate: today\n\n    title\n\ncommit bbbb\nAuthor: Bob <b@example.test>\nDate: yesterday\n\n    autre\n";
        assert_eq!(git_log(raw), "commit aaaa\n  Author: Alice <a@example.test>\n  Date: today\n  title\n  [+4 lines omitted]");
    }
    #[test]
    fn unicode_truncation_never_splits_a_character() {
        assert_eq!(clipped(&"é".repeat(100), 80), "é".repeat(77) + "...");
    }
}
