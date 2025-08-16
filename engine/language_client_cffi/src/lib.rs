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

// WASM-specific exports
#[cfg(target_arch = "wasm32")]
mod wasm_exports {
    use wasm_bindgen::prelude::*;
    use crate::ffi::runtime::create_baml_runtime;
    use crate::ffi::functions::{call_function_from_c, call_function_stream_from_c};
    use std::ffi::CString;
    
    #[wasm_bindgen]
    pub fn create_baml_runtime_wasm(
        baml_src: String,
        config: String,
        env_vars: String
    ) -> *mut std::ffi::c_void {
        let baml_src = CString::new(baml_src).unwrap();
        let config = CString::new(config).unwrap();
        let env_vars = CString::new(env_vars).unwrap();
        
        create_baml_runtime(
            baml_src.as_ptr(),
            config.as_ptr(),
            env_vars.as_ptr()
        ) as *mut std::ffi::c_void
    }
    
    #[wasm_bindgen]
    pub fn call_function_wasm(
        runtime: *mut std::ffi::c_void,
        function_name: String,
        args_proto: Vec<u8>,
        callback_id: u32
    ) {
        let function_name = CString::new(function_name).unwrap();
        let args_ptr = args_proto.as_ptr() as *const i8;
        let args_len = args_proto.len();
        
        call_function_from_c(
            runtime as *const std::ffi::c_void,
            function_name.as_ptr(),
            args_ptr,
            args_len,
            callback_id
        );
    }
    
    #[wasm_bindgen]
    pub fn call_function_stream_wasm(
        runtime: *mut std::ffi::c_void,
        function_name: String,
        args_proto: Vec<u8>,
        callback_id: u32
    ) {
        let function_name = CString::new(function_name).unwrap();
        let args_ptr = args_proto.as_ptr() as *const i8;
        let args_len = args_proto.len();
        
        call_function_stream_from_c(
            runtime as *const std::ffi::c_void,
            function_name.as_ptr(),
            args_ptr,
            args_len,
            callback_id
        );
    }
}
