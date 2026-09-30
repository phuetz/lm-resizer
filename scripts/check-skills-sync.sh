#!/usr/bin/env bash
# The lm-resizer skill lives in three places on purpose:
#   skills/lm-resizer          -> Claude Code plugin (+ `npx skills add phuetz/lm-resizer`)
#   .claude/skills/lm-resizer  -> auto-loaded when this repo is the working project
#   .codex/skills/lm-resizer   -> Codex (frontmatter differs: no allowed-tools/argument-hint, codex client)
# The first two must stay byte-identical. Codex keeps different frontmatter
# and client-specific instructions, but its normalized body must also match.
# Run from the repo root.
set -euo pipefail
if ! diff -q skills/lm-resizer/SKILL.md .claude/skills/lm-resizer/SKILL.md >/dev/null; then
  echo "skills/lm-resizer/SKILL.md and .claude/skills/lm-resizer/SKILL.md differ — copy one over the other" >&2
  exit 1
fi
# Ignore the client-specific frontmatter, not the user-facing instructions.
skill_body() {
  awk 'NR == 1 && /^---$/ { front = 1; next } front && /^---$/ { front = 0; next } !front { print }' "$1"
}
expected_codex_body() {
  skill_body skills/lm-resizer/SKILL.md | sed \
    -e 's/--client claude/--client codex/g' \
    -e 's/CLAUDE\.md/AGENTS.md/g' \
    -e 's/Install once per project: `lm-resizer install --client codex --scope project`/Install for the user: `lm-resizer install --client codex --scope global`/'
}
if ! diff -q <(expected_codex_body) <(skill_body .codex/skills/lm-resizer/SKILL.md) >/dev/null; then
  echo "Codex skill instructions differ from the canonical skill after client normalization" >&2
  exit 1
fi
echo "skills in sync (Claude and Codex)"
