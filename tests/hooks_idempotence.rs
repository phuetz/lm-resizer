//! install-hooks must be idempotent; uninstall-hooks must reverse install cleanly.
#![cfg(unix)]
use std::process::Command;

fn cli(dir: &std::path::Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lm-resizer"));
    cmd.env("HOME", dir.join("home"))
        .env("LM_RESIZER_STATE_DIR", dir.join("state"))
        .env("LM_RESIZER_STORE", dir.join("store.sqlite"));
    cmd
}

#[test]
fn install_hooks_is_idempotent_and_uninstall_removes_helpers() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("AGENTS.md"), "# Project\n\nKeep this.\n").unwrap();

    let first = cli(dir.path())
        .args(["install-hooks", "--client", "codex", "--project-dir"])
        .arg(&project)
        .args(["--json"])
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "first install: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let agents_after_install = std::fs::read_to_string(project.join("AGENTS.md")).unwrap();
    assert!(agents_after_install.contains("<!-- LM-RESIZER:HOOKS:START -->"));
    assert!(agents_after_install.contains("Keep this."));
    let helper = project.join(".lm-resizer/hooks/rewrite.sh");
    assert!(helper.is_file());
    let helper_body = std::fs::read_to_string(&helper).unwrap();

    let second = cli(dir.path())
        .args(["install-hooks", "--client", "codex", "--project-dir"])
        .arg(&project)
        .args(["--json"])
        .output()
        .unwrap();
    assert!(
        second.status.success(),
        "second install must be idempotent without --force: status={:?} stderr={}",
        second.status.code(),
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(project.join("AGENTS.md")).unwrap(),
        agents_after_install,
        "idempotent install must not duplicate the marked block"
    );
    assert_eq!(std::fs::read_to_string(&helper).unwrap(), helper_body);

    let uninstall = cli(dir.path())
        .args(["uninstall-hooks", "--client", "codex", "--project-dir"])
        .arg(&project)
        .args(["--json"])
        .output()
        .unwrap();
    assert!(
        uninstall.status.success(),
        "uninstall: {}",
        String::from_utf8_lossy(&uninstall.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&uninstall.stdout).unwrap();
    assert_eq!(report["removed"], 1);
    let agents_after = std::fs::read_to_string(project.join("AGENTS.md")).unwrap();
    assert!(!agents_after.contains("<!-- LM-RESIZER:HOOKS:START -->"));
    assert!(agents_after.contains("Keep this."));
    assert!(
        !helper.exists(),
        "clean uninstall must remove generated helpers"
    );
    assert!(
        !project.join(".lm-resizer/hooks").exists(),
        "hooks helper directory must be gone after clean uninstall"
    );

    let uninstall_again = cli(dir.path())
        .args(["uninstall-hooks", "--client", "codex", "--project-dir"])
        .arg(&project)
        .args(["--json"])
        .output()
        .unwrap();
    assert!(
        uninstall_again.status.success(),
        "second uninstall must stay idempotent: {}",
        String::from_utf8_lossy(&uninstall_again.stderr)
    );
    let report2: serde_json::Value = serde_json::from_slice(&uninstall_again.stdout).unwrap();
    assert_eq!(report2["removed"], 0);

    let reinstall = cli(dir.path())
        .args(["install-hooks", "--client", "codex", "--project-dir"])
        .arg(&project)
        .args(["--json"])
        .output()
        .unwrap();
    assert!(
        reinstall.status.success(),
        "reinstall after clean uninstall must work without --force: {}",
        String::from_utf8_lossy(&reinstall.stderr)
    );
    assert!(project.join(".lm-resizer/hooks/rewrite.sh").is_file());
}

#[test]
fn native_init_is_idempotent_when_content_matches_and_uninstall_removes_config() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();

    for client in ["gemini", "copilot", "cursor"] {
        let first = cli(dir.path())
            .args(["init", "--client", client, "--project-dir"])
            .arg(&project)
            .args(["--json"])
            .output()
            .unwrap();
        assert!(
            first.status.success(),
            "{client} first init: {}",
            String::from_utf8_lossy(&first.stderr)
        );
        let rel = match client {
            "gemini" => ".gemini/settings.json",
            "copilot" => ".github/hooks/lm-resizer.json",
            _ => ".cursor/hooks.json",
        };
        let path = project.join(rel);
        let original = std::fs::read_to_string(&path).unwrap();
        assert!(original.contains("hook --client"));

        let second = cli(dir.path())
            .args(["init", "--client", client, "--project-dir"])
            .arg(&project)
            .args(["--json"])
            .output()
            .unwrap();
        assert!(
            second.status.success(),
            "{client} second init must succeed when content already matches: status={:?} stderr={}",
            second.status.code(),
            String::from_utf8_lossy(&second.stderr)
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);

        let uninstall = cli(dir.path())
            .args(["uninstall-hooks", "--client", client, "--project-dir"])
            .arg(&project)
            .args(["--json"])
            .output()
            .unwrap();
        assert!(
            uninstall.status.success(),
            "{client} uninstall native: {}",
            String::from_utf8_lossy(&uninstall.stderr)
        );
        assert!(
            !path.exists(),
            "{client} clean uninstall must remove native hook config {}",
            path.display()
        );
    }
}

#[test]
fn install_hooks_refuses_native_only_clients_without_writing() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();

    for client in ["gemini", "copilot", "cursor"] {
        let out = cli(dir.path())
            .args(["install-hooks", "--client", client, "--project-dir"])
            .arg(&project)
            .output()
            .unwrap();
        assert!(!out.status.success(), "{client} install-hooks must fail");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(&format!("init-native-hooks --client {client}")),
            "{client} stderr: {stderr}"
        );
        assert!(
            !project.join(".lm-resizer").exists(),
            "{client} must not write helpers"
        );
        assert!(!project.join("AGENTS.md").exists());
        assert!(!project.join("CLAUDE.md").exists());
    }
}
