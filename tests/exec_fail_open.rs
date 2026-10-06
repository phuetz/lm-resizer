//! `exec` and the hook must stay out of the way: when the state directory is
//! not writable (read-only HOME, sandboxed agent, full disk) the wrapped command
//! still runs, its output and exit code pass through, and one warning line is
//! the only trace.
// Unix only: the scenario is a 0555 directory (`chmod`), whose semantics Windows
// ACLs do not reproduce, so these tests are not compiled there.
#![cfg(unix)]
use serde_json::Value;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const WARNING: &str = "lm-resizer: état non inscriptible (";

struct ReadOnlyHome {
    _dir: tempfile::TempDir,
    home: PathBuf,
}

/// A HOME whose mode is 555. Returns `None` when the process can write there
/// anyway (running as root), because the scenario cannot be reproduced.
fn read_only_home() -> Option<ReadOnlyHome> {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir(&home).unwrap();
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o555)).unwrap();
    if std::fs::write(home.join("probe"), b"x").is_ok() {
        eprintln!("skipped: the process can write to a 0555 directory (root?)");
        return None;
    }
    Some(ReadOnlyHome { _dir: dir, home })
}

fn lmr(home: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lm-resizer"));
    cmd.env("HOME", home)
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env_remove("LM_RESIZER_STATE_DIR")
        .env_remove("LM_RESIZER_STORE")
        .env_remove("LM_RESIZER_TEE")
        .env_remove("LM_RESIZER_TRACKING")
        .env_remove("LOCALAPPDATA")
        .env_remove("USERPROFILE");
    cmd
}

fn exec(home: &Path, extra: &[&str], command: &[&str]) -> Output {
    let mut cmd = lmr(home);
    cmd.arg("exec").args(extra).arg("--").args(command);
    cmd.output().unwrap()
}

fn warnings(out: &Output) -> usize {
    String::from_utf8_lossy(&out.stderr)
        .matches(WARNING)
        .count()
}

fn describe(out: &Output) -> String {
    format!(
        "code={:?}\nstdout={}\nstderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn read_only_home_still_runs_the_command() {
    let Some(ro) = read_only_home() else { return };
    let out = exec(&ro.home, &[], &["echo", "bonjour"]);
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "bonjour\n",
        "{}",
        describe(&out)
    );
    assert_eq!(warnings(&out), 1, "{}", describe(&out));
}

#[test]
fn read_only_home_keeps_the_exit_code() {
    let Some(ro) = read_only_home() else { return };
    let out = exec(&ro.home, &[], &["sh", "-c", "exit 3"]);
    assert_eq!(out.status.code(), Some(3), "{}", describe(&out));
    assert_eq!(warnings(&out), 1, "{}", describe(&out));
}

#[test]
fn read_only_home_big_output_is_reduced_without_a_dangling_archive_pointer() {
    let Some(ro) = read_only_home() else { return };
    for code in [0, 5] {
        let script = format!("seq 1 4000; echo 'error: boom' >&2; exit {code}");
        let out = exec(&ro.home, &[], &["sh", "-c", &script]);
        assert_eq!(out.status.code(), Some(code), "{}", describe(&out));
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("error: boom"), "{}", describe(&out));
        // Nothing may point at an archive that was never written.
        assert!(!stdout.contains("[tee:"), "{}", describe(&out));
        assert!(!stdout.contains("[raw:"), "{}", describe(&out));
        assert!(!stdout.contains("hash="), "{}", describe(&out));
        assert_eq!(warnings(&out), 1, "{}", describe(&out));
    }
}

#[test]
fn read_only_home_covers_raw_on_failure_stream_and_json() {
    let Some(ro) = read_only_home() else { return };
    let out = exec(
        &ro.home,
        &["--raw-on-failure"],
        &["sh", "-c", "echo oops; exit 4"],
    );
    assert_eq!(out.status.code(), Some(4), "{}", describe(&out));
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("oops"),
        "{}",
        describe(&out)
    );
    assert_eq!(warnings(&out), 1, "{}", describe(&out));

    let out = exec(&ro.home, &["--stream"], &["sh", "-c", "echo flux; exit 6"]);
    assert_eq!(out.status.code(), Some(6), "{}", describe(&out));
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("flux"),
        "{}",
        describe(&out)
    );
    assert_eq!(warnings(&out), 1, "{}", describe(&out));

    let out = exec(&ro.home, &["--json"], &["echo", "bonjour"]);
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["exit_code"], 0);
    assert!(report["output"].as_str().unwrap().contains("bonjour"));
    assert_eq!(warnings(&out), 1, "{}", describe(&out));
}

#[test]
fn read_only_home_covers_native_commands() {
    let Some(ro) = read_only_home() else { return };
    let work = tempfile::tempdir().unwrap();
    std::fs::write(work.path().join("a.txt"), "needle\n").unwrap();
    let out = lmr(&ro.home)
        .current_dir(work.path())
        .args(["grep", "needle", "a.txt"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("needle"),
        "{}",
        describe(&out)
    );
    assert_eq!(warnings(&out), 1, "{}", describe(&out));
}

/// The sandbox case from Codex: the state directory exists but SQLite cannot
/// open its database there (`unable to open database file`, code 14).
#[test]
fn existing_read_only_state_dir_still_runs_the_command() {
    let Some(ro) = read_only_home() else { return };
    let out = lmr(&ro.home)
        .env("LM_RESIZER_STATE_DIR", &ro.home)
        .args(["exec", "--", "sh", "-c", "echo bonjour; exit 3"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3), "{}", describe(&out));
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("bonjour"),
        "{}",
        describe(&out)
    );
    assert_eq!(warnings(&out), 1, "{}", describe(&out));
}

#[test]
fn writable_state_gives_no_warning() {
    let state = tempfile::tempdir().unwrap();
    let out = lmr(state.path())
        .env("LM_RESIZER_STATE_DIR", state.path())
        .args(["exec", "--", "echo", "bonjour"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    assert_eq!(warnings(&out), 0, "{}", describe(&out));
}

/// The Codex PreToolUse hook either leaves the command alone or rewrites it
/// into an `exec` that works when the state is read-only.
#[test]
fn codex_pretooluse_hook_does_not_break_the_command() {
    let Some(ro) = read_only_home() else { return };
    let work = tempfile::tempdir().unwrap();
    std::fs::write(work.path().join("README.md"), "# titre\nligne\n").unwrap();
    let event = serde_json::json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": "cat README.md"},
    });
    let mut hook = lmr(&ro.home)
        .args(["hook", "--client", "codex", "--event", "PreToolUse"])
        .current_dir(work.path())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        hook.stdin
            .take()
            .unwrap()
            .write_all(event.to_string().as_bytes())
            .unwrap();
    }
    let out = hook.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", describe(&out));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let command = if stdout.trim().is_empty() {
        "cat README.md".to_string()
    } else {
        let reply: Value = serde_json::from_str(&stdout).unwrap();
        reply["hookSpecificOutput"]["updatedInput"]["command"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let ran = Command::new("sh")
        .arg("-c")
        .arg(&command)
        .current_dir(work.path())
        .env("HOME", &ro.home)
        .env("XDG_DATA_HOME", ro.home.join(".local/share"))
        .env("XDG_STATE_HOME", ro.home.join(".local/state"))
        .env_remove("LM_RESIZER_STATE_DIR")
        .env_remove("LM_RESIZER_STORE")
        .output()
        .unwrap();
    assert_eq!(
        ran.status.code(),
        Some(0),
        "command={command}\n{}",
        describe(&ran)
    );
    assert!(
        String::from_utf8_lossy(&ran.stdout).contains("# titre"),
        "command={command}\n{}",
        describe(&ran)
    );
}
