# Known misses in the Sol comparison (30 September 2026)

Scope: the 23 versioned fixtures in `bench/cases.json`. A saving counts only when the declared oracle is fully retained. Two cases lose to RTK; none had a technical execution error, and Headroom won none of these cases. The six code fixtures with zero saving were ties, not losses. These figures describe this corpus, not general performance. The current three-tool replay was run from implementation commit `83e9b25`; its measurements are in `bench/resultats.json` and `bench/RAPPORT.md`. `bench/resultats-avant.json` uses an older corpus and is not comparable for these captures (`bench/README.md`).

## `dotnet_ok` — success summary remains verbose

- **What happens:** On the real `dotnet test` capture (`bench/corpus/dotnet_ok.txt`), LM Resizer removes setup lines but keeps the long `Passed!` line and adds a raw-output reference. Its output retains the `65 passed` oracle (`bench/cases.json`). RTK emits a shorter success summary.
- **Exact measurement:** `bench/resultats.json`: 111 input tokens, LM Resizer 55 output tokens (50.45% saving), RTK 21 (81.08%); both retain 100% of the oracle. Headroom retains all 111 tokens (`bench/resultats.json`). The benchmark's rounded comparison is at `bench/RAPPORT.md`.
- **Likely cause:** The structured .NET route handles TRX or binlog files, then returns `None` when neither is available (`src/parity_filters.rs`). The console route selects the TOML filter (`src/main.rs`), whose `Passed!` rule retains the original line (`src/main.rs`); `apply_toml_filter` joins retained lines without turning the counts into a compact summary (`src/main.rs`).
- **Possible fix:** Parse the successful console verdict into a short summary that preserves passed, failed, skipped, total and test scope. Keep the existing failure blocks and source locations. Test against actual .NET output variants before enabling the rewrite.

## `git_diff` — former miss, fixed

- **What happened:** On the real two-file patch (`bench/corpus/git_diff.txt`), the filter retained each `diff --git` header as well as the `---` and `+++` path markers. The patch changes and all eight declared oracle facts (`bench/cases.json`) survived, but the duplicate headers cost tokens.
- **Earlier measurement (revision `9e73489`):** 195 input tokens, LM Resizer 133 output tokens (31.79% saving), RTK 125 (35.90%); both retained the full stated oracle.
- **Cause and fix:** `filter_diff_summary` kept all three path headers (`src/main.rs`). The generic failure regex also sees the word `diff` in `diff --git` as a diagnostic (`crates/lm-resizer-core/src/transforms/diagnostic_gate.rs`), so simply removing that line triggered the diagnostic fallback (`src/main.rs`). The filter now omits `diff --git` only when both `---` and `+++` exist in that file block, and the diagnostic guard excludes this structural line. A binary or truncated diff keeps its Git header (`src/main.rs`).
- **Current full three-tool replay:** The same fixture and `o200k_base` tokenizer yield 195 → 103 tokens (47.18% saving) for LM Resizer, versus 125 tokens (35.90%) for RTK, with the full stated oracle retained by both (`bench/resultats.json`, `bench/RAPPORT.md`). The regression test is `git_diff_capture_omits_redundant_file_headers_without_losing_patch_facts` in `src/main.rs`.

## `compile_error` — the diagnostic guard restores all text

- **What happens:** The real failed `cargo build` capture (`bench/corpus/compile_error.txt`) is returned verbatim. LM Resizer preserves the file location, error code and type mismatch (`bench/cases.json`), but saves no tokens. RTK keeps the same oracle in a shorter output. The CLI exits with code 101, matching the fixture; this is a compression miss, not an execution failure.
- **Exact measurement:** `bench/resultats.json`: 106 input tokens, LM Resizer 106 output tokens (0% saving), RTK 78 (26.42%); both retain 100% of the oracle. Headroom also outputs 106 (`bench/resultats.json`). The rounded comparison is at `bench/RAPPORT.md`.
- **Likely cause:** The `cargo build` route tries `filter_diagnostics` (`src/main.rs`, `src/main.rs`). On this capture the post-filter diagnostic check finds a removed line matching the broad failure signal and restores the raw text (`src/main.rs`, `src/main.rs`). A local replay reports `cargo_diagnostics:diagnostic-guard` and 390 → 390 bytes. The benchmark does not pass `--raw-on-failure`, so that option is not the cause (`bench/src/main.rs`).
- **Possible fix:** Group each compiler diagnostic by its location, code, message and source frame, then classify boilerplate such as `rustc --explain` separately. Compare those structured facts before and after compression so the guard can accept a shorter equivalent diagnostic; cover failures with several diagnostics and notes.

## `git_log_stat` — complete facts, small token saving on the fixture

The former `git_log` filter omitted all 40 dates: 7,728 → 6,870 tokens, but only 440/480 oracle facts survived, so qualified saving was zero. The dedicated route keeps all 480 facts associated with their commit: 7,728 → 7,671 tokens, **57 saved out of 7,728 (0.7%)**, versus zero for RTK and Headroom. The fixture comes from real Git over a generated repository and contains almost only facts; it is not a random production sample. A separate frozen real log saves 978 out of 6,110 tokens (16.0%), with message bodies omitted and raw output recoverable. Histograms can be scaled by Git; `graph:+N/-N` describes bars, not exact per-file line counts. Custom formats, graph prefixes and unrecognized lines are retained conservatively and may yield no saving.

## Exact counting has latency and scope limits

Release median CLI latency in the 23-case replay is 207 ms (previous implementation, same corpus: 14 ms), with RTK at 12 ms and Headroom at 769 ms. The default BPE must be loaded in every short-lived CLI process. Historical byte-only data cannot be retokenized reliably; its estimate is labelled and kept separate. These counters cover final output text, including recovery markers, rather than JSON envelopes, CLI stderr, raw live streaming or provider billing. Whole agent sessions, retrieval costs and provider cache effects remain unmeasured.
