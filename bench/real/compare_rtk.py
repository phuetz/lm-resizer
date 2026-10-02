#!/usr/bin/env python3
"""RTK on identical captured text where pipe supports the filter; live otherwise."""
import argparse
import json
import os
import re
import statistics
from pathlib import Path

from run import invoke, metrics, missing

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--work", type=Path, required=True)
parser.add_argument("--repos", type=Path, required=True)
parser.add_argument("--binary", type=Path, required=True)
parser.add_argument("--label", default="rtk")
args = parser.parse_args()
destination = args.work / args.label
destination.mkdir()
env = dict(os.environ, RTK_TELEMETRY_DISABLED="1", DO_NOT_TRACK="1",
           XDG_DATA_HOME=str(destination / "data"), XDG_CONFIG_HOME=str(destination / "config"))
mapping = {"log": "git-log", "diff": "git-diff", "status": "git-status", "find": "find", "grep": "grep"}
version = invoke([str(args.binary), "--version"])[0].stdout.decode().strip()

def recognized_output(kind, text):
    # Reconstruct RTK's grouping before comparing facts; formatting is not loss.
    if kind == "grep":
        file = None
        lines = []
        for line in text.splitlines():
            header = re.fullmatch(r"\[file\] (.+) \(\d+\):", line)
            row = re.match(r"^\s+(\d+): (.*)$", line)
            if header:
                file = header[1]
            elif file and row:
                lines.append(f"{file}:{row[1]}:{row[2]}")
            else:
                lines.append(line)
        return "\n".join(lines)
    if kind == "find":
        folder = None
        lines = []
        for line in text.splitlines():
            header = re.fullmatch(r"(.+/)  \(\d+\)", line)
            if header:
                folder = header[1]
            elif folder and line.startswith("  ") and not line.strip().startswith("..."):
                lines.append(folder + line[2:])
            else:
                lines.append(line)
        return "\n".join(lines)
    return text

results = []
for case in json.loads((args.work / "captures.json").read_text()):
    cap = args.work / "captures" / case["id"]
    stdout, stderr = (cap / "stdout").read_bytes(), (cap / "stderr").read_bytes()
    raw = stdout.decode(errors="replace")
    if stderr:
        raw += ("\n[stderr]\n" if raw else "") + stderr.decode(errors="replace")
    kind, original = case["kind"], case["command"]
    filter_name = mapping.get(kind)
    if kind == "test":
        filter_name = {"cargo": "cargo-test", "pytest": "pytest"}.get(original[0])
    if filter_name:
        command = [str(args.binary), "pipe", "--filter", filter_name]
        p, elapsed = invoke(command, env=env, data=raw.encode())
        mode = "identical-capture"
    else:
        if kind == "cat":
            command = [str(args.binary), "read", original[1]]
        else:
            command = [str(args.binary), *original]
        p, elapsed = invoke(command, cwd=args.repos / case["repo"], env=env)
        mode = "live-command"
    output = p.stdout.decode(errors="replace")
    if p.stderr:
        output += "\n[stderr]\n" + p.stderr.decode(errors="replace")
    recognized = recognized_output(kind, output)
    required, lost = missing(case, raw, recognized)
    # Search indentation changed by RTK is reported separately from missing lines.
    whitespace_changed = 0
    if kind == "grep":
        from run import facts
        semantic = { (f[0], f[1], f[2].strip()) for f in facts(kind, recognized, original) if len(f) == 3 }
        whitespace_changed = sum(isinstance(f, tuple) and len(f) == 3 and (f[0], f[1], f[2].strip()) in semantic for f in lost)

    (destination / (case["id"]+".output")).write_text(output)
    results.append(dict(id=case["id"], mode=mode, seconds=elapsed,
                        tokens=metrics(raw, output), output_bytes=len(output.encode()),
                        facts=len(required), oracle_missing_count=len(lost),
                        search_whitespace_changed=whitespace_changed,
                        exit_code=p.returncode, unavailable=p.returncode != 0 and mode == "identical-capture"))
    print(case["id"], mode, p.returncode, len(lost), flush=True)
summary = {}
for encoding in ("cl100k_base", "o200k_base"):
    savings = [100*(1-row["tokens"][encoding]["output"]/row["tokens"][encoding]["raw"])
               for row in results if not row["id"].startswith("compare-") and not row["unavailable"] and row["tokens"][encoding]["raw"]]
    summary[encoding] = dict(median=statistics.median(savings), mean=statistics.mean(savings), cases=len(savings))
(destination / "results.json").write_text(json.dumps(dict(version=version, summary=summary, cases=results), indent=2)+"\n")
print(json.dumps(summary, indent=2))
