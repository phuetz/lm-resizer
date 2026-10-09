# Known limits of the native views

Filter migration is still in progress. Earlier measurements do not describe this version. See [the benchmark](../bench/native/README.md) for measured differences, remaining commands and raw recovery. French version: [KNOWN-MISSES.fr.md](KNOWN-MISSES.fr.md).

## `git log`: what stays raw

**Rule, closed and read from `argv` only**: a view that removes or rewrites lines applies only to a producer named by `argv` whose output format is established. Everything else comes out **raw, byte for byte** (only the `[FAIL] Command failed (exit code: N)` header is added for a non-zero code). No rule recognises a `git log` from the content of its output.

**Only shortened form**: `git [-C <dir>]… log`, run by `exec` (or `lm-resizer git log`), followed only by `--decorate`, `--all`, `-n <N>`, `-<N>`, `--oneline`, `--graph`, revisions or ranges (words without a leading `-`) and, after `--`, paths; and only if `git config --get-regexp '^(format|log)\.'`, read with the same `-C` in the command's directory, finds no key. The view then shows every commit (header, `Merge:`, `Author:`, `Date:`, title and at most three body lines; the rest is counted in `[+N message lines omitted]`, `Signed-off-by` and `Co-authored-by` trailers are removed). It is refused, and the raw output returned, if the sequence of `commit <hash>` headers in the view differs from the raw one, if the output does not start with a default-format header (so `--oneline` and `--graph` stay raw), or contains a NUL or a `diff --` line.

**Raw**: any other option (`--format`, `--pretty`, `--stat`, `-p`, `--author`, `--date`, `--no-merges`, `-z`…); any other global option (`--no-pager`, `-c`, `--git-dir`…); an alias; `pipe` and `tool-output`; any program in front of `git` (`env`, `timeout`, `nice`, `stdbuf`, `xargs`, a shell, `awk`, `python3 -c`…); a script run by its path or through `PATH`; a command assembled from variables (`g=git; "$g" log`). More broadly, **any program without a native view comes out raw**: the generic summary no longer applies by itself (it remains available on request, `lm-resizer summary -- <command>`). For such a program only two fully structured shapes keep a codec in which every distinct line stays visible: a whole JSON document (`lossless:json-table`, `lossless:json-compact`) and at least twenty lines all prefixed by a log level (`INFO`, `WARN`…), whose consecutive repeats are counted (`lossless:log-runs`). Path folds, removal of a diff's `index` lines and code outlines no longer apply to output from an unknown producer.

**Recipe runners, raw**: `make`/`gmake` (any target), `npm`, `pnpm`, `yarn`, `bun` with `run`, `test`, `start`, `stop` or `restart`, `cargo run`, `go run`, `uv`/`poetry`/`pipenv run`, `just`, `task`. They run a line chosen by the user, `git log --format=%s` included. Exception: a harness named in `argv` itself keeps its view (`npm run vitest`, `npx jest`, `uv run pytest`). `npm exec <program>`, `npx`, `pnpm exec|dlx` and `bundle exec` designate the program that follows, recognised or raw.

**Test runners, rule read from `argv`**: a test runner's view is raw, byte for byte (`lossless:test-output`), when `argv` asks to display the test output, or when the runner has no capture. Without such a request, output that a failing test causes to be displayed follows the runner's view; the raw output is in tee. Recognised flags, after the `npx`, `bunx`, `npm|pnpm|yarn|bundle exec`, `uv run`, `python -m`, `pnpm|yarn|bun jest|vitest` wrappers, and behind `sh -c`:
- `cargo test`, `cargo nextest`: `--nocapture`, `--no-capture`, `--show-output`, `--success-output…`, before or after `--`;
- `pytest`, `py.test`, `python -m pytest`: `-s` (also grouped, `-vs`), `--capture=no|tee-sys`, `-p no:capture`, `-r` with `P` or `A`, `--log-cli-level`, `-o log_cli=true` (`-rs` asks for the skipped-test report, not `-s`);
- `go test`: `-v`, `-v=true`, `-test.v…`, `-json`;
- `jest`, `vitest`: `--silent=false`, `--no-silent`, `--disableConsoleIntercept`, `--printConsoleTrace`;
- `dotnet test`: `-v|--verbosity normal|detailed|diagnostic`, `--logger` with verbosity `normal`, `detailed` or `diagnostic`.

Runners without capture, always raw: `rspec`, minitest (`ruby …_test.rb`, `rake test`, `rails test`, also behind `bundle exec`), `mvn` with a phase that runs tests (`test`, `verify`, `package`, `install`, `deploy`), `playwright test`; `gradle` only with `-i`, `--info`, `-d` or `--debug` (it hides test output by default). The built-in `rspec`, `minitest`, `jvm-build` (for those phases) and `js-quality` (for `playwright test`) filters are therefore no longer reached by `exec`. An environment variable exported before the call (`RUST_TEST_NOCAPTURE=1`) is not in `argv`: it is not read.

**Outside the rule, measured** (output that a **failing** test causes to be displayed without any flag; transcripts passed through `tool-output`, 23-commit repository, `git log --format=%s` printed by the test):

| Runner and situation | View | Visible subjects |
|---|---|---:|
| `pytest` without `-s`, failing test, `Captured stdout call` section | `native:pytest`, 1,023 → 199 bytes | **0/22** |
| `jest` directly, a failing suite, `console.log` of a passing test | `native:js-test`, 683 → 127 bytes | **0/22** |
| real `cargo test` without a flag, a test that prints then fails | `native:cargo-test`, 595 → 526 bytes | 22/22, the empty line of the commit without a message is removed (`%s %H`: everything kept) |
| `npx jest`, `yarn jest`, `vitest run`, `npx vitest run`, `yarn vitest run`, text `go test`, same situations | raw view or diagnostic kept | 22/22 |

These losses only affect hash-free formats (`%s`, `%B`): as soon as a hash appears in the lines (`%s %H`, indented hash, hash followed by a NUL), the content guard returns the raw output (measured on the same failing pytest and jest transcripts: 23/23 hashes, `native:git-identity-guard` through `exec`).

Proposal, not applied: keep pytest's `Captured …` blocks and jest's `console.*` blocks verbatim in their views (no benchmark capture contains them). This is a view change, not a rule change.

**Log viewers, documented limit**: `docker logs`, `kubectl logs`, `journalctl` and `gh run view --log` display the output of other programs and keep their log views; a `git log` printed by a container or a service can lose lines there. The raw output can always be recovered with `lm-resizer tee read <id>`. Build and quality tools (`cargo build`, `tsc`, `eslint`, `docker build`) and project or user TOML filters, which choose their command by regular expression, also remain outside the guarantee.

Other limits: configuration is read where `lm-resizer` runs; body lines are cut at 100 characters in the view, the title never is; the content guard (`commit <hash>` or a word of 4 to 40 hexadecimal digits at the start of a line, and its subject) remains a last line of defence on the shortened paths, recognises a hash at the start of a line (also indented, or followed by a NUL: `%H%x00%s`) or at the end of a line after a subject (`%s %H`, 7 to 40 digits); message prose in the default format is not read. A word such as `added` or `face` at the start of a line can make it return the raw output (less compression, never a loss).

Measured consequence on the 61-capture benchmark: the five multi-commit `git log` captures, passed through `pipe`, go from 91-97% "reduction" (first commit only, as the comparison oracle still does) to 0%; the two `npm test` captures (`TypeScript-test`, `visible-jest`) are raw. Median 4.87%, mean 27.61%.
