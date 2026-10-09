//! Une désinstallation « all » retire tout ce que les installations ont écrit : les crochets natifs
//! des cinq clients et l'entrée MCP de chaque client, sans toucher au reste des fichiers.
use serde_json::{json, Value};
use std::path::Path;
use std::process::{Command, Output};

fn cli(home: &Path, project: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .current_dir(project)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("CODEX_HOME", home.join(".codex"))
        .env("CLAUDE_CONFIG_DIR", home.join(".claude"))
        .env("LM_RESIZER_STATE_DIR", home.join("state"))
        .args(args)
        .output()
        .unwrap()
}

fn files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in std::fs::read_dir(&next).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path.strip_prefix(dir).unwrap().display().to_string());
            }
        }
    }
    out.sort();
    out
}

/// Audit du 9 octobre 2026 : `uninstall-hooks --client all` laissait `.cursor/hooks.json` (le
/// crochet qui répondait `allow`), et rien ne retirait l'entrée MCP.
#[test]
fn uninstall_all_removes_every_native_hook_and_every_mcp_entry() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let (home, project) = (home.path(), project.path());
    let dir = project.to_str().unwrap();
    for client in ["cursor", "gemini", "copilot", "all"] {
        let out = cli(
            home,
            project,
            &[
                "init-native-hooks",
                "--client",
                client,
                "--project-dir",
                dir,
            ],
        );
        assert!(out.status.success(), "{client}: {out:?}");
    }
    // Contenu d'autrui, à garder : un autre serveur MCP et une autre table Codex.
    std::fs::write(
        project.join(".mcp.json"),
        json!({"mcpServers": {"other": {"command": "other-server"}}}).to_string(),
    )
    .unwrap();
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    std::fs::write(
        home.join(".codex/config.toml"),
        "model = \"o4\"\n\n[mcp_servers.other]\ncommand = \"other-server\"\n",
    )
    .unwrap();
    let out = cli(
        home,
        project,
        &[
            "install",
            "--client",
            "all",
            "--scope",
            "project",
            "--project-dir",
            dir,
        ],
    );
    assert!(out.status.success(), "{out:?}");

    let out = cli(
        home,
        project,
        &["uninstall-hooks", "--client", "all", "--project-dir", dir],
    );
    assert!(out.status.success(), "{out:?}");
    let left: Vec<_> = [
        ".cursor/hooks.json",
        ".gemini/settings.json",
        ".github/hooks/lm-resizer.json",
        ".codex/hooks.json",
        ".claude/settings.json",
    ]
    .into_iter()
    .filter(|hook| project.join(hook).exists())
    .collect();
    assert!(left.is_empty(), "crochets restants : {left:?}");
    let out = cli(
        home,
        project,
        &[
            "uninstall",
            "--client",
            "all",
            "--scope",
            "project",
            "--project-dir",
            dir,
        ],
    );
    assert!(out.status.success(), "{out:?}");

    // Seuls restent le fichier d'autrui, réduit à son contenu d'origine, et la config Codex sans
    // la table lm-resizer.
    assert_eq!(files(project), vec![".mcp.json".to_string()]);
    let mcp: Value =
        serde_json::from_str(&std::fs::read_to_string(project.join(".mcp.json")).unwrap()).unwrap();
    assert_eq!(
        mcp,
        json!({"mcpServers": {"other": {"command": "other-server"}}})
    );
    let codex = std::fs::read_to_string(home.join(".codex/config.toml")).unwrap();
    assert!(!codex.contains("lm_resizer"), "{codex}");
    assert!(
        codex.contains("[mcp_servers.other]") && codex.contains("model = \"o4\""),
        "{codex}"
    );

    // Un second passage ne trouve plus rien et réussit.
    let again = cli(
        home,
        project,
        &[
            "uninstall",
            "--client",
            "all",
            "--scope",
            "project",
            "--project-dir",
            dir,
        ],
    );
    assert!(again.status.success(), "{again:?}");
    assert_eq!(files(project), vec![".mcp.json".to_string()]);
}

/// Une configuration de crochet modifiée à la main n'est pas retirée (la règle d'origine reste).
#[test]
fn uninstall_all_keeps_a_hand_edited_cursor_hook() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let (home, project) = (home.path(), project.path());
    let dir = project.to_str().unwrap();
    std::fs::create_dir_all(project.join(".cursor")).unwrap();
    std::fs::write(
        project.join(".cursor/hooks.json"),
        "{\"version\":1,\"hooks\":{}}",
    )
    .unwrap();
    let out = cli(
        home,
        project,
        &["uninstall-hooks", "--client", "all", "--project-dir", dir],
    );
    assert!(out.status.success(), "{out:?}");
    assert!(project.join(".cursor/hooks.json").exists());
}

/// Contre-audit Grok du 9 octobre : un crochet installé par un binaire et désinstallé par une
/// copie à un autre chemin restait en place (`Removed 0`), avec sa commande vers le premier chemin.
/// Le fichier est reconnu s'il est identique au contenu généré pour le chemin qu'il contient.
#[cfg(unix)]
#[test]
fn uninstall_from_another_binary_path_removes_the_generated_hooks() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let (home, project) = (home.path(), project.path());
    let dir = project.to_str().unwrap();
    for client in ["cursor", "gemini", "copilot", "all"] {
        let out = cli(
            home,
            project,
            &[
                "init-native-hooks",
                "--client",
                client,
                "--project-dir",
                dir,
            ],
        );
        assert!(out.status.success(), "{client}: {out:?}");
    }
    let copy = home.join("ailleurs").join("lm-resizer");
    std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_lm-resizer"), &copy).unwrap();
    let out = Command::new(&copy)
        .current_dir(project)
        .env("HOME", home)
        .env("LM_RESIZER_STATE_DIR", home.join("state"))
        .args(["uninstall-hooks", "--client", "all", "--project-dir", dir])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let left: Vec<_> = [
        ".cursor/hooks.json",
        ".gemini/settings.json",
        ".github/hooks/lm-resizer.json",
        ".codex/hooks.json",
        ".claude/settings.json",
    ]
    .into_iter()
    .filter(|hook| project.join(hook).exists())
    .collect();
    assert!(left.is_empty(), "crochets restants : {left:?}");
}

/// Même contre-audit : installer en portée globale puis désinstaller en portée projet laissait
/// `~/.mcp.json` et `~/.cursor/mcp.json`. `--scope all` retire les deux portées.
#[test]
fn uninstall_with_scope_all_removes_project_and_global_entries() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let (home, project) = (home.path(), project.path());
    let dir = project.to_str().unwrap();
    for (client, scope) in [
        ("claude", "global"),
        ("cursor", "global"),
        ("all", "project"),
    ] {
        let out = cli(
            home,
            project,
            &[
                "install",
                "--client",
                client,
                "--scope",
                scope,
                "--project-dir",
                dir,
            ],
        );
        assert!(out.status.success(), "{client} {scope}: {out:?}");
    }
    let out = cli(
        home,
        project,
        &[
            "uninstall",
            "--client",
            "all",
            "--scope",
            "all",
            "--project-dir",
            dir,
        ],
    );
    assert!(out.status.success(), "{out:?}");
    let mut left = files(project);
    left.extend(
        files(home)
            .into_iter()
            .filter(|path| !path.starts_with("state"))
            .map(|path| format!("~/{path}")),
    );
    assert!(left.is_empty(), "restent : {left:?}");
}
