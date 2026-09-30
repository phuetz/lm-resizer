# LM Resizer: Frequently Asked Questions

### What do the saved tokens and percentages measure?
They compare raw command-output text with the final output payload, recovery markers included, using `o200k_base` by default. Set `--tokenizer` or `LM_RESIZER_TOKENIZER` to another supported BPE encoding or model name. Fallback models and legacy byte-only history are explicitly estimated; different counters stay separate. JSON envelopes, CLI stderr, raw streaming, follow-up retrievals, reasoning, provider caching and billing are outside this denominator. `discover` measures potential filter savings; it does not run commands. The legacy JSON byte-density fields are retained and deprecated; new consumers should use `token_savings`. Use `gain` for a readable summary and `doctor` when no savings are recorded.

This page provides factual answers about LM Resizer's integration, safety, and benchmarks, based on the repository's data and documentation.

### Is it safe to use shell-rewriting hooks? Are they opt-in? What if parsing fails?
Yes, shell hooks are **strictly opt-in**. You must manually run `lm-resizer init-native-hooks` or `lm-resizer init-shims` to enable them. If the hook handler encounters an unknown bash event or fails to parse a command line, it gracefully exits with success, leaving the command untouched (see `docs/PORTING.md`). LM Resizer **does not** modify your shell or agent configs without your explicit permission.

### Does compression lose important information? How is this measured?
Information loss is strictly measured by ensuring the "oracle" (the facts required to solve the task, such as error codes, line numbers, and expected vs actual values) is fully retained. In our benchmarks (`bench/RAPPORT.md`), an output only counts as a "qualified saving" if 100.0% of the oracle facts are preserved. For the benchmarked compression routes, the raw output is kept locally for recovery through CCR or a tee file. It remains subject to local retention and explicit purge commands; the oracle only checks the facts declared for each fixture.

### Why use LM Resizer instead of `head`, `tail`, or `--quiet`?
Flags like `--quiet` typically suppress the actual error message you need to fix the problem. Truncating with `head` or `tail` often cuts off the middle of the output where the relevant context or failing test details live. LM Resizer uses specific parsers (e.g., `cargo-test`, `vitest`) to extract the exact failures and context without losing them. LM Resizer **does not** magically guess the error; it relies on structured parsing.

### How can I trust the binary?
LM Resizer is open-source and can be built locally with `cargo build --release`. The repository tracks exact SHA-256 hashes of the binaries during benchmarks (`bench/RAPPORT.md` documents the current replay). LM Resizer **does not** run any background telemetry collector or phone home by default.

### What is the license? Does it force me to use Code Explorer?
LM Resizer is licensed under **Apache-2.0**. The optional [Code Explorer](https://github.com/phuetz/code-explorer) tool uses BUSL-1.1 (transitioning to Apache-2.0 in 2030). Because of this difference, integration is strictly via CLI (`code-explorer cypher --repo ...`) to keep distributions and licenses distinct (`docs/COMPRESSION-SYNTAXIQUE.md`). Code Explorer is entirely optional; LM Resizer **does not** require it to function.

### Doesn't RTK already do this?
While both tools compress agent context, LM Resizer preserves a much more complete oracle in many cases. For example, on `git_log`, LM Resizer retains 100% of the oracle compared to RTK's 4%; on `json_large`, 100% vs 33%; and wins outright on commands like `docker` and `psql` where RTK has no filter (`bench/resultats.json`). On `git_diff`, the current replay saves 47% versus RTK's 36%, with both stated oracles complete. LM Resizer remains behind RTK on `dotnet_ok` and `compile_error`.

### Headroom claims 60–95% savings. Why use LM Resizer?
On our suite of 23 real and synthetic fixtures, Headroom achieved a median qualified saving of 0.0% while exhibiting a median latency of 769 ms, compared to LM Resizer's 207 ms. Furthermore, Headroom fell back to its Python detector 0 times out of 23 (`bench/RAPPORT.md`). These timings depend on the machine and cache.

### Will modifying the prompt break provider prompt caching?
For Anthropic requests, `steer_verbosity` checks `frozen_count` and skips injection when the latest user message is frozen; it does not edit the system message. The repository has offline cache-control and live-zone tests. Provider cache-hit rates have not been verified here.

### Why does LM Resizer report 0% savings on source code files?
In our benchmarks, without an external code index, LM Resizer correctly yields 0.0% savings on source code (C#, Rust, Python, TypeScript, Go, Java). To structurally compress source code by keeping signatures but removing inner function bodies, you **must** have Code Explorer installed and indexed (`docs/COMPRESSION-SYNTAXIQUE.md`). LM Resizer **does not** blindly strip source code lines without structural awareness.

### Can it be installed with one command? What about Windows?
The README gives one-command installers for Linux/macOS and Windows, conditional on publication of the v0.2.3 binary release. They check the archive's SHA-256 checksum and binary version. Until those release assets are published, build from source with Cargo. `scripts/install-grok-skill.sh` installs an optional skill, not the binary (`README.md`, `docs/RELEASE.md`).
