//! Deterministic local relevance scorer.
//!
//! This module deliberately avoids model runtimes, downloaded weights, and
//! native inference backends. It keeps the historical `EmbeddingScorer` API
//! name for callers, but internally uses a fixed-size lexical feature vector
//! built from words and character trigrams.

use super::base::{RelevanceScore, RelevanceScorer};

const VECTOR_DIMS: usize = 256;

/// Local semantic-ish relevance scorer backed by hashed lexical features.
///
/// `model_name` is retained for API/reporting compatibility. No model is
/// loaded and no network or filesystem access occurs during construction.
#[derive(Clone, Debug)]
pub struct EmbeddingScorer {
    pub model_name: String,
}

impl Default for EmbeddingScorer {
    fn default() -> Self {
        EmbeddingScorer {
            model_name: "local-hash-lexical-v1".to_string(),
        }
    }
}

impl EmbeddingScorer {
    pub fn try_new() -> Result<Self, String> {
        Ok(Self::default())
    }

    pub fn try_new_with_model_name(model_name: impl Into<String>) -> Result<Self, String> {
        Ok(EmbeddingScorer {
            model_name: model_name.into(),
        })
    }
}

impl RelevanceScorer for EmbeddingScorer {
    fn score(&self, item: &str, context: &str) -> RelevanceScore {
        if item.is_empty() || context.is_empty() {
            return RelevanceScore::empty("Embedding: empty input");
        }

        let item_vec = hashed_feature_vector(item);
        let context_vec = hashed_feature_vector(context);
        let sim = cosine_similarity(&item_vec, &context_vec);
        RelevanceScore::new(
            sim,
            format!("Embedding: local lexical similarity {:.2}", sim),
            Vec::new(),
        )
    }

    fn score_batch(&self, items: &[&str], context: &str) -> Vec<RelevanceScore> {
        if items.is_empty() {
            return Vec::new();
        }
        if context.is_empty() {
            return items
                .iter()
                .map(|_| RelevanceScore::empty("Embedding: empty context"))
                .collect();
        }

        let context_vec = hashed_feature_vector(context);
        items
            .iter()
            .map(|item| {
                if item.is_empty() {
                    return RelevanceScore::empty("Embedding: empty input");
                }
                let sim = cosine_similarity(&hashed_feature_vector(item), &context_vec);
                RelevanceScore::new(sim, format!("Embedding: local {:.2}", sim), Vec::new())
            })
            .collect()
    }

    fn is_available(&self) -> bool {
        true
    }
}

fn hashed_feature_vector(text: &str) -> Vec<f32> {
    let mut vector = vec![0.0_f32; VECTOR_DIMS];
    let normalized = text.to_lowercase();

    for token in normalized
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-')
        .filter(|token| !token.is_empty())
    {
        add_feature(&mut vector, token, 1.0);
        if token.len() >= 4 {
            for gram in token.as_bytes().windows(3) {
                add_feature_bytes(&mut vector, gram, 0.35);
            }
        }
    }

    normalize(&mut vector);
    vector
}

fn add_feature(vector: &mut [f32], feature: &str, weight: f32) {
    add_feature_bytes(vector, feature.as_bytes(), weight);
}

fn add_feature_bytes(vector: &mut [f32], feature: &[u8], weight: f32) {
    let hash = blake3::hash(feature);
    let bytes = hash.as_bytes();
    let raw = u64::from_le_bytes(bytes[0..8].try_into().expect("hash prefix has 8 bytes"));
    let idx = (raw as usize) % vector.len();
    let sign = if bytes[8] & 1 == 0 { 1.0 } else { -1.0 };
    vector[idx] += sign * weight;
}

fn normalize(vector: &mut [f32]) {
    let norm = vector
        .iter()
        .map(|value| (*value as f64) * (*value as f64))
        .sum::<f64>()
        .sqrt();
    if norm == 0.0 {
        return;
    }
    for value in vector {
        *value = (*value as f64 / norm) as f32;
    }
}

/// Cosine similarity for two vectors. Clamped to `[0, 1]` since relevance
/// only needs positive similarity.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let mut dot: f64 = 0.0;
    let mut norm_a: f64 = 0.0;
    let mut norm_b: f64 = 0.0;
    for i in 0..a.len() {
        let av = a[i] as f64;
        let bv = b[i] as f64;
        dot += av * bv;
        norm_a += av * av;
        norm_b += bv * bv;
    }
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    let sim = dot / (norm_a.sqrt() * norm_b.sqrt());
    sim.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_similarity_orthogonal_vectors() {
        let a = vec![1.0_f32, 0.0, 0.0, 0.0];
        let b = vec![0.0_f32, 1.0, 0.0, 0.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn cosine_similarity_identical_vectors() {
        let v = vec![1.0_f32, 2.0, 3.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-9, "got {}", sim);
    }

    #[test]
    fn cosine_similarity_opposite_clamped_to_zero() {
        let a = vec![1.0_f32, 1.0];
        let b = vec![-1.0_f32, -1.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn cosine_similarity_zero_vector_returns_zero() {
        let zero = vec![0.0_f32; 4];
        let v = vec![1.0_f32, 2.0, 3.0, 4.0];
        assert_eq!(cosine_similarity(&zero, &v), 0.0);
        assert_eq!(cosine_similarity(&v, &zero), 0.0);
    }

    #[test]
    fn scorer_is_available_without_external_runtime() {
        let s = EmbeddingScorer::try_new().expect("local scorer constructs");
        assert!(s.is_available());
        assert_eq!(s.model_name, "local-hash-lexical-v1");
    }

    #[test]
    fn lexical_match_outranks_unrelated_text() {
        let s = EmbeddingScorer::default();
        let related = s.score(
            "authentication failed for user",
            "login authentication error",
        );
        let unrelated = s.score("the weather is nice today", "login authentication error");
        assert!(
            related.score > unrelated.score,
            "related={}, unrelated={}",
            related.score,
            unrelated.score
        );
    }

    #[test]
    fn batch_returns_one_score_per_item() {
        let s = EmbeddingScorer::default();
        let items = ["auth error", "weather forecast", "login failure"];
        let scores = s.score_batch(&items, "authentication login error");
        assert_eq!(scores.len(), 3);
        for sc in scores {
            assert!((0.0..=1.0).contains(&sc.score));
        }
    }

    #[test]
    fn empty_inputs_short_circuit() {
        let s = EmbeddingScorer::default();
        let r = s.score("", "query");
        assert_eq!(r.score, 0.0);
        assert!(r.reason.contains("empty"));
    }
}
