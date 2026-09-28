//! Model-free prose compression: only consecutive, byte-identical long lines.
//! The retained line and repeat count carry the original meaning. Unique
//! sentences, diagnostics and short lines remain untouched.

use super::content_detector::ContentType;
use super::diagnostic_gate::FAILURE_SIGNAL;
use super::pipeline::{CompressionContext, OffloadOutput, OffloadTransform, TransformError};
use crate::ccr::{compute_key, CcrStore};

#[derive(Default)]
pub struct ProseCompressor;

impl OffloadTransform for ProseCompressor {
    fn name(&self) -> &'static str {
        "prose_repetition"
    }

    fn applies_to(&self) -> &[ContentType] {
        &[ContentType::PlainText]
    }

    fn estimate_bloat(&self, content: &str) -> f32 {
        let mut previous = "";
        let mut run = 0;
        for line in content.lines() {
            if line == previous && line.len() >= 40 && !FAILURE_SIGNAL.is_match(line) {
                run += 1;
                if run >= 3 {
                    return 1.0;
                }
            } else {
                previous = line;
                run = 1;
            }
        }
        0.0
    }

    fn apply(
        &self,
        content: &str,
        _ctx: &CompressionContext,
        store: &dyn CcrStore,
    ) -> Result<OffloadOutput, TransformError> {
        let lines: Vec<&str> = content.split_inclusive('\n').collect();
        let mut output = String::with_capacity(content.len());
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            let mut end = i + 1;
            while end < lines.len() && lines[end] == line {
                end += 1;
            }
            let run = end - i;
            if run >= 3 && line.len() >= 40 && !FAILURE_SIGNAL.is_match(line) {
                let marker = format!("[same line repeated {} additional times]\n", run - 1);
                if marker.len() < line.len() * (run - 1) {
                    output.push_str(line);
                    output.push_str(&marker);
                } else {
                    for original in &lines[i..end] {
                        output.push_str(original);
                    }
                }
            } else {
                for original in &lines[i..end] {
                    output.push_str(original);
                }
            }
            i = end;
        }
        let key = compute_key(content.as_bytes());
        if !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(&format!("[full original: lm-resizer retrieve {key}]\n"));
        if output.len() >= content.len() {
            return Err(TransformError::skipped(self.name(), "no repeated prose"));
        }
        store.put(&key, content);
        Ok(OffloadOutput::from_lengths(content.len(), output, key))
    }

    fn confidence(&self) -> f32 {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ccr::InMemoryCcrStore;

    fn ctx() -> CompressionContext {
        CompressionContext {
            query: String::new(),
            token_budget: None,
        }
    }

    #[test]
    fn repeated_prose_keeps_sentence_and_count() {
        let repeated = "The rollout completed and every checked item passed.\n";
        let input = format!("{}A distinct conclusion remains.\n", repeated.repeat(12));
        let store = InMemoryCcrStore::default();
        let result = ProseCompressor.apply(&input, &ctx(), &store).unwrap();
        assert!(result.output.contains(repeated));
        assert!(result.output.contains("11 additional times"));
        assert!(result.output.contains("A distinct conclusion remains."));
        assert!(result.output.len() < input.len());
        assert_eq!(
            store.get(&result.cache_key).as_deref(),
            Some(input.as_str())
        );
    }

    #[test]
    fn failures_remain_verbatim() {
        let input = "ERROR: src/lib.rs:42:9 missing symbol\n".repeat(10);
        assert!(ProseCompressor
            .apply(&input, &ctx(), &InMemoryCcrStore::default())
            .is_err());
    }
}
