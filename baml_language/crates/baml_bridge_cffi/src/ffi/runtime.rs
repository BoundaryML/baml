//! Runtime management FFI functions.

use std::{collections::HashMap, ffi::CStr};

use crate::{Buffer, engine::initialize_engine, panic::ffi_safe_ptr};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Returns the BAML version as a Buffer containing raw UTF-8 bytes.
/// Caller must free with free_buffer().
#[unsafe(no_mangle)]
pub extern "C" fn version() -> Buffer {
    Buffer::from(VERSION.as_bytes().to_vec())
}

/// Create/initialize the BAML runtime (global BexEngine).
///
/// # Arguments
/// * `root_path` - Root path for BAML files (C string)
/// * `src_files_json` - JSON-encoded HashMap<String, String> of file contents
/// * `env_vars_json` - JSON-encoded HashMap<String, String> of env vars
///
/// # Returns
/// Non-null pointer on success (value is opaque, not used), null on failure.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn create_baml_runtime(
    root_path: *const libc::c_char,
    src_files_json: *const libc::c_char,
    env_vars_json: *const libc::c_char,
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

        // Parse env_vars JSON
        let env_vars_str = unsafe {
            CStr::from_ptr(env_vars_json)
                .to_str()
                .map_err(|e| format!("Invalid UTF-8 in env_vars_json: {e}"))?
        };
        let env_vars: HashMap<String, String> = serde_json::from_str(env_vars_str)
            .map_err(|e| format!("Failed to parse env_vars JSON: {e}"))?;

        // Initialize global engine
        initialize_engine(root_path_str, src_files, env_vars)
            .map_err(|e| format!("Failed to initialize engine: {e}"))?;

        // Return non-null pointer to indicate success
        // The actual value doesn't matter since we use global engine
        Ok(std::ptr::dangling::<libc::c_void>())
    })
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
    eprintln!("invoke_runtime_cli not implemented in baml_bridge_cffi");
    1
}
