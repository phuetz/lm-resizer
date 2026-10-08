//! `--force` écrase le fichier entier : le refus le dit (D12 de la recette).
use std::process::Command;

#[test]
fn refusing_an_existing_hook_config_warns_that_force_overwrites_the_whole_file() {
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join(".claude/settings.json");
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    std::fs::write(&settings, "{\"permissions\":{\"deny\":[\"Bash(rm *)\"]}}\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", dir.path().join("state"))
        .args(["init-native-hooks", "--client", "claude", "--project-dir"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("already exists"), "{stderr}");
    assert!(stderr.contains("--force"), "{stderr}");
    assert!(
        stderr.contains("whole file") && stderr.contains("back it up"),
        "le message ne dit pas que --force écrase tout : {stderr}"
    );
    // Le fichier est resté intact.
    assert!(std::fs::read_to_string(&settings)
        .unwrap()
        .contains("Bash(rm *)"));
}
