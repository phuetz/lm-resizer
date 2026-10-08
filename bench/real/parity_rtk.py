#!/usr/bin/env python3
"""Byte parity with the pinned official RTK binary, not an in-process oracle.

Captured replay is distinct from live argv equivalence. The direct replayer
serves identical captured bytes to every invocation of the mocked producer;
it cannot establish that RTK's extra probes used the correct original argv.
"""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import statistics
import subprocess
import time

VERSION = "rtk 0.50.0"
ROOT = Path(__file__).resolve().parents[2]
CORPUS = ROOT / "bench/rtk-parity/corpus.json.gz"
PIPE = {"log": "git-log", "diff": "git-diff", "status": "git-status",
        "grep": "grep", "find": "find"}


def anonymize(text):
    text = text.replace(str(ROOT), "/workspace/project")
    text = re.sub(r"/home/[^/\s]+", "/home/user", text)
    text = re.sub(r"/Users/[^/\s]+", "/Users/user", text)
    text = re.sub(r"[A-Z]:\\Users\\[^\\\s]+", r"C:\\Users\\user", text)
    text = re.sub(r"(?m)^Author:.*$", "Author: Contributor <contributor@example.test>", text)
    text = re.sub(r"[\w.+-]+@[\w.-]+\.[A-Za-z]{2,}", "contributor@example.test", text)
    text = re.sub(r"\b(?:p[a]trice|phuetz)\b", "user", text, flags=re.I)
    return text


def import_captures(directories):
    cases = []
    for directory in directories:
        for case in json.loads((directory / "captures.json").read_text()):
            cap = directory / "captures" / case["id"]
            stdout, stderr = [(cap / stream).read_text(errors="replace")
                              for stream in ("stdout", "stderr")]
            command = case["command"]
            filter_name = PIPE.get(case["kind"])
            if command[0] == "cargo" and command[1:2] == ["test"]:
                filter_name = "cargo-test"
            if command[0] == "pytest":
                filter_name = "pytest"
            if command[0] in ("tsc", "vitest"):
                filter_name = command[0]
            cases.append(dict(id=case["id"], command=[anonymize(word) for word in command],
                              filter=filter_name, stdout=anonymize(stdout), stderr=anonymize(stderr),
                              exit_code=case["exit_code"], source="real-command-capture"))
    blob = json.dumps(cases, ensure_ascii=False).encode()
    CORPUS.parent.mkdir(parents=True, exist_ok=True)
    if CORPUS.exists():
        raise FileExistsError(CORPUS)
    CORPUS.write_bytes(gzip.compress(blob, mtime=0))


def invoke(command, *, data=None, env=None, cwd=None):
    started = time.perf_counter()
    p = subprocess.run(command, input=data, capture_output=True, env=env, cwd=cwd, timeout=120)
    return p, (time.perf_counter() - started) * 1000


def failure_lines(text):
    return [line for line in text.splitlines()
            if re.search(r"failed|FAIL|error|Error|Assertion|panic|CrashLoop", line)]


def adapted_reference(ref_bytes, lm_report):
    hint = lm_report.get("tee_hint")
    if not hint:
        return ref_bytes
    key = hint.removeprefix("[raw: ").removesuffix("]")
    return re.sub(rb"\[(full output:|\+\d+ hidden:) rtk recall [0-9a-f]{12}\]",
                  lambda m: (b"[" + (b"" if m[1] == b"full output:" else m[1] + b" ")
                             + f"lm-resizer tee read {key}]".encode()), ref_bytes)


def visible_body(report):
    """Remove only an actual tee trailer, never slice by pre-view byte count."""
    output = report["output"].encode()
    hint = report.get("tee_hint")
    if hint:
        key = hint.removeprefix("[raw: ").removesuffix("]")
        trailer = f"[tee:{key}] lm-resizer tee read {key}\n".encode()
        if output.endswith(trailer):
            return output[:-len(trailer)]
    return output

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--import-captures", type=Path, nargs="+")
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--before", type=Path)
    parser.add_argument("--built-oracle", type=Path, help="Build receipt from the pinned source archive")
    parser.add_argument("--rtk", type=Path)
    parser.add_argument("--headroom-python", type=Path)
    parser.add_argument("--work", type=Path)
    parser.add_argument("--repetitions", type=int, default=3)
    args = parser.parse_args()
    if args.import_captures:
        import_captures(args.import_captures)
        return
    from check_public_captures import check_repository
    check_repository()
    import tiktoken
    enc = tiktoken.get_encoding("o200k_base")
    tokens = lambda text: len(enc.encode(text, disallowed_special=()))
    binary, rtk, work = args.binary.resolve(), args.rtk.resolve(), args.work.resolve()
    work.mkdir(parents=True, exist_ok=False)
    version = subprocess.check_output([rtk, "--version"]).decode().strip()
    assert version == VERSION, version
    pinned = json.loads((ROOT / "bench/rtk-parity/reference.json").read_text())
    if args.built_oracle:
        receipt = json.loads(args.built_oracle.read_text())
        assert receipt["source_archive_sha256"] == pinned["source_archive_sha256"]
        assert receipt["version"] == version
        pinned = receipt
    assert hashlib.sha256(rtk.read_bytes()).hexdigest() == pinned["binary_sha256"], "unverified RTK binary"
    (work / "home").mkdir()
    base_env = dict(os.environ, HOME=str(work / "home"), RTK_TELEMETRY_DISABLED="1", DO_NOT_TRACK="1",
                    RTK_SUPPRESS_HOOK_WARNING="1",
                    LM_RESIZER_TRACKING="0", LM_RESIZER_STORE=str(work / "ccr.sqlite"),
                    LM_RESIZER_STATE_DIR=str(work / "state"),
                    HF_HOME=str(work / "hf"), XDG_CACHE_HOME=str(work / "cache"),
                    PYTHONDONTWRITEBYTECODE="1", HEADROOM_CCR_SQLITE_PATH=str(work / "headroom.sqlite"),
                    XDG_CONFIG_HOME=str(work / "config"), XDG_DATA_HOME=str(work / "data"))
    base_env.pop("RTK_RECALL", None)
    base_env.pop("RTK_TEE", None)
    rows = []
    cases = json.loads(gzip.decompress(CORPUS.read_bytes()))
    for case in cases:
        folder = work / case["id"]
        folder.mkdir()
        stdout, stderr = case["stdout"], case["stderr"]
        raw = stdout + (("\n[stderr]\n" if stdout else "[stderr]\n") + stderr if stderr else "")
        (folder / "stdout").write_text(stdout)
        (folder / "stderr").write_text(stderr)
        env = dict(base_env)
        if case["filter"]:
            rtk_args = [rtk, "pipe", "--filter", case["filter"]]
            lm_args = [binary, "pipe", "--filter", case["filter"], "--json",
                       "--exit-code", str(case["exit_code"])]
            data, mode = raw.encode(), "identical-pipe-capture"
        else:
            # No shell interpolation of command arguments or capture contents.
            shim = folder / "bin"
            shim.mkdir()
            body = ('#!/bin/sh\n/bin/cat "$PARITY_CAPTURE/stdout"\n'
                    '/bin/cat "$PARITY_CAPTURE/stderr" >&2\nexit "$PARITY_EXIT"\n')
            for program in {case["command"][0], "npx", "npm", "pnpm", "yarn"}:
                target = shim / program
                target.write_text(body)
                target.chmod(0o755)
            env.update(PATH=str(shim) + os.pathsep + env["PATH"],
                       PARITY_CAPTURE=str(folder), PARITY_EXIT=str(case["exit_code"]))
            command = case["command"][:]
            if command[0] == "cat":
                command = ["cat", "stdout"]
            rtk_command = (["read", "stdout"] if command[0] == "cat" else
                           ["lint", *command] if command[0] == "eslint" else command)
            rtk_args, lm_args = [rtk, *rtk_command], [binary, "exec", "--json", "--", *command]
            data, mode = None, "direct-capture-replay"
        ref, ref_ms = invoke(rtk_args, data=data, env=env, cwd=folder)
        lm, lm_ms = invoke(lm_args, data=data, env=env, cwd=folder)
        report = json.loads(lm.stdout)
        view = report["output"].encode()
        # Filtered byte count belongs to the intermediate filter, not the
        # final view (which may add an exit status or change line lengths).
        body = visible_body(report)
        reference = ref.stdout + ref.stderr
        strict_exact = body == reference
        exact = body == adapted_reference(reference, report)
        reference_hashes = [hashlib.sha256(reference).hexdigest()]
        lm_hashes = [hashlib.sha256(body).hexdigest()]
        ref_times, lm_times = [ref_ms], [lm_ms]
        exit_preserved = lm.returncode == case["exit_code"]
        # Three pairs can accidentally agree on every randomized TSC tie.
        # Stress this upstream-randomized filter without normalizing either side.
        repetitions = max(64 if case["filter"] == "tsc" else 1, args.repetitions)
        for _ in range(repetitions - 1):
            repeated_ref, elapsed_ref = invoke(rtk_args, data=data, env=env, cwd=folder)
            repeated_lm, elapsed_lm = invoke(lm_args, data=data, env=env, cwd=folder)
            repeated_report = json.loads(repeated_lm.stdout)
            repeated_view = visible_body(repeated_report)
            repeated_reference = repeated_ref.stdout + repeated_ref.stderr
            reference_hashes.append(hashlib.sha256(repeated_reference).hexdigest())
            lm_hashes.append(hashlib.sha256(repeated_view).hexdigest())
            strict_exact = strict_exact and repeated_view == repeated_reference
            exact = exact and repeated_view == adapted_reference(repeated_reference, repeated_report)
            exit_preserved = exit_preserved and repeated_lm.returncode == case["exit_code"]
            ref_times.append(elapsed_ref)
            lm_times.append(elapsed_lm)
        ref_ms, lm_ms = statistics.median(ref_times), statistics.median(lm_times)
        expected_raw = raw.encode() if case["filter"] else (stdout + stderr).encode()
        recovered = body == expected_raw
        if report["tee_hint"]:
            key = report["tee_hint"].removeprefix("[raw: ").removesuffix("]")
            recovered_bytes = invoke([binary, "tee", "read", key], env=env)[0].stdout
            recovered = recovered_bytes == expected_raw
        before_tokens = None
        if args.before:
            before = invoke([args.before.resolve(), "tool-output", "--json", "--command",
                             shlex.join(case["command"]), "--exit-code", str(case["exit_code"])],
                            data=raw.encode(), env=env, cwd=folder)[0]
            before_tokens = tokens(json.loads(before.stdout)["output"])
        hr_text, hr_ms, hr_error = None, None, None
        if args.headroom_python:
            try:
                hr, hr_ms = invoke(["sh", str(ROOT / "bench/headroom_once.sh"),
                                    str(args.headroom_python.absolute())], data=raw.encode(), env=env)
                if hr.returncode == 0:
                    hr_text = hr.stdout.decode()
                else:
                    hr_error = hr.stderr.decode()[-2000:]
            except subprocess.TimeoutExpired:
                hr_error = "timeout 120 seconds"
        upstream_variance = (case["id"] == "fresh-tsc" and len(set(reference_hashes)) > 1
                             and set(reference_hashes) == set(lm_hashes)
                             and set(reference_hashes) == {
                                 "4a51fc9d3c5e5c477a8914d4a0577c77619bdaae553a52cbb83a0015ab51a1d1",
                                 "ddbb2e416eaa91f351adf32c6cbeaa40ad7ab73a6ea0c4a4fd6b99cba117a8fd"})
        row = dict(id=case["id"], command=case["command"], mode=mode,
                   parity=exact, strict_byte_parity=strict_exact,
                   accepted_upstream_variance=upstream_variance,
                   contract_pass=exact or upstream_variance,
                   recall_adapted=adapted_reference(reference, report) != reference,
                   view_token_delta=tokens(body.decode()) - tokens(reference.decode()),
                   tee_verified=recovered, exit_preserved=exit_preserved,
                   producer_exit_preserved=exit_preserved,
                   reference_exit_equal=lm.returncode == ref.returncode,
                   producer_exit=case["exit_code"],
                   repetitions=repetitions,
                   reference_hashes=reference_hashes, lm_hashes=lm_hashes,
                   reference_deterministic=len(set(reference_hashes)) == 1,
                   lm_deterministic=len(set(lm_hashes)) == 1,
                   raw_tokens=tokens(raw), before_tokens=before_tokens,
                   rtk_tokens=tokens(reference.decode()), lm_view_tokens=tokens(body.decode()),
                   lm_total_tokens=tokens(view.decode()),
                   rtk_ms=ref_ms, lm_ms=lm_ms, rtk_exit=ref.returncode, lm_exit=lm.returncode,
                   headroom_tokens=tokens(hr_text) if hr_text is not None else None,
                   headroom_ms=hr_ms, headroom_error=hr_error,
                   failures={"raw": failure_lines(raw), "rtk": failure_lines(reference.decode()),
                             "lm": failure_lines(body.decode()),
                             "headroom": failure_lines(hr_text) if hr_text is not None else None})
        rows.append(row)
        (folder / "reference").write_bytes(reference)
        (folder / "lm-view").write_bytes(body)
        print(case["id"], mode, "OK" if exact and recovered else "FAIL", flush=True)
    summary = {}
    for key in ("before_tokens", "rtk_tokens", "lm_view_tokens", "lm_total_tokens", "headroom_tokens"):
        savings = [100 * (1 - row[key] / row["raw_tokens"]) for row in rows
                   if row[key] is not None and row["raw_tokens"]]
        summary[key] = dict(cases=len(savings), median=statistics.median(savings) if savings else None,
                            mean=statistics.mean(savings) if savings else None)
    result = dict(version=version, rtk_sha256=hashlib.sha256(rtk.read_bytes()).hexdigest(),
                  lm_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                  corpus_sha256=hashlib.sha256(CORPUS.read_bytes()).hexdigest(),
                  tokenizer="tiktoken 0.14.0 o200k_base", repetitions=max(1, args.repetitions),
                  tsc_min_repetitions=64,
                  median_goal_met=summary["lm_total_tokens"]["median"] >= summary["rtk_tokens"]["median"],
                  summary=summary, cases=rows)
    (work / "results.json").write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n")
    print(json.dumps(summary, indent=2))
    if not result["median_goal_met"] or not all(row["contract_pass"] and row["tee_verified"] and row["exit_preserved"] for row in rows):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
