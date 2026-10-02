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

Before publication, use a checkout of the `release/v0.2.4-preparation-2026-10-02` candidate supplied by the maintainer. This local branch may not be available on GitHub. **Only after publication**, download the public tagged sources (Bash or PowerShell):

~~~sh
git clone --branch v0.2.4 https://github.com/phuetz/lm-resizer.git
cd lm-resizer
~~~

Run the Rust blocks below from this checkout's root. If you already have a checkout, do not clone it a second time.

Linux/macOS (Bash):

~~~bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path --profile minimal --default-toolchain 1.86.0
. "$HOME/.cargo/env"
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
cargo --version
$installRoot = Join-Path $env:USERPROFILE '.local'
cargo install --quiet --path . --locked --root "$installRoot"
$env:Path = "$installRoot\bin;$env:Path"
lm-resizer --version
~~~

When distribution Rust is already installed, rustup may print `cannot install while Rust is installed`, followed by `continuing (because the -y flag is set and the error is ignorable)`. In the command above this warning is nonfatal: check the exit code and `cargo --version` after sourcing `.cargo/env`; it should report 1.86.0. The distribution Rust remains installed.

Cargo downloads dependencies into `~/.cargo` (`%USERPROFILE%\.cargo` on Windows) even when a build fails; rustup stores compilers in `.rustup`. These caches are separate from the `.local` install prefix. If Cargo is not 1.86.0 in this checkout, check PATH (`command -v cargo` in Bash, `Get-Command cargo` in PowerShell), `rustup show active-toolchain` and any `RUSTUP_TOOLCHAIN` override. Stop if `cargo install` fails: the examples and uninstall command below require a successfully installed binary.

Try it on real output from this checkout. `exec` preserves normal child exit codes; on Unix a signal produces `128 + signal`. stdout and stderr are captured separately, combined with a protected `[stderr]` boundary, and their byte counts are available in JSON. Cross-stream chronology is not reconstructed.

~~~bash
lm-resizer git log -20
lm-resizer exec --raw-on-failure -- cargo test
lm-resizer tee list
lm-resizer gain --history --project
~~~

Repetitive output can use `LMR-LINES/2`: prefixes and earlier lines are referenced without truncating them. Git dates, full hashes and diff lines remain reconstructible. Successful test rows can be summarized by suite counts; diagnostics remain complete. To restore a saved view without the original command:

~~~bash
lm-resizer expand -i view.txt
~~~

A shortened view includes a `[raw: …]` recovery identifier. Use `tee read` with that identifier. Small output can remain unchanged and create no tee entry. Recovery currently covers UTF-8 text: non-UTF-8 bytes are replaced during decoding. Tee files remain local until deletion or purge; CCR entries expire after **30 minutes by default**. Export evidence before expiry if it must last.

From Bash, retrieve a listed original when one exists:

~~~bash
tee_listing=$(lm-resizer tee list)
tee_file=${tee_listing%% *}
if [ -n "$tee_file" ]; then lm-resizer tee read "$tee_file"; fi
~~~

In PowerShell, select the file from the JSON list:

~~~powershell
$teeFiles = lm-resizer tee list --json | ConvertFrom-Json
if ($teeFiles.files.Count -gt 0) { lm-resizer tee read $teeFiles.files[0].name }
~~~

If several files are listed, use the filename or `[raw: …]` identifier for the command you need.

To remove a binary installed with Cargo:

~~~bash
cargo uninstall --root "$HOME/.local" lm-resizer
~~~

Windows (PowerShell):

~~~powershell
cargo uninstall --root "$installRoot" lm-resizer
~~~

The CLI also offers `compress` for files or standard input, `tool-output` for already captured command output, and opt-in MCP, HTTP and agent hook integrations. `install --client all --scope project` also writes Codex user configuration and replaces an existing `mcp_servers.lm_resizer` table without backup; save it before installation. See [the agent integration guide](docs/CLAUDE_CODEX.md) and [the release guide](docs/RELEASE.md) for those workflows.

Copyable examples from this checkout (Bash):

~~~bash
printf 'hello world\n' | lm-resizer compress
git log -20 '--format=Date: %ad%n%h %s' --date=short | lm-resizer tool-output --command 'git log -20'
~~~

`compress` reads text from stdin; `tool-output` filters already captured output. Its `--command` argument describes the command and does not execute it. Small output may stay unchanged.

To configure all four clients from your project root, after backing up any existing Codex configuration:

~~~bash
lm-resizer install --client all --scope project
~~~

This writes project files and your account's Codex configuration, including with `--scope project`.

## Reproducible token statistics

`lm-resizer stats --markdown` reports exact text token counts using the existing **tiktoken-rs / o200k_base** tokenizer (GPT-4o family). `exec`, `tool-output` and `compress` JSON expose `original_tokens`, `compressed_tokens`, signed `tokens_saved`, `tokenizer` and `token_count_method: "exact"`. Counts include the final recovery markers; a negative saving means the output uses more tokens. This reference encoding is not a claim about Claude, Llama or provider billing.

New execution history records persist both counts. Stats keep existing byte counters and JSON fields, and add measured token totals and `measured_commands` / `unmeasured_commands`. Old records lack the text needed for recounting: their `estimated_tokens_saved` remains explicitly labeled **legacy bytes / 4**, separate from measured totals. `discover`, `discover-sessions`, `eval` and `learn` tokenize available original and filtered text; these are prospective filter savings. For JSON compatibility, `estimated_tokens_saved` in discovery/evaluation is an alias of the exact prospective `tokens_saved`, not a byte estimate.

[Counting method, compatibility and reproduction](docs/TOKEN-STATISTICS.md). The real benchmark below uses both `o200k_base` and `cl100k_base`.

## Measured on real commands

**2026-10-02, Linux x86_64:** 30 commands across pinned ripgrep, FastAPI and TypeScript repositories, plus five comparison cases. Python tiktoken 0.14.0 counts actual outputs with both encodings. These are unweighted per-command savings, including recovery markers. RTK's savings remain counted even when its output fails a fact check.

| Encoding / statistic | LMR before | LMR after | RTK 0.50.0 |
|---|---:|---:|---:|
| cl100k median | 0.00% | 3.74% | 43.28% |
| cl100k mean | 7.94% | 13.21% | 47.33% |
| o200k median | 0.00% | 3.75% | 43.21% |
| o200k mean | 7.96% | 13.26% | 47.41% |

LMR preserves **160,014/160,014 declared facts**, all 35 exit codes, and 24 changed originals recovered byte-for-byte from tee. The before views missed 359 facts under the strengthened oracle, which checks multiplicity. The grep regression was traced to `bb85e73`; the repair uses reversible views rather than restoring its old row limit.

**RTK still has much higher median savings.** On this corpus, its pytest view omits the `457 errors` collection count, and its recursive TypeScript listing omits `lib.dom.d.ts` and `checker.ts`. LMR keeps those facts. Retaining them costs tokens; feature parity and comparable median savings are **not achieved**.

On the pinned TypeScript listing (2.65 MB), the integrated CLI takes a median **0.191 s**, maximum **0.200 s**, across five cold processes. The original real-command replay took **43.6 s** on this machine. The six large-output performance cases pass their strict thresholds; these measurements do not guarantee timing on other machines.

[Reproduction and limitations](bench/real/README.md) · [Full before/after table](bench/real/RESULTATS.md) · [Feature inventory and remaining gaps](bench/real/PARITE-RTK.md) · [Performance benchmark](bench/perf/README.md).

The previous mostly synthetic, “qualified savings” scoreboard is retained only as [historical evidence](bench/README.md), not as a current headline or a general claim. The real benchmark includes failed test commands and incomplete Python dependencies. RTK uses identical captures via `pipe` where available and live commands otherwise; those modes are labeled. A failed strict RTK oracle can also reflect an unrecognized reformulation, so its count is not presented as a universal semantic-loss measure.

## When not to use it

- Do not treat a shortened view as a complete audit trail: inspect the saved original for security, compliance or subtle failures.
- Do not expect savings on every input. Short commands and text with little repetition may stay unchanged.
- Do not infer lower API bills or better agent decisions from output-token counts alone. Agent tasks and billing were not tested here.

LM Resizer is Apache-2.0 licensed. [Contributing](CONTRIBUTING.md) · [Security](SECURITY.md)
