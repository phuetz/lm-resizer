#!/usr/bin/env bash
# Copy the Grok-authored LM Resizer skill into ~/.grok/skills.
# Does not overwrite a customized SKILL.md unless --force.
set -euo pipefail
FORCE=0
if [ "${1:-}" = "--force" ]; then FORCE=1; fi
SRC="$(cd "$(dirname "$0")/.." && pwd)/skills/grok/lm-resizer"
DEST="${GROK_SKILLS_DIR:-$HOME/.grok/skills}/lm-resizer"
mkdir -p "$(dirname "$DEST")"
if [ -f "$DEST/SKILL.md" ]; then
  if cmp -s "$SRC/SKILL.md" "$DEST/SKILL.md"; then
    echo "already installed: $DEST"
    exit 0
  fi
  if [ "$FORCE" != 1 ]; then
    echo "skip: $DEST/SKILL.md exists and differs (pass --force to replace)"
    exit 0
  fi
fi
mkdir -p "$DEST/agents"
cp "$SRC/SKILL.md" "$DEST/SKILL.md"
cp "$SRC/agents/openai.yaml" "$DEST/agents/openai.yaml"
echo "installed: $DEST"
