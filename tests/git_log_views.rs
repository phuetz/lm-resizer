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

#[test]
fn every_git_log_form_keeps_each_commit_visible_or_raw() {
    check_every_form(&repository(false), 7);
}

#[test]
fn a_sentinel_inside_a_message_never_hides_a_commit() {
    check_every_form(&repository(true), 9);
}

fn check_every_form(repo: &tempfile::TempDir, total: usize) {
    let state = tempfile::tempdir().unwrap();
    // (arguments, nombre de commits attendus, la forme montre-t-elle le hash court ?)
    let forms: &[(&[&str], usize, bool)] = &[
        (&["log"], total, true),
        (&["log", "-n", "3"], 3, true),
        (&["log", "--oneline"], total, true),
        (&["log", "-p", "-n", "2"], 2, true),
        (&["log", "-p"], total, true),
        (&["log", "--stat"], total, true),
        (&["log", "--stat", "-n", "3"], 3, true),
        (&["log", "--pretty=fuller"], total, true),
        (&["log", "--pretty=short"], total, true),
        (&["log", "--decorate", "-n", "4"], 4, true),
        (&["log", "--graph", "--oneline"], total, true),
        (&["log", "--format=%h %s"], total, true),
        (&["log", "--format=%s"], total, false),
    ];
    let mut failures = Vec::new();
    for (args, count, has_hash) in forms {
        let mut listing = vec!["log", "--format=%h%x09%s"];
        if let Some(position) = args.iter().position(|a| *a == "-n") {
            listing.extend(&args[position..position + 2]);
        }
        let expected = String::from_utf8(git(repo.path(), &listing)).unwrap();
        let commits: Vec<(&str, &str)> = expected
            .lines()
            .map(|row| row.split_once('\t').unwrap())
            .collect();
        assert_eq!(commits.len(), *count, "{args:?}");

        let direct = git(repo.path(), args);
        let mut wrapped = vec!["exec", "--", "git"];
        wrapped.extend(*args);
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
                if !subject.is_empty() && !view.contains(subject) {
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
            .filter_map(|row| row.strip_suffix(']'))
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
