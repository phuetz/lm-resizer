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
                .env_remove("CODEX_HOME")
                .env_remove("CLAUDE_CONFIG_DIR")
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
                .env_remove("CODEX_HOME")
                .env_remove("CLAUDE_CONFIG_DIR")
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
        .env_remove("CODEX_HOME")
        .env_remove("CLAUDE_CONFIG_DIR")
        .current_dir(root.path())
        .args(["init", "--gemini"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "{broken");
    assert!(!root.path().join("GEMINI.md").exists());
}

#[test]
fn plugin_clients_install_and_preserve_foreign_files() {
    for (agent, file) in [
        ("opencode", ".config/opencode/plugins/lm-resizer.ts"),
        ("pi", ".pi/agent/extensions/lm-resizer.ts"),
        ("omp", ".omp/agent/extensions/lm-resizer.ts"),
        ("hermes", ".hermes/plugins/lm-resizer-rewrite/__init__.py"),
    ] {
        let root = tempfile::tempdir().unwrap();
        let run = |extra: &[&str]| {
            Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
                .env("HOME", root.path())
                .env_remove("CODEX_HOME")
                .env_remove("CLAUDE_CONFIG_DIR")
                .env_remove("XDG_CONFIG_HOME")
                .env_remove("HERMES_HOME")
                .current_dir(root.path())
                .args(["init", "--agent", agent, "--global"])
                .args(extra)
                .output()
                .unwrap()
        };
        assert!(run(&["--dry-run"]).status.success());
        let path = root.path().join(file);
        assert!(!path.exists());
        let out = run(&[]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let bytes = std::fs::read(&path).unwrap();
        assert!(!bytes.is_empty());
        assert!(run(&[]).status.success());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        std::fs::write(&path, "user changes").unwrap();
        assert!(!run(&["--uninstall"]).status.success());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "user changes");
        std::fs::write(&path, bytes).unwrap();
        assert!(run(&["--uninstall"]).status.success());
        assert!(!path.exists());
    }
}

#[test]
fn legacy_guidance_and_additive_opencode_flags_have_distinct_effects() {
    let root = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("HOME", root.path())
            .env_remove("CODEX_HOME")
            .env_remove("CLAUDE_CONFIG_DIR")
            .env_remove("XDG_CONFIG_HOME")
            .current_dir(root.path())
            .args(args)
            .output()
            .unwrap()
    };
    assert!(run(&["init", "--claude-md"]).status.success());
    assert!(root.path().join("CLAUDE.md").exists());
    assert!(!root.path().join(".claude/settings.json").exists());
    assert!(run(&["init", "--opencode"]).status.success());
    assert!(root.path().join(".claude/settings.json").exists());
    assert!(root.path().join(".opencode/plugins/lm-resizer.ts").exists());
    assert!(run(&["init", "--opencode", "--uninstall"]).status.success());
    assert!(!root.path().join(".opencode/plugins/lm-resizer.ts").exists());
}

#[test]
fn vibe_toml_install_and_uninstall_preserve_personal_content() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".vibe")).unwrap();
    let path = root.path().join(".vibe/hooks.toml");
    std::fs::write(&path, "# personal comment\n").unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("HOME", root.path())
            .current_dir(root.path())
            .args(["init", "--agent", "vibe", "--global"])
            .args(args)
            .output()
            .unwrap()
    };
    assert!(run(&[]).status.success());
    let content = std::fs::read_to_string(&path).unwrap();
    let parsed: toml::Value = toml::from_str(&content).unwrap();
    assert_eq!(
        parsed["hooks"][0]["command"].as_str(),
        Some("lm-resizer hook vibe")
    );
    assert!(run(&[]).status.success());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
    assert!(run(&["--uninstall"]).status.success());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "# personal comment\n"
    );
}

#[test]
fn codex_global_install_respects_explicit_home_override() {
    let root = tempfile::tempdir().unwrap();
    let custom = root.path().join("custom-codex");
    let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("HOME", root.path())
        .env("CODEX_HOME", &custom)
        .current_dir(root.path())
        .args(["init", "--codex", "--global"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(custom.join("hooks.json").exists());
    assert!(custom.join("AGENTS.md").exists());
    assert!(!root.path().join(".codex").exists());
}

#[test]
fn plugin_upgrade_accepts_owned_old_bytes_but_preserves_manual_edits() {
    use sha2::{Digest, Sha256};
    let root = tempfile::tempdir().unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("HOME", root.path())
            .current_dir(root.path())
            .args(["init", "--agent", "pi"])
            .output()
            .unwrap()
    };
    assert!(run().status.success());
    let path = root.path().join(".pi/extensions/lm-resizer.ts");
    let current = std::fs::read(&path).unwrap();
    let old = "// previous managed version\n";
    std::fs::write(&path, old).unwrap();
    std::fs::write(
        path.with_file_name("lm-resizer.ts.lm-resizer.sha256"),
        format!("{:x}\n", Sha256::digest(old.as_bytes())),
    )
    .unwrap();
    assert!(run().status.success());
    assert_eq!(std::fs::read(&path).unwrap(), current);
    std::fs::write(&path, "manual changes").unwrap();
    assert!(!run().status.success());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "manual changes");
}

#[test]
fn vibe_rejects_incompatible_hooks_without_mutation_and_repairs_legacy_block() {
    const BLOCK: &str = "\n# lm-resizer hook begin\n[[hooks]]\nname = \"lm-resizer-rewrite\"\ntype = \"pre_tool\"\nmatch = \"bash\"\ncommand = \"lm-resizer hook vibe\"\n# lm-resizer hook end\n";
    for original in [
        "# personal\n[hooks]\nenabled = true\n",
        "hooks = true\n",
        "hooks = []\n",
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(".vibe/hooks.toml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, original).unwrap();
        let run = |args: &[&str]| {
            Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
                .env("HOME", root.path())
                .current_dir(root.path())
                .args(["init", "--agent", "vibe", "--global"])
                .args(args)
                .output()
                .unwrap()
        };
        let out = run(&[]);
        assert!(
            !out.status.success(),
            "incompatible TOML must be refused: {original}"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        assert!(!root.path().join(".vibe/prompts/lm-resizer.md").exists());
        std::fs::write(&path, format!("{original}{BLOCK}")).unwrap();
        assert!(
            run(&["--uninstall"]).status.success(),
            "must repair exact legacy block"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        let _: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    }
}

#[test]
fn vibe_preserves_existing_array_and_refuses_modified_or_duplicate_owned_blocks() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join(".vibe/hooks.toml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original = "# audit\n[[hooks]]\nname = 'personal'\ncommand = 'audit'\n";
    std::fs::write(&path, original).unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("HOME", root.path())
            .current_dir(root.path())
            .args(["init", "--agent", "vibe", "--global"])
            .args(args)
            .output()
            .unwrap()
    };
    assert!(run(&[]).status.success());
    let installed = std::fs::read_to_string(&path).unwrap();
    let parsed: toml::Value = toml::from_str(&installed).unwrap();
    assert_eq!(parsed["hooks"].as_array().unwrap().len(), 2);
    let block = installed.strip_prefix(original).unwrap();
    for invalid in [
        installed.replace("lm-resizer hook vibe", "personal command"),
        format!("{installed}{block}"),
    ] {
        std::fs::write(&path, &invalid).unwrap();
        assert!(!run(&["--uninstall"]).status.success());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), invalid);
    }
}

#[test]
fn hermes_preserves_yaml_comments_order_and_other_plugin_entries() {
    for original in [
        "# personal settings\nmodel: 'custom' # keep model\nplugins:\n  enabled: # keep list\n    - other # keep plugin\n# end\n",
        "# flow style\nplugins: {enabled: [other]} # keep flow\nmodel: custom\n",
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(".hermes/config.yaml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, original).unwrap();
        let run = |args: &[&str]| Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("HOME", root.path()).env_remove("HERMES_HOME").current_dir(root.path())
            .args(["init", "--agent", "hermes", "--global"]).args(args).output().unwrap();
        for args in [&[][..], &["--uninstall"][..]] {
            let out = run(args);
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
            let content = std::fs::read_to_string(&path).unwrap();
            for comment in original.lines().filter_map(|line| line.split_once('#').map(|(_, c)| c)) {
                assert!(content.contains(&format!("#{comment}")), "lost comment {comment}: {content}");
            }
            assert_eq!(content.find("model:").unwrap() < content.find("plugins:").unwrap(), original.find("model:").unwrap() < original.find("plugins:").unwrap());
            let data: serde_yaml::Value = serde_yaml::from_str(&content).unwrap();
            assert_eq!(data["model"], "custom");
            let enabled = data["plugins"]["enabled"].as_sequence().unwrap();
            assert!(enabled.contains(&serde_yaml::Value::String("other".into())));
            assert_eq!(enabled.len(), if args.is_empty() { 2 } else { 1 });
        }
    }
}
