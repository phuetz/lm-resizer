//! L'exemple de « See it work » est une sortie réelle : la commande du README, rejouée, la redonne.
use std::process::Command;

#[test]
fn first_readme_example_is_the_real_output() {
    for readme in ["README.md", "README.fr.md"] {
        let text =
            std::fs::read_to_string(format!("{}/{readme}", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let start = text.find("~~~console\n$ ").expect("bloc console") + "~~~console\n".len();
        let block = &text[start..text[start..].find("~~~").unwrap() + start];
        let (command, expected) = block.split_once('\n').unwrap();
        let command = command.strip_prefix("$ lm-resizer ").unwrap();
        // `tool-output --command 'cargo test' --exit-code 101 --input <fichier>` : pas de shell,
        // découpage à la main. Le code est celui d'un `cargo test` en échec : un code 0 rendrait la
        // sortie d'un lanceur de tests intacte.
        let (head, input) = command.split_once(" --input ").unwrap();
        assert_eq!(head, "tool-output --command 'cargo test' --exit-code 101");
        let state = tempfile::tempdir().unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env("LM_RESIZER_STATE_DIR", state.path())
            .env("LM_RESIZER_TRACKING", "0")
            .args([
                "tool-output",
                "--command",
                "cargo test",
                "--exit-code",
                "101",
                "--input",
                input.trim(),
            ])
            .output()
            .unwrap();
        assert!(out.stderr.is_empty(), "{readme}: {out:?}");
        assert_eq!(String::from_utf8(out.stdout).unwrap(), expected, "{readme}");
    }
}
