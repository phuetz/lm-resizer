#!/usr/bin/env bash
set -euo pipefail
binary="${LM_RESIZER_BIN:-target/release/lm-resizer}"
test -x "$binary"
for command in err test summary exec tool-output gain stats; do
  if ! "$binary" --help | rg -q "(^[[:space:]]+$command[[:space:]]|aliases: $command)"; then
    echo "CLI command missing: $command" >&2
    exit 1
  fi
  if ! rg -q "\b$command\b" docs/CLI-REFERENCE.md; then
    echo "CLI reference missing: $command" >&2
    exit 1
  fi
done
"$binary" gain --help | rg -q -- '--json'
echo 'CLI reference: OK'
