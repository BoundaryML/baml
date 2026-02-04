//! baml_bridge_cffi - C FFI bindings for BAML using bex_engine.
//!
//! This crate provides the same FFI interface as `engine/language_client_cffi/`
//! but powered by `bex_engine` instead of `baml-runtime`.

mod ctypes;
mod engine;
mod error;
mod ffi;
mod panic;
mod schema_map;

pub use error::BridgeError;
// Re-export FFI functions
pub use ffi::{
    callbacks::{CallbackFn, OnTickCallbackFn, register_callbacks},
    functions::{
        call_function_from_c, call_function_parse_from_c, call_function_stream_from_c,
        cancel_function_call,
    },
    objects::{call_object_constructor, call_object_method, free_buffer},
    runtime::{create_baml_runtime, destroy_baml_runtime, invoke_runtime_cli, version},
};

// Generated protobuf module
pub mod baml {
    pub mod cffi {
        include!(concat!(env!("OUT_DIR"), "/baml.cffi.v1.rs"));
    }
}

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
