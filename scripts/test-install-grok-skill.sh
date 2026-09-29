#!/usr/bin/env bash
# Real installer cases against a temp dest (not the user skill tree).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
INS="$ROOT/scripts/install-grok-skill.sh"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

# unknown argument
set +e
out=$(bash "$INS" --nope 2>&1)
ec=$?
set -e
[ "$ec" -eq 2 ] || fail "unknown arg exit $ec"
echo "$out" | grep -q 'unknown argument' || fail "unknown arg message"

# --target without value
set +e
out=$(bash "$INS" --target 2>&1)
ec=$?
set -e
[ "$ec" -eq 2 ] || fail "--target missing value exit $ec"

# unsupported target
set +e
out=$(bash "$INS" --target claude --dest "$TMP/x" 2>&1)
ec=$?
set -e
[ "$ec" -eq 2 ] || fail "claude target exit $ec"

# --force on a dest that does not exist: no empty .bak
fresh_force="$TMP/newforce/lm-resizer"
force_new=$(bash "$INS" --target grok --dest "$fresh_force" --force)
echo "$force_new" | grep -q installed || fail "force-new install"
echo "$force_new" | grep -q backup && fail "force-new must not backup"
ls -d "$fresh_force".bak.* >/dev/null 2>&1 && fail "force-new created bak"

# 1. fresh install
dest="$TMP/fresh/lm-resizer"
bash "$INS" --target grok --dest "$dest" | grep -q installed || fail "fresh"
[ -f "$dest/SKILL.md" ] && [ -f "$dest/agents/openai.yaml" ] || fail "fresh files"

# 2. second identical
bash "$INS" --target grok --dest "$dest" | grep -q 'already installed' || fail "identical"

# 3. personalized SKILL.md — skip, keep yaml from first install
echo 'custom-skill' > "$dest/SKILL.md"
bash "$INS" --target grok --dest "$dest" | grep -q skip || fail "custom skill skip"
grep -qx 'custom-skill' "$dest/SKILL.md" || fail "skill preserved"
[ -f "$dest/agents/openai.yaml" ] || fail "yaml still there"

# 4. personalized YAML — skip, keep both
mkdir -p "$TMP/yaml/lm-resizer/agents"
cp "$ROOT/skills/grok/lm-resizer/SKILL.md" "$TMP/yaml/lm-resizer/SKILL.md"
echo 'custom-yaml' > "$TMP/yaml/lm-resizer/agents/openai.yaml"
bash "$INS" --target grok --dest "$TMP/yaml/lm-resizer" | grep -q skip || fail "custom yaml skip"
grep -qx 'custom-yaml' "$TMP/yaml/lm-resizer/agents/openai.yaml" || fail "yaml preserved"

# 5. missing yaml repaired when SKILL matches
mkdir -p "$TMP/miss/lm-resizer"
cp "$ROOT/skills/grok/lm-resizer/SKILL.md" "$TMP/miss/lm-resizer/SKILL.md"
bash "$INS" --target grok --dest "$TMP/miss/lm-resizer" | grep -q installed || fail "repair"
[ -f "$TMP/miss/lm-resizer/agents/openai.yaml" ] || fail "yaml repaired"

# extra personal file never deleted
echo extra > "$TMP/miss/lm-resizer/NOTES.txt"
bash "$INS" --target grok --dest "$TMP/miss/lm-resizer" | grep -q 'already installed' || fail "extra"
[ -f "$TMP/miss/lm-resizer/NOTES.txt" ] || fail "extra kept"

# 6. --force backups and replaces SKILL, keeps extra
echo 'old' > "$dest/NOTES.txt"
force_out=$(bash "$INS" --target grok --dest "$dest" --force)
echo "$force_out" | grep -q backup || fail "force backup: $force_out"
grep -q 'Compress large command' "$dest/SKILL.md" || fail "force replaced skill"
[ -f "$dest/NOTES.txt" ] || fail "force kept extra"
ls -d "$dest".bak.* >/dev/null || fail "backup dir"
grep -qx 'custom-skill' "$dest".bak.*/SKILL.md || fail "backup had custom skill"
# A linked destination must back up its contents before --force replaces them.
mkdir -p "$TMP/link-real"
echo 'linked-custom' > "$TMP/link-real/SKILL.md"
if ln -s "$TMP/link-real" "$TMP/link-dest" 2>/dev/null && [ -L "$TMP/link-dest" ]; then
  bash "$INS" --target grok --dest "$TMP/link-dest" --force >/dev/null
  grep -qx 'linked-custom' "$TMP"/link-dest.bak.*/SKILL.md || fail "linked destination backup lost original content"
else
  case "$(uname -s)" in
    Linux|Darwin) fail "could not create a test symlink" ;;
    *) echo "SKIP symlink backup: this runner cannot create a real symlink" >&2 ;;
  esac
fi

# A failed backup must stop --force before reporting an installation.
mkdir -p "$TMP/failing-bin" "$TMP/failing-dest"
echo 'do-not-replace' > "$TMP/failing-dest/SKILL.md"
cat > "$TMP/failing-bin/cp" <<'EOF'
#!/bin/sh
if [ "$1" = '-a' ]; then exit 1; fi
exec "$REAL_CP" "$@"
EOF
chmod +x "$TMP/failing-bin/cp"
set +e
PATH="$TMP/failing-bin:$PATH" REAL_CP="$(command -v cp)" bash "$INS" --target grok --dest "$TMP/failing-dest" --force >/dev/null 2>&1
backup_status=$?
set -e
[ "$backup_status" -ne 0 ] || fail "failed backup reported success"
grep -qx 'do-not-replace' "$TMP/failing-dest/SKILL.md" || fail "failed backup changed destination"

# 7. --target codex with dest
bash "$INS" --target codex --dest "$TMP/codex/lm-resizer" | grep -q 'target=codex' || fail "codex target"

echo "PASS installer tests $TMP"
