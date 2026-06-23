# Social Post Drafts

## Short Post

I am testing lm-resizer with Claude Code and Codex: a Rust-native context
compression layer for noisy tool output.

Instead of pasting full `cargo test`, `rg`, `kubectl`, or `docker logs` output
into the agent, run:

```bash
lm-resizer exec --stream -- cargo test
```

It streams the command live, filters the output after completion, keeps the
important failures/summaries, and stores recoverable raw output when needed.

Docs: `docs/CLAUDE_CODEX.md`
Contributing fixtures/filters: `CONTRIBUTING.md`

## Technical Post

Claude Code and Codex often spend context on repeated command noise: passing
tests, long logs, package install output, and huge search results.

lm-resizer adds an explicit wrapper:

```bash
lm-resizer exec -- cargo test
lm-resizer exec -- rg -n "TODO|FIXME" .
lm-resizer discover-sessions --agent all --markdown
lm-resizer init-native-hooks --client all --project-dir .
lm-resizer audit-filters --path .lm-resizer/filters.toml --review
lm-resizer sanitize-provider-fixture --provider openai --input payload.json --output fixtures/provider-cache/openai-real.json
powershell -File scripts/smoke-proxy-preview.ps1
powershell -File scripts/check-publish-readiness.ps1
powershell -File scripts/generate-checksums.ps1
```

The interesting part: it is opt-in. No shell replacement, no hidden command
execution. Project instructions can be installed reversibly in `CLAUDE.md` or
`AGENTS.md`; native Codex/Claude PostToolUse hooks can record savings without
blocking the agent; and the wrapper keeps raw-output recovery files for failed
or large commands.

Useful when you want agents to see the signal, not 20,000 repeated lines.

The repo also ships issue templates for two contribution paths: sanitized real
provider payloads and reusable command filters. That keeps the public examples
grounded in real traffic without asking anyone to publish secrets or private
prompts.

## Thread Outline

1. Problem: coding agents waste context on noisy command output.
2. Solution: wrap selected commands with `lm-resizer exec -- ...`.
3. Claude/Codex setup: install MCP plus reversible project instructions.
4. Automation: optional PATH shims route known commands through `lm-resizer exec`.
5. Recovery: raw output is saved for failed or large commands.
6. Audit: `discover` estimates savings from previous session logs.
7. Learning: `learn` turns session mining into AGENTS.md / CLAUDE.md guidance.
8. Proxy smoke: release checks boot the local proxy and verify provider preview.
9. Release hygiene: WASM/npm preflight, checksums, proxy smoke, and publish-readiness checks validate the package before publish.
10. Sample hygiene: provider payloads can be sanitized before becoming fixtures.
11. Contribution path: issue templates for provider fixtures and command filters.
12. Next work: external npm publish approval and real-world project/provider samples.
