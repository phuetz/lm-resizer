//! Optional ONNX content detection backed by Google's official `magika` crate.
//!
//! Compiled only with the `magika` feature on native targets. The bundled
//! `standard_v3_3` model is loaded once into a process-wide [`Session`] (guarded
//! by a `Mutex` because `identify_content_sync` takes `&mut self`). The result's
//! Magika label (e.g. `"python"`, `"json"`, `"diff"`) is mapped onto our coarse
//! [`ContentType`] via [`super::magika_detector::map_magika_label`].
//!
//! Any failure (model init, inference error, lock poisoning) returns `None` so
//! the caller transparently falls back to the deterministic detector.

use std::sync::{Mutex, OnceLock};

use magika::Session;

use super::magika_detector::map_magika_label;
use crate::transforms::content_detector::ContentType;

/// Lazily-initialized Magika session. `None` if the model/runtime failed to
/// load — in that case every call falls back to deterministic detection.
static SESSION: OnceLock<Option<Mutex<Session>>> = OnceLock::new();

fn session() -> Option<&'static Mutex<Session>> {
    SESSION
        .get_or_init(|| match Session::new() {
            Ok(session) => Some(Mutex::new(session)),
            Err(err) => {
                // Never on stdout (MCP JSON-RPC purity); the caller degrades
                // gracefully to the deterministic detector.
                eprintln!("lm-resizer: Magika ONNX session init failed: {err}; using deterministic detection");
                None
            }
        })
        .as_ref()
}

/// Classify `content` with the Magika ONNX model, mapped to our [`ContentType`].
///
/// Returns `None` on any error so the caller falls back to the deterministic
/// detector. Magika applies its own low-confidence overwrite internally, so the
/// returned label is already confidence-gated.
pub fn detect_with_onnx(content: &str) -> Option<ContentType> {
    let session = session()?;
    let mut guard = session.lock().ok()?;
    let file_type = guard.identify_content_sync(content.as_bytes()).ok()?;
    Some(map_magika_label(file_type.info().label))
}

#[cfg(test)]
mod tests {
    use super::*;

    // These exercise the real bundled ONNX model; they only run when the
    // `magika` feature is enabled (the whole module is feature-gated).
    #[test]
    fn classifies_python_source() {
        let src = "def add(a, b):\n    return a + b\n\nclass Foo:\n    pass\n";
        // Magika labels Python as "python" → SourceCode in our mapping.
        assert_eq!(detect_with_onnx(src), Some(ContentType::SourceCode));
    }

    #[test]
    fn classifies_json() {
        let json = r#"{"name": "Alice", "age": 30, "tags": ["a", "b", "c"]}"#;
        assert_eq!(detect_with_onnx(json), Some(ContentType::JsonArray));
    }

    #[test]
    fn empty_input_does_not_panic() {
        // Whatever Magika returns for empty input, this must not panic.
        let _ = detect_with_onnx("");
    }
}
