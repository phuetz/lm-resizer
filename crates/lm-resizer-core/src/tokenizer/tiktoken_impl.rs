//! `tiktoken-rs` adapter implementing [`Tokenizer`].
//!
//! `tiktoken-rs` and Python `tiktoken` use the same BPE merge tables; for the
//! same model and same input, this returns byte-identical token IDs and
//! therefore byte-identical token *counts*. This is what makes the parity
//! tests "byte-equal" rather than "approximate".
//!
//! Initialization (loading the BPE table) is non-trivial. Each encoding is
//! built lazily on first use and shared. o200k counting loads a packed rank
//! table derived at build time from tiktoken-rs, avoiding decoder construction
//! and vocabulary sorting. Other encodings use the reference `CoreBPE`.

use std::sync::{Arc, LazyLock};

use thiserror::Error;
use tiktoken_rs::CoreBPE;

use super::{count_only::CountOnlyBpe, Backend, Tokenizer};

#[derive(Debug, Error)]
pub enum TiktokenError {
    /// We don't know which encoding `model` should use. The caller can fall
    /// back to estimation; the registry handles that automatically.
    #[error("unknown encoding for model `{0}`")]
    UnknownEncoding(String),
}

/// Lazy-built shared BPE for the four named encodings. Init failure here would
/// indicate `tiktoken-rs` itself is broken; we treat that as a programmer error
/// and panic.
static O200K: LazyLock<Arc<CountOnlyBpe>> = LazyLock::new(|| Arc::new(CountOnlyBpe::new()));
#[cfg(test)]
static O200K_REFERENCE: LazyLock<CoreBPE> =
    LazyLock::new(|| tiktoken_rs::o200k_base().expect("reference o200k_base"));
static CL100K: LazyLock<Arc<CoreBPE>> =
    LazyLock::new(|| Arc::new(tiktoken_rs::cl100k_base().expect("cl100k_base init")));
static P50K: LazyLock<Arc<CoreBPE>> =
    LazyLock::new(|| Arc::new(tiktoken_rs::p50k_base().expect("p50k_base init")));
static R50K: LazyLock<Arc<CoreBPE>> =
    LazyLock::new(|| Arc::new(tiktoken_rs::r50k_base().expect("r50k_base init")));

/// BPE token counter for OpenAI / o-series models.
pub struct TiktokenCounter {
    model: String,
    encoding_name: &'static str,
    bpe: CounterBpe,
}

enum CounterBpe {
    Counting(Arc<CountOnlyBpe>),
    Reference(Arc<CoreBPE>),
}

impl CounterBpe {
    fn count(&self, text: &str) -> usize {
        match self {
            Self::Counting(bpe) => bpe.count(text),
            Self::Reference(bpe) => bpe.encode_ordinary(text).len(),
        }
    }
}

impl std::fmt::Debug for TiktokenCounter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TiktokenCounter")
            .field("model", &self.model)
            .field("encoding", &self.encoding_name)
            .finish()
    }
}

impl TiktokenCounter {
    /// Build a counter for `model`. Returns `UnknownEncoding` if the model
    /// doesn't fall into any of the supported BPE families.
    pub fn for_model(model: &str) -> Result<Self, TiktokenError> {
        let encoding_name = encoding_for(model)?;
        let bpe = match encoding_name {
            "o200k_base" => CounterBpe::Counting(O200K.clone()),
            "cl100k_base" => CounterBpe::Reference(CL100K.clone()),
            "p50k_base" => CounterBpe::Reference(P50K.clone()),
            "r50k_base" => CounterBpe::Reference(R50K.clone()),
            // unreachable: encoding_for only returns the four names above.
            _ => return Err(TiktokenError::UnknownEncoding(model.to_string())),
        };
        Ok(Self {
            model: model.to_string(),
            encoding_name,
            bpe,
        })
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn encoding_name(&self) -> &'static str {
        self.encoding_name
    }
}

impl Tokenizer for TiktokenCounter {
    fn count_text(&self, text: &str) -> usize {
        if text.is_empty() {
            // Match Python `TiktokenCounter.count_text`: short-circuit empty.
            return 0;
        }
        // For ORDINARY input (no literal special-token strings) `encode_ordinary`
        // here and `encoding.encode(text)` in Python yield identical token IDs
        // and counts — that's the byte-equality the parity harness verifies.
        //
        // Divergence (rare in practice): if `text` contains a literal
        // `<|endoftext|>` (or any other special-token string), Python's default
        // `encode` raises (because `disallowed_special="all"`) while we treat
        // it as ordinary text. We chose tolerance over panic since proxy users
        // can legitimately send those substrings; document for future readers.
        // o200k's letter prefixes exclude CR/LF; numbers cannot cross LF.
        // Its whitespace/newline branch ends at the last LF in a whitespace
        // run. Punctuation's trailing [\r\n/]* stops at indentation or a
        // a non-slash graphic character. Split only at those boundaries, never
        // at arbitrary byte/line offsets (blank lines and slashes can merge).
        #[cfg(not(target_arch = "wasm32"))]
        if self.encoding_name == "o200k_base" && text.len() >= 128 * 1024 {
            use rayon::prelude::*;
            let bytes = text.as_bytes();
            let mut chunks = Vec::new();
            let mut start = 0;
            for (i, &byte) in bytes.iter().enumerate() {
                if i + 1 - start < 32 * 1024 || byte != b'\n' {
                    continue;
                }
                let mut next = i + 1;
                while next < bytes.len() && matches!(bytes[next], b' ' | b'\t') {
                    next += 1;
                }
                let safe = bytes
                    .get(next)
                    .is_some_and(|b| b.is_ascii_graphic() && (next > i + 1 || *b != b'/'));
                if safe {
                    chunks.push(&text[start..i + 1]);
                    start = i + 1;
                }
            }
            chunks.push(&text[start..]);
            if chunks.len() > 1 {
                return chunks.par_iter().map(|chunk| self.bpe.count(chunk)).sum();
            }
        }
        self.bpe.count(text)
    }

    fn backend(&self) -> Backend {
        Backend::Tiktoken
    }
}

/// Map model → encoding name. Mirrors `MODEL_ENCODINGS` and the prefix
/// fallbacks in `lm-resizer/tokenizers/tiktoken_counter.py`.
fn encoding_for(model: &str) -> Result<&'static str, TiktokenError> {
    let m = model.to_ascii_lowercase();

    // o200k_base: GPT-4o + o1/o3 reasoning families.
    if m.starts_with("gpt-4o") || m.starts_with("o1") || m.starts_with("o3") {
        return Ok("o200k_base");
    }

    // cl100k_base: GPT-4, GPT-3.5-turbo, embeddings.
    if m.starts_with("gpt-4") || m.starts_with("gpt-3.5") || m.starts_with("text-embedding") {
        return Ok("cl100k_base");
    }

    // p50k_base: code-* and the davinci-002/003 text-completion line.
    if m.starts_with("code-")
        || m.starts_with("text-davinci-002")
        || m.starts_with("text-davinci-003")
    {
        return Ok("p50k_base");
    }

    // r50k_base: legacy davinci-001 and earlier completion families.
    if m.starts_with("text-davinci")
        || m.starts_with("davinci")
        || m.starts_with("curie")
        || m.starts_with("babbage")
        || m.starts_with("ada")
    {
        return Ok("r50k_base");
    }

    Err(TiktokenError::UnknownEncoding(model.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string_is_zero() {
        let t = TiktokenCounter::for_model("gpt-4o-mini").unwrap();
        assert_eq!(t.count_text(""), 0);
    }

    #[test]
    fn nonempty_text_is_at_least_one_token() {
        let t = TiktokenCounter::for_model("gpt-4o-mini").unwrap();
        assert!(t.count_text("a") >= 1);
    }

    #[test]
    fn known_token_counts_for_o200k() {
        // These constants are the o200k_base BPE token counts produced by
        // both Python `tiktoken.encoding_for_model("gpt-4o-mini")` and
        // `tiktoken-rs::o200k_base()` for the given strings. They lock in
        // byte-equal parity so a future tiktoken-rs upgrade that subtly
        // changes BPE behavior would fail this test.
        let t = TiktokenCounter::for_model("gpt-4o-mini").unwrap();
        assert_eq!(t.count_text("hello"), 1);
        assert_eq!(t.count_text("Hello, world!"), 4);
        assert_eq!(
            t.count_text("the quick brown fox jumps over the lazy dog"),
            9
        );
    }

    #[test]
    fn determinism() {
        let t = TiktokenCounter::for_model("gpt-4o").unwrap();
        let s = "Determinism check across many calls.";
        let first = t.count_text(s);
        for _ in 0..1000 {
            assert_eq!(t.count_text(s), first);
        }
    }

    #[test]
    fn unicode_input_does_not_panic() {
        let t = TiktokenCounter::for_model("gpt-4o-mini").unwrap();
        // Each call should produce a reasonable count (>=1 for non-empty),
        // not panic, and not return absurd values.
        for s in [
            "héllo wörld",        // accented
            "你好世界",           // CJK
            "مرحبا بالعالم",      // Arabic (RTL)
            "🦀 ferris the crab", // emoji
            "\n\t\r\x07",         // control chars
        ] {
            let n = t.count_text(s);
            assert!(n >= 1, "{s:?}");
            assert!(n < s.len() * 4 + 10, "absurd count {n} for {s:?}");
        }
    }

    #[test]
    fn very_long_input() {
        let t = TiktokenCounter::for_model("gpt-4o-mini").unwrap();
        let s = "the quick brown fox ".repeat(50_000); // ~1MB
        let n = t.count_text(&s);
        // 50k repeats * ~5 tokens per repeat = ~250k tokens, sanity bound.
        assert!(n > 100_000 && n < 1_000_000, "n={n}");
    }

    #[test]
    fn encoding_dispatch() {
        for (model, expected) in [
            ("gpt-4o", "o200k_base"),
            ("gpt-4o-mini", "o200k_base"),
            ("gpt-4o-2024-08-06", "o200k_base"),
            ("o1-preview", "o200k_base"),
            ("o3-mini", "o200k_base"),
            ("gpt-4", "cl100k_base"),
            ("gpt-4-turbo", "cl100k_base"),
            ("gpt-3.5-turbo", "cl100k_base"),
            ("text-embedding-3-small", "cl100k_base"),
            ("code-davinci-002", "p50k_base"),
            ("text-davinci-002", "p50k_base"),
            ("text-davinci-003", "p50k_base"),
            ("text-davinci-001", "r50k_base"),
            ("davinci", "r50k_base"),
            ("curie", "r50k_base"),
            ("babbage", "r50k_base"),
            ("ada", "r50k_base"),
        ] {
            let t = TiktokenCounter::for_model(model)
                .unwrap_or_else(|e| panic!("for_model({model}) failed: {e}"));
            assert_eq!(t.encoding_name(), expected, "{model}");
        }
    }

    #[test]
    fn unknown_model_returns_error() {
        let r = TiktokenCounter::for_model("claude-3-opus");
        assert!(matches!(r, Err(TiktokenError::UnknownEncoding(_))));
    }

    #[test]
    fn case_insensitive_dispatch() {
        let t = TiktokenCounter::for_model("GPT-4o-Mini").unwrap();
        assert_eq!(t.encoding_name(), "o200k_base");
    }

    proptest::proptest! {
        #[test]
        fn count_only_matches_reference_for_unicode(text in proptest::collection::vec(proptest::char::any(), 0..1000)) {
            let text: String = text.into_iter().collect();
            let counter = TiktokenCounter::for_model("gpt-4o").unwrap();
            proptest::prop_assert_eq!(counter.count_text(&text), O200K_REFERENCE.encode_ordinary(&text).len());
        }
    }

    #[test]
    fn parallel_counts_match_unsplit_bpe_at_safe_boundaries() {
        let counter = TiktokenCounter::for_model("gpt-4o").unwrap();
        let lines = [
            "ASCII camelCase isn't I'm we'll\r\n",
            "1234567890\n",
            "éà 中文 العربية 🦀\n",
            "punctuation!!!\n///\nA\n",
            " \t\r\n\n   trailing spaces   \nNext\n",
            "\n\n\n9 leading numbers\n",
            "<|endoftext|>\nQ\n",
            "code;\n    const value = 1;\n    }\n\t//comment\n",
            "./path/to/file.ts\n./more.ts\n!punctuation\n}\n",
            "punctuation!\n\t /slashes\n\t\n  next\n",
            "\n   éà\n    中文\n",
        ];
        let text = lines.concat().repeat(1500);
        assert!(text.len() > 128 * 1024);
        for suffix in ["", "   ", "\r\n", "\n///", "終"] {
            let input = format!("{text}{suffix}");
            assert_eq!(
                counter.count_text(&input),
                O200K_REFERENCE.encode_ordinary(&input).len(),
                "suffix {suffix:?}"
            );
        }
        // A long input without safe boundaries must use the unsplit path.
        let text = "éà 中文 / ".repeat(15000);
        assert_eq!(
            counter.count_text(&text),
            O200K_REFERENCE.encode_ordinary(&text).len()
        );
    }

    #[test]
    fn shared_bpe_instances() {
        // Two counters for the same encoding should share the underlying BPE
        // (same Arc), proving the LazyLock cache works.
        let a = TiktokenCounter::for_model("gpt-4o").unwrap();
        let b = TiktokenCounter::for_model("gpt-4o-mini").unwrap();
        match (&a.bpe, &b.bpe) {
            (CounterBpe::Counting(a), CounterBpe::Counting(b)) => assert!(Arc::ptr_eq(a, b)),
            _ => panic!("expected o200k counting tables"),
        }
    }

    #[test]
    fn backend_is_tiktoken() {
        let t = TiktokenCounter::for_model("gpt-4o-mini").unwrap();
        assert_eq!(t.backend(), Backend::Tiktoken);
    }
}
