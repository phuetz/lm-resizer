//! Un lanceur de tests qui réussit (code 0) rend sa sortie intacte : un test réussi peut écrire
//! n'importe quoi sur la sortie héritée (un `git log` relayé, un avertissement de sécurité) et la vue
//! des tests n'en garderait que le bilan. Un échec (code non nul) garde la vue réduite.
#![cfg(unix)]
use serde_json::Value;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};

/// Lignes qu'un test réussi peut faire afficher sans aucun drapeau, au milieu d'une sortie de lanceur.
const LEAK: &str = "Permission denied: cannot read /home/alice/.ssh/id_rsa\n\
CVE-2024-99999: token sk-live-SECRET left in target/debug\n\
insecure: TLS verification disabled\n\
1 file changed, 2 insertions(+)\n\
\n\
commit abc\n";

fn fake(dir: &Path, name: &str, stdout: &str, code: i32) {
    let body = dir.join(format!("{name}.out"));
    std::fs::write(&body, stdout).unwrap();
    let script = dir.join(name);
    std::fs::write(
        &script,
        format!("#!/bin/sh\ncat '{}'\nexit {code}\n", body.display()),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn path_with(dir: &Path) -> std::ffi::OsString {
    std::env::join_paths(
        std::iter::once(dir.to_path_buf())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap()
}

fn exec_json(dir: &Path, argv: &[&str]) -> (Output, Value) {
    let state = dir.join("state");
    let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", &state)
        .env("PATH", path_with(dir))
        .args(["exec", "--json", "--"])
        .args(argv)
        .output()
        .unwrap();
    let report: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|_| panic!("{argv:?}: {}", String::from_utf8_lossy(&out.stdout)));
    (out, report)
}

fn cargo_success() -> String {
    format!(
        "\nrunning 1 test\ntest relay ... ok\n{LEAK}\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n\n"
    )
}

fn pytest_success() -> String {
    format!(
        "============================= test session starts ==============================\ncollected 2 items\n\ntests/test_a.py ..                                                       [100%]\n{LEAK}============================== 2 passed in 0.03s ===============================\n"
    )
}

fn jest_success() -> String {
    format!(" PASS  src/a.test.js\n  ✓ relays (3 ms)\n{LEAK}\nTests:       1 passed, 1 total\nTest Suites: 1 passed, 1 total\n")
}

fn go_json_success() -> String {
    format!(
        "{{\"Action\":\"run\",\"Package\":\"p\",\"Test\":\"TestA\"}}\n{{\"Action\":\"output\",\"Package\":\"p\",\"Test\":\"TestA\",\"Output\":\"{}\"}}\n{{\"Action\":\"pass\",\"Package\":\"p\",\"Test\":\"TestA\"}}\n{{\"Action\":\"pass\",\"Package\":\"p\"}}\n",
        "CVE-2024-99999: token sk-live-SECRET\\n"
    )
}

fn nextest_success() -> String {
    format!("    Starting 1 test across 1 binary\n        PASS [   0.010s] relay tests::relay\n{LEAK}------------\n     Summary [   0.011s] 1 test run: 1 passed, 0 skipped\n")
}

fn dotnet_success() -> String {
    format!("  Determining projects to restore...\n{LEAK}Passed!  - Failed:     0, Passed:     3, Skipped:     0, Total:     3, Duration: 12 ms - a.dll (net8.0)\n")
}

#[test]
fn successful_test_runners_return_the_raw_output_byte_for_byte() {
    let dir = tempfile::tempdir().unwrap();
    let cases: Vec<(&str, String, Vec<&str>)> = vec![
        ("cargo", cargo_success(), vec!["cargo", "test"]),
        ("cargo", cargo_success(), vec!["cargo", "test", "--quiet"]),
        ("cargo", nextest_success(), vec!["cargo", "nextest", "run"]),
        ("cargo", cargo_success(), vec!["sh", "-c", "cargo test"]),
        ("pytest", pytest_success(), vec!["pytest", "-q"]),
        ("python3", pytest_success(), vec!["python3", "-m", "pytest"]),
        ("jest", jest_success(), vec!["jest"]),
        ("npx", jest_success(), vec!["npx", "jest"]),
        ("vitest", jest_success(), vec!["vitest", "run"]),
        ("go", go_json_success(), vec!["go", "test", "./..."]),
        ("dotnet", dotnet_success(), vec!["dotnet", "test"]),
        (
            "mvn",
            format!("[INFO] Scanning\n{LEAK}[INFO] BUILD SUCCESS\n"),
            vec!["mvn", "-q", "test"],
        ),
        (
            "gradle",
            format!("> Task :test\n{LEAK}BUILD SUCCESSFUL in 2s\n"),
            vec!["gradle", "test"],
        ),
    ];
    let mut failures = Vec::new();
    for (program, stdout, argv) in &cases {
        fake(dir.path(), program, stdout, 0);
        let (out, report) = exec_json(dir.path(), argv);
        let output = report["output"].as_str().unwrap_or_default();
        if out.status.code() != Some(0)
            || output != stdout.as_str()
            || report["filter"] != "lossless:test-success"
        {
            failures.push(format!(
                "{argv:?}: code {:?}, filter {}, view {:?}",
                out.status.code(),
                report["filter"],
                output
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_failed_test_run_keeps_the_reduced_view_and_its_code() {
    let dir = tempfile::tempdir().unwrap();
    let failing = "\nrunning 2 tests\ntest a ... ok\ntest b ... FAILED\n\nfailures:\n\n---- b stdout ----\nthread 'b' panicked at src/lib.rs:9:5:\nassertion `left == right` failed\n  left: 1\n right: 2\n\nfailures:\n    b\n\ntest result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n\nerror: test failed, to rerun pass `--lib`\n";
    fake(dir.path(), "cargo", failing, 101);
    let (out, report) = exec_json(dir.path(), &["cargo", "test"]);
    assert_eq!(out.status.code(), Some(101));
    assert_eq!(report["filter"], "native:cargo-test");
    let view = report["output"].as_str().unwrap();
    assert!(
        view.starts_with("FAILURES (1):\n1. ---- b stdout ----\n"),
        "{view}"
    );
    assert!(view.contains("assertion `left == right` failed"), "{view}");
    assert!(
        view.contains("error: test failed, to rerun pass `--lib`"),
        "{view}"
    );
}

#[test]
fn tool_output_and_pipe_follow_the_exit_code_they_are_given() {
    let state = tempfile::tempdir().unwrap();
    let raw = cargo_success();
    let run = |args: &[&str]| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("LM_RESIZER_STATE_DIR", state.path())
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(raw.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        let report: Value = serde_json::from_slice(&out.stdout).unwrap();
        (
            report["filter"].as_str().unwrap().to_string(),
            report["output"].as_str().unwrap().to_string(),
        )
    };
    let (filter, output) = run(&[
        "tool-output",
        "--json",
        "--command",
        "cargo test",
        "--exit-code",
        "0",
    ]);
    assert_eq!(
        (filter.as_str(), output.as_str()),
        ("lossless:test-success", raw.as_str())
    );
    let (filter, output) = run(&["pipe", "--json", "--filter", "cargo-test"]);
    assert_eq!(
        (filter.as_str(), output.as_str()),
        ("lossless:test-success", raw.as_str())
    );
    let (filter, output) = run(&[
        "tool-output",
        "--json",
        "--command",
        "cargo test",
        "--exit-code",
        "3",
    ]);
    assert_eq!(filter, "native:cargo-test");
    assert_ne!(output, raw);
}

fn git(dir: &Path, args: &[&str]) -> Vec<u8> {
    let out = Command::new("git")
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    out.stdout
}

/// Contre-revue du 9 octobre 2026 sur `de2ff41` : un vrai test réussi écrit `git log --format=%s`
/// par `std::io::stdout().write_all`, sans drapeau d'affichage ; `cargo test` émet l'historique et la
/// vue n'en gardait que `1 passed, 0 failed`. Le code 0 rend la sortie intacte, entrée vide comprise.
#[test]
fn a_real_passing_cargo_test_writing_git_log_to_stdout_keeps_every_entry() {
    let qa = tempfile::tempdir().unwrap();
    let repo = qa.path().join("repo");
    let harness = qa.path().join("harness");
    std::fs::create_dir_all(harness.join("src")).unwrap();
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.name", "Revue"]);
    git(&repo, &["config", "user.email", "revue@example.test"]);
    for message in [
        "origine",
        "1 file changed, 2 insertions(+)",
        "chemin.txt | 12 ++++----",
        "image.png | Bin 0 -> 512 bytes",
    ] {
        git(&repo, &["commit", "-q", "--allow-empty", "-m", message]);
    }
    git(
        &repo,
        &[
            "commit",
            "-q",
            "--allow-empty",
            "--allow-empty-message",
            "-m",
            "",
        ],
    );
    std::fs::write(
        harness.join("Cargo.toml"),
        "[package]\nname = \"relais-direct\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[workspace]\n",
    )
    .unwrap();
    std::fs::write(
        harness.join("src/lib.rs"),
        r#"#[test]
fn relay() {
    use std::io::Write;
    let out = std::process::Command::new("git")
        .current_dir(std::env::var("LOG_REPO").unwrap())
        .args(["log", "--format=%s"])
        .output()
        .unwrap();
    assert!(out.status.success());
    std::io::stdout().write_all(&out.stdout).unwrap();
}
"#,
    )
    .unwrap();
    let direct = String::from_utf8(git(&repo, &["log", "--format=%s"])).unwrap();
    assert!(
        direct.starts_with("\nimage.png | Bin 0 -> 512 bytes\n"),
        "{direct:?}"
    );
    let manifest = harness.join("Cargo.toml");
    let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", qa.path().join("state"))
        .env("LOG_REPO", &repo)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("CARGO_TARGET_DIR", qa.path().join("target"))
        .env("RUSTC_WRAPPER", "")
        .env("CARGO_NET_OFFLINE", "true")
        .env_remove("RUST_TEST_NOCAPTURE")
        .args([
            "exec",
            "--json",
            "--",
            "cargo",
            "test",
            "--quiet",
            "--offline",
        ])
        .arg("--manifest-path")
        .arg(&manifest)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    let view = report["output"].as_str().unwrap();
    assert_eq!(report["filter"], "lossless:test-success", "{view}");
    assert!(view.contains(&direct), "{view:?}");
}
