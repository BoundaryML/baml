use std::ffi::CString;

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
