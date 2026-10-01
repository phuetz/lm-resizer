use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(root: &std::path::Path, args: &[&str], input: &str) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_lm-resizer"));
    command
        .args(args)
        .current_dir(root)
        .env("LM_RESIZER_STATE_DIR", root.join("state"))
        .env_remove("LM_RESIZER_NO_TOML_FILTERS")
        .env_remove("LM_RESIZER_TRUST_PROJECT_FILTERS")
        .env_remove("LM_RESIZER_STORE")
        .env("LM_RESIZER_TRACKING", "1")
        .env("LM_RESIZER_TEE", "0")
        .env("LM_RESIZER_FILTERS", root.join("filters.toml"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn malformed_custom_filters_keep_builtin_filtering() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("filters.toml"), "invalid toml = [").unwrap();
    let raw = format!(
        "{}\nwarning: container restarted\n",
        "routine progress\n".repeat(300)
    );
    let output = run(
        root.path(),
        &["tool-output", "--command", "docker logs app", "--json"],
        &raw,
    );
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["filter"], "toml:docker-logs");
    assert!(String::from_utf8_lossy(&output.stderr).contains("custom filters ignored"));
    assert!(report["output"]
        .as_str()
        .unwrap()
        .contains("warning: container restarted"));
}

#[test]
fn trust_rejects_filters_without_tests_or_with_duplicates() {
    let root = tempfile::tempdir().unwrap();
    let definition = "[[filters]]\nname = 'example'\nmatch_command = '^example'\n";
    let inline = "[[tests]]\nfilter = 'example'\nname = 'sample'\ninput = 'signal'\nexpected = 'signal\\n'\n".replace("'signal\\n'", "\"signal\\n\"");
    for content in [
        definition.to_string(),
        format!("{definition}{definition}{inline}"),
    ] {
        let path = root.path().join("filters.toml");
        std::fs::write(&path, content).unwrap();
        let output = run(
            root.path(),
            &["trust-filters", "--path", path.to_str().unwrap(), "--json"],
            "",
        );
        assert!(!output.status.success(), "unverified filters were trusted");
        assert!(!root.path().join("state/trusted-filters.json").exists());
    }
}

#[test]
fn explicit_filter_override_still_precedes_native_vitest() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("filters.toml"), "[[filters]]\nname = 'vitest-custom'\nmatch_command = '^vitest'\nkeep_lines_matching = ['^CUSTOM']\n").unwrap();
    let output = run(
        root.path(),
        &["tool-output", "--command", "vitest run", "--json"],
        "CUSTOM useful signal\nnoise\n",
    );
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["filter"], "toml:vitest-custom");
    assert!(report["output"]
        .as_str()
        .unwrap()
        .contains("CUSTOM useful signal"));
}

#[cfg(unix)]
#[test]
fn persisted_history_removes_command_credentials() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("filters.toml"), "").unwrap();
    let output = run(
        root.path(),
        &[
            "exec",
            "--",
            "sh",
            "-c",
            "printf hello",
            "--token",
            "transient-history-value",
        ],
        "",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let history = std::fs::read_to_string(root.path().join("state/exec-history.jsonl")).unwrap();
    assert!(!history.contains("transient-history-value"));
    assert!(history.contains("<redacted>"));
}

#[test]
fn named_custom_override_replaces_builtin_definition() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("filters.toml"), "[[filters]]\nname = 'docker-logs'\nmatch_command = '^unrelated'\nkeep_lines_matching = ['^CUSTOM']\n").unwrap();
    let output = run(
        root.path(),
        &["tool-output", "--command", "docker logs app", "--json"],
        "routine\nwarning: container restarted\n",
    );
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["filter"], "generic");
}
