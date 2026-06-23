//! WASM ABI for lm-resizer JavaScript hosts.
//!
//! This crate exposes the **real** lm-resizer compression pipeline (the same
//! `lm-resizer-core` default pipeline the CLI and native C ABI use) over a
//! small C ABI that compiles to `wasm32-unknown-unknown`. The matching JS
//! wrapper lives in `packages/wasm/index.js`.
//!
//! The ABI mirrors `lm-resizer-core`'s native C ABI exactly
//! (`content_ptr`, `content_len`, `query_ptr`, `query_len`). Core gates its
//! own `#[no_mangle]` C ABI to non-wasm targets and the `#[no_mangle]`
//! functions below are wasm32-only, so exactly one definition of each symbol
//! exists per target — no duplicate symbols at link time.

/// Compress `content` through the default lm-resizer pipeline (optionally
/// biased by a relevance `query`) and return the serialized
/// `CompressionReport` JSON. This is the target-independent core of the ABI so
/// it can be unit-tested on the host; the wasm `#[no_mangle]` entry point just
/// marshals pointers into `&str` and calls this.
///
/// Only reachable from the wasm `abi` module (or tests), so it is gated to
/// those configs to stay dead-code-clean on a plain native build.
#[cfg(any(target_arch = "wasm32", test))]
fn compress_to_json(content: &str, query: &str) -> String {
    let report = lm_resizer_core::LmResizer::new().compress(content, query);
    serde_json::to_string(&report).unwrap_or_else(|err| {
        serde_json::json!({
            "error": format!("failed to encode report: {err}"),
            "output": "",
            "steps_applied": [],
            "cache_keys": [],
        })
        .to_string()
    })
}

#[cfg(target_arch = "wasm32")]
mod abi {
    use super::compress_to_json;
    use std::alloc::{alloc, dealloc, Layout};
    use std::ffi::{c_char, CString};

    /// Compress `content` (UTF-8) through the real lm-resizer pipeline.
    /// Returns a null-terminated JSON `CompressionReport`
    /// (`content_type`, `original_bytes`, `compressed_bytes`, `bytes_saved`,
    /// `steps_applied`, `cache_keys`, `output`) or `{"error":"…"}`.
    ///
    /// # Safety
    /// `content_ptr`/`query_ptr` must each point to `*_len` initialized bytes
    /// (or be null with length 0). Release the result with
    /// [`lm_resizer_string_free`].
    #[no_mangle]
    pub unsafe extern "C" fn lm_resizer_compress_json(
        content_ptr: *const u8,
        content_len: usize,
        query_ptr: *const u8,
        query_len: usize,
    ) -> *mut c_char {
        let content = match slice_to_str(content_ptr, content_len) {
            Ok(value) => value,
            Err(message) => return string_to_ptr(error_report(&message)),
        };
        let query = slice_to_str(query_ptr, query_len).unwrap_or("");
        string_to_ptr(compress_to_json(content, query))
    }

    /// Free strings returned by [`lm_resizer_compress_json`].
    ///
    /// # Safety
    /// `ptr` must have been returned by [`lm_resizer_compress_json`] and not
    /// yet freed.
    #[no_mangle]
    pub unsafe extern "C" fn lm_resizer_string_free(ptr: *mut c_char) {
        if !ptr.is_null() {
            drop(CString::from_raw(ptr));
        }
    }

    /// Allocate a byte buffer for the JS host to write input into.
    ///
    /// # Safety
    /// Release with [`lm_resizer_free`] passing the same `len`.
    #[no_mangle]
    pub unsafe extern "C" fn lm_resizer_alloc(len: usize) -> *mut u8 {
        if len == 0 {
            return std::ptr::null_mut();
        }
        let Ok(layout) = Layout::array::<u8>(len) else {
            return std::ptr::null_mut();
        };
        alloc(layout)
    }

    /// Free a byte buffer allocated by [`lm_resizer_alloc`].
    ///
    /// # Safety
    /// `ptr`/`len` must come from a prior [`lm_resizer_alloc`] call.
    #[no_mangle]
    pub unsafe extern "C" fn lm_resizer_free(ptr: *mut u8, len: usize) {
        if ptr.is_null() || len == 0 {
            return;
        }
        if let Ok(layout) = Layout::array::<u8>(len) {
            dealloc(ptr, layout);
        }
    }

    /// # Safety
    /// `ptr` must point to `len` initialized bytes, or be null with `len == 0`.
    unsafe fn slice_to_str<'a>(ptr: *const u8, len: usize) -> Result<&'a str, String> {
        if len == 0 {
            return Ok("");
        }
        if ptr.is_null() {
            return Err("null pointer with non-zero length".to_string());
        }
        std::str::from_utf8(std::slice::from_raw_parts(ptr, len))
            .map_err(|err| format!("input is not valid UTF-8: {err}"))
    }

    fn error_report(message: &str) -> String {
        serde_json::json!({
            "error": message,
            "output": "",
            "steps_applied": [],
            "cache_keys": [],
        })
        .to_string()
    }

    fn string_to_ptr(value: String) -> *mut c_char {
        match CString::new(value) {
            Ok(string) => string.into_raw(),
            Err(_) => CString::new("{\"error\":\"interior nul byte\"}")
                .expect("static string has no nul")
                .into_raw(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::compress_to_json;

    #[test]
    fn runs_the_real_pipeline_not_minify() {
        // A large uniform-schema JSON array should be compressed by the real
        // pipeline (SmartCrusher / JSON offload), recording pipeline steps —
        // unlike the old minify-only stub.
        let rows: Vec<serde_json::Value> = (0..80)
            .map(|i| serde_json::json!({"id": i, "name": "row", "status": "ok"}))
            .collect();
        let payload = serde_json::to_string(&serde_json::Value::Array(rows)).unwrap();
        let json = compress_to_json(&payload, "");
        let report: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(report.get("error").is_none(), "unexpected error: {json}");
        assert_eq!(
            report["original_bytes"].as_u64().unwrap() as usize,
            payload.len()
        );
        assert!(report["compressed_bytes"].as_u64().unwrap() > 0);
        assert!(!report["output"].as_str().unwrap().is_empty());
        // The stub reported only ["json_minify"]; the real pipeline applies
        // real reformat/offload steps.
        let steps = report["steps_applied"].as_array().unwrap();
        assert!(
            steps.iter().any(|s| s != "json_minify"),
            "expected real pipeline steps, got {steps:?}"
        );
    }

    #[test]
    fn plain_text_passes_through() {
        let json = compress_to_json("hello world", "");
        let report: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(report.get("error").is_none());
        assert_eq!(report["output"].as_str().unwrap(), "hello world");
    }
}
