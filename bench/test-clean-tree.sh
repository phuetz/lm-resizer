#!/usr/bin/env bash
# Integration test: a normal full replay must leave tracked files unchanged.
set -euo pipefail
repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_dir"
mkdir -p "$repo_dir/target/banc"

before="$(git status --porcelain=v1 --untracked-files=no)"
./bench/rejouer.sh >"$repo_dir/target/banc/clean-tree-test.log" 2>&1
after="$(git status --porcelain=v1 --untracked-files=no)"

if [[ "$before" != "$after" ]]; then
  echo "benchmark modified tracked files" >&2
  git status --short >&2
  exit 1
fi
echo "benchmark left tracked files unchanged"
