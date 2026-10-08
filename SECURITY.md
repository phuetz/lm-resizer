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

The local proxy (`serve`, `wrap`) has no client authentication. It listens on loopback only
unless `--allow-non-loopback` is given, answers only requests whose `Host` is local, and
follows no upstream redirect. Any local account that can reach the port can still use the
upstream key it holds: do not run it on a shared machine. Give the key through
`LM_RESIZER_API_KEY` or `--api-key-file`, never on the command line.

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
