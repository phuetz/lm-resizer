//! Sampled checks for the direct compressor APIs. Structured source and tool
//! transcript cases below cover paths that random short strings do not reach.
//! Passing these tests does not prove the properties for every input format.

use proptest::prelude::*;
use regex::Regex;

use lm_resizer_core::ccr::InMemoryCcrStore;
use lm_resizer_core::transforms::diff_compressor::DiffCompressor;
use lm_resizer_core::transforms::log_compressor::{LogCompressor, LogCompressorConfig};
use lm_resizer_core::transforms::pipeline::{CompressionContext, OffloadTransform};
use lm_resizer_core::transforms::prose_compressor::ProseCompressor;
use lm_resizer_core::transforms::retention_advice::{
    sha256_hex_public, RetentionAdvice, RetentionRange,
};
use lm_resizer_core::transforms::search_compressor::{SearchCompressor, SearchCompressorConfig};
use lm_resizer_core::transforms::source_compressor::SourceCompressor;
use std::fs;

fn check_invariants(input: &str, output: &str, filter_name: &str) {
    if output.len() > input.len() {
        panic!(
            "[{}] Compression bloat detected: input len {}, output len {}",
            filter_name,
            input.len(),
            output.len()
        );
    }

    let error_re = Regex::new(r"(?i)error").unwrap();
    let assert_re = Regex::new(r"(?i)assert").unwrap();
    let path_re = Regex::new(r"(?i)[a-z0-9_-]+/[a-z0-9_-]+\.[a-z0-9]+").unwrap();

    if error_re.is_match(input) && !error_re.is_match(output) {
        panic!(
            "[{}] Error disappeared!\nInput: {}\nOutput: {}",
            filter_name, input, output
        );
    }
    if assert_re.is_match(input) && !assert_re.is_match(output) {
        panic!(
            "[{}] Assertion disappeared!\nInput: {}\nOutput: {}",
            filter_name, input, output
        );
    }

    for mat in path_re.find_iter(input) {
        let path = mat.as_str();
        if !output.contains(path) {
            panic!(
                "[{}] File path {} disappeared!\nInput: {}\nOutput: {}",
                filter_name, path, input, output
            );
        }
    }
}

fn apply_prose(input: &str) -> String {
    let compressor = ProseCompressor;
    let store = InMemoryCcrStore::default();
    let ctx = CompressionContext {
        query: String::new(),
        token_budget: None,
    };
    match compressor.apply(input, &ctx, &store) {
        Ok(res) => res.output,
        Err(_) => input.to_string(),
    }
}

fn apply_diff(input: &str) -> String {
    let compressor = DiffCompressor::default();
    let (res, _) = compressor.compress_with_stats(input, "");
    res.compressed
}

fn apply_search(input: &str) -> String {
    let compressor = SearchCompressor::new(SearchCompressorConfig::default());
    let (res, _) = compressor.compress(input, "", 1.0);
    res.compressed
}

fn apply_log(input: &str) -> String {
    let compressor = LogCompressor::new(LogCompressorConfig::default());
    let (res, _) = compressor.compress(input, 1.0);
    res.compressed
}

fn apply_source(input: &str) -> String {
    let compressor = SourceCompressor::default();
    let res = compressor.compress(input);
    res.compressed
}

fn critical_string() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("error".to_string()),
        Just("ERROR".to_string()),
        Just("assert".to_string()),
        Just("ASSERT".to_string()),
        Just("src/main.rs".to_string()),
        Just("foo/bar.py".to_string()),
    ]
}

proptest! {
    #[test]
    fn test_prose_compressor_invariants(prefix in ".*", crit in critical_string(), suffix in ".*") {
        let input = format!("{}{}{}", prefix, crit, suffix);
        check_invariants(&input, &apply_prose(&input), "Prose");
    }

    #[test]
    fn test_diff_compressor_invariants(prefix in ".*", crit in critical_string(), suffix in ".*") {
        let input = format!("{}{}{}", prefix, crit, suffix);
        check_invariants(&input, &apply_diff(&input), "Diff");
    }

    #[test]
    fn test_search_compressor_invariants(prefix in ".*", crit in critical_string(), suffix in ".*") {
        let input = format!("{}{}{}", prefix, crit, suffix);
        check_invariants(&input, &apply_search(&input), "Search");
    }

    #[test]
    fn test_log_compressor_invariants(prefix in ".*", crit in critical_string(), suffix in ".*") {
        let input = format!("{}{}{}", prefix, crit, suffix);
        check_invariants(&input, &apply_log(&input), "Log");
    }

    #[test]
    fn test_source_compressor_invariants(prefix in ".*", crit in critical_string(), suffix in ".*") {
        let input = format!("{}{}{}", prefix, crit, suffix);
        check_invariants(&input, &apply_source(&input), "Source");
    }
}

#[test]
fn test_fixtures() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/parity/captured");
    for name in ["cargo-test.txt", "dotnet-test.txt"] {
        let input = fs::read_to_string(root.join(name)).unwrap();
        check_invariants(&input, &apply_source(&input), "Source");
    }
    let source = fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/critical_comments.rs.txt"),
    )
    .unwrap();
    check_invariants(&source, &apply_source(&source), "Source fixture");
}

#[test]
fn test_source_structural_and_advice_preserve_critical_comments() {
    let body: String = (0..20)
        .map(|i| format!("    let value_{i} = {i};\n"))
        .collect();
    let input = format!("// error: disk full\n// see src/main.rs\npub fn run() {{\n{body}}}\n");
    let structural = SourceCompressor::embedded_only().compress(&input);
    assert_eq!(structural.engine_used, "embedded-regex-braces");
    for marker in ["// error: disk full", "// see src/main.rs"] {
        assert!(structural.compressed.contains(marker), "lost {marker}");
    }

    let advice = RetentionAdvice {
        source_sha256: Some(sha256_hex_public(input.as_bytes())),
        ranges: vec![RetentionRange {
            start_line: 3,
            end_line: 3,
            weight: 1.0,
            label: None,
            kind: None,
        }],
        ..RetentionAdvice::default()
    };
    let advised = SourceCompressor::embedded_only().compress_with_advice(&input, &advice, None);
    assert_eq!(advised.engine_used, "advice-guided");
    for marker in ["// error: disk full", "// see src/main.rs"] {
        assert!(advised.compressed.contains(marker), "lost {marker}");
    }
}

#[test]
fn test_advice_preserves_unprotected_critical_comments() {
    let input = "// error: bad\n// see src/main.rs\nlet value = 1;\n";
    let advice = RetentionAdvice {
        source_sha256: Some(sha256_hex_public(input.as_bytes())),
        ranges: vec![RetentionRange {
            start_line: 3,
            end_line: 3,
            weight: 1.0,
            label: None,
            kind: None,
        }],
        ..RetentionAdvice::default()
    };
    let output = SourceCompressor::embedded_only().compress_with_advice(input, &advice, None);
    assert_eq!(output.engine_used, "advice-guided");
    assert!(output.compressed.contains("// error: bad"));
    assert!(output.compressed.contains("// see src/main.rs"));
    check_invariants(input, &output.compressed, "Advice");
}

#[test]
fn test_typescript_structural_preserves_critical_comments() {
    let body: String = (0..20)
        .map(|i| format!("  const value_{i} = {i};\n"))
        .collect();
    let input = format!("// assert result\n// see src/main.ts\nexport function run(): number {{\n{body}  return 1;\n}}\n");
    let output = SourceCompressor::embedded_only().compress(&input);
    assert_eq!(output.engine_used, "embedded-regex-braces");
    assert!(output.compressed.contains("// assert result"));
    assert!(output.compressed.contains("// see src/main.ts"));
    check_invariants(&input, &output.compressed, "TypeScript");
}

#[test]
fn test_structural_body_with_critical_comment_falls_back() {
    let body: String = (0..20)
        .map(|i| format!("    let value_{i} = {i};\n"))
        .collect();
    let input =
        format!("pub fn run() {{\n{body}    // error: disk full\n    // see src/main.rs\n}}\n");
    let output = SourceCompressor::embedded_only().compress(&input);
    assert!(output.compressed.contains("// error: disk full"));
    assert!(output.compressed.contains("// see src/main.rs"));
    check_invariants(&input, &output.compressed, "Rust body");
}

#[test]
fn test_search_does_not_drop_rspec_failure() {
    let input = fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/exec/ruby-prisma/rspec_raw.txt"),
    )
    .unwrap();
    let output = apply_search(&input);
    assert!(output.contains("Failure/Error"));
    assert!(output.contains("2 examples, 2 failures"));
    check_invariants(&input, &output, "Search RSpec");
}

#[test]
fn test_source_critical_comments() {
    for marker in [
        "error",
        "ERROR",
        "assert",
        "ASSERT",
        "src/main.rs",
        "foo/bar.py",
    ] {
        let input = format!("// ordinary comment\n// {marker}\nlet value = 1;\n");
        check_invariants(&input, &apply_source(&input), "Source");
    }
}
