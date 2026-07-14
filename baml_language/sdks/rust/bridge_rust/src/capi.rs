//! The BAML engine's C ABI, as consumed by this crate.
//!
//! Every call into the engine goes through [`Api`] — a table of the C
//! symbols — regardless of how those symbols are obtained:
//!
//! - the `static` backend links `bridge_cffi` into the host binary and
//!   resolves the symbols at link time (the development / `sdk_tests`
//!   configuration);
//! - the `dylib` backend resolves the same symbols from the prebuilt
//!   `bridge_cffi` shared library at runtime (the published
//!   configuration).
//!
//! Keeping both behind one table means the call semantics cannot diverge
//! between configurations. The type definitions here mirror
//! `bridge_cffi`'s exported ABI exactly and are asserted against it under
//! the `static` backend's tests.

use std::ffi::{c_char, c_void};

/// The engine's result-delivery callback. `content` is borrowed only for
/// the synchronous duration of the call — implementations must copy.
pub(crate) type CallbackFn = extern "C" fn(call_id: u32, content: *const c_char, length: usize);

/// Owned byte buffer returned by the engine. Must be released exactly once
/// with [`Api::free_buffer`]. Layout-identical to `bridge_cffi::Buffer`.
#[repr(C)]
pub(crate) struct Buffer {
    pub(crate) ptr: *const c_char,
    pub(crate) len: usize,
}

/// Function table over the engine's C ABI. Holds exactly the symbols the
/// bridge calls today; entries are added alongside the features that use
/// them (the version handshake lands with the dylib loader).
pub(crate) struct Api {
    pub(crate) create_baml_runtime:
        unsafe extern "C" fn(*const c_char, *const c_char) -> *const c_void,
    /// Returns a status buffer: empty on success, otherwise a UTF-8 error
    /// message. Read it with [`Api::take_status`].
    pub(crate) initialize_runtime_from_bytecode: unsafe extern "C" fn(*const u8, usize) -> Buffer,
    pub(crate) register_callback: unsafe extern "C" fn(CallbackFn),
    pub(crate) new_function_call: unsafe extern "C" fn() -> u64,
    pub(crate) call_function: unsafe extern "C" fn(*const c_char, *const u8, usize, u32),
    pub(crate) free_buffer: unsafe extern "C" fn(Buffer),
}

impl Api {
    /// Consume an engine status buffer: empty means success, anything else
    /// is the engine's UTF-8 error message. Frees the buffer either way.
    pub(crate) fn take_status(&self, buffer: Buffer) -> Result<(), String> {
        // SAFETY: the engine hands over an owned buffer of `len` bytes that
        // we must free exactly once; the bytes are copied before the free.
        #[expect(unsafe_code)]
        let message = unsafe {
            let bytes = if buffer.ptr.is_null() || buffer.len == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts(buffer.ptr.cast::<u8>(), buffer.len).to_vec()
            };
            (self.free_buffer)(buffer);
            bytes
        };
        if message.is_empty() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&message).into_owned())
        }
    }
}

#[cfg(feature = "static")]
mod static_backend {
    use super::{Api, Buffer, CallbackFn};
    use std::ffi::{c_char, c_void};

    // The engine's exported symbols, resolved at link time. `bridge_cffi`
    // is a dependency of this backend purely so its `#[no_mangle]` symbols
    // are linked into the final binary; nothing calls its Rust API.
    #[expect(unsafe_code)]
    unsafe extern "C" {
        fn create_baml_runtime(
            root_path: *const c_char,
            src_files_json: *const c_char,
        ) -> *const c_void;
        fn initialize_runtime_from_bytecode(bytecode: *const u8, length: usize) -> Buffer;
        fn register_callback(callback_fn: CallbackFn);
        fn new_function_call() -> u64;
        fn call_function(
            function_name: *const c_char,
            encoded_args: *const u8,
            length: usize,
            id: u32,
        );
        fn free_buffer(buf: Buffer);
    }

    pub(super) fn api() -> Api {
        // Force the linker to keep bridge_cffi in the dependency graph.
        let _ = bridge_cffi::new_function_call_id;
        Api {
            create_baml_runtime,
            initialize_runtime_from_bytecode,
            register_callback,
            new_function_call,
            call_function,
            free_buffer,
        }
    }
}

/// The process-wide API table for the selected backend.
pub(crate) fn api() -> &'static Api {
    static API: std::sync::OnceLock<Api> = std::sync::OnceLock::new();
    API.get_or_init(|| {
        #[cfg(feature = "static")]
        {
            static_backend::api()
        }
        #[cfg(not(feature = "static"))]
        {
            // The dylib backend replaces this in the loader workstream.
            unimplemented!("baml_bridge built without a backend: enable the `static` feature")
        }
    })
}
