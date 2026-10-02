# LM Resizer

**Shorten noisy command output before it reaches a coding agent.** LM Resizer is a local Rust CLI that can run a command, keep a compact result for the agent, and save the original output for inspection. It handles test logs, Git output, build diagnostics, JSON and other text; the result depends on the input and command.

![Example of LM Resizer processing command output](docs/lm-resizer-hero.png)

[Français](README.fr.md) · [Website](https://phuetz.github.io/lm-resizer/) · [Benchmark method and all 22 cases](bench/README.md) · [FAQ](docs/FAQ.md) · [Known misses](docs/KNOWN-MISSES.md)

## Install and try it

### Prebuilt binary, after the v0.2.4 release

**Do not run the commands below before the v0.2.4 release and its archives are published: they currently return HTTP 404. To install now, follow “Build from source” below.**

Only after publication, use the command for your platform. The installer verifies the archive's SHA-256 checksum and binary version before placing `lm-resizer` in `~/.local/bin` by default. Prepared platforms are Linux x86_64, macOS x86_64/arm64 and Windows x86_64. The existing v0.2.2 release has no prebuilt archives.

Linux and macOS:

~~~sh
bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/phuetz/lm-resizer/v0.2.4/install.sh | sh'
~~~

Windows PowerShell:

~~~powershell
irm https://raw.githubusercontent.com/phuetz/lm-resizer/v0.2.4/install.ps1 | iex
~~~

On Linux and macOS, add `~/.local/bin` to `PATH` if the installer prompts you. The Windows installer updates the user `PATH`. `LM_RESIZER_INSTALL_DIR` selects another destination. To uninstall a prebuilt binary, remove it from that destination.

### Build from source

Rust **1.86.0 or newer** is required. Debian 13 ships Rust 1.85.1, which is too old: use **official rustup**, rather than only the distribution Cargo. The repository’s `rust-toolchain.toml` selects 1.86.0 through rustup’s commands and includes `rustfmt` and `clippy` for development checks. An explicit `cargo +stable` or `RUSTUP_TOOLCHAIN` can override this selection.

Install these system prerequisites before Rust:

- **Debian 13 / Ubuntu**: `curl`, HTTPS certificates, Git, a C compiler (`cc`/`gcc`), a C++ compiler (`c++`/`g++`), libc headers and a linker (`binutils`, pulled in by GCC). In a terminal with sudo (or as root without `sudo`):

~~~bash
sudo apt-get update
sudo apt-get install -y --no-install-recommends ca-certificates curl git gcc g++ libc6-dev
~~~

- **macOS**: install Xcode Command Line Tools (`xcode-select --install`), which provide Git, Clang/Clang++, headers and the SDK. Finish the installation dialog before continuing.
- **Windows x86_64**: install [Git for Windows](https://git-scm.com/downloads/win) and [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022) with **Desktop development with C++**, **MSVC C++ x64/x86 tools** and the **Windows SDK**. Keep existing installations if these components are already available.

The default native build was verified on Debian 13 with Rust 1.86.0 **without `make`, `pkg-config` or `cmake`**. GCC alone is insufficient: `esaxx-rs` compiles C++. SQLite and Oniguruma are bundled; their system development packages are not required. Optional features, including `--features magika`, are outside this installation test.

If Rust is missing or too old, run the rustup installation steps at the start of your platform’s block. If rustup is already installed and its bin directory is on PATH, you can skip those steps: it will install the repository’s selected toolchain. The installer below selects 1.86.0 as your account’s default; `--no-modify-path` leaves your profile files and permanent PATH intact. Subsequent commands set PATH for the current terminal only. [Official Rust installation instructions](https://www.rust-lang.org/tools/install/).

Linux/macOS (Bash):

~~~bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path --profile minimal --default-toolchain 1.86.0
. "$HOME/.cargo/env"
git clone --branch release/v0.2.4-recette-2026-10-01 https://github.com/phuetz/lm-resizer.git
cd lm-resizer
cargo --version
cargo install --quiet --path . --locked --root "$HOME/.local"
export PATH="$HOME/.local/bin:$PATH"
lm-resizer --version
~~~

Windows x86_64 (PowerShell):

~~~powershell
$rustupInstaller = Join-Path $env:TEMP 'rustup-init.exe'
Invoke-WebRequest -Uri 'https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe' -OutFile $rustupInstaller
& $rustupInstaller -y --no-modify-path --profile minimal --default-toolchain 1.86.0
$env:Path = "$(Join-Path $env:USERPROFILE '.cargo\bin');$env:Path"
git clone --branch release/v0.2.4-recette-2026-10-01 https://github.com/phuetz/lm-resizer.git
cd lm-resizer
cargo --version
$installRoot = Join-Path $env:USERPROFILE '.local'
cargo install --quiet --path . --locked --root "$installRoot"
$env:Path = "$installRoot\bin;$env:Path"
lm-resizer --version
~~~

Cargo downloads dependencies into `~/.cargo` (`%USERPROFILE%\.cargo` on Windows) even when a build fails; rustup stores compilers in `.rustup`. These caches are separate from the `.local` install prefix. If Cargo is not 1.86.0 in this checkout, check PATH (`command -v cargo` in Bash, `Get-Command cargo` in PowerShell), `rustup show active-toolchain` and any `RUSTUP_TOOLCHAIN` override. Stop if `cargo install` fails: the examples and uninstall command below require a successfully installed binary.

Try it on **real output** from this checkout. `exec` runs the child command, then prints its processed output. Normal child exit codes are preserved. A child terminated by a signal or a command that cannot be started returns 1; for a failing command, use `--raw-on-failure` if you need the complete failure output immediately.

~~~bash
lm-resizer exec -- git log -20 '--format=Date: %ad%n%h %s' --date=short
lm-resizer tee list
~~~

Output excerpts captured on Debian 13 with Rust 1.86.0 at commit `86bf58f` (commit IDs, recovery filenames and paths will vary on your machine):

`lm-resizer --version`:

~~~text
lm-resizer 0.2.4
~~~

`exec`: first three lines, removed-line summary and final recovery marker:

~~~text
86bf58f Livrer la documentation des jetons dans les archives binaires
4694506 Corriger les avertissements Clippy dans les tests du workspace
5a12cfb Éviter le débordement de pile du CLI de développement sous Windows
... omitted 20 low-signal lines
[raw: e6c2d124d10b]
~~~

`lm-resizer tee list`:

~~~text
e6c2d124d10b5924173b779b04b40df78dce01a9678d825a706afa3bd3ad556a.log 1786 bytes /qa/debian-home/.local/state/lm-resizer/tee/e6c2d124d10b5924173b779b04b40df78dce01a9678d825a706afa3bd3ad556a.log
~~~

A small result can stay unchanged, with no saving or recovery marker:

~~~bash
lm-resizer exec -- git rev-parse --short HEAD
~~~

~~~text
86bf58f
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
