# LM Resizer

**Shorten noisy command output before it reaches a coding agent.** LM Resizer is a local Rust CLI that can run a command, keep a compact result for the agent, and save the original output for inspection. It handles test logs, Git output, build diagnostics, JSON and other text; the result depends on the input and command.

![Example of LM Resizer processing command output](docs/lm-resizer-hero.png)

[Français](README.fr.md) · [Comparison benchmark](bench/native/README.md)

[Windows: TLS fallback, PowerShell 5.1 and byte-exact recovery](docs/WINDOWS.md).

## Install and try it

### Prebuilt binary, after the v0.2.4 release

**Do not run the commands below before the v0.2.4 release and its archives are published: they currently return HTTP 404. To install now, follow “Build from source” below.**

Only after publication, use the command for your platform. The installer verifies the archive's SHA-256 checksum and binary version before placing `lm-resizer` in `~/.local/bin` by default. Prepared platforms are Linux x86_64, macOS x86_64/arm64 and Windows x86_64. The existing v0.2.2 release has no prebuilt archives.

Linux and macOS. Prerequisites: a POSIX `sh` (Bash is not required), `tar`, `gzip`, and `curl` or `wget`. Minimal Debian/Ubuntu images have neither (Alpine ships a limited `wget`): install curl first (`sudo apt-get update && sudo apt-get install -y curl ca-certificates` on Debian/Ubuntu, `sudo apk add curl` on Alpine; drop `sudo` when you are root, as in a container). With wget instead of curl, download the file with `wget -qO install.sh https://raw.githubusercontent.com/phuetz/lm-resizer/v0.2.4/install.sh`, then run `sh install.sh`. The Linux x86_64 binary is static (no glibc, no libstdc++): it runs on Debian 12, Ubuntu 20.04+ and Alpine.

~~~sh
curl -fsSL https://raw.githubusercontent.com/phuetz/lm-resizer/v0.2.4/install.sh -o install.sh && sh install.sh
~~~

Windows PowerShell:

~~~powershell
irm https://raw.githubusercontent.com/phuetz/lm-resizer/v0.2.4/install.ps1 | iex
~~~

On Linux and macOS the installer prints the line to add when `~/.local/bin` is not on your `PATH`; to make it permanent in Bash, run `echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc` (use `~/.zshrc` for zsh) and open a new terminal. For the current terminal only:

~~~sh
export PATH="$HOME/.local/bin:$PATH"
lm-resizer --version
~~~

The Windows installer updates the user `PATH`. `LM_RESIZER_INSTALL_DIR` selects another destination. To uninstall a prebuilt binary, remove it from that destination.

### Build from source

The CLI requires Rust **1.91 or newer**; `rust-toolchain.toml` pins **1.95.0** with rustfmt and clippy. Use official rustup when the distribution compiler is older.

Install these system prerequisites before Rust:

- **Debian 13 / Ubuntu**: `curl`, HTTPS certificates, Git, a C compiler (`cc`/`gcc`), a C++ compiler (`c++`/`g++`), libc headers and a linker (`binutils`, pulled in by GCC). In a terminal with sudo (or as root without `sudo`):

~~~bash
sudo apt-get update
sudo apt-get install -y --no-install-recommends ca-certificates curl git gcc g++ libc6-dev
~~~

- **macOS**: install Xcode Command Line Tools (`xcode-select --install`), which provide Git, Clang/Clang++, headers and the SDK. Finish the installation dialog before continuing.
- **Windows x86_64**: install [Git for Windows](https://git-scm.com/downloads/win) and [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022) with **Desktop development with C++**, **MSVC C++ x64/x86 tools** and the **Windows SDK**. Keep existing installations if these components are already available.

The build was checked on Linux x86_64 with Rust 1.95.0. Other platforms have not been reverified.

If Rust is missing or too old, run the rustup installation steps at the start of your platform’s block. If rustup is already installed and its bin directory is on PATH, you can skip those steps: it will install the repository’s selected toolchain. The installer below selects 1.95.0 as your account’s default; `--no-modify-path` leaves your profile files and permanent PATH intact. Subsequent commands set PATH for the current terminal only. [Official Rust installation instructions](https://www.rust-lang.org/tools/install/).

Before publication, use a checkout of the `feat/filtres-natifs-2026-10-03` candidate supplied by the maintainer. This local branch may not be available on GitHub. **Only after publication**, download the public tagged sources (Bash or PowerShell):

~~~sh
git clone --branch v0.2.4 https://github.com/phuetz/lm-resizer.git
cd lm-resizer
~~~

Run the Rust blocks below from this checkout's root. If you already have a checkout, do not clone it a second time.

Linux/macOS (Bash):

~~~bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path --profile minimal --default-toolchain 1.95.0
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
& $rustupInstaller -y --no-modify-path --profile minimal --default-toolchain 1.95.0
$env:Path = "$(Join-Path $env:USERPROFILE '.cargo\bin');$env:Path"
cargo --version
$installRoot = Join-Path $env:USERPROFILE '.local'
cargo install --quiet --path . --locked --root "$installRoot"
$env:Path = "$installRoot\bin;$env:Path"
lm-resizer --version
~~~

When distribution Rust is already installed, rustup may print `cannot install while Rust is installed`, followed by `continuing (because the -y flag is set and the error is ignorable)`. In the command above this warning is nonfatal: check the exit code and `cargo --version` after sourcing `.cargo/env`; it should report 1.95.0. The distribution Rust remains installed.

Cargo downloads dependencies into `~/.cargo` (`%USERPROFILE%\.cargo` on Windows) even when a build fails; rustup stores compilers in `.rustup`. These caches are separate from the `.local` install prefix. If Cargo is not 1.95.0 in this checkout, check PATH (`command -v cargo` in Bash, `Get-Command cargo` in PowerShell), `rustup show active-toolchain` and any `RUSTUP_TOOLCHAIN` override. Stop if `cargo install` fails: the examples and uninstall command below require a successfully installed binary. If `~/.local/bin/lm-resizer` already exists (for example after a prebuilt install), Cargo refuses with `binary lm-resizer already exists in destination`; remove that file or rerun with `--force`.

`exec` preserves the producer’s status (128 + signal on Unix). Shell commands, `--stream` and `--raw-on-failure` retain separate streams and the `[stderr]` marker.

Check the install works anywhere, with no repository needed:

~~~bash
lm-resizer exec -- echo hello
~~~

The next examples assume what they wrap is present: `git` and a current directory inside a Git repository (outside one, Git itself fails with exit 128 and there is nothing to shorten), and a Rust project with Cargo for `cargo test`. If a wrapped command is not installed, `exec` reports `cannot execute <name>: command not found` and exits 127.

~~~bash
lm-resizer git log -20
lm-resizer exec --raw-on-failure -- cargo test
lm-resizer tee list
lm-resizer gain --history --project
~~~

The audited grep/find/listing/file/git/container/linter views retain literal numbers, paths, identifiers, commit authors and diagnostics. They no longer emit `LMR-LINES` or `LMR-TEXT` references. Recognized successful Cargo/pytest progress can be summarized by suite counts; failures remain visible. `lm-resizer expand -i view.txt` reconstructs reversible views, including `Patch v1` and `Patch v2` histories and diffs: shared file headers and identical runs are factored without dropping source or context lines.

A sufficiently reduced view may display `[tee:<id>]`. Read it with `lm-resizer tee read <id>`; otherwise `tee list` and the JSON `tee_hint` field provide recovery without adding tokens to the view.

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

`lm-resizer --version` reports `lm-resizer 0.2.4`. JSON recovery metadata uses a marker such as `[raw: e3b0c44298fc]`; the archives have a `.log` extension.

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

[Counting method, compatibility and reproduction](docs/TOKEN-STATISTICS.md). The parity benchmark below uses `o200k_base`.

## Native filters and measurements

The product uses its own Rust and TOML filters. Normal `exec` drains a shared stdout/stderr pipe through EOF, without a 10 MiB ceiling. Use `lm-resizer tee list` and `lm-resizer tee read <id>` to recover the complete original. The optional `--stream` and `--raw-on-failure` paths still capture the streams separately.

**Median saving including tee on the official 61-capture corpus: 22.79%, up from 21.74%, against a 15.81% reference.** Mean saving is 31.78%; all 61 raw recoveries and producer exit codes pass. These corpus medians are not a claim about arbitrary live repositories. Interleaved startup measurements give 7.70 ms (7.68 ms before). Windows byte-contract replays pass on Linux; actual Windows installation remains unverified. [Current measurements, exact differences and limitations](bench/native/windows-release/delivery.md).

Explicit inspection commands include `err`, `test`, `summary`, `json`, `deps`, `env`, `format`, `outline` and `dedup`. File reads remain literal. Reversible path/match folds, JSON tables and identical-line runs supplement command filters; syntax outlines and exact repeated-message folding are opt-in. [Additional agent hooks](docs/AGENT_HOOKS.md) support Gemini, Copilot and Cursor configuration.

`env` masks names containing `PASSPHRASE` (including `PASSPHRASE_FILE`) and a `PASS` name component. This deliberately also masks benign names such as `PASS_COUNT`; filtering is conservative, based on names and credential URL shapes.

## When not to use it

- Do not treat a shortened view as a complete audit trail: inspect the saved original for security, compliance or subtle failures.
- Do not expect savings on every input. Short commands and text with little repetition may stay unchanged.
- Do not infer lower API bills or better agent decisions from output-token counts alone. Agent tasks and billing were not tested here.

LM Resizer is Apache-2.0 licensed. [Contributing](CONTRIBUTING.md) · [Security](SECURITY.md)

[Credits and inspiration licenses](THIRD-PARTY-NOTICES).
