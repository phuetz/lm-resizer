#!/usr/bin/env python3
"""Pinned real-command captures, exact token counts and exhaustive fact oracles.

Capture once, replay the identical streams through exec before/after. Large
artifacts stay outside git. A missing fact or exit-code change is a failing run.
"""
import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import statistics
import subprocess
import time

import tiktoken

HERE = Path(__file__).resolve().parent
ENCODINGS = {name: tiktoken.get_encoding(name) for name in ("cl100k_base", "o200k_base")}


def cases():
    result = []
    for repo, file, test in [
        ("ripgrep", "crates/core/flags/defs.rs", ["cargo", "test"]),
        ("fastapi", "docs/en/docs/release-notes.md", ["pytest", "-q"]),
        ("TypeScript", "tsc/testdata/fixtures/compiler/checker.ts", ["npm", "test"]),
    ]:
        for kind, command in [
            ("log", ["git", "log", "-20"]),
            ("diff", ["git", "diff", "HEAD~1"]),
            ("status", ["git", "status"]),
            ("show", ["git", "show", "--stat", "HEAD"]),
            ("ls", ["ls", "-la"]),
            ("recursive", ["ls", "-R"]),
            ("find", ["find", "."]),
            ("grep", ["grep", "-rn", "--exclude-dir=.git", "TODO", "."]),
            ("cat", ["cat", file]),
            ("test", test),
        ]:
            result.append(dict(id=f"{repo}-{kind}", repo=repo, kind=kind, command=command))
    for ident, repo, kind, command in [
        ("compare-log", "ripgrep", "log", ["git", "log", "-n", "40"]),
        ("compare-diff", "ripgrep", "diff", ["git", "diff", "HEAD~20"]),
        ("compare-grep", "fastapi", "grep", ["grep", "-rn", "async def", "fastapi"]),
        ("compare-find", "fastapi", "find", ["find", ".", "-name", "*.py"]),
        ("compare-cat", "TypeScript", "cat", ["cat", "tsc/internal/bundled/libs/lib.dom.d.ts"]),
    ]:
        result.append(dict(id=ident, repo=repo, kind=kind, command=command))
    return result


def invoke(command, cwd=None, env=None, data=None):
    start = time.perf_counter()
    p = subprocess.run(command, cwd=cwd, env=env, input=data, capture_output=True, timeout=600)
    return p, time.perf_counter() - start


def capture(args):
    args.work.mkdir(parents=True, exist_ok=True)
    records = []
    for case in cases():
        destination = args.work / "captures" / case["id"]
        if destination.exists():
            raise RuntimeError(f"refusing to overwrite {destination}")
        destination.mkdir(parents=True)
        p, elapsed = invoke(case["command"], args.repos / case["repo"])
        (destination / "stdout").write_bytes(p.stdout)
        (destination / "stderr").write_bytes(p.stderr)
        case.update(exit_code=p.returncode, command_seconds=elapsed,
                    stdout_sha256=hashlib.sha256(p.stdout).hexdigest(),
                    stderr_sha256=hashlib.sha256(p.stderr).hexdigest())
        (destination / "meta.json").write_text(json.dumps(case, indent=2)+"\n")
        records.append(case)
        print(case["id"], p.returncode, len(p.stdout)+len(p.stderr), flush=True)
    (args.work / "captures.json").write_text(json.dumps(records, indent=2)+"\n")


def expand(output):
    """Independent decoder for the lossless line-prefix view (no LMR import)."""
    output = re.sub(r"\n?\[raw: [0-9a-f]+\]\n?$", "", output)
    version2 = output.startswith("LMR-LINES/2\n")
    if not version2 and not output.startswith("LMR-LINES/1\n"):
        return output
    lines = output.split("\n")[1:]
    prefix = ""
    decoded = []
    for line in lines:
        if line.startswith("@"):
            prefix = json.loads(line[1:])
        elif line.startswith("="):
            decoded.extend([decoded[-1]] * int(line[1:]))
        elif line.startswith("&"):
            decoded.append(decoded[int(line[1:])])
        elif line.startswith("!"):
            # Final newline is part of the reversible representation.
            return "\n".join(decoded) + ("\n" if line == "!1" else "")
        else:
            decoded.append(prefix + (line.removeprefix("\\") if version2 else json.loads(line)))
    raise ValueError("missing end record")


def facts(kind, raw, command):
    if kind == "cat":
        # Exact content, whitespace, order and final newline, not a keyword oracle.
        return [raw]
    if kind == "recursive":
        directory = ""
        result = []
        for line in raw.splitlines():
            if line.endswith(":"):
                directory = line[:-1]
                result.append(("directory", directory))
            elif line and line != "[stderr]":
                result.append(("entry", directory + "/" + line))
        return result
    if kind == "grep":
        result = []
        file = None
        for line in raw.splitlines():
            match = re.match(r"^(.+?):([0-9]+):(.*)$", line)
            if match:
                result.append(tuple(match.groups()))
            elif re.match(r"^(.+): \d+ matches$", line):
                file = line.rsplit(": ", 1)[0]
            elif file and re.match(r"^  [0-9]+:", line):
                number, text = line[2:].split(":", 1)
                result.append((file, number, text))
            elif line.strip() and line != "[stderr]":
                result.append(("literal", line))
        return result
    rows = []
    for line in raw.splitlines():
        if not line.strip() or line in ("[stdout]", "[stderr]"):
            continue
        if kind == "test" and command[0] == "cargo":
            if re.fullmatch(r"test .+ \.\.\. ok", line):
                continue  # Individual successful names are explicitly summarizable.
            if re.match(r"^\s*(Compiling|Finished|Running|Doc-tests|running \d+ tests?)\b", line):
                continue
        if kind == "test" and command[0] == "pytest":
            if re.fullmatch(r"[.sFxE]+\s*(\[\s*\d+%\])?", line.strip()):
                continue  # Only progress decorations; diagnostics are exhaustive.
        rows.append(line.strip())
    return rows


def missing(case, raw, output):
    output = output if output == raw else expand(output)
    required = facts(case["kind"], raw, case["command"])
    if case["kind"] in ("recursive", "grep"):
        present = Counter(facts(case["kind"], output, case["command"]))
        lost = list((Counter(required) - present).elements())
    elif case["kind"] == "cat":
        lost = [] if output == raw else ["exact file content"]
    else:
        present = Counter(line.strip() for line in output.splitlines())
        lost = list((Counter(required) - present).elements())
    return required, lost


def metrics(raw, output):
    return {name: dict(raw=len(enc.encode(raw, disallowed_special=())),
                       output=len(enc.encode(output, disallowed_special=())))
            for name, enc in ENCODINGS.items()}


def replay(args):
    records = json.loads((args.work / "captures.json").read_text())
    outdir = args.work / args.label
    if outdir.exists():
        raise RuntimeError(f"refusing to overwrite {outdir}")
    outdir.mkdir()
    bindir = outdir / "bin"
    bindir.mkdir()
    for name in {case["command"][0] for case in records}:
        target = bindir / name
        target.write_text(f"#!{os.sys.executable}\n" + (HERE / "replay.py").read_text())
        target.chmod(0o755)
    results = []
    failed = False
    for case in records:
        cap = args.work / "captures" / case["id"]
        stdout, stderr = (cap / "stdout").read_bytes(), (cap / "stderr").read_bytes()
        assert hashlib.sha256(stdout).hexdigest() == case["stdout_sha256"]
        assert hashlib.sha256(stderr).hexdigest() == case["stderr_sha256"]
        # Denominator is both real streams, including the boundary when mixed.
        raw = stdout.decode(errors="replace")
        if stderr:
            raw += ("\n[stderr]\n" if raw else "") + stderr.decode(errors="replace")
        env = dict(os.environ, LMR_CAPTURE=str(cap),
                   LM_RESIZER_STATE_DIR=str(outdir / "state"),
                   PATH=str(bindir)+os.pathsep+os.environ["PATH"])
        command = [str(bindir / case["command"][0]), *case["command"][1:]]
        _, replay_seconds = invoke(command, env=env)
        p, elapsed = invoke([str(args.binary), "exec", "--json", "--", *command], env=env)
        report = json.loads(p.stdout)
        required, lost = missing(case, raw, report["output"])
        good_exit = p.returncode == case["exit_code"] == report["exit_code"]
        recovery_verified = None
        if report.get("tee_hint"):
            recovery_text = ("[stderr]\n" if report.get("streams") and stderr and not stdout else "") + raw
            digest = hashlib.sha256(recovery_text.encode()).hexdigest()
            tee = outdir / "state" / "tee" / (digest + ".log")
            recovery_verified = tee.exists() and tee.read_bytes() == recovery_text.encode()
        failed |= bool(lost) or not good_exit or recovery_verified is False
        (outdir / (case["id"]+".output")).write_text(report["output"])
        (outdir / (case["id"]+".facts.json")).write_text(json.dumps(required, ensure_ascii=False, indent=2)+"\n")
        # Public metadata excludes local absolute command/state paths and raw content.
        row = dict(case, raw_bytes=len(stdout)+len(stderr), output_bytes=len(report["output"].encode()),
                   replay_seconds=replay_seconds, exec_seconds=elapsed,
                   added_seconds=elapsed-replay_seconds, filter=report["filter"],
                   tokens=metrics(raw, report["output"]), facts=len(required),
                   missing_count=len(lost), missing_sample=lost[:3], exit_preserved=good_exit,
                   recovery_verified=recovery_verified)
        # Tracebacks may contain the capture root; redact it in public samples.
        row["missing_sample"] = json.loads(json.dumps(row["missing_sample"]).replace(str(args.repos), "<repos>"))
        results.append(row)
        print(case["id"], "missing", len(lost), "seconds", round(elapsed, 3), flush=True)
    summary = {}
    main = [row for row in results if not row["id"].startswith("compare-")]
    for name in ENCODINGS:
        savings = [100*(1-row["tokens"][name]["output"]/row["tokens"][name]["raw"])
                   for row in main if row["tokens"][name]["raw"]]
        summary[name] = dict(median=statistics.median(savings), mean=statistics.mean(savings), cases=len(savings))
    artifact = dict(label=args.label, tokenizer=tiktoken.__version__, summary=summary, cases=results)
    (outdir / "results.json").write_text(json.dumps(artifact, indent=2, ensure_ascii=False)+"\n")
    print(json.dumps(summary, indent=2))
    raise SystemExit(1 if failed else 0)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["capture", "replay"])
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--repos", type=Path, required=True)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--label", default="before")
    args = parser.parse_args()
    args.work, args.repos = args.work.resolve(), args.repos.resolve()
    if args.binary:
        args.binary = args.binary.resolve()
    if args.mode == "capture":
        for repo in json.loads((HERE / "repos.json").read_text()):
            head = subprocess.check_output(["git", "-C", str(args.repos / repo["name"]), "rev-parse", "HEAD"], text=True).strip()
            if head != repo["commit"]:
                raise RuntimeError(f"wrong commit for {repo['name']}: {head}")
        capture(args)
    else:
        replay(args)


if __name__ == "__main__":
    main()
