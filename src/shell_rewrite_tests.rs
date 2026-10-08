//! `rewrite-shell` ne doit jamais produire une ligne que `sh` lirait autrement que la
//! ligne d'origine. Chaque cas est exécuté pour de vrai par `sh -c`, avec des programmes
//! de substitution qui n'enregistrent que leurs arguments : c'est le shell, pas notre
//! lecteur de citations, qui juge.
#![cfg(unix)]

use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command as Process;

const STUBS: &[&str] = &["lm-resizer", "head", "cargo", "git", "psql", "grep", "ls"];

struct Sandbox {
    dir: tempfile::TempDir,
}

impl Sandbox {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        fs::create_dir(&bin).unwrap();
        for name in STUBS {
            let path = bin.join(name);
            fs::write(
                &path,
                "#!/bin/sh\n{ printf '%s' \"${0##*/}\"; for a in \"$@\"; do printf '\\000%s' \"$a\"; done; } > \"$LMR_CALLS/call.$$\"\n",
            )
            .unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        fs::create_dir(dir.path().join("calls")).unwrap();
        fs::create_dir(dir.path().join("work")).unwrap();
        Self { dir }
    }

    /// Exécute `command` avec `sh -c` ; rend les appels (programme + arguments) dans l'ordre
    /// de leur numéro de processus, et les fichiers apparus dans le dossier de travail.
    fn run(&self, command: &str) -> (Vec<Vec<String>>, Vec<String>) {
        let calls = self.dir.path().join("calls");
        for entry in fs::read_dir(&calls).unwrap() {
            fs::remove_file(entry.unwrap().path()).unwrap();
        }
        let work = self.dir.path().join("work");
        for entry in fs::read_dir(&work).unwrap() {
            fs::remove_file(entry.unwrap().path()).unwrap();
        }
        let path = format!(
            "{}:/usr/bin:/bin",
            self.dir.path().join("bin").to_str().unwrap()
        );
        Process::new("/bin/sh")
            .arg("-c")
            .arg(command)
            .current_dir(&work)
            .env("PATH", path)
            .env("LMR_CALLS", &calls)
            .env("HOME", "/nonexistent-home")
            .output()
            .unwrap();
        (read_calls(&calls), list(&work))
    }
}

fn read_calls(dir: &Path) -> Vec<Vec<String>> {
    let mut files: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort_by_key(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix("call."))
            .and_then(|n| n.parse::<u64>().ok())
            .unwrap_or(0)
    });
    files
        .iter()
        .map(|path| {
            fs::read(path)
                .unwrap()
                .split(|b| *b == 0)
                .map(|part| String::from_utf8_lossy(part).into_owned())
                .collect()
        })
        .collect()
}

fn list(dir: &Path) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Citation de référence, écrite ici et non importée : le test ne doit pas dépendre du code
/// qu'il juge.
fn apostrophes(text: &str) -> String {
    let mut quoted = String::from("'");
    for ch in text.chars() {
        if ch == '\'' {
            // Ferme la citation, ajoute une apostrophe entre guillemets doubles, rouvre.
            quoted.push_str("'\"'\"'");
        } else {
            quoted.push(ch);
        }
    }
    quoted.push('\'');
    quoted
}

/// La ligne réécrite fait, dans le shell, exactement ce que faisait la ligne d'origine.
fn assert_same_behaviour(sandbox: &Sandbox, original: &str) {
    let report = rewrite_shell_report(original);
    let (before_calls, before_files) = sandbox.run(original);
    let (after_calls, after_files) = sandbox.run(&report.rewritten);
    assert_eq!(
        after_files, before_files,
        "la ligne réécrite crée d'autres fichiers que l'originale\n  originale : {original:?}\n  réécrite  : {:?}",
        report.rewritten
    );
    if !report.changed {
        assert_eq!(report.rewritten, original.trim());
        return;
    }
    assert_eq!(
        before_calls.len(),
        after_calls.len(),
        "nombre de commandes lancées\n  originale : {original:?}\n  réécrite  : {:?}\n  avant {before_calls:?}\n  après {after_calls:?}",
        report.rewritten
    );
    for (before, after) in before_calls.iter().zip(&after_calls) {
        if after[0] == "lm-resizer" && after.get(1).map(String::as_str) == Some("exec") {
            // `lm-resizer exec -- PROGRAMME ARGS…` : mêmes PROGRAMME et ARGS que l'originale.
            // L'enveloppe est un substitut qui n'exécute rien : elle est jugée sur les
            // arguments que le shell lui a donnés.
            assert_eq!(after[2], "--", "{after:?}");
            assert_eq!(
                &after[3..],
                &before[..],
                "arguments modifiés\n  originale : {original:?}\n  réécrite  : {:?}",
                report.rewritten
            );
        } else if after[0] == "lm-resizer" && after.get(1).map(String::as_str) == Some("read") {
            // `head -n COUNT FILE` devient `lm-resizer read FILE --head-lines COUNT`.
            assert_eq!(before[0], "head", "{before:?}");
            let (count, file) = (&before[2], &before[3]);
            assert_eq!(
                after[1..],
                [
                    "read".to_string(),
                    file.clone(),
                    "--head-lines".into(),
                    count.clone()
                ],
                "head mal converti\n  originale : {original:?}\n  réécrite  : {:?}",
                report.rewritten
            );
        } else {
            assert_eq!(after, before, "{original:?} -> {:?}", report.rewritten);
        }
    }
}

#[test]
fn backslash_quote_in_an_argument_cannot_close_the_quoting() {
    // Reproduction de l'audit du 08/10/2026 : en 0.2.5, `rewrite-shell` rendait
    // `lm-resizer read "\\";touch pwned;#" --head-lines 1` et `sh -c` créait `pwned`.
    let sandbox = Sandbox::new();
    let original = "head -n 1 '\\\" ;touch pwned;#'";
    let report = rewrite_shell_report(original);
    let (calls, files) = sandbox.run(&report.rewritten);
    assert!(
        !files.iter().any(|name| name == "pwned"),
        "injection : {} -> {files:?}",
        report.rewritten
    );
    assert_eq!(calls.len(), 1, "une seule commande attendue: {calls:?}");
    if report.changed {
        assert_eq!(
            calls[0],
            [
                "lm-resizer",
                "read",
                "\\\" ;touch pwned;#",
                "--head-lines",
                "1"
            ]
        );
    }
}

#[test]
fn trap_corpus_never_changes_what_the_shell_runs() {
    let sandbox = Sandbox::new();
    let traps = [
        "head -n 1 '\\\" ;touch pwned;#'",
        "head -n 1 \"\\\" ;touch pwned;#\"",
        "head -n 1 '\\'",
        "head -n 1 '\\\\'",
        "head -n 1 a\\ b",
        "head -n 1 \"a b\"",
        "head -n 1 \"a\\nb\"",
        "head -n 1 \"a\\\\nb\"",
        "head -n 1 'it'\\''s'",
        "head -n 1 \"it's\"",
        "head -n 1 \"$HOME/x\"",
        "head -n 1 $HOME",
        "head -n 1 ~/x",
        "head -n 1 *.rs",
        "head -n 1 {a,b}",
        "head -n 1 ''",
        "head -n 1 \"\"",
        "head -n 1 a'b'\"c\"",
        "head -n 1 x\ntouch pwned",
        "head -n 1 x \\\ntouch pwned",
        "head -n 1 x # touch pwned",
        "head -n 1 x#y",
        "head -n 1 'x';touch pwned",
        "head -n 1 \"x;touch pwned\"",
        "head -n 1 'x' && touch pwned",
        "head -n 1 \"$(touch pwned)\"",
        "head -n 1 '$(touch pwned)'",
        "head -n 1 `touch pwned`",
        "head -n 1 \\$(touch pwned)",
        "head -n 1 \"\\$(touch pwned)\"",
        "head -n 1 \"a\\\"; touch pwned; \\\"b\"",
        "cargo test \"a\\\"b\"",
        "cargo test 'a\"b'",
        "cargo test '\\\" ;touch pwned;#'",
        "cargo test \\\" ;touch pwned",
        "cargo test --features \"a b\" -- --nocapture",
        "git status '\\\" ;touch pwned;#'",
        "git log -1 --format=\"%h %s\"",
        "git log -1 \"--format=\\\"%h\\\"\"",
        "git status && cargo test '\\\" ;touch pwned;#'",
        "psql -c 'select 1' ; cargo test",
        "grep -rn \"a\\|b\" src",
        "grep -rn 'a\\|b' src",
        "ls *.md",
        "ls $HOME",
        "ls -la \"$HOME\"",
        "cargo test é ü 日本",
        "cargo test \\\\",
        "cargo test \\",
        "cargo test 'unterminated",
        "cargo test \"unterminated",
    ];
    let mut rewritten = 0;
    for trap in traps {
        assert_same_behaviour(&sandbox, trap);
        rewritten += usize::from(rewrite_shell_report(trap).changed);
    }
    // Garde contre un test vacueux : si plus rien n'était réécrit, rien ne serait jugé.
    assert!(rewritten >= 10, "seulement {rewritten} lignes réécrites");
}

#[test]
fn quoted_literals_survive_a_round_trip_through_the_shell() {
    let sandbox = Sandbox::new();
    let literals = [
        "plain",
        "with space",
        "it's",
        "\"",
        "\\",
        "\\\"",
        "\\\" ;touch pwned;#",
        "$(touch pwned)",
        "`touch pwned`",
        ";touch pwned",
        "a\nb",
        "$HOME",
        "*",
        "~",
        "é",
        "'",
        "''",
        "--flag=value with space",
        "#",
        "!",
    ];
    for literal in literals {
        let line = format!("head -n 1 {}", apostrophes(literal));
        assert_same_behaviour(&sandbox, &line);
        // Et la citation de production, jugée par le shell, redonne le même argument.
        let produced = shell_join(&["lm-resizer".into(), "read".into(), literal.into()]);
        let (calls, files) = sandbox.run(&produced);
        assert!(files.is_empty(), "{produced:?} -> {files:?}");
        assert_eq!(calls, [vec!["lm-resizer", "read", literal]], "{produced:?}");
        assert_eq!(
            posix_split(&shell_join(&[literal.into()])),
            Some(vec![literal.to_string()]),
            "{literal:?}"
        );
    }
}

#[test]
fn argument_vectors_are_rendered_without_double_quotes() {
    // `rewrite` (avec arguments déjà découpés) produit aussi une chaîne destinée à `sh`.
    let report = rewrite_command_report(&[
        "git".into(),
        "log".into(),
        "--format=%h \"x\" $HOME \\".into(),
    ]);
    let rewritten = report.rewritten.unwrap();
    assert!(
        !rewritten.contains('"') || rewritten.contains('\''),
        "{rewritten}"
    );
    let sandbox = Sandbox::new();
    let (calls, files) = sandbox.run(&rewritten);
    assert!(files.is_empty());
    assert_eq!(
        calls,
        [vec![
            "lm-resizer",
            "exec",
            "--",
            "git",
            "log",
            "--format=%h \"x\" $HOME \\"
        ]]
    );
}

/// Un chemin d'installation peut contenir `"`, `$(...)` ou un accent grave (le système de fichiers
/// l'accepte) : les scripts générés ne doivent jamais l'exécuter comme du shell.
fn executable_stub_in_hostile_dir() -> (tempfile::TempDir, String, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let work = root.path().join("work");
    fs::create_dir(&work).unwrap();
    let hostile = root
        .path()
        .join("we\"ird $(touch pwned-by-path) `touch pwned-by-tick` ';touch pwned-by-semicolon;'");
    fs::create_dir(&hostile).unwrap();
    let stub = hostile.join("lm-resizer");
    fs::write(&stub, "#!/bin/sh\nprintf 'stub-ran' > \"$LMR_MARK\"\n").unwrap();
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
    let path = stub.to_str().unwrap().to_string();
    (root, path, work)
}

fn run_script(script: &str, work: &Path, args: &[&str]) -> Vec<String> {
    let file = work.join("script.sh");
    fs::write(&file, script).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
    Process::new("/bin/sh")
        .arg(&file)
        .args(args)
        .current_dir(work)
        .env("LMR_MARK", work.join("mark"))
        .env_remove("LM_RESIZER_BIN")
        .output()
        .unwrap();
    let mut names = list(work);
    names.retain(|name| name != "script.sh");
    names
}

#[test]
fn generated_hook_helper_does_not_execute_a_hostile_install_path() {
    let (_root, exe, work) = executable_stub_in_hostile_dir();
    let files = run_script(&hook_rewrite_sh(&exe), &work, &["git status"]);
    assert_eq!(
        files,
        ["mark"],
        "le script a exécuté une partie du chemin : {files:?}"
    );
}

#[test]
fn generated_command_shim_does_not_execute_a_hostile_install_path() {
    let (_root, exe, work) = executable_stub_in_hostile_dir();
    // Le programme d'origine est lui aussi un chemin trouvé sur le disque.
    let files = run_script(&command_shim_sh(&exe, Path::new(&exe)), &work, &[]);
    assert_eq!(
        files,
        ["mark"],
        "le shim a exécuté une partie du chemin : {files:?}"
    );
}
