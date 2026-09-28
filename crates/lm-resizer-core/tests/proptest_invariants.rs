use proptest::prelude::*;
use regex::Regex;

use lm_resizer_core::ccr::InMemoryCcrStore;
use lm_resizer_core::transforms::diff_compressor::DiffCompressor;
use lm_resizer_core::transforms::log_compressor::{LogCompressor, LogCompressorConfig};
use lm_resizer_core::transforms::pipeline::{CompressionContext, OffloadTransform};
use lm_resizer_core::transforms::prose_compressor::ProseCompressor;
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
    let compressor = ProseCompressor::default();
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
