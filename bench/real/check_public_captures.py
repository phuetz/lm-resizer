#!/usr/bin/env python3
"""Reject private identifiers in every tracked public file, including gzip captures.

Vendor sources are upstream evidence and deliberately excluded. Git's tracked
file list excludes build caches and local agent configuration from publication.
Character classes keep the detector itself free of the prohibited identifiers.
"""
import gzip
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[2]
PRIVATE = re.compile(r"p[a]trice|mini[s]tar|dark[s]tar|192[.]168[.]", re.I)


def check_repository():
    paths = subprocess.check_output(
        ["git", "-c", "core.fsmonitor=false", "ls-files", "-z"], cwd=ROOT
    ).decode().split("\0")
    failures = []
    for name in paths:
        if not name or name.startswith("vendor/"):
            continue
        path = ROOT / name
        if not path.is_file():
            continue
        data = path.read_bytes()
        if name.endswith(".gz"):
            data = gzip.decompress(data)
        text = data.decode("utf-8", errors="replace")
        if PRIVATE.search(text):
            failures.append(name)
    if failures:
        raise SystemExit("Private identifiers in public files: " + ", ".join(failures))
    print("Public tracked files, including decompressed gzip captures: no private identifiers")


if __name__ == "__main__":
    check_repository()
