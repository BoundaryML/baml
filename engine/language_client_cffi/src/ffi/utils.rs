use std::ffi::{CStr, CString};

/// Safely convert C string to Rust string
pub fn c_str_to_string(ptr: *const libc::c_char) -> Result<String, String> {
    if ptr.is_null() {
        return Err("Null pointer provided".to_string());
    }

    unsafe {
        CStr::from_ptr(ptr)
            .to_str()
            .map(|s| s.to_string())
            .map_err(|e| format!("Invalid UTF-8: {e}"))
    }
}

/// Safely create CString with fallback
pub fn create_c_string_safe(s: &str, fallback: &str) -> *const libc::c_char {
    match CString::new(s) {
        Ok(c_string) => c_string.into_raw() as *const libc::c_char,
        Err(_) => {
            eprintln!("Warning: Failed to create CString from: {s}");
            match CString::new(fallback) {
                Ok(fallback_cstring) => fallback_cstring.into_raw() as *const libc::c_char,
                Err(_) => std::ptr::null(), // This should never happen with simple fallback strings
            }
        }
    }
}

/// Common error handling for FFI functions that return error pointers
pub fn handle_ffi_error(e: anyhow::Error) -> *const libc::c_void {
    let error_str = e.to_string();
    match CString::new(error_str.clone()) {
        Ok(c_string) => Box::into_raw(Box::new(c_string)) as *const libc::c_void,
        Err(_) => {
            eprintln!("Error: Failed to create CString from error message: {error_str}");
            // Return a generic error CString
            match CString::new("Error creating error message") {
                Ok(fallback) => Box::into_raw(Box::new(fallback)) as *const libc::c_void,
                Err(_) => std::ptr::null(), // This should never happen
            }
        }
    }
}
