# LM Resizer

**Shorten noisy command output before it reaches a coding agent.** LM Resizer is a local Rust CLI that can run a command, keep a compact result for the agent, and save the original output for inspection. It handles test logs, Git output, build diagnostics, JSON and other text; the result depends on the input and command.

![Example of LM Resizer processing command output](docs/lm-resizer-hero.png)

[Français](README.fr.md) · [Website](https://phuetz.github.io/lm-resizer/) · [Benchmark method and all 22 cases](bench/README.md) · [FAQ](docs/FAQ.md) · [Known misses](docs/KNOWN-MISSES.md)

## Install and try it

### Prebuilt binary, after the v0.2.3 release

Once the v0.2.3 release and its binary archives are published, use the command for your platform. The installer verifies the archive's SHA-256 checksum and binary version before placing `lm-resizer` in `~/.local/bin` by default. Prepared platforms are Linux x86_64, macOS x86_64/arm64 and Windows x86_64. The existing v0.2.2 release has no prebuilt archives.

Linux and macOS:

~~~sh
bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/phuetz/lm-resizer/v0.2.3/install.sh | sh'
~~~

Windows PowerShell:

~~~powershell
irm https://raw.githubusercontent.com/phuetz/lm-resizer/v0.2.3/install.ps1 | iex
~~~

On Linux and macOS, add `~/.local/bin` to `PATH` if the installer prompts you. The Windows installer updates the user `PATH`. `LM_RESIZER_INSTALL_DIR` selects another destination. To uninstall a prebuilt binary, remove it from that destination.

### Build from source now

Requires Rust/Cargo (Rust 1.80 or newer). From a checkout of this repository:

~~~bash
cargo install --quiet --path . --locked --root "$HOME/.local"
export PATH="$HOME/.local/bin:$PATH"
lm-resizer --version
~~~

Try it on **real output** from this checkout. `exec` runs the child command, then prints its processed output. The command's exit code is preserved; for a failing command, use `--raw-on-failure` if you need the complete failure output immediately.

~~~bash
lm-resizer exec -- git log -20 '--format=Date: %ad%n%h %s' --date=short
lm-resizer tee list
~~~

The first command reads 20 actual Git commits and removes their date lines from the agent view. When output is shortened, `exec` may show a `[raw: …]` identifier. Pass that identifier to `tee read` to retrieve the original text. Raw files and the CCR store are local; keep them if you need the evidence later. Output can also remain unchanged when compression would not help.

To remove a binary installed with Cargo:

~~~bash
cargo uninstall --root "$HOME/.local" lm-resizer
~~~

The CLI also offers `compress` for files or standard input, `tool-output` for already captured command output, and opt-in MCP, HTTP and agent hook integrations. See [the agent integration guide](docs/CLAUDE_CODEX.md) and [the release guide](docs/RELEASE.md) for those workflows.

## Measured against RTK and Headroom

**Replayed 2026-09-30** with source from revision `30d563d` merged into this branch, on Linux x86_64, 24 logical CPU cores and 93 GiB RAM. The comparison uses RTK 0.50.0, Headroom 0.39.1 with ONNX Runtime 1.24.4, and the same 22 input fixtures. `o200k_base` counts output tokens. A saving counts only when every fact in the case's stated oracle survives; otherwise its *qualified saving* is zero. Three fixtures are captured from real tools; the others are synthetic. [Method, fixtures and full results](bench/README.md).

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
| `git_diff` | 195 | **47%** (103 tokens) | 36% (125 tokens) | 0% | LM Resizer saves more. |
| `compile_error` | 106 | 0% | **26%** | 0% | RTK saves more. |
| Six source-code cases | 379–481 each | 0% | 0% | 0% | No measured saving. |

RTK's raw reductions on seven cases omit at least one required oracle fact and therefore count as zero qualified saving. These fixtures do not measure provider bills, coding-agent task success or performance on arbitrary real-world output. Latency depends on the machine and cache. See the [case-level data](bench/resultats.json) and [remaining misses](docs/KNOWN-MISSES.md).

## When not to use it

- Do not treat a shortened view as a complete audit trail: inspect the saved original for security, compliance or subtle failures.
- Do not expect savings on every input. The six source-code cases above stayed unchanged, and RTK beat LM Resizer on two measured cases (`dotnet_ok` and `compile_error`).
- Do not infer lower API bills or better agent decisions from output-token counts alone. Agent tasks and billing were not tested here.

LM Resizer is Apache-2.0 licensed. [Contributing](CONTRIBUTING.md) · [Security](SECURITY.md)
