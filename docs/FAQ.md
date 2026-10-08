# LM Resizer: Frequently Asked Questions

This page provides factual answers about LM Resizer's integration, safety, and benchmarks, based on the repository's data and documentation.

### Is it safe to use shell-rewriting hooks? Are they opt-in? What if parsing fails?
Yes, shell hooks are **strictly opt-in**. You must manually run `lm-resizer init-native-hooks` or `lm-resizer init-shims` to enable them. If the hook handler encounters an unknown bash event or fails to parse a command line, it gracefully exits with success, leaving the command untouched (see `docs/PORTING.md`). LM Resizer **does not** modify your shell or agent configs without your explicit permission.

### Does compression lose important information? How is this measured?
Information loss is strictly measured by ensuring the "oracle" (the facts required to solve the task, such as error codes, line numbers, and expected vs actual values) is fully retained. In our benchmarks (`bench/RAPPORT.md`), an output only counts as a "qualified saving" if 100.0% of the oracle facts are preserved. For the benchmarked compression routes, the raw output is kept locally for recovery through CCR or a tee file. CCR expires after 30 minutes by default; export retrieved text before expiry. Tee files have no CCR TTL and remain until removal or purge. Exec recovery is UTF-8 text, with stdout/stderr combined and invalid bytes replaced; the oracle only checks the facts declared for each fixture.

### Why use LM Resizer instead of `head`, `tail`, or `--quiet`?
Flags like `--quiet` typically suppress the actual error message you need to fix the problem. Truncating with `head` or `tail` often cuts off the middle of the output where the relevant context or failing test details live. LM Resizer uses specific parsers (e.g., `cargo-test`, `vitest`) to extract the exact failures and context without losing them. LM Resizer **does not** magically guess the error; it relies on structured parsing.

### How can I trust the binary?
LM Resizer is open-source and can be built locally with `cargo build --release`. The repository tracks exact SHA-256 hashes of the binaries during benchmarks (`bench/RAPPORT.md` documents the current replay). LM Resizer **does not** run any background telemetry collector or phone home by default.

### What is the license? Does it force me to use Code Explorer?
LM Resizer is licensed under **Apache-2.0**. The optional [Code Explorer](https://github.com/phuetz/code-explorer) tool uses BUSL-1.1 (transitioning to Apache-2.0 in 2030). Because of this difference, integration is strictly via CLI (`code-explorer cypher --repo ...`) to keep distributions and licenses distinct (`docs/COMPRESSION-SYNTAXIQUE.md`). Code Explorer is entirely optional; LM Resizer **does not** require it to function.


### Will modifying the prompt break provider prompt caching?
In the standalone output module (not wired into the HTTP proxy), for Anthropic requests, `steer_verbosity` checks `frozen_count` and skips injection when the latest user message is frozen; it does not edit the system message. The repository has offline cache-control and live-zone tests. Provider cache-hit rates have not been verified here.

### Why does LM Resizer report 0% savings on source code files?
In the six small source-code fixtures of the benchmark, the output stays unchanged without an external code index. This is not a guarantee for all source files: long files can produce an approximate structural summary. `compress --input` and `smart` try indexed Code Explorer symbols when available (`docs/COMPRESSION-SYNTAXIQUE.md`) and fall back to ordinary compression when advice is unavailable. Inspect the original through CCR when you need the complete source.

### Can it be installed with one command? What about Windows?
The README gives one-command installers for Linux/macOS and Windows, conditional on publication of the v0.2.6 binary release. They check the archive's SHA-256 checksum and binary version. Building from source with Cargo is also available. `scripts/install-grok-skill.sh` installs an optional skill, not the binary (`README.md`, `docs/RELEASE.md`).
