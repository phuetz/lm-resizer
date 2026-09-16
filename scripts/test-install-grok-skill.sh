#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
INS="$ROOT/scripts/install-grok-skill.sh"
chmod +x "$INS"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

set +e
"$INS" --nope >/dev/null 2>&1
ec=$?
set -e
[ "$ec" -eq 2 ] || fail "unknown arg"

set +e
"$INS" --target buddy --dest "$TMP/x" >/dev/null 2>&1
ec=$?
set -e
[ "$ec" -eq 2 ] || fail "buddy target"

dest="$TMP/fresh/lm-resizer"
"$INS" --target grok --dest "$dest" | grep -q installed || fail "fresh"
[ -f "$dest/SKILL.md" ] && [ -f "$dest/agents/openai.yaml" ] || fail "fresh files"
"$INS" --target grok --dest "$dest" | grep -q 'already installed' || fail "identical"

echo 'custom-skill' > "$dest/SKILL.md"
"$INS" --target grok --dest "$dest" | grep -q skip || fail "custom skill"
grep -qx 'custom-skill' "$dest/SKILL.md" || fail "preserved"

mkdir -p "$TMP/yaml/lm-resizer/agents"
cp "$ROOT/skills/grok/lm-resizer/SKILL.md" "$TMP/yaml/lm-resizer/SKILL.md"
echo 'custom-yaml' > "$TMP/yaml/lm-resizer/agents/openai.yaml"
"$INS" --target grok --dest "$TMP/yaml/lm-resizer" | grep -q skip || fail "custom yaml"

mkdir -p "$TMP/miss/lm-resizer"
cp "$ROOT/skills/grok/lm-resizer/SKILL.md" "$TMP/miss/lm-resizer/SKILL.md"
"$INS" --target grok --dest "$TMP/miss/lm-resizer" | grep -q installed || fail "repair"
[ -f "$TMP/miss/lm-resizer/agents/openai.yaml" ] || fail "yaml repaired"

echo extra > "$dest/NOTES.txt"
force_out=$("$INS" --target grok --dest "$dest" --force)
echo "$force_out" | grep -q backup || fail "force backup: $force_out"
grep -q 'Compress large command' "$dest/SKILL.md" || fail "force skill"
[ -f "$dest/NOTES.txt" ] || fail "extra kept"

"$INS" --target codex --dest "$TMP/codex/lm-resizer" | grep -q 'target=codex' || fail "codex"

echo "PASS installer tests"
