#!/usr/bin/env python3
"""RTK on identical captured text where pipe supports the filter; live otherwise."""
import argparse
import json
import os
from pathlib import Path

from run import invoke, metrics, missing

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--work", type=Path, required=True)
parser.add_argument("--repos", type=Path, required=True)
parser.add_argument("--binary", type=Path, required=True)
args = parser.parse_args()
destination = args.work / "rtk"
destination.mkdir()
env = dict(os.environ, RTK_TELEMETRY_DISABLED="1", DO_NOT_TRACK="1",
           XDG_DATA_HOME=str(destination / "data"), XDG_CONFIG_HOME=str(destination / "config"))
mapping = {"log": "git-log", "diff": "git-diff", "status": "git-status", "find": "find", "grep": "grep"}
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
    required, lost = missing(case, raw, output)
    (destination / (case["id"]+".output")).write_text(output)
    results.append(dict(id=case["id"], mode=mode, seconds=elapsed,
                        tokens=metrics(raw, output), output_bytes=len(output.encode()),
                        facts=len(required), oracle_missing_count=len(lost),
                        exit_code=p.returncode, unavailable=p.returncode != 0 and mode == "identical-capture"))
    print(case["id"], mode, p.returncode, len(lost), flush=True)
(destination / "results.json").write_text(json.dumps(dict(version="0.50.0", cases=results), indent=2)+"\n")
