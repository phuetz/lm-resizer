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

/// Formes relevées par les revues du 9 octobre 2026 (revue indépendante B1, contre-audit Grok) et
/// voisines. Chacune lance des tests ; à code 0, sa sortie doit passer intacte.
const RUNNER_FORMS: &[(&str, &str)] = &[
    ("cargo", "cargo test"),
    ("cargo", "cargo.cmd test"),
    ("cargo", "cargo.EXE test"),
    ("cargo", "cargo +nightly test"),
    ("cargo", "cargo --locked test"),
    ("cargo", "cargo t"),
    ("pytest", "pytest -q"),
    ("pytest", "pytest.cmd -q"),
    ("pytest", "py.test -q"),
    ("pytest", "python -m pytest"),
    ("pytest", "python3 -m pytest"),
    ("pytest", "python3.11 -m pytest -q"),
    ("pytest", "python3.12 -m pytest -q"),
    ("pytest", "python3.12.exe -m pytest"),
    ("pytest", "py -3 -m pytest"),
    ("pytest", "python -X dev -m pytest"),
    ("pytest", "pypy3 -m pytest"),
    ("pytest", "uv run pytest"),
    ("pytest", "uv run --frozen pytest"),
    ("pytest", "uv run python -m pytest"),
    ("pytest", "poetry run pytest"),
    ("js", "jest"),
    ("js", "jest.cmd"),
    ("js", "npx jest"),
    ("js", "npx.cmd jest"),
    ("js", "npx --no-install jest"),
    ("js", "npx -y jest"),
    ("js", "npx --yes -- jest"),
    ("js", "npx -p jest jest"),
    ("js", "npm exec jest"),
    ("js", "npm exec --yes jest"),
    ("js", "npm exec -- jest"),
    ("js", "pnpm exec jest"),
    ("js", "pnpm dlx jest"),
    ("js", "pnpm jest"),
    ("js", "yarn jest"),
    ("js", "yarn exec jest"),
    ("js", "bunx jest"),
    ("js", "npm run jest"),
    ("js", "npm run vitest"),
    ("js", "vitest run"),
    ("js", "npx vitest run"),
    ("js", "./node_modules/.bin/jest"),
    ("js", "npm test"),
    ("js", "npm --silent test"),
    ("js", "npm --prefix app test"),
    ("js", "npm run test:unit"),
];

fn success_transcript(family: &str) -> String {
    match family {
        "cargo" => cargo_success(),
        "pytest" => pytest_success(),
        _ => jest_success(),
    }
}

fn failure_transcript(family: &str) -> &'static str {
    match family {
        "cargo" => "\nrunning 2 tests\ntest a ... ok\ntest b ... FAILED\n\nfailures:\n\n---- b stdout ----\nthread 'b' panicked at src/lib.rs:9:5:\nassertion failed\n\nfailures:\n    b\n\ntest result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n\nerror: test failed, to rerun pass `--lib`\n",
        "pytest" => "============================= test session starts ==============================\ncollected 3 items\n\ntests/test_a.py .F.                                                      [100%]\n\n=================================== FAILURES ===================================\n_________________________________ test_reject __________________________________\n\n    def test_reject():\n>       assert status(0) == 422\nE       assert 200 == 422\n\ntests/test_a.py:9: AssertionError\n=========================== short test summary info ============================\nFAILED tests/test_a.py::test_reject - assert 200 == 422\n========================= 1 failed, 2 passed in 0.05s ==========================\n",
        _ => " FAIL  src/a.test.js\n  ● sums › adds\n\n    expect(received).toBe(expected)\n\n    Expected: 3\n    Received: 4\n\n      at Object.<anonymous> (src/a.test.js:3:17)\n\nTests:       1 failed, 2 passed, 3 total\nTest Suites: 1 failed, 1 total\n",
    }
}

fn tool_output(state: &Path, command: &str, code: &str, raw: &str) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", state)
        .args([
            "tool-output",
            "--json",
            "--command",
            command,
            "--exit-code",
            code,
        ])
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
    serde_json::from_slice(&out.stdout).unwrap()
}

/// Une seule reconnaissance des lanceurs de tests : toute forme qu'une vue de tests réduit en échec
/// sort intacte à code 0. Revue indépendante B1 (`npm exec --yes jest`, `npx --no-install jest`,
/// `cargo.cmd test`, `cargo.EXE test`, `pytest.cmd -q`, `npx.cmd jest`) et contre-audit Grok
/// (`python3.11 -m pytest`, `python3.12 -m pytest`) : à code 0, les avertissements disparaissaient.
#[test]
fn every_test_runner_form_is_raw_on_success_and_every_reduced_form_is_recognized() {
    let state = tempfile::tempdir().unwrap();
    let mut wrong = Vec::new();
    for (family, command) in RUNNER_FORMS {
        let success = success_transcript(family);
        let report = tool_output(state.path(), command, "0", &success);
        if report["filter"] != "lossless:test-success" || report["output"] != success.as_str() {
            wrong.push(format!(
                "code 0 : {command} -> {} {:?}",
                report["filter"], report["output"]
            ));
        }
        // Propriété : si une route réduit l'échec, la même forme est reconnue comme lanceur.
        let failure = failure_transcript(family);
        let failed = tool_output(state.path(), command, "1", failure);
        let view = failed["output"].as_str().unwrap();
        let raw_view = format!("[FAIL] Command failed (exit code: 1)\n{failure}");
        if view != failure && view != raw_view && report["filter"] != "lossless:test-success" {
            wrong.push(format!("réduit en échec sans être reconnu : {command}"));
        }
    }
    // Des commandes qui ne lancent pas de tests gardent leur route.
    for command in [
        "cargo build -p test",
        "cargo build",
        "npm install",
        "git status",
        "python3 -m http.server",
        "python3 script.py",
        "uv run ruff",
        "node node_modules/.bin/jest",
    ] {
        let report = tool_output(state.path(), command, "0", &cargo_success());
        if report["filter"] == "lossless:test-success" {
            wrong.push(format!("pas un lanceur de tests : {command}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Les formes de la revue B1, lancées pour de vrai par `exec` avec des doubles sur le `PATH`.
#[test]
fn launcher_forms_from_the_reviews_are_raw_on_success_through_exec() {
    let dir = tempfile::tempdir().unwrap();
    let cases: Vec<(&str, String, Vec<&str>)> = vec![
        ("npm", jest_success(), vec!["npm", "exec", "--yes", "jest"]),
        ("npx", jest_success(), vec!["npx", "--no-install", "jest"]),
        ("npx.cmd", jest_success(), vec!["npx.cmd", "jest"]),
        ("cargo.cmd", cargo_success(), vec!["cargo.cmd", "test"]),
        ("cargo.EXE", cargo_success(), vec!["cargo.EXE", "test"]),
        ("pytest.cmd", pytest_success(), vec!["pytest.cmd", "-q"]),
        (
            "python3.11",
            pytest_success(),
            vec!["python3.11", "-m", "pytest", "-q"],
        ),
        (
            "python3.12",
            pytest_success(),
            vec!["python3.12", "-m", "pytest", "-q"],
        ),
    ];
    let mut wrong = Vec::new();
    for (program, stdout, argv) in &cases {
        fake(dir.path(), program, stdout, 0);
        let (out, report) = exec_json(dir.path(), argv);
        if out.status.code() != Some(0)
            || report["output"] != stdout.as_str()
            || report["filter"] != "lossless:test-success"
        {
            wrong.push(format!(
                "{argv:?}: {} {:?}",
                report["filter"], report["output"]
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Même règle par l'outil MCP `lm_resizer_tool_output` (revue B1, preuve MCP).
#[test]
fn mcp_tool_output_keeps_a_successful_launcher_form_raw() {
    let state = tempfile::tempdir().unwrap();
    let raw = jest_success();
    let mut requests = format!(
        "{}\n",
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}})
    );
    for (id, command) in [(2, "npm exec --yes jest"), (3, "npx --no-install jest")] {
        requests.push_str(&format!(
            "{}\n",
            serde_json::json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{
                "name":"lm_resizer_tool_output",
                "arguments":{"content":raw,"command":command,"exit_code":0}}})
        ));
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", state.path())
        .args(["mcp", "--store"])
        .arg(state.path().join("ccr.sqlite3"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(requests.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let replies: Vec<Value> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    for id in [2, 3] {
        let reply = replies.iter().find(|r| r["id"] == id).expect("réponse MCP");
        let text = reply["result"]["content"][0]["text"].as_str().unwrap();
        let report: Value = serde_json::from_str(text).unwrap();
        assert_eq!(report["filter"], "lossless:test-success", "{id}: {report}");
        assert_eq!(report["output"], raw.as_str(), "{id}");
    }
}

/// Contre-audit Grok du 9 octobre : en échec, la vue `cargo test` ne gardait que les blocs
/// `---- … stdout ----` et le bilan ; le CVE, le `Permission denied`, un `warning:` et un `error:`
/// écrits hors des blocs disparaissaient. La vue ne retire plus que les lignes de grammaire connue
/// sans information (tests réussis, `running N tests`, lignes d'état de cargo, listes `failures:`).
#[test]
fn a_failed_cargo_test_keeps_every_line_outside_the_known_grammar() {
    let dir = tempfile::tempdir().unwrap();
    let raw = "   Compiling leak v0.1.0 (/tmp/leak)\n    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.31s\n     Running unittests src/main.rs (target/debug/deps/leak-1)\n\nrunning 2 tests\ntest ok ... ok\ntest leak ... FAILED\nPermission denied: cannot read /home/alice/.ssh/id_rsa\nCVE-2024-99999: token sk-live-SECRET left in target/debug\nwarning: something\nerror: boom outside the block\n\nfailures:\n\n---- leak stdout ----\nassertion left == right failed\n\nfailures:\n    leak\n\ntest result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n\nerror: test failed, to rerun pass `--bin leak`\n";
    fake(dir.path(), "cargo", raw, 101);
    let (out, report) = exec_json(dir.path(), &["cargo", "test"]);
    assert_eq!(out.status.code(), Some(101));
    assert_eq!(report["filter"], "native:cargo-test");
    let view = report["output"].as_str().unwrap();
    for kept in [
        "Permission denied: cannot read /home/alice/.ssh/id_rsa",
        "CVE-2024-99999: token sk-live-SECRET left in target/debug",
        "warning: something",
        "error: boom outside the block",
        "1. ---- leak stdout ----",
        "assertion left == right failed",
        "test result: FAILED. 1 passed; 1 failed",
        "error: test failed, to rerun pass `--bin leak`",
    ] {
        assert!(view.contains(kept), "{kept} absent :\n{view}");
    }
    for dropped in ["Compiling leak", "test ok ... ok", "running 2 tests"] {
        assert!(!view.contains(dropped), "{dropped} gardé :\n{view}");
    }
}
