//! lm-resizer-core: Rust-native context compression primitives.

pub mod auth_mode;
pub mod cache_control;
pub mod ccr;
pub mod compression_policy;
pub mod relevance;
pub mod signals;
pub mod tokenizer;
pub mod transforms;

use std::ffi::{c_char, CString};
use std::sync::Arc;

use ccr::{CcrStore, InMemoryCcrStore};
use serde::Serialize;
use transforms::{
    detect_content_type, CompressionContext, CompressionPipeline, DiffNoise, DiffOffload,
    JsonMinifier, JsonOffload, LogOffload, LogTemplate, PipelineConfig, SourceCompressor,
};

// Re-exports for the live-zone dispatcher (Phase B PR-B2 consumes this).
// Hoisted to the crate root so the proxy crate gets one stable import
// path: `use lm_resizer_core::compute_frozen_count;`. Keeping the
// `cache_control` module public too means downstream code can reach
// the helper types directly when needed.
pub use cache_control::compute_frozen_count;

/// Stable high-level compression report for embedding applications.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CompressionReport {
    pub content_type: String,
    pub original_bytes: usize,
    pub compressed_bytes: usize,
    pub bytes_saved: usize,
    pub steps_applied: Vec<String>,
    pub cache_keys: Vec<String>,
    pub output: String,
}

/// Small embeddable facade over the default lm-resizer compression pipeline.
pub struct LmResizer {
    pipeline: CompressionPipeline,
    store: Arc<dyn CcrStore>,
}

impl LmResizer {
    /// Build an instance with the default pipeline and in-memory CCR storage.
    pub fn new() -> Self {
        Self::with_store(Arc::new(InMemoryCcrStore::new()))
    }

    /// Build an instance with caller-provided CCR storage.
    pub fn with_store(store: Arc<dyn CcrStore>) -> Self {
        Self {
            pipeline: default_pipeline(),
            store,
        }
    }

    /// Compress text using the same default transform stack as the CLI.
    pub fn compress(&self, content: &str, query: impl Into<String>) -> CompressionReport {
        let detection = detect_content_type(content);
        let ctx = CompressionContext {
            query: query.into(),
            token_budget: None,
        };
        let result = self
            .pipeline
            .run(content, detection.content_type, &ctx, self.store.as_ref());
        CompressionReport {
            content_type: detection.content_type.as_str().to_string(),
            original_bytes: content.len(),
            compressed_bytes: result.output.len(),
            bytes_saved: result.bytes_saved,
            steps_applied: result.steps_applied,
            cache_keys: result.cache_keys,
            output: result.output,
        }
    }
}

impl Default for LmResizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Build the default compression pipeline used by `LmResizer` and the CLI.
pub fn default_pipeline() -> CompressionPipeline {
    let cfg = PipelineConfig::default();
    CompressionPipeline::builder()
        .with_config(cfg.clone())
        .with_reformat(JsonMinifier)
        .with_reformat(LogTemplate::new(cfg.reformat.log_template))
        .with_reformat(SourceCompressor::default())
        .with_offload(JsonOffload::new(cfg.offload.json))
        .with_offload(LogOffload::new(cfg.bloat.log))
        .with_offload(DiffOffload::new(cfg.bloat.diff))
        .with_offload(DiffNoise::new(cfg.offload.diff_noise))
        .build()
}

/// Identity stub used by downstream crates and the Python binding to verify
/// linkage end-to-end.
pub fn hello() -> &'static str {
    "lm-resizer-core"
}

/// Compress UTF-8 text through the default pipeline and return a JSON
/// `CompressionReport` as a null-terminated string.
///
/// The returned pointer must be released with [`lm_resizer_string_free`].
/// This function is intentionally allocator-neutral for C and WASM hosts:
/// callers pass raw bytes plus lengths, and receive an owned UTF-8 JSON string.
#[no_mangle]
pub unsafe extern "C" fn lm_resizer_compress_json(
    content_ptr: *const u8,
    content_len: usize,
    query_ptr: *const u8,
    query_len: usize,
) -> *mut c_char {
    let result = ffi_compress_json(content_ptr, content_len, query_ptr, query_len);
    c_string_ptr(result)
}

/// Free strings returned by lm-resizer C/WASM ABI functions.
#[no_mangle]
pub unsafe extern "C" fn lm_resizer_string_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        let _ = CString::from_raw(ptr);
    }
}

/// Allocate a byte buffer from lm-resizer's allocator for WASM hosts.
#[no_mangle]
pub extern "C" fn lm_resizer_alloc(len: usize) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(len);
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    ptr
}

/// Free a byte buffer allocated by [`lm_resizer_alloc`].
#[no_mangle]
pub unsafe extern "C" fn lm_resizer_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        let _ = Vec::from_raw_parts(ptr, 0, len);
    }
}

fn ffi_compress_json(
    content_ptr: *const u8,
    content_len: usize,
    query_ptr: *const u8,
    query_len: usize,
) -> String {
    let content = match unsafe { ffi_str(content_ptr, content_len) } {
        Ok(content) => content,
        Err(error) => return ffi_error_json(error),
    };
    let query = match unsafe { ffi_str(query_ptr, query_len) } {
        Ok(query) => query,
        Err(error) => return ffi_error_json(error),
    };
    let report = LmResizer::new().compress(content, query);
    serde_json::to_string(&report).unwrap_or_else(|err| ffi_error_json(err.to_string()))
}

unsafe fn ffi_str<'a>(ptr: *const u8, len: usize) -> Result<&'a str, String> {
    if len == 0 {
        return Ok("");
    }
    if ptr.is_null() {
        return Err("null pointer with non-zero length".to_string());
    }
    std::str::from_utf8(std::slice::from_raw_parts(ptr, len))
        .map_err(|err| format!("input is not valid UTF-8: {err}"))
}

fn ffi_error_json(error: impl AsRef<str>) -> String {
    serde_json::to_string(&serde_json::json!({
        "error": error.as_ref()
    }))
    .unwrap_or_else(|_| "{\"error\":\"serialization failed\"}".to_string())
}

fn c_string_ptr(value: String) -> *mut c_char {
    match CString::new(value) {
        Ok(value) => value.into_raw(),
        Err(_) => CString::new("{\"error\":\"interior nul in output\"}")
            .expect("static string has no nul")
            .into_raw(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;
    use std::ptr;

    #[test]
    fn hello_returns_crate_name() {
        assert_eq!(hello(), "lm-resizer-core");
    }

    #[test]
    fn high_level_api_compresses_text() {
        let resizer = LmResizer::new();
        let report = resizer.compress(r#"{ "a": 1 }"#, "");
        assert!(report.compressed_bytes <= report.original_bytes);
        assert!(!report.output.is_empty());
    }

    #[test]
    fn c_abi_compress_json_round_trips_report() {
        let content = br#"{ "a": 1 }"#;
        let query = b"";
        let ptr = unsafe {
            lm_resizer_compress_json(content.as_ptr(), content.len(), query.as_ptr(), query.len())
        };
        assert!(!ptr.is_null());
        let json = unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() };
        unsafe { lm_resizer_string_free(ptr) };
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["original_bytes"], content.len());
        assert!(value["output"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));
    }

    #[test]
    fn c_abi_reports_invalid_utf8() {
        let content = [0xff_u8];
        let ptr =
            unsafe { lm_resizer_compress_json(content.as_ptr(), content.len(), ptr::null(), 0) };
        let json = unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() };
        unsafe { lm_resizer_string_free(ptr) };
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(value["error"].as_str().unwrap().contains("valid UTF-8"));
    }

    #[test]
    fn wasm_alloc_free_round_trip() {
        let ptr = lm_resizer_alloc(8);
        assert!(!ptr.is_null());
        unsafe {
            ptr.write_bytes(0xab, 8);
            lm_resizer_free(ptr, 8);
        }
    }
}
