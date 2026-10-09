//! Une vue de `git log` ne fait jamais disparaître un commit : chaque entrée garde son hash court
//! et son sujet, ou la sortie est rendue brute. L'original reste relisible par `tee read`.
#![cfg(unix)]
use std::path::Path;
use std::process::Command;

fn git(dir: &Path, args: &[&str]) -> Vec<u8> {
    let output = Command::new("git")
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_DATE", "2026-10-01T12:00:00+0200")
        .env("GIT_COMMITTER_DATE", "2026-10-01T12:00:00+0200")
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
    output.stdout
}

fn commit_file(dir: &Path, file: &str, line: &str, message: &[&str]) {
    use std::io::Write;
    let mut handle = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(file))
        .unwrap();
    writeln!(handle, "{line}").unwrap();
    git(dir, &["add", file]);
    let mut args = vec!["commit", "-q", "--allow-empty-message"];
    for part in message {
        args.extend(["-m", part]);
    }
    if message.is_empty() {
        args.extend(["-m", ""]);
    }
    git(dir, &args);
}

/// Dépôt de sept commits : corps vide, corps de plusieurs lignes avec pied `Signed-off-by`,
/// message vide, fusion, auteur accentué.
fn repository(with_sentinel: bool) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    git(path, &["init", "-q", "-b", "main"]);
    git(path, &["config", "user.name", "Zoé Œuvre"]);
    git(path, &["config", "user.email", "zoe@example.test"]);
    commit_file(path, "a.txt", "un", &["sujet un"]);
    commit_file(
        path,
        "a.txt",
        "deux",
        &[
            "sujet deux",
            "corps 1\ncorps 2\n\ncorps 4\ncorps 5\n\nSigned-off-by: X <x@y.test>",
        ],
    );
    git(path, &["checkout", "-q", "-b", "side"]);
    commit_file(path, "b.txt", "côté", &["sujet côté"]);
    git(path, &["checkout", "-q", "main"]);
    commit_file(path, "a.txt", "trois", &["sujet trois"]);
    git(
        path,
        &["merge", "-q", "--no-ff", "side", "-m", "fusion de side"],
    );
    commit_file(path, "a.txt", "quatre", &[]);
    commit_file(path, "a.txt", "cinq", &["sujet cinq"]);
    if with_sentinel {
        // L'ancienne sentinelle de découpage, légitime dans un message.
        commit_file(
            path,
            "a.txt",
            "six",
            &["sujet six", "avant\n---END---\napres\nl1\nl2\nl3\nl4"],
        );
        commit_file(path, "a.txt", "sept", &["sujet ---END--- avec suffixe"]);
    }
    dir
}

fn lm_resizer(dir: &Path, state: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .current_dir(dir)
        .env("LM_RESIZER_STATE_DIR", state)
        .env("LM_RESIZER_STORE", state.join("ccr.sqlite"))
        .env("LM_RESIZER_TRACKING", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(args)
        .output()
        .unwrap()
}

/// (arguments après `git`, nombre de commits attendus, la sortie montre-t-elle le hash ?,
/// montre-t-elle le sujet ?)
type Form = (Vec<String>, usize, bool, bool);

fn form(args: &[&str], count: usize, has_hash: bool, has_subject: bool) -> Form {
    (
        args.iter().map(|a| a.to_string()).collect(),
        count,
        has_hash,
        has_subject,
    )
}

#[test]
fn every_git_log_form_keeps_each_commit_visible_or_raw() {
    check_forms(&repository(false), &common_forms(7));
}

#[test]
fn a_sentinel_inside_a_message_never_hides_a_commit() {
    check_forms(&repository(true), &common_forms(9));
}

fn common_forms(total: usize) -> Vec<Form> {
    vec![
        form(&["log"], total, true, true),
        form(&["log", "-n", "3"], 3, true, true),
        form(&["log", "--oneline"], total, true, true),
        form(&["log", "-p", "-n", "2"], 2, true, true),
        form(&["log", "-p"], total, true, true),
        form(&["log", "--stat"], total, true, true),
        form(&["log", "--stat", "-n", "3"], 3, true, true),
        form(&["log", "--pretty=fuller"], total, true, true),
        form(&["log", "--pretty=short"], total, true, true),
        form(&["log", "--decorate", "-n", "4"], 4, true, true),
        form(&["log", "--graph", "--oneline"], total, true, true),
        form(&["log", "--format=%h %s"], total, true, true),
        form(&["log", "--format=%s"], total, false, true),
    ]
}

/// Ajoute `extra` commits dont un à message vide : de quoi dépasser dix entrées de graphe.
fn add_commits(repo: &tempfile::TempDir, extra: usize) {
    for n in 0..extra {
        let subject = format!("sujet supplémentaire {n}");
        if n == 3 {
            commit_file(repo.path(), "a.txt", &format!("extra {n}"), &[]);
        } else {
            commit_file(repo.path(), "a.txt", &format!("extra {n}"), &[&subject]);
        }
    }
}

#[test]
fn git_global_options_never_hide_a_commit() {
    let repo = repository(false);
    add_commits(&repo, 10);
    let path = repo.path().display().to_string();
    let forms = vec![
        form(&["--no-pager", "log"], 17, true, true),
        form(&["--no-pager", "log", "--oneline"], 17, true, true),
        form(
            &["--no-pager", "log", "--graph", "--oneline"],
            17,
            true,
            true,
        ),
        form(&["--no-pager", "log", "--format=%H"], 17, true, false),
        form(&["--no-pager", "log", "--stat", "-n", "5"], 5, true, true),
        form(&["--no-pager", "log", "-p", "-n", "2"], 2, true, true),
        form(&["-C", &path, "log", "--oneline"], 17, true, true),
        form(&["-C", &path, "log", "--format=%H"], 17, true, false),
        form(
            &["-c", "color.ui=false", "log", "--oneline"],
            17,
            true,
            true,
        ),
        form(
            &["-c", "color.ui=false", "log", "--format=%H"],
            17,
            true,
            false,
        ),
        form(
            &["--git-dir", &format!("{path}/.git"), "log", "--oneline"],
            17,
            true,
            true,
        ),
        // Un alias ne se reconnaît pas : le brut est rendu.
        form(&["-c", "alias.lg=log --oneline", "lg"], 17, true, true),
        form(&["-c", "alias.lg=log --format=%H", "lg"], 17, true, false),
    ];
    check_forms(&repo, &forms);
}

/// Commits dont le message a un corps de huit lignes : la vue par commit est alors plus courte
/// que le brut et n'est pas rendue brute par la garde de non-croissance.
fn add_long_commits(repo: &tempfile::TempDir, count: usize) {
    let body = (0..8)
        .map(|n| format!("ligne de corps numéro {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    for n in 0..count {
        let subject = format!("sujet long {n}");
        commit_file(
            repo.path(),
            "a.txt",
            &format!("long {n}"),
            &[&subject, &body],
        );
    }
}

#[test]
fn a_message_that_looks_like_a_patch_never_hides_a_commit() {
    let repo = repository(false);
    add_commits(&repo, 2);
    // Le plus récent : sa première ligne est celle de `--format=%B`.
    let body = (0..1200)
        .map(|n| format!("ligne distincte {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    commit_file(
        repo.path(),
        "a.txt",
        "leurre",
        &["diff --git a/leurre b/leurre", &body],
    );
    let forms = vec![
        form(&["log", "--format=%B%n%H"], 10, true, true),
        form(&["log", "--format=%B"], 10, false, true),
        form(&["log", "--format=diff --git %H%n%B"], 10, true, true),
        form(&["log", "-p", "--format=%B%n%H"], 10, true, true),
        form(&["log"], 10, true, true),
        form(&["log", "--stat"], 10, true, true),
    ];
    check_forms(&repo, &forms);
}

#[test]
fn nul_separated_log_keeps_every_subject() {
    let repo = repository(false);
    add_long_commits(&repo, 4);
    let forms = vec![
        form(&["log", "-z"], 11, true, true),
        form(&["log", "-z", "--stat"], 11, true, true),
        form(&["log", "-z", "--format=%h %s"], 11, true, true),
        form(&["log", "-z", "--pretty=fuller"], 11, true, true),
    ];
    check_forms(&repo, &forms);
}

fn check_forms(repo: &tempfile::TempDir, forms: &[Form]) {
    let state = tempfile::tempdir().unwrap();
    let mut failures = Vec::new();
    for (args, count, has_hash, has_subject) in forms {
        let mut listing = vec!["log", "--format=%h%x09%s"];
        if let Some(position) = args.iter().position(|a| a == "-n") {
            listing.extend([args[position].as_str(), args[position + 1].as_str()]);
        }
        let expected = String::from_utf8(git(repo.path(), &listing)).unwrap();
        let commits: Vec<(&str, &str)> = expected
            .lines()
            .map(|row| row.split_once('\t').unwrap())
            .collect();
        assert_eq!(commits.len(), *count, "{args:?}");

        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let direct = git(repo.path(), &arg_refs);
        let mut wrapped = vec!["exec", "--", "git"];
        wrapped.extend(&arg_refs);
        let out = lm_resizer(repo.path(), state.path(), &wrapped);
        assert!(out.status.success(), "{args:?}: {out:?}");
        let view = String::from_utf8(out.stdout).unwrap();
        let missing: Vec<String> = commits
            .iter()
            .flat_map(|(hash, subject)| {
                let mut lost = Vec::new();
                if *has_hash && !view.contains(hash) {
                    lost.push(format!("hash {hash}"));
                }
                if *has_subject && !subject.is_empty() && !view.contains(subject) {
                    lost.push(format!("sujet {subject:?}"));
                }
                lost
            })
            .collect();
        if !missing.is_empty() {
            failures.push(format!("{args:?}: absent de la vue : {missing:?}"));
        }
        // L'original reste relisible.
        if let Some(id) = view
            .lines()
            .filter_map(|row| row.strip_prefix("[tee:"))
            .filter_map(|row| row.split_once(']').map(|(id, _)| id))
            .next_back()
        {
            let recalled = lm_resizer(repo.path(), state.path(), &["tee", "read", id]);
            if !recalled.status.success() || recalled.stdout != direct {
                failures.push(format!("{args:?}: tee read ne rend pas l'original"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Sortie de `exec -- git <args>` comparée à celle de Git seul : octet pour octet.
fn assert_raw_identical(
    repo: &tempfile::TempDir,
    state: &Path,
    args: &[&str],
    failures: &mut Vec<String>,
) {
    let direct = git(repo.path(), args);
    let mut wrapped = vec!["exec", "--", "git"];
    wrapped.extend(args);
    let out = lm_resizer(repo.path(), state, &wrapped);
    if !out.status.success() {
        failures.push(format!("{args:?}: code {:?}", out.status.code()));
    } else if out.stdout != direct {
        failures.push(format!(
            "{args:?}: la vue n'est pas le brut ({} octets contre {})",
            out.stdout.len(),
            direct.len()
        ));
    }
}

fn long_history() -> tempfile::TempDir {
    let repo = repository(false);
    add_long_commits(&repo, 8);
    repo
}

#[test]
fn user_chosen_formats_that_imitate_a_commit_header_stay_raw() {
    let repo = long_history();
    let state = tempfile::tempdir().unwrap();
    let mut failures = Vec::new();
    for args in [
        // `%T` est le hash d'arbre : ces en-têtes ne sont pas des identités de commit.
        vec!["log", "--format=commit %T%n%w(0,4,4)%b%n%H %s"],
        vec![
            "log",
            "--format=commit %T%n    ligne 1%n    ligne 2%n    ligne 3%n    ligne 4%n    %H %s",
        ],
        vec![
            "log",
            "--format=commit deadbeef%n    ligne 1%n    ligne 2%n    ligne 3%n    ligne 4%n    %H %s",
        ],
        vec![
            "log",
            "--pretty=format:commit %T%n    a%n    b%n    c%n    d%n    %H %s",
        ],
        vec![
            "log",
            "--pretty=tformat:commit %T%n    a%n    b%n    c%n    d%n    %H %s",
        ],
        vec!["-c", "format.pretty=format:commit %T%n%w(0,4,4)%b%n%H %s", "log"],
        vec!["-c", "color.ui=false", "log"],
        vec!["log", "--oneline"],
        vec!["log", "-z"],
    ] {
        assert_raw_identical(&repo, state.path(), &args, &mut failures);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn format_pretty_from_the_git_configuration_keeps_the_log_raw() {
    let repo = long_history();
    git(
        repo.path(),
        &[
            "config",
            "format.pretty",
            "format:commit %T%n    l1%n    l2%n    l3%n    l4%n    %H %s",
        ],
    );
    let state = tempfile::tempdir().unwrap();
    let mut failures = Vec::new();
    for args in [
        vec!["log"],
        vec!["log", "-n", "5"],
        vec!["--no-pager", "log"],
    ] {
        assert_raw_identical(&repo, state.path(), &args, &mut failures);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn any_log_configuration_keeps_the_log_raw() {
    let repo = long_history();
    git(repo.path(), &["config", "log.date", "iso"]);
    let state = tempfile::tempdir().unwrap();
    let mut failures = Vec::new();
    assert_raw_identical(&repo, state.path(), &["log"], &mut failures);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_default_format_without_configuration_is_still_shortened() {
    let repo = long_history();
    let state = tempfile::tempdir().unwrap();
    let direct = git(repo.path(), &["log"]);
    let out = lm_resizer(repo.path(), state.path(), &["exec", "--", "git", "log"]);
    assert!(out.status.success());
    assert!(
        out.stdout.len() < direct.len(),
        "le format par défaut n'est plus raccourci ({} contre {} octets)",
        out.stdout.len(),
        direct.len()
    );
    let view = String::from_utf8(out.stdout).unwrap();
    let hashes = String::from_utf8(git(repo.path(), &["log", "--format=%H"])).unwrap();
    for hash in hashes.lines() {
        assert!(view.contains(hash), "hash {hash} absent de la vue");
    }
}

/// Texte déjà produit (`tool-output`, `pipe`) : la commande décrite ne dit pas le format. Un format
/// qui imite un en-tête sans la disposition par défaut (`Author:`, `Date:`) reste brut.
#[test]
fn already_captured_text_in_an_imitation_format_is_never_shortened() {
    let repo = long_history();
    let state = tempfile::tempdir().unwrap();
    let hashes = String::from_utf8(git(repo.path(), &["log", "--format=%H"])).unwrap();
    for format in [
        "--format=commit %T%n%w(0,4,4)%b%n%H %s",
        "--format=commit %T%n    ligne 1%n    ligne 2%n    ligne 3%n    ligne 4%n    %H %s",
        "--format=commit deadbeef%n    ligne 1%n    ligne 2%n    ligne 3%n    ligne 4%n    %H %s",
    ] {
        let raw = git(repo.path(), &["log", format]);
        let input = repo.path().join("capture.txt");
        std::fs::write(&input, &raw).unwrap();
        let input = input.to_str().unwrap();
        let views = [
            lm_resizer(
                repo.path(),
                state.path(),
                &["tool-output", "--command", "git log", "--input", input],
            )
            .stdout,
            {
                use std::io::Write;
                let mut child = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
                    .current_dir(repo.path())
                    .env("LM_RESIZER_STATE_DIR", state.path())
                    .env("LM_RESIZER_TRACKING", "0")
                    .env("GIT_CONFIG_NOSYSTEM", "1")
                    .env("GIT_CONFIG_GLOBAL", "/dev/null")
                    .args(["pipe", "--filter", "git-log"])
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .spawn()
                    .unwrap();
                child.stdin.take().unwrap().write_all(&raw).unwrap();
                child.wait_with_output().unwrap().stdout
            },
        ];
        for (route, view) in ["tool-output", "pipe"].iter().zip(views) {
            let view = String::from_utf8(view).unwrap();
            for hash in hashes.lines() {
                assert!(view.contains(hash), "{route} {format}: hash {hash} absent");
            }
        }
    }
}

/// Dépôt de 23 commits dont un à message vide : 23 hashes, 22 sujets non vides.
fn trap_history() -> tempfile::TempDir {
    let repo = repository(false);
    add_long_commits(&repo, 16);
    repo
}

/// Chaque vrai hash de commit et chaque sujet non vide figurent dans `view`.
fn assert_all_identities(label: &str, repo: &tempfile::TempDir, view: &[u8]) {
    let view = String::from_utf8_lossy(view);
    let expected = String::from_utf8(git(repo.path(), &["log", "--format=%H%x09%s"])).unwrap();
    let (mut hashes, mut subjects) = (0, 0);
    for row in expected.lines() {
        let (hash, subject) = row.split_once('\t').unwrap();
        hashes += 1;
        // `--oneline` n'affiche que sept caractères.
        assert!(view.contains(&hash[..7]), "{label}: hash {hash} absent");
        if !subject.is_empty() {
            subjects += 1;
            assert!(view.contains(subject), "{label}: sujet {subject:?} absent");
        }
    }
    assert_eq!((hashes, subjects), (23, 22), "{label}: dépôt piège");
}

fn pipe_stdin(repo: &Path, state: &Path, args: &[&str], input: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .current_dir(repo)
        .env("LM_RESIZER_STATE_DIR", state)
        .env("LM_RESIZER_TRACKING", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap().stdout
}

/// `pipe` et `tool-output` n'ont pas de ligne de commande fiable : aucune compression de
/// `git log` par reconnaissance du contenu, que le format imite le format par défaut ou le soit.
#[test]
fn captured_git_log_text_is_returned_raw_whatever_it_looks_like() {
    let repo = trap_history();
    let state = tempfile::tempdir().unwrap();
    let captures = [
        // La revue de 222c5c4 : un format qui imite aussi Author: et Date:.
        git(
            repo.path(),
            &[
                "log",
                "--format=commit %T%nAuthor: %an <%ae>%nDate: %ad%n%n    a%n    b%n    c%n    d%n    %H %s",
            ],
        ),
        // Le vrai format par défaut.
        git(repo.path(), &["log"]),
        git(repo.path(), &["log", "--stat"]),
    ];
    for (n, raw) in captures.iter().enumerate() {
        let input = repo.path().join(format!("capture{n}.txt"));
        std::fs::write(&input, raw).unwrap();
        let input = input.to_str().unwrap();
        let views = [
            (
                "tool-output",
                lm_resizer(
                    repo.path(),
                    state.path(),
                    &["tool-output", "--command", "git log", "--input", input],
                )
                .stdout,
            ),
            (
                "pipe",
                pipe_stdin(
                    repo.path(),
                    state.path(),
                    &["pipe", "--filter", "git-log"],
                    raw,
                ),
            ),
        ];
        for (route, view) in views {
            let label = format!("capture {n} par {route}");
            assert_all_identities(&label, &repo, &view);
            assert_eq!(&view, raw, "{label}: la capture n'est pas rendue brute");
        }
    }
}

/// Une commande composée par un shell n'est pas un appel direct à `git log` : le résumé générique ne
/// doit pas supprimer le commit à message vide.
#[test]
fn composed_shell_commands_keep_every_commit() {
    let repo = trap_history();
    let state = tempfile::tempdir().unwrap();
    for (script, code) in [
        ("git log --oneline; exit 0", 0),
        ("git log --oneline; exit 7", 7),
        ("git log --oneline; exit 23", 23),
        ("git log --oneline | cat", 0),
        ("git log --oneline && true", 0),
        ("git log --oneline", 0),
        ("git log; exit 7", 7),
    ] {
        let out = lm_resizer(
            repo.path(),
            state.path(),
            &["exec", "--", "sh", "-c", script],
        );
        assert_eq!(out.status.code(), Some(code), "{script}");
        assert_all_identities(script, &repo, &out.stdout);
    }
}

/// Un sujet long est une identité du commit : la vue le garde en entier, donc la garde finale ne la
/// refuse pas et le format par défaut reste raccourci.
#[test]
fn long_subjects_do_not_disable_the_shortened_default_view() {
    let repo = repository(false);
    let body = (0..8)
        .map(|n| format!("ligne de corps numéro {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let long_subject = format!("sujet très long {}", "mot ".repeat(40));
    for n in 0..6 {
        let subject = format!("{long_subject}{n}");
        commit_file(
            repo.path(),
            "a.txt",
            &format!("long {n}"),
            &[&subject, &body],
        );
    }
    let state = tempfile::tempdir().unwrap();
    let direct = git(repo.path(), &["log"]);
    let out = lm_resizer(repo.path(), state.path(), &["exec", "--", "git", "log"]);
    assert!(out.status.success());
    assert!(
        out.stdout.len() < direct.len(),
        "sujets longs : la vue n'est plus raccourcie ({} contre {} octets)",
        out.stdout.len(),
        direct.len()
    );
    let view = String::from_utf8(out.stdout).unwrap();
    for n in 0..6 {
        assert!(
            view.contains(&format!("{long_subject}{n}")),
            "sujet {n} coupé"
        );
    }
}

/// Revue de bf84f2d : quatre formes de `git log` derrière un shell composé, trois opérateurs, trois
/// codes. Un shell composé n'a ni producteur ni format établis : la sortie est le brut de Git, octet
/// pour octet (le seul ajout permis est l'en-tête `[FAIL] Command failed (exit code: N)`).
#[test]
fn composed_shell_log_forms_are_returned_raw_for_every_operator_and_exit_code() {
    let repo = trap_history();
    let state = tempfile::tempdir().unwrap();
    let forms = [
        "--oneline --abbrev=4",
        "--format=%h --abbrev=4",
        "--format=%s",
        "--graph --oneline --abbrev=4",
    ];
    let mut failures = Vec::new();
    for form in forms {
        let direct = git(
            repo.path(),
            &form.split(' ').fold(vec!["log"], |mut acc, word| {
                acc.push(word);
                acc
            }),
        );
        for code in [0, 7, 23] {
            for (operator, script) in [
                (";", format!("git log {form}; exit {code}")),
                ("|", format!("git log {form} | cat; exit {code}")),
                ("&&", format!("git log {form} && exit {code}")),
            ] {
                let out = lm_resizer(
                    repo.path(),
                    state.path(),
                    &["exec", "--", "sh", "-c", &script],
                );
                let label = format!("{form} {operator} exit {code}");
                if out.status.code() != Some(code) {
                    failures.push(format!("{label}: code {:?}", out.status.code()));
                    continue;
                }
                let text = String::from_utf8_lossy(&out.stdout).to_string();
                let view = text
                    .strip_prefix(&format!("[FAIL] Command failed (exit code: {code})\n"))
                    .unwrap_or(&text);
                if view.as_bytes() != direct.as_slice() {
                    failures.push(format!(
                        "{label}: la vue n'est pas le brut ({} octets contre {})",
                        view.len(),
                        direct.len()
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Une ligne de shell décrite à `tool-output` n'établit pas non plus le producteur : brut.
#[test]
fn a_described_composed_shell_line_is_returned_raw_by_tool_output() {
    let repo = trap_history();
    let state = tempfile::tempdir().unwrap();
    for form in [
        "--oneline --abbrev=4",
        "--format=%h --abbrev=4",
        "--format=%s",
        "--graph --oneline --abbrev=4",
    ] {
        let words: Vec<&str> = std::iter::once("log").chain(form.split(' ')).collect();
        let raw = git(repo.path(), &words);
        let input = repo.path().join("capture.txt");
        std::fs::write(&input, &raw).unwrap();
        let command = format!("sh -c 'git log {form}; exit 0'");
        let out = lm_resizer(
            repo.path(),
            state.path(),
            &[
                "tool-output",
                "--command",
                &command,
                "--input",
                input.to_str().unwrap(),
            ],
        );
        assert_eq!(out.stdout, raw, "{command}");
    }
}
