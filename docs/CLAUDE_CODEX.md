# Using lm-resizer With Claude Code Or Codex

lm-resizer is intentionally opt-in. It does not replace your shell or silently
rewrite commands. Claude Code and Codex can use it in four practical ways:

1. MCP compression tools
2. explicit `lm-resizer exec -- ...` command wrapping
3. project hook instructions in `CLAUDE.md` or `AGENTS.md`
4. native hook config for non-blocking PostToolUse savings records

## Install

Build the binary first:

```bash
cargo build --release
```

Then add MCP configuration:

```bash
lm-resizer install --client claude --scope project
lm-resizer install --client codex --scope global
```

For a repository-local setup:

```bash
lm-resizer install --client all --scope project --project-dir /path/to/repo
```

Check the environment:

```bash
lm-resizer doctor --json
```

## Use Explicit Command Compression

For noisy commands, ask the agent to run through `exec`:

```bash
lm-resizer exec -- cargo test
lm-resizer exec --stream -- cargo test
lm-resizer exec -- rg -n "TODO|FIXME" .
lm-resizer exec --json -- kubectl get pods -A
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

Generate project-local Codex and Claude hook config:

```bash
lm-resizer init-native-hooks --client all --project-dir . --force
```

This writes `.codex/hooks.json` and `.claude/settings.json`. The generated hooks
call `lm-resizer hook` on `PostToolUse` / `Bash` events. The handler reads the
event JSON from stdin, records command-output savings when it can identify a
command and output, and exits successfully when the event shape is unknown. It
does not block or rewrite the agent action.

## Audit Existing Sessions

To estimate savings from previous Claude/Codex sessions:

```bash
lm-resizer discover ~/.claude/projects --recursive --markdown
lm-resizer discover ~/.codex --recursive --json
```

The discover command scans logs and session JSON for command/output pairs
without executing anything.

## Review Savings

```bash
lm-resizer stats --markdown
lm-resizer tee list --json
lm-resizer tee read <tee-file-name>
```

Use `tee` only when you need the original raw output that was compressed out of
the agent-facing response.
