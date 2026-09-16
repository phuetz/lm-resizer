---
name: lm-resizer
description: Compress large command or file output with the local `lm-resizer` CLI and recover originals via CCR. Use when a shell/tool transcript is too big for context. Do not use for ordinary short commands, and do not enable lossy `--token-budget` or cloud providers unless the user opts in after GPU/local coordination.
metadata:
  author: Grok
  short-description: Local lossless-first context compression
  compatibility: Requires lm-resizer on PATH. Crate 0.2.1. CLI has no --version flag
---

# LM Resizer (local CLI)

Binary: `lm-resizer` on PATH. `lm-resizer --help` lists commands. There is **no** `--version` / `-V` / `version` subcommand (verified). Do not install a duplicate when the binary is already present.

Opt-in only. Not a default wrapper for every shell call.

## Compress (default: lossless-first)

```bash
lm-resizer compress -i <file>
lm-resizer compress -i <file> --json
# stdin when -i omitted
```

From `compress --help`: `-q/--query`, `--token-budget` (lossy, omit unless asked), `--json`, `--store`.

Preserve compressor notices, hashes, and errors in the reply. Retrieve later: `lm-resizer retrieve` (see `retrieve --help`).

## Exec (filter then compress a command)

`lm-resizer exec --help` before use. `lm-resizer rewrite` / `rewrite-shell` show routing **without** running.

## Doctor / MCP

`lm-resizer doctor` — binary, CCR store, MCP tool names (`lm_resizer_compress`, `lm_resizer_retrieve`, `lm_resizer_stats`).  
`lm-resizer mcp` — stdio.  
`lm-resizer install --help` clients: `claude`, `codex`, `cursor`, `vscode`, `all`. **No `grok` client.** New session needed for discovery.

## Do not

- Invent flags.
- Auto `--token-budget` or paid cloud. If a provider is missing, document the fallback; do not claim a compression you did not run.
- Point `--store` at private/secret paths in fixtures.
