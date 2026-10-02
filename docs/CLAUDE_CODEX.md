# Using lm-resizer With Claude Code Or Codex

lm-resizer is intentionally opt-in. Nothing rewrites commands until you
explicitly install hook config. Claude Code and Codex can use it in these
practical ways:

1. MCP compression tools
2. explicit `lm-resizer exec -- ...` command wrapping
3. project hook instructions in `CLAUDE.md` or `AGENTS.md`
4. experimental native hook config and an offline-testable hook handler;
   activation depends on the agent version and event protocol
5. a drop-in **skill** (`.claude/skills/lm-resizer`, `.codex/skills/lm-resizer`)
   that teaches the agent when to wrap, how to recover raw output, and how to
   report savings — copy the folder into your repo or your home skills dir

## Install

Follow the [README](../README.md) for the exact clone command, official Rustup setup and native build prerequisites. Its prebuilt installer is usable only after v0.2.4 is published. Once the repository is cloned and Rustup is on PATH, build and
install from its root. Source builds require Rust 1.86 or newer, Git and native C/C++ build tools (MSVC C++ tools and Windows SDK in Visual Studio Build Tools on Windows):

Linux/macOS (Bash):

```bash
cargo install --quiet --path . --locked --root "$HOME/.local"
export PATH="$HOME/.local/bin:$PATH"
lm-resizer --version
```

Windows (PowerShell):

```powershell
$installRoot = Join-Path $env:USERPROFILE '.local'
cargo install --quiet --path . --locked --root "$installRoot"
$env:Path = "$installRoot\bin;$env:Path"
lm-resizer --version
```

Then add MCP configuration:

```bash
lm-resizer install --client claude --scope project
lm-resizer install --client codex --scope global
```

For several clients at once (Claude, Cursor and VS Code use project config;
Codex always uses user config, including with `--scope project`):

```bash
lm-resizer install --client all --scope project --project-dir .
```

Run the project commands from your repository directory (`.`), or replace `.` with its actual path. The Codex installer replaces an existing `[mcp_servers.lm_resizer]` table without a backup. Save your user configuration first. `--scope project` with `--client all` does not isolate Codex configuration to the repository.

Check the environment:

```bash
lm-resizer doctor --json
```

## Use Explicit Command Compression

For noisy commands, ask the agent to run through `exec`. The search example requires ripgrep (`rg`); the Kubernetes example requires `kubectl` and a configured cluster. Neither is installed by LM Resizer:

```bash
lm-resizer exec -- cargo test
lm-resizer exec --stream -- cargo test
lm-resizer exec -- rg -n "TODO|FIXME" .
lm-resizer exec --json -- kubectl get pods -A  # requires kubectl and a configured cluster
```

`exec` runs the command, keeps useful errors and summaries, stores recoverable
raw output for large or failed commands, then sends the filtered text through
the normal compression pipeline.

## Add Project Instructions

Generate helper docs and scripts:

```bash
lm-resizer init-hooks --project-dir . --force
```

Install reversible agent instructions:

```bash
lm-resizer install-hooks --client codex --project-dir . --force
lm-resizer install-hooks --client claude --project-dir . --force
```

Remove them later with:

```bash
lm-resizer uninstall-hooks --client all --project-dir .
```

The installer edits only the marked lm-resizer block. The helpers use
`rewrite` and `rewrite-shell` to recommend an `lm-resizer exec -- ...` command;
they do not execute target commands.

## Add Native Hook Config

Generate experimental project-local hook config:

```bash
lm-resizer init-native-hooks --client all --project-dir .
```

This writes `.codex/hooks.json` and `.claude/settings.json`. File generation
and the handler can be tested locally; automatic execution and rewriting in
a running Claude/Codex agent have not been verified. Confirm support for the
configuration and events in your agent version before relying on them.
An existing file is refused unless you pass `--force`, which overwrites the
whole file: back up and merge any existing settings yourself.
`uninstall-hooks` removes guidance blocks only; to undo native configuration,
remove the generated hook entries or restore your saved configuration.
The generated config wires `lm-resizer hook` on two `Bash` events:

- `PreToolUse` — if the command is supported (git, cargo, vitest/jest, rg, …),
  the hook emits `updatedInput` rewriting it to `lm-resizer exec -- <cmd>`, so
  the model sees the filtered, compressed output in place of the raw dump. The
  command line is preserved verbatim (quoting and backslashes intact), the hook
  never re-wraps its own `exec` invocations, and an unsupported or unparseable
  command emits nothing — the command runs raw. It never blocks.
- `PostToolUse` — records command-output savings telemetry when it can identify
  a command and output, and exits successfully when the event shape is unknown.

## Audit Existing Sessions

If no sessions exist yet, `discover-sessions --agent all --markdown` reports
missing known directories. Once sessions exist, inspect their output:

```bash
lm-resizer discover ~/.claude/projects --recursive --markdown
lm-resizer discover ~/.codex --recursive --json
```

Windows PowerShell (native command arguments do not expand `~`; use the explicit profile path):

```powershell
lm-resizer discover "$env:USERPROFILE\.claude\projects" --recursive --markdown
lm-resizer discover "$env:USERPROFILE\.codex" --recursive --json
```

Explicit paths must exist. The discover command scans logs and session JSON for command/output pairs
without executing anything.

## Review Savings

```bash
lm-resizer stats --markdown
lm-resizer tee list --json
lm-resizer tee read <tee-file-name>
```

Use `tee` only when you need the original raw output that was compressed out of
the agent-facing response.

`stats` records bytes saved by `exec` and MCP proxy output filtering. Its `estimated_tokens_saved` values are
the heuristic **bytes saved / 4**, not a tokenizer measurement or an API billing
measurement. JSON includes this method in `exec_history.token_estimation`;
Markdown explains it beside the estimate. The benchmark's `o200k_base` token
counts are separate measurements.
