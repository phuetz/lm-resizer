//! Conservative source-code compressor.
//!
//! The transform keeps executable lines byte-for-byte and only removes
//! full-line comments plus redundant blank-line runs. It is intended for
//! live tool-output payloads where smaller context matters but changing code
//! semantics would be worse than skipping compression.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceCompressionResult {
    pub original: String,
    pub compressed: String,
    pub removed_comment_lines: usize,
    pub removed_blank_lines: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct SourceCompressor {
    max_blank_run: usize,
}

impl Default for SourceCompressor {
    fn default() -> Self {
        Self { max_blank_run: 1 }
    }
}

impl SourceCompressor {
    pub fn compress(&self, input: &str) -> SourceCompressionResult {
        let mut output = String::with_capacity(input.len());
        let mut blank_run = 0usize;
        let mut removed_comment_lines = 0usize;
        let mut removed_blank_lines = 0usize;

        for line in input.lines() {
            let trimmed = line.trim_start();

            if is_full_line_comment(trimmed) {
                removed_comment_lines += 1;
                continue;
            }

            if trimmed.is_empty() {
                blank_run += 1;
                if blank_run > self.max_blank_run {
                    removed_blank_lines += 1;
                    continue;
                }
            } else {
                blank_run = 0;
            }

            output.push_str(line);
            output.push('\n');
        }

        if !input.ends_with('\n') {
            output.pop();
        }

        if output.len() >= input.len() {
            output.clear();
            output.push_str(input);
        }

        SourceCompressionResult {
            original: input.to_string(),
            compressed: output,
            removed_comment_lines,
            removed_blank_lines,
        }
    }
}

fn is_full_line_comment(trimmed: &str) -> bool {
    if trimmed.starts_with("#!") {
        return false;
    }

    trimmed.starts_with("//")
        || trimmed.starts_with("///")
        || trimmed.starts_with("//!")
        || trimmed.starts_with('#')
        || trimmed.starts_with("--")
        || trimmed.starts_with(';')
        || trimmed.starts_with("<!--")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.starts_with("*/")
}

impl crate::transforms::pipeline::ReformatTransform for SourceCompressor {
    fn name(&self) -> &'static str {
        "source_compressor"
    }

    fn applies_to(&self) -> &[crate::transforms::ContentType] {
        &[crate::transforms::ContentType::SourceCode]
    }

    fn apply(
        &self,
        content: &str,
    ) -> Result<
        crate::transforms::pipeline::ReformatOutput,
        crate::transforms::pipeline::TransformError,
    > {
        let result = self.compress(content);
        if result.compressed == result.original {
            return Err(crate::transforms::pipeline::TransformError::skipped(
                self.name(),
                "source already compact",
            ));
        }

        Ok(crate::transforms::pipeline::ReformatOutput::from_lengths(
            content.len(),
            result.compressed,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::SourceCompressor;

    #[test]
    fn removes_only_full_line_comments_and_extra_blank_lines() {
        let input =
            "#!/usr/bin/env bash\n# setup\n\n\nvalue=1 # keep inline\n// js note\nprintln(value)\n";
        let result = SourceCompressor::default().compress(input);

        assert_eq!(result.removed_comment_lines, 2);
        assert_eq!(result.removed_blank_lines, 1);
        assert!(result.compressed.contains("#!/usr/bin/env bash"));
        assert!(result.compressed.contains("value=1 # keep inline"));
        assert!(!result.compressed.contains("# setup"));
        assert!(!result.compressed.contains("// js note"));
    }
}
