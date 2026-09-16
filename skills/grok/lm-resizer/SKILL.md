---
name: lm-resizer
description: Compress large command or file output with the local `lm-resizer` CLI and recover originals via CCR. Use when a shell or tool transcript is too large for context. Skip ordinary short commands. Lossy `--token-budget` and remote providers need existing model/GPU authorization; default compress is local lossless-first.
metadata:
  author: Grok
  short-description: Local lossless-first context compression
  compatibility: Requires lm-resizer on PATH. CLI has no --version flag
---

# LM Resizer (local CLI)

Binary: `lm-resizer` on PATH. `lm-resizer --help` lists commands. There is no `--version` / `-V` / `version` subcommand. Do not install a duplicate when the binary is already present.

Use when shrinking a large transcript helps this turn. Do not wrap every shell call.

## Compress (lossless-first)

```bash
lm-resizer compress -i <file>
lm-resizer compress -i <file> --json
```

From `compress --help`: `-q/--query`, `--token-budget` (lossy), `--json`, `--store`. Preserve notices, hashes, and errors.

Retrieve: `lm-resizer retrieve` (`retrieve --help`).

## Exec

Read `lm-resizer exec --help` first. `lm-resizer rewrite` / `rewrite-shell` show routing without running.

## Doctor / MCP

`lm-resizer doctor` — binary, store, MCP tools `lm_resizer_compress`, `lm_resizer_retrieve`, `lm_resizer_stats`.  
`lm-resizer mcp` — stdio.  
`lm-resizer install --help` clients: `claude`, `codex`, `cursor`, `vscode`, `all`. **No `grok` client.** Grok discovery: `grok inspect --json` → `skills`.

## Do not

- Invent flags.
- Enable `--token-budget` or a remote provider without existing authorization.
- Put secrets in `--store` fixtures.
