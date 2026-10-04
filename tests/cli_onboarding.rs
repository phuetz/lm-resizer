//! First-contact regressions. Can also audit a release executable by setting
//! LM_RESIZER_TEST_BINARY, with no access to the user's configuration.
use std::path::PathBuf;
use std::process::{Command, Output};

struct IsolatedCli {
    home: tempfile::TempDir,
    binary: PathBuf,
}

impl IsolatedCli {
    fn new() -> Self {
        Self {
            home: tempfile::tempdir().unwrap(),
            binary: std::env::var_os("LM_RESIZER_TEST_BINARY")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_lm-resizer"))),
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(&self.binary)
            .args(args)
            .env_clear()
            .env("HOME", self.home.path())
            .env("USERPROFILE", self.home.path())
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("RUST_BACKTRACE", "1")
            .current_dir(self.home.path())
            .output()
            .unwrap()
    }

    fn assert_readable_error(&self, args: &[&str], message: &str) {
        let output = self.run(args);
        assert_eq!(output.status.code(), Some(1));
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains(message), "{stderr}");
        assert!(!stderr.contains("Stack backtrace:"), "{stderr}");
        assert!(!stderr.contains("stack backtrace:"), "{stderr}");
        assert!(!stderr.contains("panicked at"), "{stderr}");
    }
}

#[test]
fn missing_input_has_no_rust_backtrace() {
    IsolatedCli::new().assert_readable_error(
        &["compress", "--input", "absent.log"],
        "could not read absent.log",
    );
}

#[test]
fn missing_recovery_has_no_rust_backtrace() {
    let cli = IsolatedCli::new();
    cli.assert_readable_error(&["retrieve", "introuvable"], "CCR entry not found");
    cli.assert_readable_error(&["tee", "read", "absent.log"], "matched 0 files");
}

#[test]
fn stats_separates_measurements_with_empty_history() {
    assert_stats_legacy(&IsolatedCli::new(), 0, 0);
}

#[test]
fn stats_separates_measurements_from_legacy_history() {
    let cli = IsolatedCli::new();
    let state = cli.home.path().join("lm-resizer");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(
        state.join("exec-history.jsonl"),
        r#"{"command":"git log","filter":"git_log","original_bytes":200,"compressed_bytes":80,"bytes_saved":120}
"#,
    )
    .unwrap();
    assert_stats_legacy(&cli, 1, 120);
}

fn assert_stats_legacy(cli: &IsolatedCli, commands: u64, bytes: u64) {
    let output = cli.run(&["stats"]);
    assert!(output.status.success(), "{output:?}");
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let history = &report["exec_history"];
    assert_eq!(history["commands"], commands);
    assert_eq!(history["bytes_saved"], bytes);
    assert_eq!(history["estimated_tokens_saved"], bytes / 4);
    assert_eq!(history["tokenizer"], "tiktoken-rs/o200k_base");
    assert_eq!(history["measured_commands"], 0);
    assert_eq!(history["unmeasured_commands"], commands);
    assert_eq!(history["tokens_saved"], 0);
    assert_eq!(
        history["estimation_method"],
        "legacy bytes_saved / 4; unmeasured records only"
    );

    let markdown = cli.run(&["stats", "--markdown"]);
    assert!(markdown.status.success());
    let text = String::from_utf8(markdown.stdout).unwrap();
    assert!(
        text.contains("Legacy estimated tokens saved (bytes / 4, unmeasured records only)"),
        "{text}"
    );
    assert!(text.contains("exact text count"), "{text}");
    assert!(text.contains("0 measured"), "{text}");
}

#[test]
fn install_all_project_skips_codex() {
    let cli = IsolatedCli::new();
    let project_dir = cli.home.path().join("my_project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let output = cli.run(&[
        "install",
        "--client",
        "all",
        "--scope",
        "project",
        "--project-dir",
        project_dir.to_str().unwrap(),
    ]);

    assert!(output.status.success(), "{:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(
        stdout.contains(
            "Codex skipped: its MCP config is user-scoped (use --client codex --scope global)"
        ),
        "Missing Codex skipped message in stdout: {}",
        stdout
    );

    let codex_config = cli.home.path().join(".codex/config.toml");
    assert!(!codex_config.exists(), "Codex config should not be created");
}
