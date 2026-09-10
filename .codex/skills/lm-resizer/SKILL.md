---
name: lm-resizer
description: Keep noisy command output (tests, builds, package managers, diffs, logs, large JSON) out of the context window by running it through lm-resizer. Use whenever a shell command is likely to print more than a screen, when a previous command flooded the conversation, or when the user asks to save tokens. The full output always stays recoverable.
---

# lm-resizer — protect the context budget

lm-resizer sits between a command and the model. It keeps errors, file paths and summaries, drops repeated noise, and stores the raw output locally so nothing is lost. It never executes anything on its own: you wrap the command explicitly.

## Binary

The CLI is `lm-resizer` (`cargo build --release` → `target/release/lm-resizer`, or the `@phuetz/lm-resizer` npm package for the WebAssembly module). Check it is reachable with `lm-resizer stats` before relying on it; if it is missing, say so and run the command normally.

## When to wrap a command

Wrap anything that is likely to print more than a screen:

- test runners (`cargo test`, `npm test`, `vitest`, `jest`, `pytest`, `dotnet test`, `go test`)
- builds and package managers (`cargo build`, `npm install`, `dotnet build`, `pip install`, `mvn`, `gradle`)
- long diffs, logs, `kubectl`/`aws` listings, big JSON payloads

Do not wrap short, one-line commands (`git status`, `ls`, `pwd`): the wrapper would cost more than it saves.

## How to wrap

```bash
lm-resizer exec --raw-on-failure -- <command>            # summary on success, RAW output when the command fails
lm-resizer exec --raw-on-failure -q "why does test X fail" -- npm test   # query-aware: keeps what answers the question
lm-resizer exec --raw-on-failure --stream -- cargo build --release      # long command: live stream, then the filtered result
lm-resizer exec --json -- <command>                       # metadata (bytes before/after, steps, CCR key) instead of text
```

`--raw-on-failure` is the default you want: a failing command must never be summarised away. Always report the exit code of the wrapped command, not of `lm-resizer`.

Preview a rewrite without running it: `lm-resizer rewrite <command>` / `lm-resizer rewrite-shell '<full shell line>'`.

## Recovering the full output

Filtered does not mean lost. When you need the evidence:

```bash
lm-resizer tee list                 # raw recovery files written by exec
lm-resizer tee read <file>          # print one of them
lm-resizer retrieve <ccr-hash>      # fetch an offloaded blob by its CCR key (shown in the compressed output)
```

Prefer retrieving a specific file over re-running the noisy command.

## Measuring what you saved

```bash
lm-resizer stats --markdown
```

When you finish a task that used lm-resizer, add one line to your report: how many commands were wrapped and how many bytes/tokens were saved (from `stats`). Never claim savings you did not measure.

## MCP tools (if the server is installed)

`lm_resizer_compress`, `lm_resizer_tool_output` (post-execution reduction of output you already have; never runs commands), `lm_resizer_retrieve`, `lm_resizer_stats`. Install once per project: `lm-resizer install --client codex --scope project`.

## Automatic mode (optional, reversible)

- `lm-resizer install-hooks --client codex --project-dir .` adds a marked, reversible guidance block to `AGENTS.md`.
- `lm-resizer init-native-hooks --client codex --project-dir .` installs a native Codex hook that routes supported commands through `exec` automatically. Remove with `uninstall-hooks`.

## Pair it with Code Explorer

If the Code Explorer skill is available, map the repository first (`code-explorer status`, then `context`/`impact`/`query`) and wrap the commands you run afterwards with lm-resizer. Understand, compress, act.

## Guardrails

- Never hide a failure: `--raw-on-failure` on every wrapped command.
- Never present compressed output as the complete output; say it was filtered and how to recover the raw version.
- Do not wrap interactive commands or commands that need a TTY.
