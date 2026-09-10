#!/usr/bin/env bash
# The lm-resizer skill lives in three places on purpose:
#   skills/lm-resizer          -> Claude Code plugin (+ `npx skills add phuetz/lm-resizer`)
#   .claude/skills/lm-resizer  -> auto-loaded when this repo is the working project
#   .codex/skills/lm-resizer   -> Codex (frontmatter differs: no allowed-tools/argument-hint, codex client)
# The first two must stay byte-identical. Run from the repo root.
set -euo pipefail
if ! diff -q skills/lm-resizer/SKILL.md .claude/skills/lm-resizer/SKILL.md >/dev/null; then
  echo "skills/lm-resizer/SKILL.md and .claude/skills/lm-resizer/SKILL.md differ — copy one over the other" >&2
  exit 1
fi
echo "skills in sync"
