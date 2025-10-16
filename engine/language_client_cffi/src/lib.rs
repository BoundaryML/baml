/// cbindgen:ignore
mod ctypes;
mod ffi;
mod panic;
mod raw_ptr_wrapper;

// Explicit API exports - this is the complete public C FFI API
pub use ffi::{
    callbacks::{register_callbacks, CallbackFn, OnTickCallbackFn},
    functions::{call_function_from_c, call_function_parse_from_c, call_function_stream_from_c},
    objects::{call_object_constructor, call_object_method, free_buffer, Buffer},
    runtime::{create_baml_runtime, destroy_baml_runtime, invoke_runtime_cli, version},
};

// Keep the generated protobuf module
pub mod baml {
    pub mod cffi {
        include!(concat!(env!("OUT_DIR"), "/baml.cffi.rs"));
    }
}
