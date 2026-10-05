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
    if !details.is_empty() {
        out.push_str("\n\nFailures:\n");
        for (i, block) in details.iter().take(10).enumerate() {
            let first = block[0];
            if first.starts_with("___") {
                out.push_str(&format!(
                    "{}. [FAIL] {}\n",
                    i + 1,
                    first.trim_matches('_').trim()
                ));
                for line in block
                    .iter()
                    .skip(1)
                    .filter(|l| {
                        l.starts_with(['>', 'E'])
                            || l.to_lowercase().contains("assert")
                            || l.to_lowercase().contains("error")
                            || l.contains(".py:")
                    })
                    .take(3)
                {
                    out.push_str(&format!("     {line}\n"));
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
