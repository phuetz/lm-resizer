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

/// Nom d'un programme : dernier composant du chemin, sans `.exe`, en minuscules (`py.test` reste
/// `py.test`).
fn program_name(arg: &str) -> String {
    let name = arg.rsplit(['/', '\\']).next().unwrap_or(arg);
    let name = name.strip_suffix(".exe").unwrap_or(name);
    name.to_ascii_lowercase()
}

/// La commande demande explicitement d'afficher la sortie des programmes testés : la vue du lanceur
/// est alors refusée et la sortie rendue brute, octet pour octet. Décision lue dans l'`argv` seul,
/// jamais dans la sortie (décision du 9 octobre 2026). Un test peut imprimer n'importe quoi,
/// `git log --format=%s` compris ; les vues de ce module ne gardent que bilans et échecs.
///
/// Lanceurs ayant une vue native, après les enveloppes que le routage reconnaît (`npx`, `bunx`,
/// `npm|pnpm|yarn|bundle exec`, `uv run`, `python -m`, `pnpm|yarn|bun <jest|vitest>`) :
/// - `cargo test`, `cargo nextest` : `--nocapture`, `--no-capture`, `--show-output`,
///   `--success-output…`, avant ou après `--` ;
/// - `pytest`, `py.test`, `python -m pytest` : `-s` (aussi groupé, `-vs`), `--capture=no|tee-sys`,
///   `-p no:capture`, `-r` avec `P` ou `A` (sortie des tests réussis), `--log-cli-level`,
///   `-o log_cli=true` ;
/// - `go test` : `-v`, `-v=true`, `-test.v…`, `-json` ;
/// - `jest`, `vitest` : `--silent=false`, `--no-silent`, `--disableConsoleIntercept`,
///   `--printConsoleTrace` ;
/// - `dotnet test` : `-v|--verbosity normal|detailed|diagnostic`, `--logger` dont la verbosité est
///   `normal`, `detailed` ou `diagnostic` ;
/// - lanceurs qui affichent toujours la sortie des tests (aucune capture) : `rspec`, minitest
///   (`ruby …_test.rb`, `rake test`, `rails test`), `mvn` avec une phase qui lance les tests
///   (`test`, `verify`, `package`, `install`, `deploy`), `playwright test` ; `gradle` seulement avec
///   `-i`, `--info`, `-d` ou `--debug`.
pub fn shows_test_output(command: &[String]) -> bool {
    let rest = after_runner_wrappers(command);
    if rest.is_empty() {
        return false;
    }
    let program = program_name(&rest[0]);
    let args = &rest[1..];
    let has = |word: &str| args.iter().any(|arg| arg == word);
    match program.as_str() {
        "cargo" if has("test") || has("nextest") => args.iter().any(|arg| {
            matches!(
                arg.as_str(),
                "--nocapture" | "--no-capture" | "--show-output"
            ) || arg.starts_with("--success-output")
        }),
        "pytest" | "py.test" => pytest_shows_output(args),
        "go" if has("test") => args.iter().any(|arg| {
            let flag = arg.trim_start_matches('-');
            arg.starts_with('-')
                && (flag == "v"
                    || flag == "json"
                    || flag == "test.v"
                    || ((flag.starts_with("v=") || flag.starts_with("test.v="))
                        && !flag.ends_with("=false")
                        && !flag.ends_with("=0")))
        }),
        "jest" | "vitest" => args.iter().enumerate().any(|(i, arg)| {
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
        "dotnet" if has("test") => dotnet_shows_output(args),
        // Lanceurs sans capture : la sortie des tests est affichée par défaut, lancer les tests vaut
        // demande d'affichage. Formes reconnues par les filtres intégrés `rspec`, `minitest`,
        // `jvm-build` et `js-quality`.
        "rspec" => true,
        "ruby" | "rake" | "rails" => runs_ruby_tests(&program, args),
        "mvn" | "mvnw" => runs_maven_tests(args),
        "gradle" | "gradlew" => args
            .iter()
            .any(|arg| matches!(arg.as_str(), "-i" | "--info" | "-d" | "--debug")),
        "playwright" => has("test"),
        _ => false,
    }
}

/// La commande lance une suite de tests, lue dans l'`argv` seul après les mêmes enveloppes que
/// [`shows_test_output`] (`npx`, `uv run`, `python -m`, `bundle exec`…). Quand elle se termine
/// par le code 0, `exec`, `tool-output` et `pipe` rendent sa sortie intacte : un test qui réussit
/// peut écrire n'importe quoi sur la sortie qu'il hérite (un `git log` relayé par un processus
/// enfant, un avertissement de sécurité), et une vue de lanceur n'en garde que le bilan. Un code non
/// nul garde la vue du lanceur, qui montre les échecs. Proposition du 9 octobre 2026, en attente.
pub fn runs_tests(command: &[String]) -> bool {
    let rest = after_runner_wrappers(command);
    let Some(first) = rest.first() else {
        return false;
    };
    let program = program_name(first);
    let args = &rest[1..];
    let has = |word: &str| args.iter().any(|arg| arg == word);
    let first_arg = args.first().map(String::as_str).unwrap_or("");
    let test_task = |task: &str| {
        let task = task.rsplit(':').next().unwrap_or(task).to_ascii_lowercase();
        task.contains("test") || task == "check"
    };
    match program.as_str() {
        "cargo" => has("test") || has("nextest") || first_arg == "t",
        "pytest" | "py.test" | "unittest" | "nose2" | "tox" | "nox" | "jest" | "vitest"
        | "mocha" | "ava" | "rspec" | "ctest" | "phpunit" | "pest" | "paratest" => true,
        "go" | "dotnet" | "playwright" => has("test"),
        "deno" | "bun" | "swift" | "mix" | "zig" => first_arg == "test" || has("test"),
        "ruby" | "rake" | "rails" => runs_ruby_tests(&program, args),
        "mvn" | "mvnw" => runs_maven_tests(args),
        "gradle" | "gradlew" => args
            .iter()
            .any(|arg| !arg.starts_with('-') && (test_task(arg) || arg == "build")),
        "php" => has("artisan") && has("test"),
        "npm" | "pnpm" | "yarn" => match first_arg {
            "test" | "t" | "tst" => true,
            "run" | "run-script" => args.get(1).is_some_and(|script| test_task(script)),
            _ => false,
        },
        "make" | "gmake" | "just" | "task" => args
            .iter()
            .any(|arg| !arg.starts_with('-') && !arg.contains('=') && test_task(arg)),
        _ => false,
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

/// La commande sans les enveloppes que le routage reconnaît : elles désignent le programme qui suit.
fn after_runner_wrappers(command: &[String]) -> &[String] {
    let mut rest = command;
    loop {
        let Some(first) = rest.first() else {
            return rest;
        };
        let skip = match program_name(first).as_str() {
            "npx" | "bunx" => {
                1 + rest[1..]
                    .iter()
                    .take_while(|arg| matches!(arg.as_str(), "-y" | "--yes" | "--"))
                    .count()
            }
            "npm" | "pnpm" | "yarn" | "bundle" if rest.get(1).is_some_and(|a| a == "exec") => {
                if rest.get(2).is_some_and(|a| a == "--") {
                    3
                } else {
                    2
                }
            }
            "uv" | "poetry" | "pipenv" if rest.get(1).is_some_and(|a| a == "run") => 2,
            "python" | "python3" | "py" if rest.get(1).is_some_and(|a| a == "-m") => 2,
            "pnpm" | "yarn" | "bun"
                if rest
                    .get(1)
                    .is_some_and(|a| matches!(program_name(a).as_str(), "jest" | "vitest")) =>
            {
                1
            }
            _ => 0,
        };
        if skip == 0 || skip >= rest.len() {
            break;
        }
        rest = &rest[skip..];
    }
    rest
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
    // les noms de la liste finale `failures:` qu'aucun bloc ne couvre sont listés tels quels.
    let (details, names) = cargo_failures(raw);
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
        return format!("{passed} passed, 0 failed{extra}{suites} ({duration:.2}s)");
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
    let mut out = format!("FAILURES ({}):\n{numbered}{trailer}\n", blocks.len());
    // Chaque bilan de suite une seule fois, puis l'indication de relance de cargo, dans l'ordre.
    for line in raw.lines() {
        if line.starts_with("test result:") || line.starts_with("error: test failed") {
            out.push_str(line);
            out.push('\n');
        }
    }
    out.trim_end().into()
}

/// Blocs `---- nom stdout ----` (sans lignes vides) et noms de la liste `failures:` d'une sortie
/// `cargo test`, toutes suites confondues.
fn cargo_failures(raw: &str) -> (Vec<Vec<&str>>, Vec<&str>) {
    #[derive(PartialEq)]
    enum Zone {
        Outside,
        Detail,
        List,
    }
    let mut details: Vec<Vec<&str>> = Vec::new();
    let mut names = Vec::new();
    let mut zone = Zone::Outside;
    for line in raw.lines() {
        if line.starts_with("test result:") {
            zone = Zone::Outside;
        } else if line == "failures:" {
            zone = Zone::List;
        } else if line.starts_with("---- ") && line.ends_with(" ----") {
            details.push(vec![line]);
            zone = Zone::Detail;
        } else if line.trim().is_empty() {
            continue;
        } else if zone == Zone::Detail {
            if let Some(block) = details.last_mut() {
                block.push(line);
            }
        } else if zone == Zone::List {
            if line.starts_with("    ") {
                names.push(line);
            } else {
                zone = Zone::Outside;
            }
        }
    }
    (details, names)
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
