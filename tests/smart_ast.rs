use std::path::Path;
use std::process::{Command, Output};

fn run(path: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .arg("smart")
        .arg(path)
        .args(args)
        .env(
            "LM_RESIZER_STORE",
            path.parent().unwrap().join("store.sqlite"),
        )
        .env(
            "LM_RESIZER_CODE_EXPLORER_BIN",
            "absent-smart-ast-test-producer",
        )
        .env("LM_RESIZER_NO_CODE_EXPLORER", "1")
        .output()
        .unwrap()
}

fn summarize(extension: &str, source: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(format!("source.{extension}"));
    std::fs::write(&path, source).unwrap();
    let output = run(&path, &["--ast"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn rust_lists_real_signatures_and_types() {
    let out = summarize("rs", include_str!("fixtures/smart/sample.rs"));
    for expected in [
        "L1 import use std::fmt;",
        "L3 const pub const LIMIT: usize",
        "L4 struct pub struct Counter",
        "L5 field pub value: usize",
        "L8 method pub fn add(&mut self, amount: usize) -> usize",
        "Incrémente le compteur.",
        "L12 trait pub trait Named",
        "L13 method fn name(&self) -> &str",
        "L15 type pub type Counts = Vec<usize>",
        "L16 fn pub async fn fetch<T: Send>(value: T) -> T",
    ] {
        assert!(out.contains(expected), "missing {expected}: {out}");
    }
    assert!(!out.contains("self.value +="));
    assert!(!out.contains("999"));
    assert!(!out.contains("textual_fake"));
}

#[test]
fn typescript_lists_methods_arrow_and_imports() {
    let out = summarize("ts", include_str!("fixtures/smart/sample.ts"));
    for expected in [
        "import { readFile } from 'node:fs'",
        "interface Config",
        "size: number",
        "class Worker",
        "run(input: string): Promise<number>",
        "Convertit une valeur.",
        "const convert = (value: number): string =>",
        "function load<T>(value: T): T",
        "const LIMIT: number",
    ] {
        assert!(out.contains(expected), "missing {expected}: {out}");
    }
    assert!(!out.contains("return Promise.resolve"));
    assert!(!out.contains("value.toString()"));
    assert!(!out.contains("textualFake"));
}

#[test]
fn javascript_lists_exported_function_and_class() {
    let out = summarize("js", include_str!("fixtures/smart/sample.js"));
    for expected in [
        "import fs from 'node:fs'",
        "class Reader",
        "read(path)",
        "export default function open(path)",
        "const transform = value =>",
        "Lit un chemin.",
    ] {
        assert!(out.contains(expected), "missing {expected}: {out}");
    }
    assert!(!out.contains("fs.readFileSync(path)"));
    assert!(!out.contains("value + 1"));
}

#[test]
fn python_lists_decorators_multiline_signatures_and_docstrings() {
    let out = summarize("py", include_str!("fixtures/smart/sample.py.txt"));
    for expected in [
        "from typing import Iterable",
        "LIMIT: int",
        "class Reader:",
        "@staticmethod",
        "def read( paths: Iterable[str], ) -> list[str]:",
        "Lit les chemins.",
        "async def fetch(value: str) -> str:",
        "Première ligne du module.",
    ] {
        assert!(out.contains(expected), "missing {expected}: {out}");
    }
    assert!(!out.contains("return list(paths)"));
    assert!(!out.contains("Seconde ligne"));
    assert!(!out.contains("textual_fake"));
}

#[test]
fn ast_is_deterministic_across_two_processes() {
    for (ext, src) in [
        ("rs", include_str!("fixtures/smart/sample.rs")),
        ("ts", include_str!("fixtures/smart/sample.ts")),
        ("js", include_str!("fixtures/smart/sample.js")),
        ("py", include_str!("fixtures/smart/sample.py.txt")),
    ] {
        assert_eq!(summarize(ext, src), summarize(ext, src));
    }
}

#[test]
fn invalid_and_unknown_files_announce_legacy_fallback() {
    for (ext, src) in [
        ("rs", "pub fn broken( {"),
        ("ts", "export function broken( {"),
        ("js", "function broken( {"),
        ("py", "def broken(:\n pass"),
        ("xyz", "plain unknown content\n"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("source.{ext}"));
        std::fs::write(&path, src).unwrap();
        let legacy = run(&path, &[]);
        let ast = run(&path, &["--ast"]);
        assert!(
            ast.status.success(),
            "{}",
            String::from_utf8_lossy(&ast.stderr)
        );
        assert!(ast.stderr.is_empty());
        let text = String::from_utf8(ast.stdout).unwrap();
        assert!(text.starts_with("[smart AST: repli "), "{text}");
        assert_eq!(text.split_once('\n').unwrap().1.as_bytes(), legacy.stdout);
    }
}
