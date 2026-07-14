//! Runtime management FFI functions.

use std::{collections::HashMap, ffi::CStr};

use crate::{
    Buffer, initialize_runtime,
    initialize_runtime_from_bytecode as initialize_runtime_from_bytecode_impl, panic::ffi_safe_ptr,
};

/// Returns the BAML version as a Buffer containing raw UTF-8 bytes.
/// Caller must free with free_buffer().
#[unsafe(no_mangle)]
pub extern "C" fn version() -> Buffer {
    Buffer::from(baml_version::CANONICAL_VERSION.as_bytes().to_vec())
}

/// Create/initialize the BAML runtime (global BexEngine).
///
/// # Arguments
/// * `root_path` - Root path for BAML files (C string)
/// * `src_files_json` - JSON-encoded HashMap<String, String> of file contents
///
/// # Returns
/// Non-null pointer on success (value is opaque, not used), null on failure.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn create_baml_runtime(
    root_path: *const libc::c_char,
    src_files_json: *const libc::c_char,
) -> *const libc::c_void {
    ffi_safe_ptr(|| -> Result<*const libc::c_void, String> {
        // Parse root_path
        let root_path_str = unsafe {
            CStr::from_ptr(root_path)
                .to_str()
                .map_err(|e| format!("Invalid UTF-8 in root_path: {e}"))?
        };

        // Parse src_files JSON
        let src_files_str = unsafe {
            CStr::from_ptr(src_files_json)
                .to_str()
                .map_err(|e| format!("Invalid UTF-8 in src_files_json: {e}"))?
        };
        let src_files: HashMap<String, String> = serde_json::from_str(src_files_str)
            .map_err(|e| format!("Failed to parse src_files JSON: {e}"))?;

        // Initialize global runtime
        initialize_runtime(root_path_str, src_files)
            .map_err(|e| format!("Failed to initialize runtime: {e}"))?;

        // Return non-null pointer to indicate success
        // The actual value doesn't matter since we use global engine
        Ok(std::ptr::dangling::<libc::c_void>())
    })
}

/// Initialize the process-global BAML runtime from serialized bytecode.
///
/// An empty returned buffer means success. On failure, the buffer contains a
/// UTF-8 error message. The caller owns every returned buffer and must release
/// it with [`crate::free_buffer`].
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn initialize_runtime_from_bytecode(bytecode: *const u8, length: usize) -> Buffer {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if bytecode.is_null() && length != 0 {
            return Err("bytecode pointer is null but length is nonzero".to_string());
        }

        let bytecode = if length == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(bytecode, length) }
        };
        initialize_runtime_from_bytecode_impl(bytecode)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }));

    match result {
        Ok(Ok(())) => Buffer::from(Vec::new()),
        Ok(Err(error)) => Buffer::from(error.into_bytes()),
        Err(panic_info) => {
            let message = panic_info
                .downcast_ref::<&str>()
                .map(|message| (*message).to_string())
                .or_else(|| panic_info.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".to_string());
            Buffer::from(format!("panic while initializing BAML bytecode: {message}").into_bytes())
        }
    }
}

/// Destroy the BAML runtime.
/// This is a no-op since the global engine persists for the process lifetime.
#[unsafe(no_mangle)]
pub extern "C" fn destroy_baml_runtime(_runtime: *const libc::c_void) {
    // No-op: global engine persists
}

/// Invoke the BAML CLI.
/// Currently returns 1 (error) as CLI is not implemented for bridge.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn invoke_runtime_cli(_args: *const *const libc::c_char) -> libc::c_int {
    // TODO: Implement CLI invocation if needed
    eprintln!("invoke_runtime_cli not implemented in bridge_cffi");
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytecode_initializer_rejects_null_pointer_with_nonzero_length() {
        let buffer = initialize_runtime_from_bytecode(std::ptr::null(), 1);
        let message = unsafe { std::slice::from_raw_parts(buffer.ptr.cast::<u8>(), buffer.len) };
        assert_eq!(
            std::str::from_utf8(message).unwrap(),
            "bytecode pointer is null but length is nonzero"
        );
        crate::free_buffer(buffer);
    }
}
