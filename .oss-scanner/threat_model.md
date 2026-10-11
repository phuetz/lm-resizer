# Threat model — LM Resizer

## What this project does and where untrusted input enters
LM Resizer (`lm-resizer`) is a command proxy for AI coding agents. An agent runs `lm-resizer exec -- <command>` (often installed as a pre-tool hook in Claude Code, Codex and others, which rewrites the agent's shell commands). LM Resizer runs the command, shows the agent a reduced view of its output to save context, and archives the full raw output (the "tee") so it can be read back with `lm-resizer tee read <id>`.

Untrusted input:
- the command line the agent asks to run. It may come from a prompt-injected agent, and it is passed through hook rewriting;
- the command's stdout/stderr, which is arbitrary bytes (non-UTF-8, control characters, huge outputs, output crafted to look like a known format);
- the hook payloads (JSON) received from the agent host;
- project-level filter files (TOML) found in the repository being worked on, which a cloned malicious repository can control.

## Components that matter most / least
Most important:
- hook installation and the hook responders (`src/hooks*`, `install-hooks`). A hook must NEVER widen the agent's permissions (for example by answering "allow" and bypassing the host's ask/deny rules), and a rewritten command must never do more than the original. Command injection through rewriting is critical.
- the exec path: the exit code must always be propagated, and signals, children and stdin/`/dev/tty` handling must not hang or orphan processes.
- the raw archive (tee): file permissions, location, symlink and race handling, and whether it stores secrets that appear in output.
- output views: a reduced view must never hide a FAILURE (exit status, error lines, security warnings), nor silently drop records (for example commits in `git log`), so that the agent does not believe that a failing command succeeded.

Less important: the benchmark harness under `bench/`, docs, and `examples/`.

## How to exercise it
`lm-resizer exec -- <cmd>` and `lm-resizer exec --json -- <cmd>`; `lm-resizer tee read <id>`; `lm-resizer install-hooks --help`. Integration tests are in `tests/`. Use an isolated HOME (`HOME=$(mktemp -d)`) so that hook installation never touches a real agent configuration.

## How you rate severity
- Hook permission widening, command injection through rewriting, or arbitrary command execution triggered by repository content: critical.
- Tee archive readable by other local users, or secrets written somewhere unexpected: high.
- A view that hides a non-zero exit code or an error from the agent: high. A view that silently drops data while the command succeeded: medium, unless the raw output cannot be recovered with `tee read`, in which case it is high.
- Denial of service (hang, unbounded memory use) on crafted output: medium.

## Anything to leave alone
Known and documented limits are listed in `docs/KNOWN-MISSES.md`. Please do not report them unless you can show impact beyond what is documented.
