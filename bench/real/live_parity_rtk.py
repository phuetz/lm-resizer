#!/usr/bin/env python3
"""Live argv check on a stable, disposable project produced by capture_parity.py."""
import argparse
import json
import os
import hashlib
import tiktoken
from pathlib import Path
from parity_rtk import invoke, VERSION, anonymize, adapted_reference


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for name in ("binary", "rtk", "project", "work"):
        p.add_argument("--" + name, type=Path, required=True)
    args = p.parse_args()
    work = args.work.resolve()
    work.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ, RTK_TELEMETRY_DISABLED="1", RTK_SUPPRESS_HOOK_WARNING="1",
               LM_RESIZER_STATE_DIR=str(work / "state"), LM_RESIZER_STORE=str(work / "ccr.sqlite"),
               LM_RESIZER_TRACKING="0", XDG_CONFIG_HOME=str(work / "config"),
               XDG_DATA_HOME=str(work / "data"))
    assert invoke([args.rtk.resolve(), "--version"])[0].stdout.decode().strip() == VERSION
    enc = tiktoken.get_encoding("o200k_base")
    count = lambda data: len(enc.encode(data.decode(), disallowed_special=()))
    rows = []
    for command in (["git", "log", "-1"], ["git", "diff"], ["git", "status"],
                    ["git", "show", "--stat"], ["rg", "-n", "assert", "src"],
                    ["grep", "-rn", "assert", "src"], ["find", "src", "-type", "f"],
                    ["ls", "-la", "src"], ["ls", "-R", "src"], ["tree", "src"],
                    ["cat", "src/lib.rs"]):
        ref_command = ["read", *command[1:]] if command[0] == "cat" else command
        ref, _ = invoke([args.rtk.resolve(), *ref_command], cwd=args.project, env=env)
        lm, _ = invoke([args.binary.resolve(), "exec", "--json", "--", *command], cwd=args.project, env=env)
        result = json.loads(lm.stdout)
        view = result["output"].encode()[:result["filtered_bytes"]]
        reference = ref.stdout + ref.stderr
        equal = view == adapted_reference(reference, result) and lm.returncode == ref.returncode
        rows.append(dict(command=command, parity=equal,
                         strict_byte_parity=view == reference,
                         recall_adapted=adapted_reference(reference, result) != reference,
                         view_token_delta=count(view)-count(reference),
                         lm_sha256=hashlib.sha256(args.binary.resolve().read_bytes()).hexdigest(),
                         rtk_sha256=hashlib.sha256(args.rtk.resolve().read_bytes()).hexdigest(),
                         reference=anonymize(reference.decode()), view=anonymize(view.decode())))
        print(command, "OK" if equal else "FAIL", flush=True)
    (work / "results.json").write_text(json.dumps(rows, ensure_ascii=False, indent=2) + "\n")
    if not all(row["parity"] for row in rows):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
