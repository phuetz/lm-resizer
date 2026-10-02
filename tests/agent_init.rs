use std::process::Command;
#[test]
fn rules_clients_dry_run_install_uninstall_in_isolated_home() {
    for (agent, file) in [
        ("windsurf", ".windsurfrules"),
        ("cline", ".clinerules"),
        ("roo", ".roo/rules/lm-resizer.md"),
        ("kilocode", ".kilocode/rules/lm-resizer.md"),
        ("antigravity", ".agents/rules/lm-resizer.md"),
        ("kimi", "AGENTS.md"),
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "personal\r\n").unwrap();
        let run = |extra: &[&str]| {
            let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
                .env("HOME", root.path())
                .current_dir(root.path())
                .args(["init", "--agent", agent])
                .args(extra)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        run(&["--dry-run"]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "personal\r\n");
        run(&[]);
        let installed = std::fs::read(&path).unwrap();
        run(&[]);
        assert_eq!(std::fs::read(&path).unwrap(), installed);
        run(&["--uninstall"]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "personal\r\n");
    }
}

#[test]
fn native_clients_merge_preserve_and_remove_only_owned_hooks() {
    for (agent, file, event, flat) in [
        ("claude", ".claude/settings.json", "PreToolUse", false),
        ("codex", ".codex/hooks.json", "PreToolUse", false),
        ("gemini", ".gemini/settings.json", "BeforeTool", false),
        ("cursor", ".cursor/hooks.json", "preToolUse", true),
        ("trae", ".trae/hooks.json", "PreToolUse", false),
        (
            "copilot",
            ".copilot/hooks/lm-resizer.json",
            "PreToolUse",
            true,
        ),
        ("droid", ".factory/hooks.json", "PreToolUse", false),
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let hook = if flat {
            serde_json::json!({"command":"personal-hook"})
        } else {
            serde_json::json!({"matcher":"Bash","hooks":[{"command":"personal-hook"}]})
        };
        let existing = serde_json::json!({"custom":42,"hooks":{event:[hook]}}).to_string();
        std::fs::write(&path, &existing).unwrap();
        let run = |args: &[&str]| {
            let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
                .env("HOME", root.path())
                .current_dir(root.path())
                .args(["init", "--agent", agent, "--global"])
                .args(args)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        run(&["--dry-run"]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), existing);
        run(&[]);
        let installed = std::fs::read_to_string(&path).unwrap();
        run(&[]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), installed);
        assert!(installed.contains("personal-hook"));
        assert!(installed.contains(&format!("lm-resizer hook {agent}")));
        run(&["--uninstall"]);
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["custom"], 42);
        assert_eq!(v["hooks"][event].as_array().unwrap().len(), 1);
    }
}

#[test]
fn malformed_config_aborts_without_writing_guidance() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".gemini")).unwrap();
    let path = root.path().join(".gemini/settings.json");
    std::fs::write(&path, "{broken").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("HOME", root.path())
        .current_dir(root.path())
        .args(["init", "--gemini"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "{broken");
    assert!(!root.path().join("GEMINI.md").exists());
}
