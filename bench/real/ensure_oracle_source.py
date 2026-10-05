#!/usr/bin/env python3
"""Fetch the pinned RTK source needed by the release similarity checks."""
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
PIN = json.loads((ROOT / "bench/rtk-parity/reference.json").read_text())
ARCHIVE = ROOT / "target/rtk-parity/source.tar.gz"
URL = "https://api.github.com/repos/rtk-ai/rtk/tarball/" + PIN["commit"]


def verify(path):
    actual = hashlib.sha256(path.read_bytes()).hexdigest()
    if actual != PIN["source_archive_sha256"]:
        raise ValueError(
            f"RTK source SHA-256 mismatch: expected {PIN['source_archive_sha256']}, got {actual}"
        )


def main():
    ARCHIVE.parent.mkdir(parents=True, exist_ok=True)
    if ARCHIVE.exists():
        try:
            verify(ARCHIVE)
        except ValueError as exc:
            raise SystemExit(str(exc)) from exc
        return
    temporary = None
    try:
        request = urllib.request.Request(URL, headers={"User-Agent": "lm-resizer-release-check"})
        with urllib.request.urlopen(request, timeout=30) as response:
            with tempfile.NamedTemporaryFile(dir=ARCHIVE.parent, prefix="source-", suffix=".tmp", delete=False) as output:
                temporary = Path(output.name)
                while chunk := response.read(1024 * 1024):
                    output.write(chunk)
        verify(temporary)
        temporary.replace(ARCHIVE)
    except (OSError, urllib.error.URLError, ValueError) as exc:
        raise SystemExit(f"Cannot prepare pinned RTK source from {URL}: {exc}") from exc
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


if __name__ == "__main__":
    main()
