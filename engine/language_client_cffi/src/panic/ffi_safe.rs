use std::panic::{catch_unwind, AssertUnwindSafe};

/// Extract meaningful message from panic info
pub fn extract_panic_message(panic_info: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = panic_info.downcast_ref::<&str>() {
        format!("Panic: {s}")
    } else if let Some(s) = panic_info.downcast_ref::<String>() {
        format!("Panic: {s}")
    } else {
        "Panic with unknown error".to_string()
    }
}

/// Generic panic-safe wrapper for FFI functions returning pointers
pub fn ffi_safe_ptr<F>(f: F) -> *const libc::c_void
where
    F: FnOnce() -> Result<*const libc::c_void, String> + std::panic::UnwindSafe,
{
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(result)) => result,
        Ok(Err(e)) => {
            eprintln!("FFI function error: {e}");
            std::ptr::null()
        }
        Err(panic_info) => {
            let error_msg = extract_panic_message(panic_info);
            eprintln!("FFI function panicked: {error_msg}");
            std::ptr::null()
        }
    }
}

/// Panic-safe wrapper for FFI functions returning C strings
pub fn ffi_safe_cstring<F>(f: F) -> *const libc::c_char
where
    F: FnOnce() -> Result<*const libc::c_char, String> + std::panic::UnwindSafe,
{
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(result)) => result,
        Ok(Err(e)) => {
            eprintln!("FFI function error: {e}");
            create_fallback_cstring("error")
        }
        Err(panic_info) => {
            let error_msg = extract_panic_message(panic_info);
            eprintln!("FFI function panicked: {error_msg}");
            create_fallback_cstring("panic")
        }
    }
}

/// Create a fallback C string that's guaranteed to work
fn create_fallback_cstring(fallback: &str) -> *const libc::c_char {
    match std::ffi::CString::new(fallback) {
        Ok(c_string) => c_string.into_raw() as *const libc::c_char,
        Err(_) => std::ptr::null(), // This should never happen with simple strings
    }
}
