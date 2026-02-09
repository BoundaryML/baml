//! BAML FFI bindings with runtime dynamic library loading.
//!
//! This crate provides low-level FFI bindings to the BAML runtime library.
//! The library is loaded dynamically at runtime using `libloading`.
//!
//! # Library Resolution
//!
//! The library is searched in the following order:
//! 1. Explicit path set via [`set_library_path()`]
//! 2. `BAML_LIBRARY_PATH` environment variable
//! 3. User cache directory (`~/.cache/baml/libs/{VERSION}/`)
//! 4. Auto-download from GitHub releases (if `download` feature enabled)
//! 5. System default paths (`/usr/local/lib/`, etc.)
//!
//! # Usage
//!
//! ## Runtime Loading (Default)
//!
//! ```rust,ignore
//! use baml_sys::{get_symbols, version};
//!
//! // Library is loaded on first access
//! let v = version()?;
//! println!("BAML version: {v}");
//! ```
//!
//! ## Build-time Hook
//!
//! In your `build.rs`:
//!
//! ```rust,ignore
//! fn main() {
//!     // Ensure library is available before build completes
//!     let lib_path = baml_sys::ensure_library()
//!         .expect("Failed to find/download BAML library");
//!     println!("cargo:rerun-if-changed={}", lib_path.display());
//! }
//! ```
//!
//! # Environment Variables
//!
//! - `BAML_LIBRARY_PATH`: Explicit path to the library
//! - `BAML_CACHE_DIR`: Override cache directory location
//! - `BAML_LIBRARY_DISABLE_DOWNLOAD`: Set to "true" to disable auto-download

#![warn(missing_docs)]

mod download;
mod error;
mod loader;
mod symbols;

pub use error::{BamlSysError, Result};
use libc::{c_char, c_int, c_void, size_t};
pub use loader::{
    ensure_library, set_library_path, ENV_CACHE_DIR, ENV_DISABLE_DOWNLOAD, ENV_LIBRARY_PATH,
    VERSION,
};
pub use symbols::{get_symbols, Buffer, CallbackFn, OnTickCallbackFn, Symbols};

// ============================================================================
// Safe wrapper functions
// ============================================================================

/// Get the BAML library version.
pub fn version() -> Result<String> {
    let symbols = get_symbols()?;
    // Safety: version() returns a Buffer containing the version string
    #[allow(unsafe_code)]
    let buf = unsafe { (symbols.version)() };

    let result = if !buf.ptr.is_null() && buf.len > 0 {
        #[allow(unsafe_code)]
        let bytes = unsafe { std::slice::from_raw_parts(buf.ptr as *const u8, buf.len) };
        String::from_utf8_lossy(bytes).into_owned()
    } else {
        "unknown".to_string()
    };

    // Free the buffer
    #[allow(unsafe_code)]
    unsafe {
        (symbols.free_buffer)(buf);
    }

    Ok(result)
}

/// Register callbacks for FFI operations.
///
/// # Safety
/// The callbacks must remain valid for the lifetime of the program.
#[allow(unsafe_code)]
pub unsafe fn register_callbacks(
    callback_fn: CallbackFn,
    error_callback_fn: CallbackFn,
    on_tick_callback_fn: OnTickCallbackFn,
) -> Result<()> {
    let symbols = get_symbols()?;
    unsafe {
        (symbols.register_callbacks)(callback_fn, error_callback_fn, on_tick_callback_fn);
    }
    Ok(())
}

/// Create a new BAML runtime.
///
/// # Safety
/// All pointers must be valid C strings.
#[allow(unsafe_code)]
pub unsafe fn create_baml_runtime(
    root_path: *const c_char,
    src_files_json: *const c_char,
    env_vars_json: *const c_char,
) -> Result<*const c_void> {
    let symbols = get_symbols()?;
    Ok(unsafe { (symbols.create_baml_runtime)(root_path, src_files_json, env_vars_json) })
}

/// Destroy a BAML runtime.
///
/// # Safety
/// The runtime pointer must be valid and not already destroyed.
#[allow(unsafe_code)]
pub unsafe fn destroy_baml_runtime(runtime: *const c_void) -> Result<()> {
    let symbols = get_symbols()?;
    unsafe {
        (symbols.destroy_baml_runtime)(runtime);
    }
    Ok(())
}

/// Invoke the runtime CLI.
///
/// # Safety
/// The args pointer must be a valid null-terminated array of C strings.
#[allow(unsafe_code)]
pub unsafe fn invoke_runtime_cli(args: *const *const c_char) -> Result<c_int> {
    let symbols = get_symbols()?;
    Ok(unsafe { (symbols.invoke_runtime_cli)(args) })
}

/// Call a BAML function.
///
/// Returns a Buffer containing the InvocationResponse protobuf.
/// The caller is responsible for decoding the buffer and freeing it.
///
/// # Safety
/// All pointers must be valid.
#[allow(unsafe_code)]
pub unsafe fn call_function_from_c(
    runtime: *const c_void,
    function_name: *const c_char,
    encoded_args: *const c_char,
    length: size_t,
    id: u32,
) -> Result<Buffer> {
    let symbols = get_symbols()?;
    Ok(unsafe { (symbols.call_function_from_c)(runtime, function_name, encoded_args, length, id) })
}

/// Call a BAML function with streaming.
///
/// Returns a Buffer containing the InvocationResponse protobuf.
/// The caller is responsible for decoding the buffer and freeing it.
///
/// # Safety
/// All pointers must be valid.
#[allow(unsafe_code)]
pub unsafe fn call_function_stream_from_c(
    runtime: *const c_void,
    function_name: *const c_char,
    encoded_args: *const c_char,
    length: size_t,
    id: u32,
) -> Result<Buffer> {
    let symbols = get_symbols()?;
    Ok(unsafe {
        (symbols.call_function_stream_from_c)(runtime, function_name, encoded_args, length, id)
    })
}

/// Call a BAML function for parsing.
///
/// Returns a Buffer containing the InvocationResponse protobuf.
/// The caller is responsible for decoding the buffer and freeing it.
///
/// # Safety
/// All pointers must be valid.
#[allow(unsafe_code)]
pub unsafe fn call_function_parse_from_c(
    runtime: *const c_void,
    function_name: *const c_char,
    encoded_args: *const c_char,
    length: size_t,
    id: u32,
) -> Result<Buffer> {
    let symbols = get_symbols()?;
    Ok(unsafe {
        (symbols.call_function_parse_from_c)(runtime, function_name, encoded_args, length, id)
    })
}

/// Build an HTTP request for a BAML function without executing it.
///
/// Returns a Buffer containing the InvocationResponse protobuf.
/// The caller is responsible for decoding the buffer and freeing it.
///
/// # Safety
/// All pointers must be valid.
#[allow(unsafe_code)]
pub unsafe fn build_request_from_c(
    runtime: *const c_void,
    function_name: *const c_char,
    encoded_args: *const c_char,
    length: size_t,
    id: u32,
) -> Result<Buffer> {
    let symbols = get_symbols()?;
    Ok(unsafe { (symbols.build_request_from_c)(runtime, function_name, encoded_args, length, id) })
}

/// Cancel a function call.
///
/// Returns a Buffer containing the InvocationResponse protobuf.
/// The caller is responsible for decoding the buffer and freeing it.
///
/// # Safety
/// The id must be a valid call ID.
#[allow(unsafe_code)]
pub unsafe fn cancel_function_call(id: u32) -> Result<Buffer> {
    let symbols = get_symbols()?;
    Ok(unsafe { (symbols.cancel_function_call)(id) })
}

/// Call an object constructor.
///
/// # Safety
/// The `encoded_invocation` pointer must be valid.
#[allow(unsafe_code)]
pub unsafe fn call_object_constructor(
    encoded_invocation: *const c_char,
    length: size_t,
) -> Result<Buffer> {
    let symbols = get_symbols()?;
    Ok(unsafe { (symbols.call_object_constructor)(encoded_invocation, length) })
}

/// Call an object method.
///
/// # Safety
/// All pointers must be valid.
#[allow(unsafe_code)]
pub unsafe fn call_object_method(
    runtime: *const c_void,
    encoded_invocation: *const c_char,
    length: size_t,
) -> Result<Buffer> {
    let symbols = get_symbols()?;
    Ok(unsafe { (symbols.call_object_method)(runtime, encoded_invocation, length) })
}

/// Free a buffer returned from object operations.
///
/// # Safety
/// The buffer must have been returned from this library.
#[allow(unsafe_code)]
pub unsafe fn free_buffer(buf: Buffer) -> Result<()> {
    let symbols = get_symbols()?;
    unsafe {
        (symbols.free_buffer)(buf);
    }
    Ok(())
}

/// Get the path to the loaded library.
pub fn library_path() -> Result<std::path::PathBuf> {
    let lib = loader::get_library()?;
    Ok(lib.path.clone())
}
