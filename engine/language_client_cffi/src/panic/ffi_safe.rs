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
