//! bridge_cffi - C FFI bindings for BAML using bex_engine.
//!
//! This crate provides the same FFI interface as `engine/language_client_cffi/`
//! but powered by `bex_engine` instead of `baml-runtime`.

pub mod collector;
pub mod engine;
pub mod error;
mod ffi;
pub mod host_spans;
mod panic;

pub use bridge_ctypes::baml;
pub use error::BridgeError;
pub use ffi::{
    callbacks::{CallbackFn, OnTickCallbackFn, register_callbacks},
    functions::{
        call_function_from_c, call_function_parse_from_c, call_function_stream_from_c,
        cancel_function_call,
    },
    objects::{call_object_constructor, call_object_method, free_buffer},
    runtime::{create_baml_runtime, destroy_baml_runtime, invoke_runtime_cli, version},
};

/// Buffer type for returning data across FFI boundary.
/// Caller must free with `free_buffer()`.
///
/// This matches the Buffer struct expected by baml-sys.
#[repr(C)]
pub struct Buffer {
    pub ptr: *const i8,
    pub len: usize,
}

impl Buffer {
    pub fn from(data: Vec<u8>) -> Self {
        let data = data.into_boxed_slice();
        let ptr = data.as_ptr() as *const i8;
        let len = data.len();
        std::mem::forget(data);
        Buffer { ptr, len }
    }

    pub fn as_ptr(&self) -> *const i8 {
        self.ptr
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}
