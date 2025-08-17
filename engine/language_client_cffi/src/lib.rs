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
pub mod wasm_exports {
    use wasm_bindgen::prelude::*;
    use crate::ffi::runtime::{create_baml_runtime, destroy_baml_runtime};
    use crate::ffi::functions::{call_function_from_c, call_function_stream_from_c};
    use crate::ffi::callbacks::register_callbacks;
    use std::ffi::CString;
    
    // JavaScript callback functions will be called through web_sys
    
    #[wasm_bindgen]
    pub fn init_wasm() {
        // Set up console error panic hook for better debugging
        #[cfg(feature = "console_error_panic_hook")]
        console_error_panic_hook::set_once();
        
        // Initialize baml_log directly in init_wasm
        // This ensures BAML logging is set up before any callbacks are registered
        match baml_log::init() {
            Ok(_) => web_sys::console::log_1(&"BAML logger initialized".into()),
            Err(e) => web_sys::console::error_1(&format!("Failed to initialize BAML logger: {e:#}").into()),
        }
        
        // Register WASM-specific callbacks that will call JavaScript functions
        extern "C" fn result_callback(call_id: u32, is_done: i32, content: *const i8, length: usize) {
            let data = unsafe {
                std::slice::from_raw_parts(content as *const u8, length)
            };
            
            // Convert data to JavaScript array format
            let data_array = data.iter()
                .map(|b| b.to_string())
                .collect::<Vec<String>>()
                .join(",");
            
            // Call JavaScript callback through global object
            let js_code = format!(
                "if (window.__baml_callbacks && window.__baml_callbacks[{}]) {{
                    const data = new Uint8Array([{}]);
                    window.__baml_callbacks[{}].onResult(data, {});
                    console.log('Callback {} invoked with {} bytes');
                }}",
                call_id,
                data_array,
                call_id,
                is_done != 0,
                call_id,
                length
            );
            web_sys::js_sys::eval(&js_code).ok();
        }
        
        extern "C" fn error_callback(call_id: u32, _is_done: i32, content: *const i8, length: usize) {
            let error_msg = unsafe {
                std::str::from_utf8_unchecked(std::slice::from_raw_parts(content as *const u8, length))
            };
            
            // Call JavaScript callback through global object
            let js_code = format!(
                "if (window.__baml_callbacks && window.__baml_callbacks[{}]) {{
                    window.__baml_callbacks[{}].onError('{}');
                    console.log('Error callback {} invoked: {}');
                }}",
                call_id,
                call_id,
                error_msg.replace('\'', "\\'").replace('"', "\\\"").replace('\n', "\\n"),
                call_id,
                error_msg.replace('\'', "\\'").replace('"', "\\\"").replace('\n', "\\n")
            );
            web_sys::js_sys::eval(&js_code).ok();
        }
        
        extern "C" fn on_tick_callback(call_id: u32) {
            // Call JavaScript callback through global object
            let js_code = format!(
                "if (window.__baml_callbacks && window.__baml_callbacks[{}] && window.__baml_callbacks[{}].onTick) {{
                    window.__baml_callbacks[{}].onTick();
                    console.log('Tick callback {} invoked');
                }}",
                call_id,
                call_id,
                call_id,
                call_id
            );
            web_sys::js_sys::eval(&js_code).ok();
        }
        
        register_callbacks(result_callback, error_callback, on_tick_callback);
    }
    
    #[wasm_bindgen]
    pub fn create_baml_runtime_wasm(
        root_path: String,
        src_files: String,  // JSON string
        env_vars: String    // JSON string
    ) -> usize {
        // Parse JSON strings
        let src_files = CString::new(src_files).unwrap();
        let env_vars = CString::new(env_vars).unwrap();
        let root_path = CString::new(root_path).unwrap();
        
        let runtime = create_baml_runtime(
            root_path.as_ptr(),
            src_files.as_ptr(),
            env_vars.as_ptr()
        );
        
        runtime as usize
    }
    
    #[wasm_bindgen]
    pub fn destroy_baml_runtime_wasm(runtime: usize) {
        destroy_baml_runtime(runtime as *const std::ffi::c_void);
    }
    
    #[wasm_bindgen]
    pub fn call_function_wasm(
        runtime: usize,
        function_name: String,
        args_proto: Vec<u8>,
        callback_id: u32
    ) -> Result<(), JsValue> {
        let function_name = CString::new(function_name)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let args_ptr = args_proto.as_ptr() as *const i8;
        let args_len = args_proto.len();
        
        call_function_from_c(
            runtime as *const std::ffi::c_void,
            function_name.as_ptr(),
            args_ptr,
            args_len,
            callback_id
        );
        
        Ok(())
    }
    
    #[wasm_bindgen]
    pub fn call_function_stream_wasm(
        runtime: usize,
        function_name: String,
        args_proto: Vec<u8>,
        stream_id: String
    ) -> Result<(), JsValue> {
        let function_name = CString::new(function_name)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let args_ptr = args_proto.as_ptr() as *const i8;
        let args_len = args_proto.len();
        
        // Use a deterministic callback ID based on stream_id
        let callback_id = stream_id.chars().fold(0u32, |acc, c| acc.wrapping_add(c as u32));
        
        call_function_stream_from_c(
            runtime as *const std::ffi::c_void,
            function_name.as_ptr(),
            args_ptr,
            args_len,
            callback_id
        );
        
        Ok(())
    }
}
