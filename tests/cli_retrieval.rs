//! CLI regression for the original bytes advertised by a source banner or
//! by the first JSON cache key. Override LM_RESIZER_TEST_BINARY to audit an
//! older or packaged executable with the same fixtures and assertions.
use std::path::PathBuf;
use std::process::{Command, Output};

struct IsolatedCli {
    binary: PathBuf,
    root: tempfile::TempDir,
}

impl IsolatedCli {
    fn new() -> Self {
        let home = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("_qa/release/home");
        std::fs::create_dir_all(&home).unwrap();
        let root = tempfile::Builder::new()
            .prefix("cli-retrieval-")
            .tempdir_in(&home)
            .unwrap();
        let binary = std::env::var_os("LM_RESIZER_TEST_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_lm-resizer")));
        Self { binary, root }
    }

    fn run(&self, args: &[&str]) -> Output {
        let home = self.root.path();
        Command::new(&self.binary)
            .args(args)
            .current_dir(home)
            .env("HOME", home)
            .env("USERPROFILE", home)
            .env("XDG_CONFIG_HOME", home.join("config"))
            .env("XDG_STATE_HOME", home.join("state"))
            .env("CODEX_HOME", home.join("codex"))
            .env("LOCALAPPDATA", home.join("local"))
            .env("LM_RESIZER_STATE_DIR", home.join("state"))
            .env_remove("LM_RESIZER_STORE")
            .output()
            .unwrap()
    }

    fn compress(&self, name: &str, input: &[u8], json: bool) -> Output {
        let path = self.root.path().join(name);
        std::fs::write(&path, input).unwrap();
        let mut args = vec!["compress", "--input", path.to_str().unwrap()];
        if json {
            args.push("--json");
        }
        let output = self.run(&args);
        assert!(output.status.success(), "{:?}", output);
        output
    }

    fn assert_original(&self, key: &str, original: &[u8]) {
        assert_eq!(key.len(), 24, "expected the displayed bare CCR hash");
        let retrieved = self.run(&["retrieve", key]);
        let identical = retrieved.stdout == original;
        println!(
            "hash={key}; original={} bytes; retrieved={} bytes; exit={:?}; identical={identical}",
            original.len(),
            retrieved.stdout.len(),
            retrieved.status.code()
        );
        assert!(
            retrieved.status.success(),
            "retrieve failed: {}",
            String::from_utf8_lossy(&retrieved.stderr)
        );
        assert!(identical, "retrieve did not return the original bytes");
    }
}

#[test]
fn source_banner_hash_recovers_original_bytes() {
    let cli = IsolatedCli::new();
    let input = include_bytes!("../src/main.rs");
    let output = cli.compress("source.rs", input, false);
    assert!(
        output.stdout.len() < input.len(),
        "source was not shortened"
    );
    let text = String::from_utf8(output.stdout).unwrap();
    let banner = "Retrieve full source: hash=";
    let key = text
        .split_once(banner)
        .expect("source output must advertise its recovery hash")
        .1
        .chars()
        .take_while(|c| c.is_ascii_hexdigit())
        .collect::<String>();
    cli.assert_original(&key, input);
}

#[test]
fn json_first_displayed_hash_recovers_bytes_before_minification() {
    let cli = IsolatedCli::new();
    let rows: Vec<_> = (0..80)
        .map(|id| serde_json::json!({"id": id, "status": "ok", "score": 100, "note": "été"}))
        .collect();
    // Include accents and a final CRLF to distinguish byte equality
    // from a semantically equivalent minified JSON document.
    let input = format!("{}\r\n", serde_json::to_string_pretty(&rows).unwrap());
    let output = cli.compress("rows.json", input.as_bytes(), true);
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report["bytes_saved"].as_u64().unwrap() > 0);
    let key = report["cache_keys"][0]
        .as_str()
        .expect("JSON output must display its original recovery hash");
    cli.assert_original(key, input.as_bytes());
}

#[test]
fn every_displayed_log_hash_recovers_the_unmodified_crlf_input() {
    let cli = IsolatedCli::new();
    let input = (0..500)
        .map(|n| format!("INFO événement {n} C:\\projet été\\résumé.txt\r\n"))
        .collect::<String>()
        + "ERROR sentinel\r\n";
    let output = cli.compress("events.txt", input.as_bytes(), true);
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report["bytes_saved"].as_u64().unwrap() > 0);
    for key in report["cache_keys"].as_array().unwrap() {
        cli.assert_original(key.as_str().unwrap(), input.as_bytes());
    }
    let view = report["output"].as_str().unwrap();
    assert!(view.contains("hash="));
    for part in view.split("hash=").skip(1) {
        let key: String = part.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
        cli.assert_original(&key, input.as_bytes());
    }
}

#[test]
fn tool_output_recovery_keys_refer_to_input_before_command_filtering() {
    let cli = IsolatedCli::new();
    let input = (0..500)
        .map(|n| format!("INFO événement {n} C:\\projet été\\résumé.txt\r\n"))
        .collect::<String>()
        + "ERROR sentinel\r\n";
    let source = cli.root.path().join("journal.txt");
    std::fs::write(&source, &input).unwrap();
    let output = cli.run(&[
        "tool-output",
        "--json",
        "--command",
        "journalctl",
        "--input",
        source.to_str().unwrap(),
    ]);
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let keys = report["cache_keys"].as_array().unwrap();
    assert!(!keys.is_empty());
    for key in keys {
        cli.assert_original(key.as_str().unwrap(), input.as_bytes());
    }
}
