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
    // Failure sections are separated into paragraphs without altering paths,
    // assertion operands or stack locations. A section-ending summary is also
    // retained in the section's footer, as well as the suite summary list.
    let mut blocks = Vec::new();
    let mut section = Vec::new();
    let mut active = false;
    for line in raw.lines() {
        if line == "failures:" {
            active = true;
            continue;
        }
        if line.starts_with("test result:") {
            active = false;
            continue;
        }
        if active {
            section.push(line);
        }
    }
    let text = section.join("\n");
    for paragraph in text.split("\n\n") {
        let p = paragraph.trim_matches('\n');
        if !p.trim().is_empty() {
            blocks.push(p);
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
    // Preserve the transcript order and section footer occurrence.
    let mut inside = false;
    for line in raw.lines() {
        if line == "failures:" {
            inside = true;
        }
        if line.starts_with("test result:") {
            out.push_str(line);
            out.push('\n');
            if inside {
                out.push_str(line);
                out.push('\n');
                inside = false;
            }
        }
    }
    out.trim_end().into()
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
