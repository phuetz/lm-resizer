# Implementation Tracker

Goal: provide a Rust-native context compression stack with speed, predictable
memory use, and parallel processing.

Non-goal: shipping any Python runtime surface. The repository is Cargo-only:
no `.py` files, no Python package manifests, and no Python helper scripts.

## Ported Now

- Rust workspace and standalone `lm-resizer` binary.
- `lm-resizer-core` compression core.
- Content detection, JSON minification, log compression primitives, diff
  compression, conservative source compression, SmartCrusher-backed JSON
  offload, CCR SQLite store.
- CLI commands:
  - `compress`
  - `batch` with parallel Rayon processing
  - `retrieve`
  - `stats`
  - `doctor`
  - `mcp`
  - `install`
  - `exec` with RTK-inspired command output filtering before compression
  - `wrap`
  - `serve`
- Minimal MCP tools:
  - `lm_resizer_compress`
  - `lm_resizer_retrieve`
  - `lm_resizer_stats`
- MCP config installer for Claude Code, Codex, Cursor, and VS Code.
- Explicit command wrapper for `git`, `cargo`, `rg`/`grep`, listings, diffs,
  and generic command output. This adapts the useful RTK idea of reducing noisy
  tool output before agent consumption without installing shell hooks.
- RTK-inspired declarative TOML filter layer for `exec`, with built-in filters
  and project/user override path via `.lm-resizer/filters.toml` or
  `LM_RESIZER_FILTERS`.
- Project-local filter trust by content hash via `trust-filters`, with an
  explicit environment override for controlled local automation.
- `verify-filters` validates TOML filter files and runs inline `[[tests]]`
  cases before trust approval.
- `verify-filters` rejects unknown TOML fields and reports schema diagnostics
  such as duplicate filter names, missing tests, and untested filters.
- `audit-filters` reports the current hash, trusted hash when present, and
  whether the file is untrusted, trusted-current, or trusted-stale before
  approval.
- `audit-filters --review` emits a Markdown approval packet with trust status,
  diagnostics, inline test results, filter actions, and next trust commands.
- `init-filters` creates a project-local starter filter file with inline tests
  so custom filters can be verified and trusted without hand-writing the schema.
- `init-filters --profile` ships generic, Rust, Node, Python, and infra starter
  filters with inline tests.
- `sanitize-provider-fixture` converts real provider JSON into shareable
  fixtures by redacting secret-like keys and replacing long strings with
  placeholders.
- Raw-output tee recovery for large or failed `exec` commands, emitting a
  `[full output: ...]` hint so compressed output remains recoverable.
- Lightweight `exec` savings history in JSONL, summarized by `lm-resizer stats`.
- Lightweight CCR retrieval feedback in JSONL, also summarized by
  `lm-resizer stats`.
- Offline `discover` audit for JSON/JSONL/log/text files that estimates savings
  from command/output pairs without executing commands.
- Lightweight `eval` harness over session/log fixtures for CI or release
  evidence.
- `discover-sessions` provider for known Claude/Codex local session stores,
  reusing the same discover audit without requiring manual paths.
- `rewrite`, `rewrite-shell`, and `init-hooks` primitives for safe hook
  integration: helpers can ask how argv or full shell lines should be rewritten
  without executing them or modifying agent configuration.
- `init-native-hooks` writes project-local Codex `.codex/hooks.json` and Claude
  `.claude/settings.json` PostToolUse hooks that call the Rust `lm-resizer hook`
  handler.
- `hook` reads native hook JSON from stdin, records savings when it recognizes
  a Bash command/output pair, and exits successfully for unknown event shapes.
- `init-shims` writes opt-in PATH command wrappers that automatically route
  known noisy command families through `lm-resizer exec -- <original>`.
- Generated `AGENT_RULES.md` hook guidance that can be referenced by Claude,
  Codex, or project-level agent instructions without auto-editing those files.
- `install-hooks` / `uninstall-hooks` for reversible marked blocks in
  `AGENTS.md` and `CLAUDE.md`.
- `learn` mines session logs plus local `exec` history, writes
  `.lm-resizer/learning` memory, emits Markdown recommendations, and can
  install a separate reversible learned-guidance block in `AGENTS.md` /
  `CLAUDE.md`.
- `exec --stream` for live child output plus post-exit compression/reporting.
- Runtime proxy wrapper launcher for Claude Code, Codex, Cursor, OpenCode,
  OpenClaw, Aider, Copilot, and custom binaries.
- High-level Rust API via `LmResizer` and `CompressionReport`.
- Versioning guidance plus a compilable Rust example for the high-level API.
- Compilable Rust example for a persistent SQLite CCR store via
  `LmResizer::with_store(...)`.
- Release tests keep Cargo crate versions and the npm WASM package version in
  sync.
- Minimal C/WASM ABI exported by `lm-resizer-core` for non-Rust hosts.
- Public C header in `include/lm_resizer.h`.
- npm-style WASM wrapper package under `packages/wasm`.
- Local WASM npm tarball packaging via `scripts/package-wasm.*`.
- Local WASM npm preflight via `scripts/check-wasm-package.*`, validating
  wrapper syntax and required package contents before publication.
- Release evidence generation via `scripts/release-evidence.*`, written to
  `dist/release-evidence.json`.
- Release approval checklist in `docs/RELEASE.md`.
- Contribution and security guidance in `CONTRIBUTING.md` and `SECURITY.md`,
  with GitHub issue templates for provider fixtures and reusable command
  filters.
- Dependabot maintenance for Cargo, npm WASM wrapper metadata, and GitHub
  Actions.
- WASM npm publication dry-run via `scripts/publish-wasm.* --dry-run` /
  `scripts/publish-wasm.ps1 -DryRun`, so release approval can inspect the exact
  publish payload without credentials.
- External WASM npm publication scripts via `scripts/publish-wasm.*`, gated by
  `NPM_TOKEN` or `npm login`.
- GitHub Actions CI runs release checks on Linux and Windows, packages release
  archives, and uploads release evidence plus WASM tarballs as artifacts.
- GitHub Actions `Publish WASM Package` workflow runs the real npm publish via
  the protected `npm-production` environment with npm trusted publishing/OIDC
  and `NPM_TOKEN` fallback.
- Publish-readiness scripts validate the local npm package metadata, protected
  workflow shape, release evidence, tarball presence, and external approval
  requirements before maintainers run the real publish workflow.
- Release checksums via `dist/SHA256SUMS`, plus an optional Windows signing
  helper that requires a maintainer-provided code-signing certificate.
- Proxy embedding smoke scripts start the release binary, call an
  OpenAI-compatible preview route, verify the compressed envelope, and shut the
  proxy down.
- Minimal HTTP endpoints:
  - `GET /health`
  - `POST /compress`
  - `GET /retrieve/:hash`
  - `GET /stats`
  - opt-in `GET /dashboard` when `serve --dashboard` is used
- OpenAI-compatible proxy preview/forwarding endpoints:
  - `POST /v1/chat/completions`
  - `POST /v1/responses`
  - `POST /v1/*` JSON fallback for OpenAI-compatible endpoints such as
    embeddings
  - SSE preview response when `stream=true` and no upstream is configured
  - Non-JSON `/v1/*` upload preview/forwarding without compression.
  - WebSocket preview handling on `GET /v1/*` realtime-style paths.
  - Bidirectional upstream WebSocket bridging on `/v1/*` when `--upstream` is
    configured.
- Anthropic-compatible proxy preview/forwarding endpoint:
  - `POST /v1/messages`
- Bedrock/Vertex-compatible proxy route shapes for compressed preview and
  forwarding:
  - `POST /model/:model_id/invoke`
  - `POST /model/:model_id/invoke-with-response-stream`
  - `POST /v1/projects/:project/locations/:location/publishers/:publisher/models/*model_method`
  - `POST /v1beta/projects/:project/locations/:location/publishers/:publisher/models/*model_method`
- Bedrock SigV4 signing from environment credentials:
  `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, optional `AWS_SESSION_TOKEN`,
  and `AWS_REGION` / `AWS_DEFAULT_REGION`.
- Bedrock credential fallback through `AWS_PROFILE`,
  `AWS_SHARED_CREDENTIALS_FILE`, `AWS_CONFIG_FILE`, and EC2 IMDSv2 role
  credentials.
- Vertex bearer auth from explicit env tokens or the GCE metadata server.
- Vertex service-account JSON ADC flow via `GOOGLE_APPLICATION_CREDENTIALS`:
  lm-resizer signs a JWT assertion with the service-account private key and
  exchanges it at the configured `token_uri`.
- Shareable provider cache fixtures under `fixtures/provider-cache` lock
  OpenAI, Anthropic, Bedrock, and Vertex envelope/cache-marker behavior.
- Deterministic local content detection by default. Optional Magika/ONNX
  classification is available only when `LM_RESIZER_ENABLE_MAGIKA=1` is set.

## Rust Optimizations Already Used

- One compression pipeline instance is reused across `batch` files.
- `batch` uses Rayon to process files in parallel.
- The core pipeline already runs reformat and bloat estimation in parallel.
- CCR storage is persistent SQLite with a short critical section around writes.
- No telemetry subscriber or dashboard is active by default.
- `serve --dashboard` exposes a local HTML view over existing counters only
  when explicitly enabled; no background telemetry collector is started.
- Optional ML classification is not loaded on the hot path unless explicitly
  enabled.
- Local extras:
  - `image` inspects PNG/JPEG/GIF dimensions and size for context-budget
    decisions.
  - `voice` removes common transcript filler tokens.
  - `ml-status` reports optional Magika / ONNX configuration.

## Remaining Implementation Work

### Agent Launchers

- Deeper agent-native hook integrations beyond the current wrapper env mapping,
  helper scripts, reversible instruction blocks, PATH shims, and PostToolUse
  savings hooks.
- More agent-specific session providers beyond the current Claude/Codex local
  store discovery as new clients expose stable session locations.

### Command Filters

- Keep adding niche command filters as users contribute project-specific
  fixtures beyond the current built-in filters, profile templates, and filter
  issue template.
- Expand `init-filters --profile` templates when recurring project ecosystems
  appear in real usage.
- Expand trust review into a fully interactive prompted workflow only if local
  teams outgrow the current explicit `audit-filters --review` and
  `trust-filters` approval flow.

### Proxy

- Add real provider traffic samples once teams run `sanitize-provider-fixture`
  on shareable OpenAI, Anthropic, Bedrock, and Vertex payloads and submit them
  through the provider fixture issue template.

### SDKs

- Add deeper host-specific proxy embedding examples only when real downstream
  wrappers need code beyond the current release smoke scripts.
- Execute the `Publish WASM Package` workflow once repository maintainers have
  configured the protected `npm-production` environment and npm trusted
  publishing or `NPM_TOKEN` secret.
- Sign public Windows binaries once maintainers provide a real code-signing
  certificate; until then publish checksums and avoid broad antivirus
  allowlisting.

### Observability Without Runtime Cost

- Keep explicit counters returned by commands.
- Avoid background telemetry.
- Add metrics/export integrations only behind explicit opt-in features if they
  are needed later.


## Suggested Order

1. Finish CLI/MCP parity for compression and retrieval.
2. Complete proxy routes one provider at a time.
3. Expand wrappers only for clients that are installed and testable locally.
4. Add memory/learn after the hot path is stable.
5. Add optional extras behind disabled-by-default Cargo features.
