#!/usr/bin/env bash
# Genuine Git output from an isolated, fictitious repository, deterministic identities/dates.
set -euo pipefail
capture_dir="$(mktemp -d)"
trap 'rm -rf "$capture_dir"' EXIT
cd "$capture_dir"
git init -q
git config user.name Fixture
git config user.email fixture@example.invalid
git config core.quotePath true
mkdir -p src
for batch in $(seq 1 40); do
  for file in $(seq 1 8); do
    seq 1 "$((30 * (batch + file)))" | sed "s/^/batch $batch value /" > "src/module-$file.rs"
  done
  git add src/module-1.rs
  git add src/module-2.rs
  git add src/module-3.rs
  git add src/module-4.rs
  git add src/module-5.rs
  git add src/module-6.rs
  git add src/module-7.rs
  git add src/module-8.rs
  GIT_AUTHOR_DATE="2026-09-30T12:00:00+$((1000 + batch))" GIT_COMMITTER_DATE="2026-09-30T12:00:00+$((1000 + batch))" git commit -q -m "Maintenance batch $batch"
done
LC_ALL=C git -c color.ui=false log -40 --stat
