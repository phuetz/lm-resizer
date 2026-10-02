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
