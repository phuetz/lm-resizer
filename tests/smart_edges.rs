use std::process::Command;

fn ast(extension: &str, source: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(format!("source.{extension}"));
    std::fs::write(&path, source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .arg("smart")
        .arg(path)
        .arg("--ast")
        .env("LM_RESIZER_STORE", dir.path().join("store.sqlite"))
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn rust_keeps_module_docs_foreign_signatures_and_unicode_positions() {
    let out = ast("rs", "//! Première ligne du module.\r\n//! Seconde ligne ignorée.\r\n\r\nextern \"C\" {\r\n    pub fn lire(ptr: *const u8) -> usize;\r\n}\r\n/// Documente une fonction.\r\npub fn été(valeur: &str) -> &str { valeur }\r\n");
    for expected in [
        "Première ligne du module.",
        "extern \"C\"",
        "L5 fn pub fn lire(ptr: *const u8) -> usize",
        "L8 fn pub fn été(valeur: &str) -> &str",
    ] {
        assert!(out.contains(expected), "missing {expected}: {out}");
    }
    assert!(!out.contains("Seconde ligne ignorée"));
    assert!(!out.contains("{ valeur }"));
}

#[test]
fn javascript_class_arrow_fields_keep_their_parameters() {
    let out = ast("js", "export class Calc {\n  // Calcule une valeur.\n  handler = (n, factor = 2) => { return n * factor; };\n  other = function(value) { return value + 1; };\n}\n");
    assert!(out.contains("handler = (n, factor = 2) =>"), "{out}");
    assert!(out.contains("other = function(value)"), "{out}");
    assert!(out.contains("Calcule une valeur."));
    assert!(!out.contains("return"));
}

#[test]
fn python_keeps_type_aliases_and_annotated_class_fields() {
    let out = ast("py", "type Names[T] = list[T]\nclass Reader:\n    name: str\n    count: int = 7\n    def read(self, value: str = 'two  words') -> str:\n        return value\n");
    for expected in [
        "L1 type type Names[T] = list[T]",
        "L3 field name: str",
        "L4 field count: int",
        "L5 method def read",
        "value: str = 'two  words'",
    ] {
        assert!(out.contains(expected), "missing {expected}: {out}");
    }
    assert!(!out.contains("= 7"));
    assert!(!out.contains("return value"));
}

#[test]
fn python_module_doc_uses_its_real_line_number() {
    let out = ast(
        "py",
        "#!/usr/bin/env python3\n\n\"\"\"Documente le module.\"\"\"\n",
    );
    assert!(out.contains("L3 doc Documente le module."), "{out}");
}

#[test]
fn rust_bom_and_shebang_do_not_shift_signature_offsets() {
    for (source, expected) in [
        (
            "\u{feff}pub fn été(x: &str) -> &str { x }\n",
            "L1 fn pub fn été(x: &str) -> &str",
        ),
        (
            "#!/usr/bin/env rust-script\n\npub fn été(x: &str) -> &str { x }\n",
            "L3 fn pub fn été(x: &str) -> &str",
        ),
    ] {
        let out = ast("rs", source);
        assert!(out.contains(expected), "{out}");
        assert!(!out.contains("{ x }"));
    }
}

#[test]
fn typescript_keeps_block_docs_and_source_order() {
    let out = ast("ts", "/**\n * Additionne deux nombres.\n * Ligne suivante ignorée.\n */\nexport function add(a: number, b: number): number { return a + b; }\nexport enum State { Ready = 1, Done }\nexport interface Later { act(value: string): void; }\n");
    assert!(out.contains("Additionne deux nombres."), "{out}");
    assert!(!out.contains("Ligne suivante ignorée"));
    for expected in [
        "function add(a: number, b: number): number",
        "Ready",
        "Done",
        "act(value: string): void",
    ] {
        assert!(out.contains(expected), "missing {expected}: {out}");
    }
    let mut previous = 0;
    for line in out.lines().skip(1) {
        let number: usize = line
            .strip_prefix('L')
            .unwrap()
            .split_once(' ')
            .unwrap()
            .0
            .parse()
            .unwrap();
        assert!(number >= previous, "ordre incorrect : {out}");
        previous = number;
    }
    assert!(!out.contains("return a + b"));
}
