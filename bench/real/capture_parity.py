#!/usr/bin/env python3
"""Additional real command captures for the RTK parity corpus; no services needed."""
import argparse
import json
import os
from pathlib import Path
import subprocess


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--work", type=Path, required=True)
    p.add_argument("--pytest", type=Path, required=True)
    p.add_argument("--vitest", type=Path)
    p.add_argument("--node-bin", type=Path)
    args = p.parse_args()
    work = args.work.resolve()
    work.mkdir(parents=True, exist_ok=False)
    project = work / "project"
    project.mkdir()
    (project / "Cargo.toml").write_text('[package]\nname="parity-capture"\nversion="0.1.0"\nedition="2021"\n[workspace]\n')
    (project / "src").mkdir()
    (project / "src/lib.rs").write_text('''pub fn total() -> i32 { 42 }
#[test] fn succeeds() { assert_eq!(total(), 42); }
#[test] fn invoice_total() { assert_eq!(total(), 43); }
''')
    (project / "test_invoice.py").write_text('''def test_pass():
    assert 1 == 1
def test_invoice_total():
    assert 42 == 43
''')
    (project / "invoice.vitest.test.js").write_text("import {test,expect} from 'vitest';\ntest('invoice total',()=>expect(42).toBe(43));\n")
    (project / "invoice.jest.test.js").write_text("test('invoice total',()=>expect(42).toBe(43));\n")
    (project / "numbers.ts").write_text('const invoice: string = 42;\n')
    (project / "lint.js").write_text('const unused_invoice = 42;\nmissing_invoice();\n')
    (project / ".eslintrc.json").write_text('{"env":{"es6":true},"rules":{"no-undef":"error","no-unused-vars":"error"}}')
    (project / "package.json").write_text('{"private":true,"jest":{"testMatch":["**/*.jest.test.js"]}}')
    if args.vitest:
        (project / "node_modules").symlink_to(args.vitest.absolute().parent.parent, target_is_directory=True)
    env = dict(os.environ, CARGO_TARGET_DIR=str(work / "cargo-target"),
               CARGO_TERM_COLOR="never", NO_COLOR="1", LC_ALL="C")
    for cmd in (["git", "init", "-q"], ["git", "config", "user.name", "Contributor"],
                ["git", "config", "user.email", "contributor@example.test"],
                ["git", "add", "Cargo.toml"], ["git", "add", "src/lib.rs"],
                ["git", "add", "test_invoice.py"], ["git", "commit", "-qm", "Initial fixture"]):
        subprocess.run(cmd, cwd=project, env=env, check=True, capture_output=True)
    (project / "src/lib.rs").write_text((project / "src/lib.rs").read_text() + "// changed\n")
    commands = [
        ("fresh-git-log", "log", ["git", "log", "-1"]),
        ("fresh-git-diff", "diff", ["git", "diff"]),
        ("fresh-git-status", "status", ["git", "status", "--porcelain=v1", "-b"]),
        ("fresh-git-show", "show", ["git", "show", "--stat"]),
        ("fresh-rg", "grep", ["rg", "-n", "assert", "src"]),
        ("fresh-find", "find", ["find", "src", "-type", "f"]),
        ("fresh-ls", "ls", ["ls", "-lh", "src"]),
        ("fresh-tree", "tree", ["tree", "src"]),
        ("fresh-cargo-build", "build", ["cargo", "build"]),
        ("fresh-cargo-test", "test", ["cargo", "test", "--", "--test-threads=1"]),
        ("fresh-pytest", "test", [str(args.pytest.resolve()), "-q", "test_invoice.py"]),
    ]
    if args.vitest:
        commands.append(("fresh-vitest", "test", [str(args.vitest.absolute()), "run", "invoice.vitest.test.js", "--maxWorkers=1", "--minWorkers=1"]))
    if args.node_bin:
        for name, flags in [("tsc", ["--noEmit", "--pretty", "false", "numbers.ts"]),
                            ("eslint", ["--no-color", "lint.js"]),
                            ("jest", ["--runInBand", "--no-colors", "invoice.jest.test.js"])]:
            commands.append(("fresh-" + name, "test", [str(args.node_bin.absolute() / name), *flags]))
    cases, unavailable = [], []
    for name, kind, command in commands:
        try:
            result = subprocess.run(command, cwd=project, env=env, capture_output=True, timeout=120)
        except FileNotFoundError:
            unavailable.append(command[0])
            continue
        cap = work / "captures" / name
        cap.mkdir(parents=True)
        (cap / "stdout").write_bytes(result.stdout)
        (cap / "stderr").write_bytes(result.stderr)
        if name in ("fresh-pytest", "fresh-vitest", "fresh-tsc", "fresh-eslint", "fresh-jest"):
            command[0] = name.removeprefix("fresh-")
        cases.append(dict(id=name, repo="fixture", kind=kind, command=command, exit_code=result.returncode))
    (work / "captures.json").write_text(json.dumps(cases, indent=2))
    (work / "unavailable.json").write_text(json.dumps(unavailable))
    print(f"{len(cases)} captures; unavailable: {unavailable}")


if __name__ == "__main__":
    main()
