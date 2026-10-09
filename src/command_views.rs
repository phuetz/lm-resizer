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

/// `git [options globales] <sous-commande> …` : la commande sans les options globales
/// (`-C <dir>`, `-c <clé=valeur>`, `--git-dir`, `--no-pager`…), pour reconnaître la sous-commande.
/// `None` quand aucune sous-commande ne se lit sûrement (option inconnue en fin de ligne).
pub fn git_without_globals(command: &[String]) -> Option<Vec<String>> {
    let (_, from_subcommand) = git_split_globals(command)?;
    let mut out = vec![command.first()?.clone()];
    out.extend(from_subcommand);
    Some(out)
}

/// Sépare `git [options globales] <sous-commande> …` en (options globales, sous-commande et suite).
fn git_split_globals(command: &[String]) -> Option<(Vec<String>, Vec<String>)> {
    let mut rest = &command[1..];
    // Options globales qui prennent la valeur suivante (sans `=`).
    const WITH_VALUE: [&str; 7] = [
        "-C",
        "-c",
        "--git-dir",
        "--work-tree",
        "--namespace",
        "--super-prefix",
        "--config-env",
    ];
    while let Some(first) = rest.first() {
        if !first.starts_with('-') {
            let globals = command[1..command.len() - rest.len()].to_vec();
            return Some((globals, rest.to_vec()));
        }
        rest = if WITH_VALUE.contains(&first.as_str()) {
            rest.get(2..)?
        } else {
            &rest[1..]
        };
    }
    None
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

/// `python -m pytest …`, `uv run pytest …` et `uv run python -m pytest …` produisent la sortie
/// de `pytest …` : rend la commande réduite à `pytest …`, ou `None` pour tout autre module ou script.
fn pytest_through_runner(command: &[String]) -> Option<&[String]> {
    let mut rest = command;
    if super::command_basename(rest.first()?) == "uv" && rest.get(1).is_some_and(|a| a == "run") {
        rest = &rest[2..];
    }
    let program = super::command_basename(rest.first()?);
    if matches!(program.as_str(), "python" | "python3" | "py")
        && rest.get(1).is_some_and(|a| a == "-m")
        && rest.get(2).is_some_and(|a| a == "pytest")
    {
        return Some(&rest[2..]);
    }
    (program == "pytest" && rest.len() != command.len()).then_some(rest)
}

pub fn filter(command: &[String], raw: &str) -> Option<(String, String)> {
    let command = pytest_through_runner(command).unwrap_or(command);
    let name = super::command_basename(command.first()?);
    let sub = command.get(1).map(String::as_str).unwrap_or("");
    let (kind, candidate) = match (name.as_str(), sub) {
        // Appelant sans lecture de la configuration (tube, test) : jamais de vue compressée.
        ("git", "log") => (
            "git-log",
            git_log(raw, log_requests_patch(&command[2..]), false),
        ),
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
    git_diff_view(raw, true)
}

/// `truncate` : `git diff` garde la fenêtre d'ouverture d'un grand patch ; `git log` ne tronque
/// jamais, une troncature y ferait disparaître des commits entiers.
fn git_diff_view(raw: &str, truncate: bool) -> String {
    let rows: Vec<&str> = raw.lines().collect();
    // Large Git patches can dwarf every other command in an agent transcript.
    // Keep the opening patch records through the reference tool's visible
    // window and make the complete producer output recoverable through tee.
    if truncate && rows.len() > 1000 && rows.first().is_some_and(|r| r.starts_with("diff --git ")) {
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

/// `commit <hash>` : 40 chiffres, ou une abréviation (`--abbrev-commit`, quatre au minimum pour git).
/// Hashes de commit et sujets non vides qu'un texte présente comme un historique git : en-têtes
/// `commit <hash>` (sujet = première ligne indentée du message) et lignes `<hash> <sujet>` ou
/// `<hash>` seul (7 à 40 chiffres hexadécimaux, éventuellement après un graphe `*`/`|`).
fn git_log_identities(text: &str) -> (Vec<&str>, Vec<&str>) {
    let rows: Vec<&str> = text.lines().collect();
    let (mut hashes, mut subjects) = (Vec::new(), Vec::new());
    for (i, row) in rows.iter().enumerate() {
        let body = strip_graph(row);
        if is_commit_header(body) {
            hashes.push(body.split_whitespace().nth(1).unwrap_or_default());
            let message = rows[i + 1..]
                .iter()
                .skip_while(|l| !l.trim().is_empty())
                .find(|l| !l.trim().is_empty());
            if let Some(line) = message.filter(|l| l.starts_with("    ")) {
                subjects.push(line.trim());
            }
        } else if let Some(hash) = body.split(' ').next().filter(|w| is_hex_id(w)) {
            hashes.push(hash);
            let subject = body[hash.len()..].trim();
            if !subject.is_empty() {
                subjects.push(subject);
            }
        }
    }
    (hashes, subjects)
}

fn strip_graph(row: &str) -> &str {
    if row.starts_with(['*', '|', '/', '\\']) {
        row.trim_start_matches(['*', '|', '/', '\\', ' ', '_'])
    } else {
        row
    }
}

fn is_hex_id(word: &str) -> bool {
    (7..=40).contains(&word.len())
        && word
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Garde finale, pour toute sortie : `output` perd-il un hash de commit ou un sujet non vide que
/// `raw` présente ? Elle ne reconnaît le contenu que pour **refuser** une vue (le brut est alors
/// rendu) : se tromper la rend inutile, jamais dangereuse.
pub fn lost_git_identity(raw: &str, output: &str) -> bool {
    if raw == output {
        return false;
    }
    let (hashes, subjects) = git_log_identities(raw);
    if hashes.is_empty() {
        return false;
    }
    let mut words = std::collections::HashSet::new();
    let mut lines = std::collections::HashSet::new();
    for row in output.lines() {
        words.extend(row.split_whitespace());
        let body = strip_graph(row).trim();
        lines.insert(body);
        if let Some((_, rest)) = body.split_once(' ') {
            lines.insert(rest.trim());
        }
    }
    hashes.iter().any(|h| !words.contains(h)) || subjects.iter().any(|s| !lines.contains(s))
}

/// Défense en profondeur pour le texte déjà produit (`tool-output`, `pipe`), dont le format n'est pas
/// connu par la commande : chaque en-tête `commit <hash>` est suivi de la disposition par défaut, des
/// lignes `Merge:`, `Author:` et `Date:` seules jusqu'à la ligne vide. Un format qui imite un en-tête
/// (`commit %T%n    …`) ne l'a pas. Ce contrôle ne remplace pas le contrat de la commande (voir
/// [`git_log_default_format`]) : il en réduit seulement les trous.
fn has_default_layout(raw: &str) -> bool {
    let rows: Vec<&str> = raw.lines().collect();
    let mut headers = 0usize;
    for (i, row) in rows.iter().enumerate() {
        if !is_commit_header(row) {
            continue;
        }
        headers += 1;
        let (mut author, mut date) = (false, false);
        for line in rows[i + 1..].iter().take_while(|l| !l.trim().is_empty()) {
            if line.starts_with("Author:") {
                author = true;
            } else if line.starts_with("Date:") {
                date = true;
            } else if !line.starts_with("Merge:") {
                return false;
            }
        }
        if !(author && date) {
            return false;
        }
    }
    headers > 0
}

fn is_commit_header(row: &str) -> bool {
    row.strip_prefix("commit ").is_some_and(|rest| {
        rest.split_whitespace()
            .next()
            .is_some_and(|hash| hash.len() >= 4 && hash.chars().all(|c| c.is_ascii_hexdigit()))
    })
}

/// `git log` sans sentinelle (avec ou sans `--stat`) : un enregistrement par commit, aucun commit
/// perdu. Les barres de proportion `+++---` disparaissent (le nombre de lignes reste), ainsi que
/// les lignes vides et les pieds `Signed-off-by` / `Co-authored-by`. Les trois premières
/// lignes de corps suivent le titre ; le reste est compté et récupérable dans tee.
/// `None` quand la sortie ne commence pas par un en-tête `commit <hash>` (`--oneline`, `--format`,
/// `--graph`, `--pretty=email`…) : aucune compaction n'y est sûre, le brut est rendu.
fn git_log_records(raw: &str) -> Option<String> {
    if !raw
        .lines()
        .find(|row| !row.trim().is_empty())
        .is_some_and(is_commit_header)
        || !has_default_layout(raw)
    {
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
            // Titre + trois lignes de corps au plus. Le titre est une identité du commit : jamais
            // coupé ; les lignes de corps le sont à 100 caractères.
            if message_rows < 4 {
                if message_rows == 0 {
                    out.push(format!("  {text}"));
                } else {
                    out.push(format!("  {}", clipped(text, 100)));
                }
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
    let view = out.join("\n");
    // Une vue ne perd jamais un commit et ne dépasse jamais le brut.
    let commits = |text: &str| text.lines().filter(|row| is_commit_header(row)).count();
    (commits(&view) == commits(raw)
        && crate::token_metrics::TokenCounts::measure(raw, &view).tokens_saved > 0)
        .then_some(view)
}

/// Les options de `git log` qui produisent un patch (`-p`, `-u`, `-U<n>`, `--patch…`, `--cc`, `-c`,
/// `-L`, `--word-diff…`), sauf annulation par `--no-patch` / `-s` ensuite. `args` suit `log`.
fn log_requests_patch(args: &[String]) -> bool {
    let mut patch = false;
    for arg in args {
        let arg = arg.as_str();
        if arg == "--" {
            break;
        }
        if matches!(arg, "-s" | "--no-patch") {
            patch = false;
        } else if matches!(arg, "-p" | "-u" | "-c" | "--cc" | "--combined")
            || arg.starts_with("-U")
            || arg.starts_with("-L")
            || arg.starts_with("--unified")
            || arg.starts_with("--word-diff")
            || arg.starts_with("--color-words")
            || (arg.len() >= 5 && arg.starts_with("--") && "--patch-with-stat".starts_with(arg))
            || arg.starts_with("--patch")
        {
            patch = true;
        }
    }
    patch
}

/// Vue d'un `git log` dont la sortie contient des patchs : jamais tronquée, et rendue brute si le
/// codec ne se déplie pas en l'original exact.
fn git_log_patch(raw: &str) -> String {
    let view = git_diff_view(raw, false);
    if view.starts_with("Patch v") && patch_view::expand(&view).ok().as_deref() != Some(raw) {
        return raw.to_owned();
    }
    view
}

/// Vue d'un `git log` exécuté : `command` est la ligne complète, options globales comprises.
/// La vue compressée par commit ne s'applique qu'au format par défaut (voir
/// [`git_log_default_format`]) ; toute autre sortie est rendue brute, sauf un patch demandé, dont
/// le codec se déplie en l'original exact.
///
/// `executed` : la commande est lancée par `lm-resizer` lui-même (`exec`, `lm-resizer git log`). Un
/// texte déjà produit (`pipe`, `tool-output`) n'a pas de ligne de commande fiable : jamais de vue
/// compressée, quel que soit son contenu.
pub fn git_log_filter(command: &[String], raw: &str, executed: bool) -> (String, String) {
    let patch = git_without_globals(command)
        .is_some_and(|plain| log_requests_patch(plain.get(2..).unwrap_or_default()));
    (
        "native:git-log".to_string(),
        git_log(raw, patch, executed && git_log_default_format(command)),
    )
}

/// `true` seulement si la sortie de `git log` a le format par défaut, que rien ne peut changer :
/// ni `--format`/`--pretty`/`--oneline`/`-z` dans la commande, ni option globale `-c` ou
/// `--config-env`, ni aucune clé `format.*` ou `log.*` dans la configuration de Git. La
/// configuration se lit avec `git config --get-regexp`, jamais en analysant le texte de la sortie.
/// Dans le doute (git absent, configuration illisible) : `false`, donc le brut.
pub fn git_log_default_format(command: &[String]) -> bool {
    let Some((globals, rest)) = git_split_globals(command) else {
        return false;
    };
    if rest.first().map(String::as_str) != Some("log") {
        return false;
    }
    if globals
        .iter()
        .any(|g| g == "-c" || g == "--config-env" || g.starts_with("--config-env="))
    {
        return false;
    }
    let chooses_a_layout = rest[1..]
        .iter()
        .take_while(|arg| arg.as_str() != "--")
        .any(|arg| {
            matches!(arg.as_str(), "--oneline" | "-z" | "--graph")
                || arg.starts_with("--format")
                || arg.starts_with("--pretty")
        });
    if chooses_a_layout {
        return false;
    }
    // Code 1 = aucune clé ne correspond ; 0 = au moins une ; tout autre résultat = inconnu.
    std::process::Command::new(&command[0])
        .args(&globals)
        .args(["config", "--get-regexp", r"^(format|log)\."])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()
        .and_then(|status| status.code())
        == Some(1)
}

fn git_log(raw: &str, patch_requested: bool, compact: bool) -> String {
    // Un séparateur NUL (`-z`) commence les en-têtes suivants par `\0` : aucun découpage sûr.
    if raw.contains('\0') {
        return raw.to_owned();
    }
    // Un patch est un type d'enregistrement distinct, jamais de la prose de message. Le texte d'un
    // message ne le décide pas : un message `diff --git …` dans `--format=%B` est de la prose.
    // Il faut que la commande demande un patch, ou que la sortie ait le format ordinaire (message
    // indenté, donc un `diff --` en colonne 0 est un vrai patch).
    let has_patch_row = raw.lines().any(|s| s.starts_with("diff --"));
    let headered = raw
        .lines()
        .find(|row| !row.trim().is_empty())
        .is_some_and(is_commit_header);
    if has_patch_row && (patch_requested || (compact && headered)) {
        return git_log_patch(raw);
    }
    if !compact {
        return raw.to_owned();
    }
    // Aucun séparateur n'est déduit du texte d'un message : seuls les en-têtes `commit <hash>`
    // découpent, et toute forme sans en-tête (`--oneline`, `--format`, `--graph`) reste brute.
    git_log_records(raw).unwrap_or_else(|| raw.to_owned())
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
        let body = "\n    ligne de corps".repeat(12);
        let raw = format!("commit aaaa\nAuthor: Alice <a@example.test>\nDate: today\n\n    title\n{body}\n\ncommit bbbb\nAuthor: Bob <b@example.test>\nDate: yesterday\n\n    autre\n{body}\n");
        let view = git_log(&raw, false, true);
        assert!(view.starts_with(
            "commit aaaa\n  Author: Alice <a@example.test>\n  Date: today\n  title\n"
        ));
        assert!(view
            .contains("commit bbbb\n  Author: Bob <b@example.test>\n  Date: yesterday\n  autre\n"));
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
        let view = git_log(&raw, false, true);
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
    fn body_line_resembling_a_stat_summary_stays_message_text() {
        let raw = "commit aaaaaaa\nAuthor: Alice <a@example.test>\nDate: today\n\n    refactor\n\n    5 files changed in this refactor\n\ncommit bbbbbbb\nAuthor: Bob <b@example.test>\nDate: yesterday\n\n    autre\n";
        let view = git_log(raw, false, true);
        for fact in [
            "commit aaaaaaa",
            "refactor",
            "5 files changed in this refactor",
            "commit bbbbbbb",
            "autre",
        ] {
            assert!(view.contains(fact), "manque {fact:?} dans\n{view}");
        }
        assert!(!view.contains("omitted"));
    }
    #[test]
    fn plain_log_shows_every_commit() {
        let raw = "commit aaaaaaaa\nAuthor: Alice <a@example.test>\nDate: today\n\n    title\n\n    ligne 1\n    ligne 2\n    ligne 3\n    ligne 4\n    ligne 5\n\n    Signed-off-by: A <a@example.test>\n\ncommit bbbbbbbb (HEAD -> main)\nMerge: aaaaaaa ccccccc\nAuthor: Bob <b@example.test>\nDate: yesterday\n\n    fusion\n\ncommit cccccccc\nAuthor: Zoé <z@example.test>\nDate: before\n\n";
        assert_eq!(
            git_log(raw, false, true),
            "commit aaaaaaaa\n  Author: Alice <a@example.test>\n  Date: today\n  title\n  ligne 1\n  ligne 2\n  ligne 3\n  [+2 message lines omitted]\ncommit bbbbbbbb (HEAD -> main)\n  Merge: aaaaaaa ccccccc\n  Author: Bob <b@example.test>\n  Date: yesterday\n  fusion\ncommit cccccccc\n  Author: Zoé <z@example.test>\n  Date: before"
        );
    }
    #[test]
    fn forms_without_a_commit_header_are_returned_raw() {
        for raw in [
            "aac6d62 sujet cinq\n956d045\n6ecefbf fusion\n7af913f trois\nad9434e côté\n",
            "* aac6d62 sujet cinq\n*   6ecefbf fusion\n|\\\n| * ad9434e côté\n",
            "sujet cinq\nfusion\ntrois\n",
            "From aac6d6244a36c451c1361bc2e13918afadc3308a Mon Sep 17 00:00:00 2001\nSubject: x\n",
        ] {
            assert_eq!(git_log(raw, false, true), raw);
        }
    }
    #[test]
    fn abbreviated_commit_headers_are_recognised() {
        let raw = format!("commit abcd\nAuthor: A <a@example.test>\nDate: d\n\n    t\n{}\ncommit ef01\nAuthor: B <b@example.test>\nDate: d\n\n    u\n", "\n    corps".repeat(40));
        let view = git_log(&raw, false, true);
        assert!(
            view.contains("commit abcd") && view.contains("commit ef01"),
            "{view}"
        );
        assert!(view.len() < raw.len());
    }
    #[test]
    fn a_view_that_would_grow_the_log_is_returned_raw() {
        let raw = "commit aaaaaaa\nAuthor: A <a@example.test>\nDate: d\n\n    t\n";
        assert_eq!(git_log(raw, false, true), raw);
    }
    #[test]
    fn old_sentinel_text_in_a_message_is_just_text() {
        let raw: String = (0..80)
            .map(|n| {
                format!(
                    "commit {n:040x}\nAuthor: A <a@example.test>\nDate: d\n\n    sujet ---END--- {n}\n\n    avant\n    ---END---\n    apres\n    l1\n    l2\n    l3\n\n"
                )
            })
            .collect();
        let view = git_log(&raw, false, true);
        assert_eq!(
            view.lines()
                .filter(|row| row.starts_with("commit "))
                .count(),
            80
        );
        // Une forme sans en-tête avec la même chaîne reste brute.
        let oneline = "abc1234 sujet ---END--- un\ndef5678 deux\n";
        assert_eq!(git_log(oneline, false, true), oneline);
    }
    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_string).collect()
    }
    #[test]
    fn git_global_options_are_skipped_to_find_the_subcommand() {
        for (line, expected) in [
            ("git log -n 3", Some("git log -n 3")),
            ("git --no-pager log --oneline", Some("git log --oneline")),
            ("git -C /tmp/repo log", Some("git log")),
            ("git -c color.ui=false -c a.b=c log -1", Some("git log -1")),
            (
                "git --git-dir=/x/.git --work-tree /x status",
                Some("git status"),
            ),
            ("git --no-pager -C . --bare lg", Some("git lg")),
            ("git --no-pager", None),
            ("git -C", None),
        ] {
            let got = git_without_globals(&words(line)).map(|c| c.join(" "));
            assert_eq!(got.as_deref(), expected, "{line}");
        }
    }
    #[test]
    fn only_patch_options_request_a_patch() {
        for yes in [
            "-p",
            "-u",
            "-U5",
            "--patch",
            "--patch-with-stat",
            "--pat",
            "--cc",
            "-c",
            "-L1,5:f",
            "--word-diff",
            "-n 2 -p",
        ] {
            assert!(log_requests_patch(&words(yes)), "{yes}");
        }
        for no in [
            "--oneline",
            "--stat",
            "--format=%B",
            "-n 3",
            "-p --no-patch",
            "-p -s",
            "--pretty=fuller",
            "--graph",
        ] {
            assert!(!log_requests_patch(&words(no)), "{no}");
        }
    }
    #[test]
    fn nul_separated_and_decoy_patch_text_stay_raw() {
        let nul = "commit aaaaaaa\nAuthor: A <a@example.test>\n\n    t\n\0commit bbbbbbb\nAuthor: B <b@example.test>\n\n    u\n";
        assert_eq!(git_log(nul, false, true), nul);
        let decoy = format!("diff --git a/x b/x\n{}", "ligne\n".repeat(1200));
        assert_eq!(git_log(&decoy, false, true), decoy);
        // Même avec -p : un message n'est pas un patch, le codec se vérifie ou le brut est rendu.
        let view = git_log(&decoy, true, true);
        assert!(view == decoy || patch_view::expand(&view).unwrap() == decoy);
    }
    #[test]
    fn a_layout_chosen_in_the_command_disables_the_shortened_view_without_asking_git() {
        for line in [
            "git log --oneline",
            "git log -z",
            "git log --format=%H",
            "git log --format=commit%x20%T",
            "git log --pretty=fuller",
            "git log --pretty=format:x",
            "git -c format.pretty=oneline log",
            "git -c color.ui=false log",
            "git --config-env=format.pretty=VAR log",
            "git --no-pager -c a.b=c log -n 3",
            "git status",
            "git",
        ] {
            assert!(!git_log_default_format(&words(line)), "{line}");
        }
    }
    #[test]
    fn the_final_guard_refuses_a_view_that_loses_a_hash_or_a_subject() {
        let oneline = "abc1234 premier\n8441500 \ndef5678 troisième\n";
        // Intact, ou sans identité reconnaissable : jamais refusée.
        assert!(!lost_git_identity(oneline, oneline));
        assert!(!lost_git_identity(
            "rien de git ici\nligne 2\n",
            "ligne 2\n"
        ));
        // Le hash du commit à message vide disparaît.
        assert!(lost_git_identity(
            oneline,
            "abc1234 premier\ndef5678 troisième\n"
        ));
        // Un sujet disparaît (ou est tronqué).
        assert!(lost_git_identity(
            oneline,
            "abc1234\n8441500\ndef5678 troisi...\n"
        ));
        // Le graphe `*` ne cache pas les identités.
        let graph = "* abc1234 premier\n| * def5678 deuxième\n";
        assert!(!lost_git_identity(graph, graph));
        assert!(lost_git_identity(graph, "* abc1234 premier\n"));
        // En-têtes `commit <hash>` : le sujet est la première ligne indentée du message.
        let long = "commit aaaaaaa1\nAuthor: A <a@e.t>\nDate: d\n\n    sujet un\n\n    corps\n";
        assert!(!lost_git_identity(
            long,
            "commit aaaaaaa1\n  Author: A\n  sujet un\n"
        ));
        assert!(lost_git_identity(long, "commit aaaaaaa1\n  Author: A\n"));
        // Une ligne de message qui commence par un mot hexadécimal n'est pas une identité.
        let prose = "commit aaaaaaa1\nAuthor: A <a@e.t>\nDate: d\n\n    sujet\n\n    deadbeef est le correctif\n";
        assert!(!lost_git_identity(prose, "commit aaaaaaa1\n  sujet\n"));
    }
    #[test]
    fn python_module_and_uv_runners_use_the_pytest_view() {
        let raw = "..F\n=== FAILURES ===\n___ test_a ___\nE   assert 0\n=== short test summary info ===\nFAILED t.py::test_a - assert 0\n1 failed, 2 passed in 0.01s\n";
        let expected = crate::test_views::pytest(raw);
        assert!(expected.starts_with("Pytest: 2 passed"), "{expected}");
        for command in [
            "pytest -q",
            "python3 -m pytest -q",
            "python -m pytest tests/",
            "/tmp/venv/bin/python -m pytest -x",
            "uv run pytest",
            "uv run python -m pytest -q",
        ] {
            let command: Vec<String> = command.split_whitespace().map(str::to_string).collect();
            let (kind, view) = filter(&command, raw)
                .unwrap_or_else(|| panic!("{command:?} n'a pas de vue native"));
            assert_eq!(kind, "native:pytest", "{command:?}");
            assert_eq!(view, expected, "{command:?}");
        }
        // Un autre module, ou un script, ne devient pas pytest.
        for command in ["python3 -m http.server", "python3 script.py", "uv run ruff"] {
            let command: Vec<String> = command.split_whitespace().map(str::to_string).collect();
            let routed = filter(&command, raw).map(|(kind, _)| kind);
            assert_ne!(routed.as_deref(), Some("native:pytest"), "{command:?}");
        }
    }
    #[test]
    fn unicode_truncation_never_splits_a_character() {
        assert_eq!(clipped(&"é".repeat(100), 80), "é".repeat(77) + "...");
    }
}
