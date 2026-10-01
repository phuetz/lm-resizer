# LM Resizer

**Shorten noisy command output before it reaches a coding agent.** LM Resizer is a local Rust CLI that can run a command, keep a compact result for the agent, and save the original output for inspection. It handles test logs, Git output, build diagnostics, JSON and other text; the result depends on the input and command.

![Example of LM Resizer processing command output](docs/lm-resizer-hero.png)

[Français](README.fr.md) · [Website](https://phuetz.github.io/lm-resizer/) · [Benchmark method and all 22 cases](bench/README.md) · [FAQ](docs/FAQ.md) · [Known misses](docs/KNOWN-MISSES.md)

## Install and try it

### Prebuilt binary, after the v0.2.4 release

Once the v0.2.4 release and its binary archives are published, use the command for your platform. The installer verifies the archive's SHA-256 checksum and binary version before placing `lm-resizer` in `~/.local/bin` by default. Prepared platforms are Linux x86_64, macOS x86_64/arm64 and Windows x86_64. The existing v0.2.2 release has no prebuilt archives.

Linux and macOS:

~~~sh
bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/phuetz/lm-resizer/v0.2.4/install.sh | sh'
~~~

Windows PowerShell:

~~~powershell
irm https://raw.githubusercontent.com/phuetz/lm-resizer/v0.2.4/install.ps1 | iex
~~~

On Linux and macOS, add `~/.local/bin` to `PATH` if the installer prompts you. The Windows installer updates the user `PATH`. `LM_RESIZER_INSTALL_DIR` selects another destination. To uninstall a prebuilt binary, remove it from that destination.

### Build from source now

Requires Rust/Cargo (Rust 1.86 or newer), Git and native build tools: a C/C++ compiler and linker on Linux/macOS; Visual Studio Build Tools with MSVC C++ tools and the Windows SDK for the Windows MSVC toolchain. LM Resizer does not install these prerequisites. From a checkout of this repository:

Linux/macOS (Bash):

~~~bash
cargo install --quiet --path . --locked --root "$HOME/.local"
export PATH="$HOME/.local/bin:$PATH"
lm-resizer --version
~~~

Windows (PowerShell):

~~~powershell
$installRoot = Join-Path $env:USERPROFILE '.local'
cargo install --quiet --path . --locked --root "$installRoot"
$env:Path = "$installRoot\bin;$env:Path"
lm-resizer --version
~~~

Try it on **real output** from this checkout. `exec` runs the child command, then prints its processed output. Normal child exit codes are preserved. A child terminated by a signal or a command that cannot be started returns 1; for a failing command, use `--raw-on-failure` if you need the complete failure output immediately.

~~~bash
lm-resizer exec -- git log -20 '--format=Date: %ad%n%h %s' --date=short
lm-resizer tee list
~~~

The first command reads 20 actual Git commits and removes their date lines from the agent view. When output is shortened, `exec` may show a `[raw: …]` identifier. Pass that identifier to `tee read` to retrieve the original text. Recovery covers UTF-8 text: non-UTF-8 bytes are replaced during decoding, and stdout/stderr are combined. Tee files remain local until removed or purged. CCR entries expire after **30 minutes by default**, even if the database file is kept; retrieve and export them before expiry for lasting evidence. Output can also remain unchanged when compression would not help.

To remove a binary installed with Cargo:

~~~bash
cargo uninstall --root "$HOME/.local" lm-resizer
~~~

Windows (PowerShell):

~~~powershell
cargo uninstall --root "$installRoot" lm-resizer
~~~

The CLI also offers `compress` for files or standard input, `tool-output` for already captured command output, and opt-in MCP, HTTP and agent hook integrations. `install --client all --scope project` also writes Codex user configuration and replaces an existing `mcp_servers.lm_resizer` table without backup; save it before installation. See [the agent integration guide](docs/CLAUDE_CODEX.md) and [the release guide](docs/RELEASE.md) for those workflows.

## Reproducible token statistics

`lm-resizer stats --markdown` reports exact text token counts using the existing **tiktoken-rs / o200k_base** tokenizer (GPT-4o family). `exec`, `tool-output` and `compress` JSON expose `original_tokens`, `compressed_tokens`, signed `tokens_saved`, `tokenizer` and `token_count_method: "exact"`. Counts include the final recovery markers; a negative saving means the output uses more tokens. This reference encoding is not a claim about Claude, Llama or provider billing.

New execution history records persist both counts. Stats keep existing byte counters and JSON fields, and add measured token totals and `measured_commands` / `unmeasured_commands`. Old records lack the text needed for recounting: their `estimated_tokens_saved` remains explicitly labeled **legacy bytes / 4**, separate from measured totals. `discover`, `discover-sessions`, `eval` and `learn` tokenize available original and filtered text; these are prospective filter savings. For JSON compatibility, `estimated_tokens_saved` in discovery/evaluation is an alias of the exact prospective `tokens_saved`, not a byte estimate.

[Counting method, compatibility and reproduction](docs/TOKEN-STATISTICS.md). The benchmark below already uses `o200k_base`; its historical fixture results are unchanged.

## Measured against RTK and Headroom

**Replayed 2026-09-30** from merge commit `df30334`, on Linux x86_64. The versioned measurement JSON does not record CPU or RAM. The comparison uses RTK 0.50.0, Headroom 0.39.1 with ONNX Runtime 1.24.4, and the same 22 input fixtures. `o200k_base` counts output tokens. A saving counts only when every fact in the case's stated oracle survives; otherwise its *qualified saving* is zero. Three fixtures are captured from real tools; the others are synthetic. [Method, fixtures and full results](bench/README.md).

| Result across 22 cases | LM Resizer | RTK | Headroom |
|---|---:|---:|---:|
| Sole wins on qualified saving | 13 | 2 | 0 |
| Shared win | 1 with RTK | 1 with LM Resizer | 0 |
| Median qualified saving across cases | 74.7% | 0.0% | 0.0% |
| Complete stated oracle | 22/22 | 15/22 | 22/22 |

Six further cases have **no qualified gain from any tool**. Examples below use the measured input token count and qualified saving; zero can mean unchanged output or an incomplete oracle.

| Case | Input tokens | LM Resizer | RTK | Headroom | What happened |
|---|---:|---:|---:|---:|---|
| `cargo_ok` | 837 | 97% | 97% | 0% | LM Resizer and RTK tie. |
| `logs` | 2,009 | 96% | 95% | 92% | All three retain the stated oracle. |
| `dotnet_ok` | 111 | 50% | **81%** | 0% | RTK saves more. |
| `git_diff` | 195 | **47%** (103 tokens remaining) | 36% (125 tokens remaining) | 0% | LM Resizer saves more. |
| `compile_error` | 106 | 0% | **26%** | 0% | RTK saves more. |
| Six source-code cases | 379–481 each | 0% | 0% | 0% | No measured saving. |

RTK's raw reductions on seven cases omit at least one required oracle fact and therefore count as zero qualified saving. These fixtures do not measure provider bills, coding-agent task success or performance on arbitrary real-world output. Latency depends on the machine and cache. See the [case-level data](bench/resultats.json) and [remaining misses](docs/KNOWN-MISSES.md).

## When not to use it

- Do not treat a shortened view as a complete audit trail: inspect the saved original for security, compliance or subtle failures.
- Do not expect savings on every input. The six source-code cases above stayed unchanged, and RTK beat LM Resizer on two measured cases (`dotnet_ok` and `compile_error`).
- Do not infer lower API bills or better agent decisions from output-token counts alone. Agent tasks and billing were not tested here.

LM Resizer is Apache-2.0 licensed. [Contributing](CONTRIBUTING.md) · [Security](SECURITY.md)
