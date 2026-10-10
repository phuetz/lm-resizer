//! Test transcripts parsed into summaries and diagnostic sections. Unknown
//! grammars stay literal, including compiler errors before a runner starts.
use std::collections::BTreeMap;

fn counts(line: &str) -> BTreeMap<&str, usize> {
    let words: Vec<_> = line.split_whitespace().collect();
    words
        .windows(2)
        .filter_map(|w| Some((w[1].trim_end_matches([',', ';']), w[0].parse().ok()?)))
        .collect()
}

/// Nom d'un programme : dernier composant du chemin, en minuscules, sans suffixe d'exécutable
/// Windows (`.exe`, `.cmd`, `.bat`, `.com`, `.ps1`). `py.test` et `python3.12` restent entiers.
pub fn program_name(arg: &str) -> String {
    let name = arg
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(arg)
        .to_ascii_lowercase();
    for suffix in [".exe", ".cmd", ".bat", ".com", ".ps1"] {
        if let Some(stem) = name.strip_suffix(suffix) {
            return stem.to_string();
        }
    }
    name
}

/// `python`, `python3`, `python3.12`, `pythonw`, `pypy3` et le lanceur Windows `py`.
fn is_python(name: &str) -> bool {
    name == "py"
        || name
            .strip_prefix("python")
            .or_else(|| name.strip_prefix("pypy"))
            .is_some_and(|rest| rest == "w" || rest.chars().all(|c| c.is_ascii_digit() || c == '.'))
}

/// Famille d'un lanceur de tests reconnu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `cargo test`, `cargo t`.
    Cargo,
    /// `cargo nextest`.
    Nextest,
    /// `pytest`, `py.test`, `python -m pytest`.
    Pytest,
    /// `go test`.
    Go,
    /// `jest`, `vitest`.
    Js,
    /// `dotnet test`.
    Dotnet,
    /// `mvn` avec une phase de tests.
    Maven,
    /// `gradle` avec une tâche `test…`, `check` ou `build`.
    Gradle,
    /// `rspec`, minitest (`ruby …_test.rb`, `rake test`, `rails test`).
    Ruby,
    /// `playwright test`.
    Playwright,
    /// Lanceurs sans vue dédiée : `python -m unittest|nose2`, `tox`, `nox`, `mocha`, `ava`,
    /// `ctest`, `phpunit`, `pest`, `paratest`, `php artisan test`, `deno|bun|swift|mix|zig test`.
    Other,
    /// Script de tests choisi par l'utilisateur : `npm|pnpm|yarn|bun test`, `run test…`,
    /// `make|just|task test…|check`.
    Script,
}

/// Lanceur de tests reconnu : sa famille, et la commande à partir du lanceur (enveloppes retirées).
#[derive(Clone, Copy, Debug)]
pub struct Runner<'a> {
    pub kind: Kind,
    pub argv: &'a [String],
}

/// LA reconnaissance des lanceurs de tests. La porte « lanceur de tests à code 0 = sortie brute »
/// d'`exec`, `tool-output`, `pipe` et du MCP, et le routage vers les vues de tests (`cargo`, pytest,
/// `go`, jest/vitest, `dotnet`, nextest), passent tous par elle : aucune autre fonction ne
/// reconnaît un lanceur de tests (règle validée le 9 octobre 2026). Lecture de l'`argv` seul :
/// - noms normalisés par [`program_name`] (`cargo.cmd`, `pytest.EXE`, `npx.cmd`) ;
/// - enveloppes `npx|bunx|pnpx`, `npm|pnpm|yarn|bun exec|dlx|x`, `pnpm|yarn|bun <jest|vitest>`,
///   `npm|pnpm|yarn|bun run <jest|vitest>`, `bundle exec`, `uv|poetry|pipenv|pdm|hatch|rye run`,
///   `python[X.Y] [options] -m <module>`, `py -3.12 -m`, avec leurs options ;
/// - sous-commande lue après les options globales (`cargo +nightly --locked test`, `go -C d test`).
///
/// Une option d'enveloppe ou de gestionnaire de paquets que la lecture ne connaît pas rend `None` :
/// on ne sait pas quel programme tourne, aucune vue de tests ne s'applique (la sortie suit les
/// autres routes, brute pour un script).
pub fn runner(command: &[String]) -> Option<Runner<'_>> {
    let argv = after_wrappers(command)?;
    let program = program_name(argv.first()?);
    let args = &argv[1..];
    let has = |word: &str| args.iter().any(|arg| arg == word);
    // `test`, `test:unit`, `:app:testDebugUnitTest`, `check` : un segment qui parle de tests.
    let test_task = |task: &str| {
        task.to_ascii_lowercase()
            .split(':')
            .any(|part| part.contains("test") || part == "check")
    };
    let kind = match program.as_str() {
        "cargo" => {
            let skip = args.iter().take_while(|arg| arg.starts_with('+')).count();
            let rest = &args[skip..];
            match rest
                .get(operand(rest, CARGO_VALUE_OPTIONS, None)?)?
                .as_str()
            {
                "test" | "t" => Kind::Cargo,
                "nextest" => Kind::Nextest,
                _ => return None,
            }
        }
        "pytest" | "py.test" => Kind::Pytest,
        "go" if args.get(operand(args, &["-C"], None)?)? == "test" => Kind::Go,
        "jest" | "vitest" => Kind::Js,
        "dotnet" if args.get(operand(args, &[], None)?)? == "test" => Kind::Dotnet,
        "mvn" | "mvnw" if runs_maven_tests(args) => Kind::Maven,
        "gradle" | "gradlew"
            if args
                .iter()
                .any(|arg| !arg.starts_with('-') && (test_task(arg) || arg == "build")) =>
        {
            Kind::Gradle
        }
        "rspec" => Kind::Ruby,
        "ruby" | "rake" | "rails" if runs_ruby_tests(&program, args) => Kind::Ruby,
        "playwright" if has("test") => Kind::Playwright,
        "unittest" | "nose2" | "tox" | "nox" | "mocha" | "ava" | "ctest" | "phpunit" | "pest"
        | "paratest" => Kind::Other,
        "php" if has("artisan") && has("test") => Kind::Other,
        "deno" | "swift" | "mix" | "zig" if args.first().is_some_and(|a| a == "test") => {
            Kind::Other
        }
        "npm" | "pnpm" | "yarn" | "bun" => {
            let at = operand(
                args,
                PACKAGE_MANAGER_VALUE_OPTIONS,
                Some(PACKAGE_MANAGER_FLAGS),
            )?;
            match args[at].as_str() {
                "test" if program == "bun" => Kind::Other,
                "test" | "t" | "tst" => Kind::Script,
                "run" | "run-script" | "rum" | "urn"
                    if args.get(at + 1).is_some_and(|script| test_task(script)) =>
                {
                    Kind::Script
                }
                _ => return None,
            }
        }
        "make" | "gmake" | "just" | "task"
            if args
                .iter()
                .any(|arg| !arg.starts_with('-') && !arg.contains('=') && test_task(arg)) =>
        {
            Kind::Script
        }
        _ => return None,
    };
    Some(Runner { kind, argv })
}

impl Runner<'_> {
    /// La commande demande explicitement d'afficher la sortie des programmes testés, ou le lanceur
    /// n'a pas de capture : la vue du lanceur est refusée et la sortie rendue brute, octet pour
    /// octet, quel que soit le code. Un test peut imprimer n'importe quoi, `git log --format=%s`
    /// compris ; les vues de ce module ne gardent que bilans et échecs.
    /// - `cargo test`, `cargo nextest` : `--nocapture`, `--no-capture`, `--show-output`,
    ///   `--success-output…`, avant ou après `--` ;
    /// - pytest : `-s` (aussi groupé, `-vs`), `--capture=no|tee-sys`, `-p no:capture`, `-r` avec
    ///   `P` ou `A` (sortie des tests réussis), `--log-cli-level`, `-o log_cli=true` ;
    /// - `go test` : `-v`, `-v=true`, `-test.v…`, `-json` ;
    /// - `jest`, `vitest` : `--silent=false`, `--no-silent`, `--disableConsoleIntercept`,
    ///   `--printConsoleTrace` ;
    /// - `dotnet test` : `-v|--verbosity normal|detailed|diagnostic`, `--logger` dont la verbosité
    ///   est `normal`, `detailed` ou `diagnostic` ;
    /// - sans capture : `rspec`, minitest, `mvn` avec une phase de tests, `playwright test` ;
    ///   `gradle` seulement avec `-i`, `--info`, `-d` ou `--debug`.
    pub fn shows_output(&self) -> bool {
        let args = &self.argv[1..];
        match self.kind {
            Kind::Cargo | Kind::Nextest => args.iter().any(|arg| {
                matches!(
                    arg.as_str(),
                    "--nocapture" | "--no-capture" | "--show-output"
                ) || arg.starts_with("--success-output")
            }),
            Kind::Pytest => pytest_shows_output(args),
            Kind::Go => args.iter().any(|arg| {
                let flag = arg.trim_start_matches('-');
                arg.starts_with('-')
                    && (flag == "v"
                        || flag == "json"
                        || flag == "test.v"
                        || ((flag.starts_with("v=") || flag.starts_with("test.v="))
                            && !flag.ends_with("=false")
                            && !flag.ends_with("=0")))
            }),
            Kind::Js => args.iter().enumerate().any(|(i, arg)| {
                matches!(
                    arg.as_str(),
                    "--silent=false"
                        | "--no-silent"
                        | "--disableConsoleIntercept"
                        | "--disable-console-intercept"
                        | "--printConsoleTrace"
                        | "--print-console-trace"
                ) || (arg == "--silent" && args.get(i + 1).is_some_and(|v| v == "false"))
            }),
            Kind::Dotnet => dotnet_shows_output(args),
            Kind::Ruby | Kind::Maven | Kind::Playwright => true,
            Kind::Gradle => args
                .iter()
                .any(|arg| matches!(arg.as_str(), "-i" | "--info" | "-d" | "--debug")),
            Kind::Other | Kind::Script => false,
        }
    }
}

/// La commande est un lanceur reconnu qui affiche la sortie des tests.
#[cfg(test)]
fn shows_test_output(command: &[String]) -> bool {
    runner(command).is_some_and(|runner| runner.shows_output())
}

/// Sous-commande de `npm|pnpm|yarn|bun` lue après leurs options, ou `None` quand une option
/// inconnue la rend illisible (ou qu'il n'y en a pas).
pub fn package_manager_subcommand(args: &[String]) -> Option<&str> {
    let at = operand(
        args,
        PACKAGE_MANAGER_VALUE_OPTIONS,
        Some(PACKAGE_MANAGER_FLAGS),
    )?;
    Some(args[at].as_str())
}

/// Options globales de `cargo` qui prennent une valeur.
const CARGO_VALUE_OPTIONS: &[&str] = &["--color", "--config", "-Z", "-C", "--explain"];

/// Options de `npm|pnpm|yarn|bun` avant la sous-commande : avec valeur, puis sans valeur. Toute
/// autre option rend la sous-commande illisible.
const PACKAGE_MANAGER_VALUE_OPTIONS: &[&str] = &[
    "--prefix",
    "-C",
    "--dir",
    "-w",
    "--workspace",
    "--filter",
    "-F",
    "--cwd",
    "--registry",
    "--cache",
    "--userconfig",
    "--loglevel",
];
const PACKAGE_MANAGER_FLAGS: &[&str] = &[
    "-s",
    "--silent",
    "-q",
    "--quiet",
    "-y",
    "--yes",
    "-ws",
    "--workspaces",
    "-r",
    "--recursive",
    "--if-present",
    "--color",
    "--no-color",
    "--offline",
    "--prefer-offline",
    "--frozen-lockfile",
    "--verbose",
    "-d",
];

/// Options de `npx|bunx|pnpx` et de `npm exec` : avec valeur, puis sans valeur. `-c`/`--call`
/// exécutent une ligne de shell : programme inconnu.
const EXEC_VALUE_OPTIONS: &[&str] = &[
    "-p",
    "--package",
    "--cache",
    "--userconfig",
    "--registry",
    "--prefix",
    "-w",
    "--workspace",
    "--node-options",
];
const EXEC_FLAGS: &[&str] = &[
    "-y",
    "--yes",
    "--no",
    "--no-install",
    "-q",
    "--quiet",
    "-s",
    "--silent",
    "--ignore-existing",
    "--prefer-offline",
    "--prefer-online",
    "--offline",
    "--no-color",
    "-ws",
    "--workspaces",
    "--include-workspace-root",
];

/// Options de `uv run` (et `poetry|pipenv|pdm|hatch|rye run`) : avec valeur, puis sans valeur.
const RUN_VALUE_OPTIONS: &[&str] = &[
    "--with",
    "--with-editable",
    "--with-requirements",
    "--python",
    "-p",
    "--project",
    "--directory",
    "--package",
    "--extra",
    "--group",
    "--only-group",
    "--no-group",
    "--env-file",
    "--index",
    "--default-index",
    "--index-url",
    "-i",
    "--extra-index-url",
    "--find-links",
    "-f",
    "--cache-dir",
    "--config-file",
    "--color",
    "--resolution",
    "--prerelease",
    "--exclude-newer",
    "--link-mode",
];
const RUN_FLAGS: &[&str] = &[
    "--frozen",
    "--locked",
    "--no-sync",
    "--isolated",
    "--all-extras",
    "--no-dev",
    "--dev",
    "--all-groups",
    "--active",
    "--no-project",
    "--offline",
    "-q",
    "--quiet",
    "-v",
    "--verbose",
    "--no-cache",
    "-n",
    "--refresh",
    "--reinstall",
    "--upgrade",
    "-U",
    "--no-env-file",
    "--exact",
    "--inexact",
    "--no-editable",
    "--compile-bytecode",
    "--native-tls",
    "--no-progress",
];

/// Index du premier opérande de `args` (premier mot qui n'est pas une option), en sautant la valeur
/// des options de `with_value` (`--opt valeur` ; `--opt=valeur` compte pour un mot). `--` termine
/// les options. Avec `known`, une option absente des deux listes rend `None` : son effet est inconnu.
fn operand(args: &[String], with_value: &[&str], known: Option<&[&str]>) -> Option<usize> {
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if arg == "--" {
            return (index + 1 < args.len()).then_some(index + 1);
        }
        if !arg.starts_with('-') || arg == "-" {
            return Some(index);
        }
        let (name, inline) = match arg.split_once('=') {
            Some((name, _)) => (name, true),
            None => (arg.as_str(), false),
        };
        if with_value.contains(&name) {
            index += if inline { 1 } else { 2 };
        } else if known.is_none_or(|flags| flags.contains(&name)) {
            index += 1;
        } else {
            return None;
        }
    }
    None
}

/// Programmes que `pnpm|yarn|bun <programme>` et `npm|pnpm|yarn|bun run <programme>` lancent
/// directement quand ils sont nommés.
fn is_named_harness(word: &str) -> bool {
    matches!(program_name(word).as_str(), "jest" | "vitest")
}

/// La commande sans les enveloppes : le programme qui tourne vraiment, ou `None` quand une option
/// d'enveloppe rend ce programme illisible (`npx -c '<ligne>'`, option inconnue). Partagée par la
/// reconnaissance des lanceurs de tests et par la détection de la sortie en direct du crochet.
pub(crate) fn after_wrappers(command: &[String]) -> Option<&[String]> {
    let mut rest = command;
    loop {
        let name = program_name(rest.first()?);
        let args = &rest[1..];
        let skip = match name.as_str() {
            "npx" | "bunx" | "pnpx" => {
                if args
                    .iter()
                    .any(|arg| arg == "-c" || arg.starts_with("--call"))
                {
                    return None;
                }
                1 + operand(args, EXEC_VALUE_OPTIONS, Some(EXEC_FLAGS))?
            }
            "npm" | "pnpm" | "yarn" | "bun" => {
                let Some(at) = operand(
                    args,
                    PACKAGE_MANAGER_VALUE_OPTIONS,
                    Some(PACKAGE_MANAGER_FLAGS),
                ) else {
                    return Some(rest);
                };
                match args[at].as_str() {
                    "exec" | "dlx" | "x" => {
                        let tail = &args[at + 1..];
                        if tail
                            .iter()
                            .any(|arg| arg == "-c" || arg.starts_with("--call"))
                        {
                            return None;
                        }
                        1 + at + 1 + operand(tail, EXEC_VALUE_OPTIONS, Some(EXEC_FLAGS))?
                    }
                    "run" | "run-script"
                        if args.get(at + 1).is_some_and(|word| is_named_harness(word)) =>
                    {
                        1 + at + 1
                    }
                    word if is_named_harness(word) => 1 + at,
                    _ => return Some(rest),
                }
            }
            "bundle" if args.first().is_some_and(|arg| arg == "exec") => 2,
            "uv" | "poetry" | "pipenv" | "pdm" | "hatch" | "rye"
                if args.first().is_some_and(|arg| arg == "run") =>
            {
                // Options de `run`, puis le programme ; `-m <module>` : le module est le programme.
                let tail = &args[1..];
                let mut index = 0;
                loop {
                    let arg = tail.get(index)?;
                    if matches!(arg.as_str(), "--" | "-m" | "--module") {
                        break 2 + index + 1;
                    }
                    if !arg.starts_with('-') {
                        break 2 + index;
                    }
                    let name = arg.split_once('=').map_or(arg.as_str(), |(name, _)| name);
                    if RUN_VALUE_OPTIONS.contains(&name) {
                        index += if arg.contains('=') { 1 } else { 2 };
                    } else if RUN_FLAGS.contains(&name) {
                        index += 1;
                    } else {
                        return None;
                    }
                }
            }
            python if is_python(python) => {
                let mut index = 0;
                loop {
                    let Some(arg) = args.get(index) else {
                        return Some(rest);
                    };
                    match arg.as_str() {
                        "-m" => break 1 + index + 1,
                        "-c" | "-" => return None,
                        "-W" | "-X" | "--check-hash-based-pycs" => index += 2,
                        flag if flag.starts_with('-') => index += 1,
                        // Un script : il n'est pas un lanceur reconnu.
                        _ => return Some(rest),
                    }
                }
            }
            _ => return Some(rest),
        };
        if skip >= rest.len() {
            return None;
        }
        rest = &rest[skip..];
    }
}

/// minitest (`ruby …_test.rb`, `ruby -Itest …minitest…`), `rake test`, `rails test`, et `ruby -S
/// rspec`.
fn runs_ruby_tests(program: &str, args: &[String]) -> bool {
    match program {
        "ruby" => args
            .iter()
            .any(|arg| arg == "rspec" || arg.contains("minitest") || arg.ends_with("_test.rb")),
        _ => args.first().is_some_and(|task| {
            let task = task.split(':').next().unwrap_or(task);
            task == "test" || task == "minitest"
        }),
    }
}

/// `mvn` avec une phase ou un but qui lance les tests.
fn runs_maven_tests(args: &[String]) -> bool {
    args.iter().any(|arg| {
        matches!(
            arg.as_str(),
            "test" | "integration-test" | "verify" | "package" | "install" | "deploy"
        ) || arg.starts_with("surefire:")
            || arg.starts_with("failsafe:")
    })
}

/// `pytest` : options courtes groupées (`-vs`), dont `k m p c o r W n` prennent une valeur (la fin de
/// l'argument ou l'argument suivant) ; `-rs` demande le rapport des tests sautés, pas `-s`.
fn pytest_shows_output(args: &[String]) -> bool {
    let shows = |option: char, value: &str| match option {
        'r' => value.contains(['P', 'A']),
        'p' => value == "no:capture",
        'o' => matches!(
            value.to_ascii_lowercase().as_str(),
            "log_cli=true" | "log_cli=1" | "log_cli=yes" | "log_cli=on"
        ),
        _ => false,
    };
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        if arg == "--" {
            break;
        }
        if let Some(long) = arg.strip_prefix("--") {
            let (name, inline) = match long.split_once('=') {
                Some((name, value)) => (name, Some(value.to_string())),
                None => (long, None),
            };
            let value = |index: &mut usize| {
                inline.clone().or_else(|| {
                    let next = args.get(*index).cloned();
                    *index += 1;
                    next
                })
            };
            let found = match name {
                "capture" => value(&mut index).is_some_and(|v| v == "no" || v == "tee-sys"),
                "override-ini" => value(&mut index).is_some_and(|v| shows('o', &v)),
                _ => name.starts_with("log-cli-level"),
            };
            if found {
                return true;
            }
            continue;
        }
        let Some(cluster) = arg.strip_prefix('-') else {
            continue;
        };
        for (offset, option) in cluster.char_indices() {
            if option == 's' {
                return true;
            }
            if "kmpcorWn".contains(option) {
                let attached = &cluster[offset + option.len_utf8()..];
                let value = if attached.is_empty() {
                    index += 1;
                    args.get(index - 1).map(String::as_str).unwrap_or("")
                } else {
                    attached
                };
                if shows(option, value) {
                    return true;
                }
                break;
            }
        }
    }
    false
}

/// Option courte `letter` de pytest, seule ou groupée (`-f`, `-fv`, `-vf`), en respectant les
/// options courtes qui prennent une valeur (`k m p c o r W n` : la fin du groupe ou l'argument
/// suivant) : dans `-kf`, `f` est la valeur de `-k`. Les options longues et ce qui suit `--` ne
/// comptent pas.
pub(crate) fn pytest_short_option(args: &[String], letter: char) -> bool {
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        if arg == "--" {
            break;
        }
        if arg.starts_with("--") {
            continue;
        }
        let Some(cluster) = arg.strip_prefix('-') else {
            continue;
        };
        for (offset, option) in cluster.char_indices() {
            if option == letter {
                return true;
            }
            if "kmpcorWn".contains(option) {
                if cluster[offset + option.len_utf8()..].is_empty() {
                    index += 1;
                }
                break;
            }
        }
    }
    false
}

/// `dotnet test` : verbosité `normal`, `detailed` ou `diagnostic` (`-v d`, `--verbosity:detailed`,
/// `--logger "console;verbosity=detailed"`).
fn dotnet_shows_output(args: &[String]) -> bool {
    let verbose = |value: &str| {
        matches!(
            value.trim_matches('"').to_ascii_lowercase().as_str(),
            "n" | "normal" | "d" | "detailed" | "diag" | "diagnostic"
        )
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let (name, attached) = match arg.find(['=', ':']) {
            Some(at) if arg.starts_with('-') => (&arg[..at], Some(&arg[at + 1..])),
            _ => (arg.as_str(), None),
        };
        let mut value = || {
            attached
                .map(str::to_string)
                .or_else(|| iter.next().cloned())
        };
        let shown = match name {
            "-v" | "--verbosity" => value().is_some_and(|v| verbose(&v)),
            "-l" | "--logger" => value().is_some_and(|v| {
                v.split(';').any(|part| {
                    part.split_once('=').is_some_and(|(key, level)| {
                        key.trim().eq_ignore_ascii_case("verbosity") && verbose(level)
                    })
                })
            }),
            _ => false,
        };
        if shown {
            return true;
        }
    }
    false
}

/// Successful .NET console runs: project discovery is progress; verdicts and
/// counters are facts. Unknown output (including warnings) stays visible.
pub fn dotnet(raw: &str) -> String {
    let mut passed = 0usize;
    let mut skipped = 0usize;
    let mut summaries = 0;
    for row in raw.lines().map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(fields) = row.strip_prefix("Passed!  - ") {
            let mut values = BTreeMap::new();
            for field in fields.split(',') {
                if let Some((name, value)) = field.split_once(':') {
                    if let Ok(number) = value.trim().parse::<usize>() {
                        values.insert(name.trim(), number);
                    }
                }
            }
            let (Some(&ok), Some(&skip), Some(&total), Some(&0)) = (
                values.get("Passed"),
                values.get("Skipped"),
                values.get("Total"),
                values.get("Failed"),
            ) else {
                return raw.into();
            };
            if ok.checked_add(skip) != Some(total) {
                return raw.into();
            }
            passed += ok;
            skipped += skip;
            summaries += 1;
        } else if !(row.contains(" -> ") && row.ends_with(".dll")
            || row.starts_with("Test run for ")
            || row.starts_with("A total of ")
                && row.ends_with(" test files matched the specified pattern."))
        {
            return raw.into();
        }
    }
    if summaries == 0 {
        return raw.into();
    }
    let extra = if skipped > 0 {
        format!(", {skipped} skipped")
    } else {
        String::new()
    };
    format!("{passed} passed, 0 failed{extra}")
}

pub fn cargo(raw: &str) -> String {
    let summaries: Vec<_> = raw
        .lines()
        .filter(|l| l.starts_with("test result:"))
        .collect();
    if summaries.is_empty() {
        return raw.into();
    }
    // Un échec = un bloc `---- nom stdout ----` (message, valeurs, position, rien n'est retouché) ;
    // les noms de la liste finale `failures:` qu'aucun bloc ne couvre sont listés tels quels. Toute
    // ligne hors des blocs qui n'est pas de grammaire connue (avertissement, sortie d'un processus
    // enfant) est gardée, dans l'ordre, en tête de la vue.
    let (details, names, others) = cargo_failures(raw);
    let others: String = others.iter().map(|line| format!("{line}\n")).collect();
    let mut blocks: Vec<String> = details.iter().map(|block| block.join("\n")).collect();
    for name in &names {
        let covered = details.iter().any(|block| {
            block[0]
                .strip_prefix("---- ")
                .and_then(|rest| rest.strip_suffix(" stdout ----"))
                == Some(name.trim())
        });
        if !covered {
            blocks.push((*name).to_string());
        }
    }
    if blocks.is_empty() {
        // Aggregate only successful, completely understood suite summaries.
        let mut passed = 0;
        let mut ignored = 0;
        let mut duration = 0.0;
        for line in &summaries {
            let c = counts(line);
            if c.get("failed").copied() != Some(0) {
                return raw.into();
            }
            passed += c.get("passed").copied().unwrap_or(0);
            ignored += c.get("ignored").copied().unwrap_or(0);
            if let Some(time) = line
                .split("finished in ")
                .nth(1)
                .and_then(|s| s.trim_end_matches('s').parse::<f64>().ok())
            {
                duration += time;
            } else {
                return summaries.join("\n");
            }
        }
        let extra = if ignored > 0 {
            format!(", {ignored} ignored")
        } else {
            String::new()
        };
        let suites = if summaries.len() > 1 {
            format!(", {} suites", summaries.len())
        } else {
            String::new()
        };
        return format!("{others}{passed} passed, 0 failed{extra}{suites} ({duration:.2}s)");
    }
    let (visible, overflow) = blocks.split_at(blocks.len().min(10));
    let numbered: String = visible
        .iter()
        .zip(1..)
        .map(|(block, number)| format!("{number}. {block}\n"))
        .collect();
    let trailer = match overflow.len() {
        0 => String::new(),
        n => format!("\n… +{n} more failures\n"),
    };
    let mut out = format!(
        "{others}FAILURES ({}):\n{numbered}{trailer}\n",
        blocks.len()
    );
    // Chaque bilan de suite une seule fois, puis l'indication de relance de cargo, dans l'ordre.
    for line in raw.lines() {
        if line.starts_with("test result:") || line.starts_with("error: test failed") {
            out.push_str(line);
            out.push('\n');
        }
    }
    out.trim_end().into()
}

/// Blocs `---- nom stdout ----` (sans lignes vides), noms de la liste `failures:`, et lignes hors
/// des blocs qui ne sont pas de grammaire connue, d'une sortie `cargo test`, toutes suites
/// confondues. Grammaire connue, retirée : lignes vides, séparateur `[stderr]` de la capture,
/// `running N tests`, `test … ok|ignored|FAILED`, lignes d'état de cargo (`Compiling`, `Finished`,
/// `Running`, `Doc-tests`…), en-têtes `failures:` et leur liste, `test result:` et
/// `error: test failed` (repris à la fin de la vue).
fn cargo_failures(raw: &str) -> (Vec<Vec<&str>>, Vec<&str>, Vec<&str>) {
    #[derive(PartialEq)]
    enum Zone {
        Outside,
        Detail,
        List,
    }
    const STATUS: [&str; 12] = [
        "Compiling ",
        "Finished ",
        "Running ",
        "Doc-tests ",
        "Blocking ",
        "Downloaded ",
        "Downloading ",
        "Updating ",
        "Locking ",
        "Adding ",
        "Fresh ",
        "Checking ",
    ];
    let known = |line: &str| {
        let row = line.trim_start();
        line.trim().is_empty()
            // Séparateur des deux flux inséré par la capture de `lm-resizer`.
            || line == "[stderr]"
            || line.starts_with("test result:")
            || line.starts_with("error: test failed")
            || (line.starts_with("running ")
                && (line.ends_with(" test") || line.ends_with(" tests")))
            || (line.starts_with("test ")
                && (line.contains(" ... ")
                    || [" ok", " ignored", " FAILED"]
                        .iter()
                        .any(|end| line.ends_with(end))))
            || (line.starts_with(' ') && STATUS.iter().any(|verb| row.starts_with(verb)))
    };
    let mut details: Vec<Vec<&str>> = Vec::new();
    let mut names = Vec::new();
    let mut others = Vec::new();
    let mut zone = Zone::Outside;
    for line in raw.lines() {
        if line.starts_with("test result:") {
            zone = Zone::Outside;
            continue;
        }
        if line == "failures:" {
            zone = Zone::List;
            continue;
        }
        if line.starts_with("---- ") && line.ends_with(" ----") {
            details.push(vec![line]);
            zone = Zone::Detail;
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        match zone {
            Zone::Detail => {
                if let Some(block) = details.last_mut() {
                    block.push(line);
                }
                continue;
            }
            Zone::List if line.starts_with("    ") => {
                names.push(line);
                continue;
            }
            Zone::List => zone = Zone::Outside,
            Zone::Outside => {}
        }
        if !known(line) {
            others.push(line);
        }
    }
    (details, names, others)
}

pub fn pytest(raw: &str) -> String {
    let Some(summary) = raw.lines().rev().find(|l| {
        l.contains(" in ")
            && [
                " passed", " failed", " skipped", " errors", " xfailed", " xpassed",
            ]
            .iter()
            .any(|s| l.contains(s))
            && !l.trim_start().starts_with(['E', '>'])
    }) else {
        return raw.into();
    };
    let c = counts(summary);
    let passed = c.get("passed").copied().unwrap_or(0);
    let failed = c.get("failed").copied().unwrap_or(0);
    let errors = c.get("errors").or(c.get("error")).copied().unwrap_or(0);
    let mut out = format!("Pytest: {passed} passed");
    if failed + errors > 0
        || c.keys()
            .any(|k| matches!(*k, "skipped" | "xfailed" | "xpassed"))
    {
        out.push_str(&format!(", {failed} failed"));
    }
    for key in ["skipped", "xfailed", "xpassed"] {
        if let Some(n) = c.get(key).filter(|n| **n > 0) {
            out.push_str(&format!(", {n} {key}"));
        }
    }
    if errors > 0 {
        out.push_str(&format!(", {errors} errors during collection"));
    }
    let mut details: Vec<Vec<&str>> = Vec::new();
    let mut active = false;
    let mut expected = Vec::new();
    for line in raw.lines().map(str::trim) {
        if line.starts_with("===") {
            active = line.contains("FAILURES");
            continue;
        }
        if line.starts_with("FAILED ") || line.starts_with("ERROR ") {
            details.push(vec![line]);
            active = false;
        } else if line.starts_with("XFAIL ") || line.starts_with("XPASS ") {
            expected.push(line);
        } else if active && line.starts_with("___") {
            details.push(vec![line]);
        } else if active && !line.is_empty() {
            if let Some(block) = details.last_mut() {
                block.push(line);
            }
        }
    }
    if !expected.is_empty() {
        out.push_str("\n\nExpected-failure outcomes:\n");
        for line in expected {
            out.push_str(&format!("  {line}\n"));
        }
        out = out.trim_end().into();
    }
    // Le bloc « ___ Classe.test ___ » de FAILURES et la ligne « FAILED chemin::Classe::test - raison »
    // de la synthèse décrivent le même échec : le second est fondu dans le premier, dont la raison
    // reste visible. Une ligne de synthèse sans bloc (`--tb=no`, autre test) est conservée.
    let block_headers: Vec<Option<String>> = details
        .iter()
        .map(|block| {
            block[0]
                .starts_with("___")
                .then(|| block[0].trim_matches('_').trim().to_string())
        })
        .collect();
    let mut reasons: Vec<Option<&str>> = vec![None; details.len()];
    let mut merged = vec![false; details.len()];
    for (index, block) in details.iter().enumerate() {
        let first = block[0];
        if first.starts_with("___") {
            continue;
        }
        let (name, reason) = first.split_once(" - ").unwrap_or((first, ""));
        let node = name.split_once(' ').map_or(name, |(_, node)| node);
        // `dir/test_x.py::Classe::test[param]` devient `Classe.test[param]`.
        let Some((_, tail)) = node.split_once("::") else {
            continue;
        };
        let wanted = tail.replace("::", ".");
        if let Some(target) = block_headers
            .iter()
            .position(|header| header.as_deref() == Some(wanted.as_str()))
        {
            merged[index] = true;
            if !reason.is_empty() {
                reasons[target] = Some(reason);
            }
        }
    }
    let details: Vec<(usize, &Vec<&str>)> = details
        .iter()
        .enumerate()
        .filter(|(index, _)| !merged[*index])
        .collect();
    if !details.is_empty() {
        out.push_str("\n\nFailures:\n");
        for (i, (original, block)) in details.iter().take(10).enumerate() {
            let first = block[0];
            if first.starts_with("___") {
                out.push_str(&format!(
                    "{}. [FAIL] {}\n",
                    i + 1,
                    first.trim_matches('_').trim()
                ));
                let mut shown = Vec::new();
                for line in block
                    .iter()
                    .skip(1)
                    .filter(|l| {
                        // Un « E » seul est la ligne vide d'un diff, sans information.
                        **l != "E"
                            && (l.starts_with(['>', 'E'])
                                || l.to_lowercase().contains("assert")
                                || l.to_lowercase().contains("error")
                                || l.contains(".py:"))
                    })
                    .take(3)
                {
                    out.push_str(&format!("     {line}\n"));
                    shown.push(*line);
                }
                if let Some(reason) = reasons[*original] {
                    if !shown.iter().any(|line| line.contains(reason)) {
                        out.push_str(&format!("     {reason}\n"));
                    }
                }
                if i + 1 < details.len() {
                    out.push('\n');
                }
            } else {
                let (name, reason) = first.split_once(" - ").unwrap_or((first, ""));
                out.push_str(&format!(
                    "{}. [FAIL] {}\n",
                    i + 1,
                    name.split_once(' ').map_or(name, |(_, n)| n)
                ));
                if !reason.is_empty() {
                    out.push_str(&format!("     {reason}\n"));
                }
            }
        }
        if details.len() > 10 {
            out.push_str(&format!("\n… +{} more failures\n", details.len() - 10));
        }
    }
    if c.is_empty() {
        raw.into()
    } else {
        out.trim_end().into()
    }
}

pub fn javascript(raw: &str) -> String {
    // JSON reporters carry an explicit schema. Console formats remain literal
    // unless their summary establishes the outcome; never manufacture success.
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        let Some(summary) = raw
            .lines()
            .map(str::trim)
            .find(|s| s.starts_with("Tests:") && s.contains(" total"))
        else {
            return raw.into();
        };
        let c = counts(summary);
        let failed = c.get("failed").copied().unwrap_or(0);
        let names: Vec<_> = raw
            .lines()
            .map(str::trim)
            .filter_map(|s| s.strip_prefix("● "))
            .collect();
        if failed == 0 || names.len() != failed {
            return raw.into();
        }
        let mut out = summary.to_owned();
        for line in raw.lines().map(str::trim) {
            if line.starts_with("● ")
                || line.starts_with("Expected")
                || line.starts_with("Received")
            {
                out.push('\n');
                out.push_str(line);
            }
        }
        return out;
    };
    let (Some(passed), Some(failed)) = (
        value["numPassedTests"].as_u64(),
        value["numFailedTests"].as_u64(),
    ) else {
        return raw.into();
    };
    let mut out = format!("Tests: {passed} passed, {failed} failed");
    if let Some(suites) = value["testResults"].as_array() {
        for suite in suites {
            if let Some(tests) = suite["assertionResults"].as_array() {
                for test in tests.iter().filter(|t| t["status"] == "failed") {
                    out.push_str(&format!(
                        "\n[FAIL] {}",
                        test["fullName"].as_str().unwrap_or("unnamed test")
                    ));
                    if let Some(messages) = test["failureMessages"].as_array() {
                        for m in messages {
                            if let Some(s) = m.as_str() {
                                out.push('\n');
                                out.push_str(s);
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

pub fn go(raw: &str) -> String {
    let mut events = Vec::new();
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            return raw.into();
        };
        if !event["Action"].is_string() {
            return raw.into();
        }
        events.push(event);
    }
    if events.is_empty() {
        return raw.into();
    }
    let mut packages = BTreeMap::new();
    let mut passed = 0;
    let mut failed = 0;
    let mut diagnostics = Vec::new();
    for e in &events {
        let package = e["Package"]
            .as_str()
            .or(e["ImportPath"].as_str())
            .unwrap_or("unknown");
        let count = packages.entry(package).or_insert((0usize, false));
        match e["Action"].as_str().unwrap_or("") {
            "pass" if e["Test"].is_string() => passed += 1,
            "fail" if e["Test"].is_string() => {
                failed += 1;
                count.0 += 1;
                diagnostics.push(format!("[FAIL] {package}::{}", e["Test"].as_str().unwrap()));
            }
            "fail" | "build-fail" => count.1 = true,
            _ => {}
        }
    }
    failed += packages
        .values()
        .filter(|(n, fail)| *n == 0 && *fail)
        .count();
    if failed == 0 && passed == 0 {
        return raw.into();
    }
    let mut out = if failed == 0 {
        format!("Go test: {passed} passed in {} packages", packages.len())
    } else {
        format!(
            "Go test: {passed} passed, {failed} failed in {} packages",
            packages.len()
        )
    };
    if failed > 0 {
        for d in diagnostics {
            out.push('\n');
            out.push_str(&d);
        }
        for e in events {
            if let Some(s) = e["Output"].as_str() {
                out.push('\n');
                out.push_str(s.trim_end());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_failed_cargo_view_keeps_unknown_lines_but_not_the_capture_separator() {
        let raw = "running 1 test\ntest b ... FAILED\nCVE-2024-99999: secret\n\nfailures:\n\n---- b stdout ----\nboom\n\nfailures:\n    b\n\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n\n[stderr]\nerror: test failed, to rerun pass `--lib`\n";
        let view = cargo(raw);
        assert!(
            view.starts_with("CVE-2024-99999: secret\nFAILURES (1):\n"),
            "{view}"
        );
        assert!(!view.contains("[stderr]"), "{view}");
        assert!(
            view.ends_with("error: test failed, to rerun pass `--lib`"),
            "{view}"
        );
    }
    #[test]
    fn explicit_test_output_requests_are_read_from_argv_only() {
        let words =
            |line: &str| -> Vec<String> { line.split_whitespace().map(str::to_string).collect() };
        for line in [
            "cargo test -- --nocapture",
            "cargo test --nocapture",
            "cargo test --release -- --show-output",
            "cargo +nightly test -- --nocapture",
            "/home/x/.cargo/bin/cargo test -q -- --test-threads=1 --nocapture",
            "cargo nextest run --no-capture",
            "cargo nextest run --success-output=immediate",
            "pytest -s",
            "pytest -vs tests",
            "pytest -svv",
            "pytest -x -s",
            "pytest --capture=no",
            "pytest --capture no",
            "pytest --capture=tee-sys",
            "pytest -p no:capture",
            "pytest -rP",
            "pytest -rfEA",
            "pytest -r A",
            "pytest --log-cli-level=INFO",
            "pytest -o log_cli=true",
            "py.test -s",
            "python -m pytest -s",
            "python3 -m pytest -vs",
            "uv run pytest -s",
            "uv run python -m pytest --capture=no",
            "go test -v ./...",
            "go test ./... -v",
            "go test -v=true ./...",
            "go test -json ./...",
            "go test ./pkg -test.v",
            "jest --silent=false",
            "jest --no-silent",
            "vitest run --disableConsoleIntercept",
            "vitest run --printConsoleTrace",
            "npx jest --silent=false",
            "npx -y vitest run --disable-console-intercept",
            "bunx vitest --silent false",
            "pnpm vitest run --silent=false",
            "yarn jest --no-silent",
            "npm exec -- jest --silent=false",
            "dotnet test -v d",
            "dotnet test --verbosity detailed",
            "dotnet test --verbosity:normal",
            "dotnet test --logger console;verbosity=detailed",
            "dotnet test -l console;verbosity=diagnostic",
            "rspec",
            "bundle exec rspec spec/relais_spec.rb",
            "ruby relais_test.rb",
            "ruby -Itest test/minitest_relais.rb",
            "ruby -S rspec",
            "rake test",
            "rails test",
            "bundle exec rake test:units",
            "mvn test",
            "mvn -q clean install",
            "./mvnw verify",
            "gradle test --info",
            "./gradlew test -i",
            "playwright test",
            "npx playwright test",
        ] {
            assert!(shows_test_output(&words(line)), "{line}");
        }
        for line in [
            "cargo test",
            "cargo test -- --test-threads=1",
            "cargo test --color never",
            "cargo build --nocapture",
            "pytest",
            "pytest -q --tb=long test_failures.py",
            "pytest -rs",
            "pytest -ra",
            "pytest -k test_s",
            "pytest -ktest_s",
            "pytest -m slow -x",
            "pytest -v",
            "pytest --capture=fd",
            "pytest -p xdist",
            "python -m pytest -q",
            "go test ./...",
            "go test -v=false ./...",
            "go test -vet=off ./...",
            "go build -v ./...",
            "jest",
            "jest --runInBand --no-colors",
            "jest --silent",
            "vitest run --maxWorkers=1",
            "npx vitest run",
            "dotnet test",
            "dotnet test -v q",
            "dotnet test --logger trx",
            "dotnet build -v d",
            "ruby script.rb",
            "rake db:migrate",
            "mvn compile",
            "gradle test",
            "playwright install",
            "grep -rn pytest -s .",
        ] {
            assert!(!shows_test_output(&words(line)), "{line}");
        }
    }

    /// Sortie réelle de `pytest -q` (pytest 9) pour trois tests en échec : une fonction, une
    /// méthode de classe et un cas paramétré.
    const PYTEST_THREE_FAILURES: &str = "\
.FF.F                                                                    [100%]
=================================== FAILURES ===================================
________________________________ test_fails_one ________________________________

    def test_fails_one():
>       assert 1 + 1 == 3
E       assert (1 + 1) == 3

test_demo.py:7: AssertionError
_____________________________ TestBox.test_method ______________________________

self = <test_demo.TestBox object at 0x700056d8f710>

    def test_method(self):
>       assert \"a\" == \"b\"
E       AssertionError: assert 'a' == 'b'
E         
E         - b
E         + a

test_demo.py:11: AssertionError
________________________________ test_param[2] _________________________________

n = 2

    @pytest.mark.parametrize(\"n\", [1, 2])
    def test_param(n):
>       assert n == 1
E       assert 2 == 1

test_demo.py:15: AssertionError
=========================== short test summary info ============================
FAILED test_demo.py::test_fails_one - assert (1 + 1) == 3
FAILED test_demo.py::TestBox::test_method - AssertionError: assert 'a' == 'b'
FAILED test_demo.py::test_param[2] - assert 2 == 1
3 failed, 2 passed in 0.04s
";

    #[test]
    fn a_failure_is_listed_once_not_once_per_section() {
        // Le bloc « ___ test ___ » de FAILURES et la ligne « FAILED chemin::test - raison » de
        // la synthèse décrivent le même échec : en 0.2.5 les trois étaient listés deux fois.
        let view = pytest(PYTEST_THREE_FAILURES);
        for name in ["test_fails_one", "TestBox.test_method", "test_param[2]"] {
            assert_eq!(
                view.matches(name).count(),
                1,
                "{name} listé plusieurs fois :\n{view}"
            );
        }
        assert!(!view.contains("test_demo.py::"), "{view}");
        assert!(view.contains("Pytest: 2 passed, 3 failed"), "{view}");
        let listed = view.lines().filter(|l| l.contains("[FAIL]")).count();
        assert_eq!(listed, 3, "{view}");
        // La raison de chaque échec reste visible.
        assert!(view.contains("assert (1 + 1) == 3"), "{view}");
        assert!(view.contains("AssertionError: assert 'a' == 'b'"), "{view}");
        assert!(view.contains("assert 2 == 1"), "{view}");
    }

    #[test]
    fn summary_only_failures_are_still_listed() {
        // Sans bloc FAILURES (`--tb=no`), la ligne de synthèse est la seule trace : elle reste.
        let raw = "=== short test summary info ===\nFAILED tests/a.py::test_x - boom\nFAILED tests/a.py::test_y\n2 failed, 1 passed in 0.01s\n";
        let view = pytest(raw);
        assert!(view.contains("1. [FAIL] tests/a.py::test_x"), "{view}");
        assert!(view.contains("boom"), "{view}");
        assert!(view.contains("2. [FAIL] tests/a.py::test_y"), "{view}");
    }

    #[test]
    fn a_summary_line_of_another_test_is_not_swallowed() {
        let raw = "=== FAILURES ===\n___ test_a ___\nE   assert 0\n=== short test summary info ===\nFAILED t.py::test_a - assert 0\nFAILED t.py::test_other - x\n2 failed in 0.01s\n";
        let view = pytest(raw);
        assert_eq!(view.matches("test_a").count(), 1, "{view}");
        assert!(view.contains("test_other"), "{view}");
    }
    #[test]
    fn successful_test_views_state_zero_failures_and_retain_unknown_diagnostics() {
        let cargo = include_str!("../bench/corpus/cargo_ok.txt");
        let view = super::cargo(cargo);
        assert!(view.contains("80 passed"));
        assert!(view.contains("0 failed"));
        let raw = include_str!("../bench/corpus/dotnet_ok.txt");
        assert_eq!(dotnet(raw), "65 passed, 0 failed");
        assert_eq!(
            dotnet(&format!("warning: unsupported\n{raw}")),
            format!("warning: unsupported\n{raw}")
        );
        let invalid = raw.replace("Total:    65", "Total:    66");
        assert_eq!(dotnet(&invalid), invalid);
    }
    #[test]
    fn collection_errors_are_not_zero_failures() {
        let view = pytest("ERROR tests/a.py - ImportError\n5 skipped, 457 errors in 1.00s\n");
        assert!(view.contains("457 errors during collection"));
        assert!(view.contains("tests/a.py"));
    }
    #[test]
    fn long_failure_statements_are_preserved_verbatim() {
        let assertion = format!("assert {} == 43", "42 + ".repeat(25));
        let pytest_raw = format!(
            "=== FAILURES ===\n___ test_invoice ___\n>   {assertion}\nE   {assertion}\n=== short test summary ===\nFAILED test_invoice.py::test_invoice - {assertion}\n1 failed in 0.01s\n"
        );
        let pytest_view = pytest(&pytest_raw);
        assert!(pytest_view.contains(&format!(">   {assertion}")));
        assert!(pytest_view.contains(&format!("E   {assertion}")));

        let cargo_raw = format!(
            "failures:\n\n---- invoice stdout ----\nthread 'invoice' panicked at src/lib.rs:1:1:\n{assertion}\n\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; finished in 0.01s\n"
        );
        assert!(cargo(&cargo_raw).contains(&assertion));
    }
    /// Sortie réelle de `cargo test` (Rust 1.95) : une suite, deux tests en échec.
    const CARGO_TWO_FAILURES: &str = include_str!("../tests/fixtures/cargo_test_two_failures.txt");

    #[test]
    fn cargo_counts_failed_tests_not_paragraphs() {
        let view = cargo(CARGO_TWO_FAILURES);
        assert!(view.starts_with("FAILURES (2):\n"), "{view}");
        assert!(
            view.contains("\n1. ---- tests::bad_other stdout ----\n"),
            "{view}"
        );
        assert!(
            view.contains("\n2. ---- tests::bad_sum stdout ----\n"),
            "{view}"
        );
        assert!(!view.contains("\n3. "), "{view}");
        for fact in [
            "assertion failed: add(1, 1) == 3",
            "assertion `left == right` failed: somme attendue",
            "  left: 4",
            " right: 5",
            "src/lib.rs:8:30",
            "src/lib.rs:7:28",
        ] {
            assert!(view.contains(fact), "manque {fact:?} dans\n{view}");
        }
    }

    #[test]
    fn cargo_prints_each_suite_summary_once_and_keeps_the_rerun_hint() {
        let view = cargo(CARGO_TWO_FAILURES);
        assert_eq!(
            view.matches("test result: FAILED. 2 passed; 2 failed")
                .count(),
            1,
            "{view}"
        );
        assert!(
            view.ends_with("error: test failed, to rerun pass `--lib`"),
            "{view}"
        );
    }

    #[test]
    fn cargo_workspace_keeps_every_suite_summary_and_counts_all_failures() {
        let raw = format!(
            "{}\n     Running tests/integ.rs (target/debug/deps/integ-1)\n\nrunning 1 test\ntest integ_ok ... FAILED\n\nfailures:\n\n---- integ_ok stdout ----\n\nthread 'integ_ok' panicked at tests/integ.rs:1:1:\nboom\n\n\nfailures:\n    integ_ok\n\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\nerror: test failed, to rerun pass `--test integ`\n",
            CARGO_TWO_FAILURES.replace("error: test failed, to rerun pass `--lib`\n", "")
        );
        let view = cargo(&raw);
        assert!(view.starts_with("FAILURES (3):\n"), "{view}");
        assert!(view.contains("\n3. ---- integ_ok stdout ----\nthread 'integ_ok' panicked at tests/integ.rs:1:1:\nboom\n"), "{view}");
        assert_eq!(view.matches("test result:").count(), 2, "{view}");
        assert!(
            view.ends_with("error: test failed, to rerun pass `--test integ`"),
            "{view}"
        );
    }

    #[test]
    fn cargo_failure_names_without_a_detail_block_are_still_counted() {
        let raw = "failures:\n    a::one\n    a::two\n\ntest result: FAILED. 0 passed; 2 failed; 0 ignored; finished in 0.01s\n";
        let view = cargo(raw);
        assert!(
            view.starts_with("FAILURES (2):\n1.     a::one\n2.     a::two\n"),
            "{view}"
        );
    }

    #[test]
    fn cargo_unknown_and_compile_errors_stay_literal() {
        for raw in [
            "error[E001]: failed to compile\n --> src/main.rs:7:2\n",
            "unrecognized output\n",
        ] {
            assert_eq!(cargo(raw), raw);
        }
    }
    #[test]
    fn go_package_failure_is_not_counted_twice() {
        let raw="{\"Action\":\"fail\",\"Package\":\"p\",\"Test\":\"TestA\"}\n{\"Action\":\"fail\",\"Package\":\"p\"}\n";
        assert!(go(raw).contains("0 passed, 1 failed"));
    }
    #[test]
    fn console_summary_requires_every_failed_test_name() {
        let raw = "FAIL example.test.js\n  ● invoice mismatch\n    Expected: 42\n    Received: 43\nTests: 1 failed, 2 passed, 3 total\n";
        let view = javascript(raw);
        assert!(view.contains("1 failed"));
        assert!(view.contains("invoice mismatch"));
        assert!(view.contains("Expected: 42"));
        assert!(view.contains("Received: 43"));
        assert_eq!(
            javascript(&raw.replace("1 failed", "2 failed")),
            raw.replace("1 failed", "2 failed")
        );
    }
    #[test]
    fn json_failures_keep_name_and_message() {
        let raw = r#"{"numPassedTests":2,"numFailedTests":1,"testResults":[{"assertionResults":[{"status":"failed","fullName":"invoice","failureMessages":["Expected 42, got 43"]}]}]}"#;
        let out = javascript(raw);
        assert!(out.contains("invoice"));
        assert!(out.contains("Expected 42, got 43"));
    }
}
