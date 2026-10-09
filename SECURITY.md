# Security Policy

## Supported Versions

The repository is pre-1.0. Security fixes should target the current `main`
branch unless maintainers publish versioned support windows later.

## Reporting a Vulnerability

Report security issues privately through GitHub Security Advisories when the
repository is hosted on GitHub. If advisories are unavailable, contact the
maintainers through a private channel before opening a public issue.

Do not include live credentials, private prompts, proprietary provider payloads,
or unsanitized customer data in public issues.

## Sensitive Data Handling

`lm-resizer` is designed to run locally and does not enable background telemetry.
Some commands can store raw command output locally for recovery:

- `exec` may write raw output to the local state directory;
- `stats` may summarize local savings and retrieval counters;
- `sanitize-provider-fixture` is available for preparing shareable provider
  payloads.

Set `LM_RESIZER_TEE=0` to disable raw-output recovery and
`LM_RESIZER_TRACKING=0` to disable local history/retrieval counters.

The state directory is created with mode 0700 and its files (raw-output archives,
`exec-history.jsonl`, the CCR database) with mode 0600 on Unix, whatever the caller's
umask. Directories and files created by an earlier version keep their old mode: run
`chmod -R go-rwx ~/lm-resizer` (or your `LM_RESIZER_STATE_DIR`) once. The history stores
command lines as typed and is not redacted: treat it like a shell history file.

A raw-output archive (`tee/<sha256>.log`) is created with `O_CREAT|O_EXCL|O_NOFOLLOW` and never
follows a symbolic link. A file that already exists under that name is used only if the open
descriptor shows a regular file with a single link, owned by the current user, with no group or
other permission; its content is then compared with the new output and rewritten if it differs (a
file of the right length but altered content was announced as the original). Otherwise nothing
is written, no `[tee:]` is shown and a warning names the file: symbolic link, hard link, FIFO,
directory (before 0.2.6, a link planted in a writable tee directory received the raw output,
secrets included). The history
(`exec-history.jsonl`) and the hook counter file (`hook-audit.jsonl`) are still opened in append
mode without these checks: keep the state directory private (0700, as created).

The local proxy (`serve`, `wrap`) has no client authentication. It listens on loopback only
unless `--allow-non-loopback` is given, answers only requests whose `Host` is local, and
follows no upstream redirect. Any local account that can reach the port can still use the
upstream key it holds: do not run it on a shared machine. Give the key through
`LM_RESIZER_API_KEY` or `--api-key-file`, never on the command line.

## Agent hooks and permission rules

The native hooks (`init-native-hooks`, `install-hooks`) rewrite a supported Bash command
into `lm-resizer exec -- <command>` through `updatedInput`.

- For Claude Code the hook does not send `permissionDecision`: the rewritten command goes
  through the normal approval prompt. Only the Codex handler answers `allow`, because Codex
  marks the hook as failed otherwise. Before 0.2.6 the Claude hook answered `allow` as well,
  and Claude Code ran the rewritten command without asking.
- For Cursor the hook answers `permission: "ask"` with `updated_input`. Before 0.2.6 it answered
  `allow`, which the Cursor Hooks documentation (read on 9 October 2026) defines as "proceed".
  What that documentation says: `allow` proceeds, `deny` blocks, `ask` "is accepted by the schema
  but not enforced for `preToolUse` today"; a response that does not match the schema blocks the
  action, and `permission` is not marked optional; across hooks `deny` wins over `ask` and `ask`
  over `allow`. `ask` is therefore the only valid answer that grants nothing and cannot override
  another hook's refusal. What it does not say, and was not checked in a running Cursor: whether
  `updated_input` is applied with `ask`, whether Cursor then asks the user or proceeds, and how an
  empty output (the hook prints nothing for a command it does not rewrite) is treated. To keep
  Cursor's own approval only, do not install the Cursor hook, or remove it with
  `uninstall-hooks --client cursor`.
- A command received as a JSON array is an argument vector: the rewritten command stays an array
  (`["<lm-resizer>", "exec", "--", …]`), run without a shell. Before 0.2.6 the elements were
  joined with spaces, so `*` was expanded and `--format=%h %s` split in two. An array with a
  non-text element is not rewritten.
- The hook rewrites only a command that a view really reduces, and never one that can read its
  input or not end: `exec` keeps the output until the process exits, so a `Password:` prompt, a
  host-key question or a server log would never show. A script (`npm test`, `npm run server`,
  `make run-server`), a program without a view (`php -S`, `docker run`) and a Git subcommand
  without a view (`git credential fill`, `git lfs pull`) run directly; so do, even with a view,
  `git push|pull|fetch|clone`, every `docker|podman run|exec|…` and `compose run|exec` (with or
  without `-T`), `kubectl exec|run|debug`, `sudo`, `ssh`, `scp`, Terraform without `-input=false`,
  `pytest --pdb`, `pip uninstall` without `-y`… (rule read from `argv`, full list in
  `docs/KNOWN-MISSES.md`). A command with a view that prompts and is not in the list is still
  captured until it exits.
- `uninstall-hooks --client all` removes the native hook files of all five clients (Codex,
  Claude, Gemini, Copilot, Cursor) when they still match the generated content; before 0.2.6 it
  left the Cursor hook. `uninstall --client all --scope project` removes the MCP server entry.
- A permission rule matches the command text. After the rewrite the text is
  `lm-resizer exec -- <command>`, so a `deny` rule written for the original, such as
  `Bash(cargo test)`, no longer applies. Checked on Claude Code 2.1.294 with `allow: Bash` and
  `deny: Bash(cargo test)`: the rewritten command ran. Adding `Bash(*exec -- cargo test*)` to
  `deny` blocked it. Add such a rule for every command you deny, or do not install the hook.
  An `allow` rule for the original form likewise stops matching.

## Supply Chain

The WASM package is published manually through a protected GitHub Actions
environment. Prefer npm trusted publishing/OIDC with provenance enabled. Token
publishing is supported only as a fallback for repositories that have not enabled
trusted publishing yet.

Release packaging writes `dist/SHA256SUMS` for public artifact verification.
Windows binaries are unsigned unless maintainers run
`scripts/sign-windows-release.ps1` with a real code-signing certificate.

## Antivirus False Positives

Local Cargo builds create unsigned `.exe` and `.dll` files under `target/`.
Some antivirus products may flag those artifacts heuristically. Treat alerts as
real until checked, but prefer deleting build artifacts with `cargo clean` over
restoring quarantined files.

For maintainers, the expected verification path is:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\check-release.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\package-release.ps1
Get-Content dist\SHA256SUMS
```

If a Windows release binary is flagged, compare its SHA-256 with `SHA256SUMS`
and submit the exact artifact/hash to the antivirus vendor for false-positive
review. Do not ask users to whitelist broad directories.
