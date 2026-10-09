# Known limits of the native views

Filter migration is still in progress. Earlier measurements do not describe this version. See [the benchmark](../bench/native/README.md) for measured differences, remaining commands and raw recovery. French version: [KNOWN-MISSES.fr.md](KNOWN-MISSES.fr.md).

## Hook: commands left as they are

`exec` keeps the output until the process exits (except with `--stream`). The hook (`hook`) and `rewrite-shell` therefore leave unwrapped any command that can read the terminal or not end, direct or inside the line of `sh|bash|zsh -c` (one of its segments is enough). The list is read from `argv`:
- editors, pagers, REPLs and interactive clients (`vim`, `less`, `top`, `ssh`, `psql` without `-c`, `python` without a script…), `docker|kubectl exec -it`;
- network commands that can ask for a credential, a passphrase or a host key: `git push|pull|fetch|clone|ls-remote|submodule|send-email|svn|p4`, `git remote update|prune|show`, `scp`, `mosh`, `ssh-add`, `ssh-copy-id`;
- password or confirmation: `sudo`, `su`, `doas`, `passwd`, `gpg`, `cargo login`, `npm|pnpm|yarn|bun init|create|login|adduser|publish`, `docker login`, `terraform|tofu apply|destroy` without `-auto-approve` or `-input=false`, `terraform console|login`, `aws configure`, `aws sso login`, `aws ssm start-session`, `aws ecs execute-command`, `gh auth login`, `gh pr|issue|repo create` without `--fill`, `--title` or `--web`, `gh pr merge` without a method;
- launched programs and servers: `cargo run`, `cargo r`, `go run`, `dotnet run`, `next dev|start`, `npm|pnpm|yarn|bun` with `dev`, `start`, `serve`, `watch` or `preview`, `make|just|task run|serve|server|dev|start|watch|up`, `mvn spring-boot:run|exec:java|exec:exec|jetty:run|quarkus:dev|liberty:dev`, `gradle run|bootRun|appRun|quarkusDev|jettyRun`, `gradle --continuous`;
- containers: `docker|podman run|create|start` with `-i` or `-t` (without `-d`), `attach`, `compose run|exec|attach` without `-T` or `-d`, `compose up` without `-d`, `kubectl attach|port-forward|proxy|edit`, `kubectl run|debug -it`, `kubectl get -w`;
- followers: `tail -f`, `journalctl -f`, `docker|kubectl logs -f`, `--watch`, `--follow`, `cargo watch`, `vitest` without `run`, `cat` without a file.

Limits: a command outside this list that asks a question (a script, a test that reads its input, `terraform plan` or `init` missing a variable) is still wrapped and its prompt only shows at the end; so is a foreground server started by `docker run` without `-i`/`-t`. `lm-resizer exec --stream` shows the output live.

**`exec` killed abruptly**: on Linux the child started by `exec` receives SIGKILL when `lm-resizer` dies, even through `kill -9` (`PR_SET_PDEATHSIG`). Its own children are not covered: after `kill -9` of `exec -- sh -c 'sleep 40'`, `sh` dies and `sleep` remains (measured on 9 October). Nothing equivalent on macOS or Windows: the child survives an abrupt stop there.

## `git log`: what stays raw

**Rule, closed and read from `argv` only**: a view that removes or rewrites lines applies only to a producer named by `argv` whose output format is established. Everything else comes out **raw, byte for byte** (only the `[FAIL] Command failed (exit code: N)` header is added for a non-zero code). No rule recognises a `git log` from the content of its output.

**Only shortened form**: `git [-C <dir>]… log`, run by `exec` (or `lm-resizer git log`), followed only by `--decorate`, `--all`, `-n <N>`, `-<N>`, `--oneline`, `--graph`, revisions or ranges (words without a leading `-`) and, after `--`, paths; and only if `git config --get-regexp '^(format|log)\.'`, read with the same `-C` in the command's directory, finds no key. The view then shows every commit (header, `Merge:`, `Author:`, `Date:`, title and at most three body lines; the rest is counted in `[+N message lines omitted]`, `Signed-off-by` and `Co-authored-by` trailers are removed). It is refused, and the raw output returned, if the sequence of `commit <hash>` headers in the view differs from the raw one, if the output does not start with a default-format header (so `--oneline` and `--graph` stay raw), or contains a NUL or a `diff --` line.

**Raw**: any other option (`--format`, `--pretty`, `--stat`, `-p`, `--author`, `--date`, `--no-merges`, `-z`…); any other global option (`--no-pager`, `-c`, `--git-dir`…); an alias; `pipe` and `tool-output`; any program in front of `git` (`env`, `timeout`, `nice`, `stdbuf`, `xargs`, a shell, `awk`, `python3 -c`…); a script run by its path or through `PATH`; a command assembled from variables (`g=git; "$g" log`). More broadly, **any program without a native view comes out raw**: the generic summary no longer applies by itself (it remains available on request, `lm-resizer summary -- <command>`). For such a program only two fully structured shapes keep a codec in which every distinct line stays visible: a whole JSON document (`lossless:json-table`, `lossless:json-compact`) and at least twenty lines all prefixed by a log level (`INFO`, `WARN`…), whose consecutive repeats are counted (`lossless:log-runs`). Path folds, removal of a diff's `index` lines and code outlines no longer apply to output from an unknown producer.

**Recipe runners, raw**: `make`/`gmake` (any target), `npm`, `pnpm`, `yarn`, `bun` with `run`, `test`, `start`, `stop` or `restart`, `cargo run`, `go run`, `uv`/`poetry`/`pipenv`/`pdm`/`hatch`/`rye run`, `just`, `task`, and `npm|pnpm|yarn|bun` preceded by an option the reading does not know (`npm --foo test`). They run a line chosen by the user, `git log --format=%s` included. Exception: a test runner named in `argv` itself (`npm run vitest`, `npx jest`, `uv run pytest`) is recognised first, by the single recognition described below.

**Test runners, rule read from `argv`**: a test runner's view is raw, byte for byte (`lossless:test-output`), when `argv` asks to display the test output, or when the runner has no capture. Without such a request, output that a failing test causes to be displayed follows the runner's view; the raw output is in tee. Recognised flags, after the wrappers of the single recognition (below), and behind `sh -c`:
- `cargo test`, `cargo nextest`: `--nocapture`, `--no-capture`, `--show-output`, `--success-output…`, before or after `--`;
- `pytest`, `py.test`, `python -m pytest`: `-s` (also grouped, `-vs`), `--capture=no|tee-sys`, `-p no:capture`, `-r` with `P` or `A`, `--log-cli-level`, `-o log_cli=true` (`-rs` asks for the skipped-test report, not `-s`);
- `go test`: `-v`, `-v=true`, `-test.v…`, `-json`;
- `jest`, `vitest`: `--silent=false`, `--no-silent`, `--disableConsoleIntercept`, `--printConsoleTrace`;
- `dotnet test`: `-v|--verbosity normal|detailed|diagnostic`, `--logger` with verbosity `normal`, `detailed` or `diagnostic`.

Runners without capture, always raw: `rspec`, minitest (`ruby …_test.rb`, `rake test`, `rails test`, also behind `bundle exec`), `mvn` with a phase that runs tests (`test`, `verify`, `package`, `install`, `deploy`), `playwright test`; `gradle` only with `-i`, `--info`, `-d` or `--debug` (it hides test output by default). The built-in `rspec`, `minitest`, `jvm-build` (for those phases) and `js-quality` (for `playwright test`) filters are therefore no longer reached by `exec`. An environment variable exported before the call (`RUST_TEST_NOCAPTURE=1`) is not in `argv`: it is not read.

**Successful test runners: raw** (rule validated on 9 October 2026). A test runner named in `argv` that exits with code 0 is returned raw, byte for byte (`lossless:test-success`), by `exec`, `tool-output`, `pipe` and the MCP `lm_resizer_tool_output` tool. Reason: a passing test can write anything to the output it inherits (`std::io::stdout().write_all`, a child process), a relayed `git log` (counter-review of `de2ff41`: 22 of 22 subjects lost, code 0) or a warning (`Permission denied`, a CVE line: audit of 9 October), and the view kept only the summary. A nonzero code keeps the runner's view. `pipe` without `--exit-code` counts as code 0. The benchmark's ten test-runner captures all exit with a nonzero code: the rule changes no benchmark view.

**A single recognition of test runners** (`test_views::runner`, since 10 October): the gate above, the display rule and the test views (`native:cargo-test`, `native:pytest`, `native:go-test`, `native:js-test`, `native:dotnet-test`, `cargo-nextest`) all go through it, and no other route recognises a runner. Any form that a test view reduces on failure is therefore raw with code 0. What it reads:
- the program name in lower case, without path or `.exe`, `.cmd`, `.bat`, `.com`, `.ps1` suffix (`cargo.cmd`, `pytest.EXE`, `npx.cmd`);
- the wrappers `npx|bunx|pnpx`, `npm|pnpm|yarn|bun exec|dlx|x` with their options (`--yes`, `--no-install`, `-p <package>`…), `pnpm|yarn|bun jest|vitest`, `npm|pnpm|yarn|bun run jest|vitest`, `bundle exec`, `uv|poetry|pipenv|pdm|hatch|rye run` with their options, `python`, `python3`, `python3.12`, `pypy3` or `py -3.12` with their options before `-m <module>`;
- the subcommand after global options: `cargo +nightly --locked test`, `cargo t`, `cargo nextest`, `go -C <dir> test`, `npm --prefix app test`;
- the runners: `cargo test|nextest`, `pytest`, `py.test`, `go test`, `jest`, `vitest`, `dotnet test`, `mvn` with a test phase, `gradle` with a `test…`, `check` or `build` task, `rspec`, minitest, `playwright test`, and without a dedicated view `python -m unittest|nose2`, `tox`, `nox`, `mocha`, `ava`, `ctest`, `phpunit`, `pest`, `paratest`, `php artisan test`, `deno|bun|swift|mix|zig test`; the test scripts `npm|pnpm|yarn|bun test|t|tst` or `run test…`, `make|just|task test…|check`.

An unknown wrapper option (`npx -c '<line>'`, `uv run --new-option pytest`) makes the command unreadable: it is not recognised, no test view applies and the output follows the other routes (raw for a script wrapper). Every form of a family gets the failure view of the direct form: `npx jest`, `npm exec jest`, `pnpm dlx jest`, `yarn jest` now get the `jest` one (`native:js-test`), `cargo +nightly test` the `cargo test` one, `uv run --frozen pytest` and `py -3 -m pytest` the `pytest` one. Forms checked one by one: `tests/test_runner_success.rs`.

**Failing test runners: what the view does not keep.** Measured on 9 October through `tool-output --exit-code 1` on failure transcripts to which four lines were added outside the failure blocks (`Permission denied: …`, `CVE-…`, `warning: …`, `error: …`):

| View | Kept | Lost |
|---|---|---|
| `native:cargo-test` | `---- name stdout ----` blocks without their blank lines, names of the `failures:` list that no block covers, `test result:` and `error: test failed…` lines | any other line, including the four added ones and `test … FAILED` |
| `native:pytest` | summary; per failure, the block header of the `FAILURES` section, at most three lines (`>`, `E`, `assert`, `error`, `.py:`) and the reason of `FAILED … - reason` | any line outside the `FAILURES` section, including the four added ones, and the rest of each block |
| `native:js-test` (jest text) | `Tests:` line, `●`, `Expected`, `Received` lines | the rest, including the four added ones and `FAIL  file` |
| `cargo-nextest`, `vitest run`, `dotnet test`, `gradle test` (diagnostic guard), `go test -json` (raw) | the four added lines | nothing that was added |

The raw output stays in tee (`lm-resizer tee read <id>`); `exec --raw-on-failure` returns every failure raw.

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
