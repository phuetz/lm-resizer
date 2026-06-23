use std::alloc::{alloc, dealloc, Layout};
use std::ffi::{c_char, CStr, CString};

#[no_mangle]
pub unsafe extern "C" fn lm_resizer_compress_json(ptr: *const c_char) -> *mut c_char {
    if ptr.is_null() {
        return string_to_ptr(error_report("input pointer is null"));
    }
    let input = match CStr::from_ptr(ptr).to_str() {
        Ok(value) => value,
        Err(err) => return string_to_ptr(error_report(&format!("input is not UTF-8: {err}"))),
    };
    let parsed: serde_json::Value = match serde_json::from_str(input) {
        Ok(value) => value,
        Err(err) => return string_to_ptr(error_report(&format!("input is not JSON: {err}"))),
    };
    let output = match serde_json::to_string(&parsed) {
        Ok(value) => value,
        Err(err) => return string_to_ptr(error_report(&format!("failed to encode JSON: {err}"))),
    };
    let report = serde_json::json!({
        "content_type": "json",
        "original_bytes": input.len(),
        "compressed_bytes": output.len(),
        "bytes_saved": input.len().saturating_sub(output.len()),
        "steps_applied": ["json_minify"],
        "cache_keys": [],
        "output": output,
    });
    string_to_ptr(report.to_string())
}

#[no_mangle]
pub unsafe extern "C" fn lm_resizer_string_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(CString::from_raw(ptr));
    }
}

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

#[no_mangle]
pub unsafe extern "C" fn lm_resizer_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    if let Ok(layout) = Layout::array::<u8>(len) {
        dealloc(ptr, layout);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compress_json_minifies() {
        let input = CString::new("{ \"a\": 1 }").unwrap();
        let ptr = unsafe { lm_resizer_compress_json(input.as_ptr()) };
        let text = unsafe { CStr::from_ptr(ptr).to_string_lossy().to_string() };
        unsafe { lm_resizer_string_free(ptr) };
        assert!(text.contains("\"output\":\"{\\\"a\\\":1}\""));
    }
}
