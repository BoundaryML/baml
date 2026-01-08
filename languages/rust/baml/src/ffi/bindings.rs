//! FFI bindings - re-exported from baml-sys.
//!
//! The actual FFI functions are loaded dynamically by baml-sys.
//! This module provides the type definitions and re-exports.

// Re-export types from baml-sys
// Re-export library management functions
// Re-export the raw FFI functions
// These return Result to handle library loading errors
pub(crate) use baml_sys::{
    call_function_from_c, call_function_parse_from_c, call_function_stream_from_c,
    call_object_constructor, call_object_method, cancel_function_call, create_baml_runtime,
    destroy_baml_runtime, free_buffer, invoke_runtime_cli, register_callbacks, version,
};
