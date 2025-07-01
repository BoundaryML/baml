/// cbindgen:ignore
mod ctypes;

mod raw_ptr_wrapper;
use std::{collections::HashMap, ffi::CStr, ops::Deref, ptr::null, sync::Arc};

use anyhow::Result;
use baml_runtime::{tracingv2::storage::storage::Collector, BamlRuntime, FunctionResult};
use once_cell::sync::{Lazy, OnceCell};

use crate::ctypes::EncodeToBuffer;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod baml {
    pub mod cffi {
        include!(concat!(env!("OUT_DIR"), "/baml.cffi.rs"));
    }
}

#[no_mangle]
pub extern "C" fn version() -> *const libc::c_char {
    let version = CString::new(VERSION).unwrap();
    version.into_raw() as *const libc::c_char
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn create_baml_runtime(
    root_path: *const libc::c_char,
    src_files_json: *const libc::c_char,
    env_vars_json: *const libc::c_char,
) -> *const libc::c_void {
    let src_files = serde_json::from_str::<HashMap<String, String>>(unsafe {
        CStr::from_ptr(src_files_json).to_str().unwrap()
    })
    .unwrap();
    let env_vars = serde_json::from_str::<HashMap<String, String>>(unsafe {
        CStr::from_ptr(env_vars_json).to_str().unwrap()
    })
    .unwrap();
    let runtime = BamlRuntime::from_file_content(
        unsafe { CStr::from_ptr(root_path).to_str().unwrap() },
        &src_files,
        env_vars,
    );
    Box::into_raw(Box::new(runtime)) as *const libc::c_void
}

#[no_mangle]
pub extern "C" fn destroy_baml_runtime(runtime: *const libc::c_void) {
    unsafe {
        let _ = Box::from_raw(runtime as *mut BamlRuntime);
    }
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn invoke_runtime_cli(args: *const *const libc::c_char) -> libc::c_int {
    // Safety: We assume `args` is a valid pointer to a null-terminated array of C strings.
    let args_vec = unsafe {
        // Ensure the pointer itself is not null.
        if args.is_null() {
            Vec::new()
        } else {
            let mut vec = Vec::new();
            let mut i = 0;
            // Iterate until a null pointer is encountered.
            while !(*args.add(i)).is_null() {
                let c_str = CStr::from_ptr(*args.add(i));
                // Convert to Rust String (lossy conversion handles non-UTF8 gracefully).
                vec.push(c_str.to_string_lossy().into_owned());
                i += 1;
            }
            vec
        }
    };
    match baml_cli::run_cli(
        args_vec,
        baml_runtime::RuntimeCliDefaults {
            output_type: baml_types::GeneratorOutputType::Go,
        },
    ) {
        Ok(exit_code) => exit_code.into(),
        Err(e) => {
            baml_log::error!("{}", e);
            1
        }
    }
}

use std::{ffi::CString, os::raw::c_char};

use baml_types::{BamlValue, HasType};

use crate::{
    ctypes::{BamlFunctionArguments, DecodeFromBuffer},
    raw_ptr_wrapper::{CollectorWrapper, UsageWrapper},
};

pub type CallbackFn = extern "C" fn(call_id: u32, is_done: i32, content: *const i8, length: usize);

/// cbindgen:ignore
static RESULT_CALLBACK_FN: OnceCell<CallbackFn> = OnceCell::new();

/// cbindgen:ignore
static ERROR_CALLBACK_FN: OnceCell<CallbackFn> = OnceCell::new();

#[no_mangle]
extern "C" fn register_callbacks(callback_fn: CallbackFn, error_callback_fn: CallbackFn) {
    let log_setup = baml_log::init();
    if let Err(e) = log_setup {
        eprintln!("Error setting up logging: {e}");
    }
    let env = env_logger::Env::new().filter("BAML_INTERNAL_LOG");
    let log_setup = env_logger::try_init_from_env(env);
    if let Err(e) = log_setup {
        eprintln!("Error setting up internal logging: {e}");
    } else {
        println!("Internal logging set up");
    }

    // Create a global runtime or pass it along as needed.
    let _ = RESULT_CALLBACK_FN.set(callback_fn);
    let _ = ERROR_CALLBACK_FN.set(error_callback_fn);
}

fn safe_trigger_callback(
    id: u32,
    is_done: bool,
    result: Result<FunctionResult>,
    runtime: &BamlRuntime,
) {
    let callback_fn = RESULT_CALLBACK_FN
        .get()
        .expect("expected callback function to be set. Did you call register_callbacks?");

    let error_callback_fn = ERROR_CALLBACK_FN
        .get()
        .expect("expected error callback function to be set. Did you call register_callbacks?");

    match result {
        Ok(result) => match result.parsed() {
            Some(Ok(content)) => {
                // Look here
                let buf = if is_done {
                    let meta = content.0.map_meta(|f| ctypes::EncodeMeta {
                        field_type: f.3.to_non_streaming_type(runtime.inner.ir.as_ref()),
                        checks: &f.1,
                    });

                    meta.encode_to_c_buffer(runtime.inner.ir.as_ref())
                } else {
                    let meta = content.0.map_meta(|f| ctypes::EncodeMeta {
                        field_type: f.3.to_streaming_type(runtime.inner.ir.as_ref()),
                        checks: &f.1,
                    });
                    meta.encode_to_c_buffer(runtime.inner.ir.as_ref())
                };

                let is_done_int = if is_done { 1 } else { 0 };
                callback_fn(id, is_done_int, buf.as_ptr() as *const i8, buf.len());
            }
            Some(Err(e)) => {
                let message = e.to_string();
                error_callback_fn(id, 1, message.as_ptr() as *const i8, message.len());
            }
            None => {
                let message = "No result from baml".to_string();
                error_callback_fn(id, 1, message.as_ptr() as *const i8, message.len());
            }
        },
        Err(e) => {
            let message = format!("Error: {e}");
            error_callback_fn(id, 1, message.as_ptr() as *const i8, message.len());
        }
    }
}

/// cbindgen:ignore
static RUNTIME: Lazy<Arc<tokio::runtime::Runtime>> =
    Lazy::new(|| Arc::new(tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime")));

/// Extern "C" function that returns immediately, scheduling the async call.
/// Once the asynchronous function completes, the provided callback is invoked.
#[no_mangle]
pub extern "C" fn call_function_from_c(
    runtime: *const libc::c_void,
    function_name: *const c_char,
    encoded_args: *const libc::c_char,
    length: usize,
    id: u32,
) -> *const libc::c_void {
    match call_function_from_c_inner(runtime, function_name, encoded_args, length, id) {
        Ok(_) => null(),
        Err(e) => {
            Box::into_raw(Box::new(CString::new(e.to_string()).unwrap())) as *const libc::c_void
        }
    }
}

fn call_function_from_c_inner(
    runtime: *const libc::c_void,
    function_name: *const c_char,
    encoded_args: *const libc::c_char,
    length: usize,
    id: u32,
) -> Result<()> {
    // Safety: assume that the pointers provided are valid.
    let runtime = unsafe { &*(runtime as *const BamlRuntime) };

    // Convert the function name.
    let func_name = match unsafe { CStr::from_ptr(function_name) }.to_str() {
        Ok(s) => s.to_owned(),
        Err(_) => {
            return Err(anyhow::anyhow!("Failed to convert function name to string"));
        }
    };

    // Convert keyword arguments.
    let ctypes::BamlFunctionArguments {
        kwargs,
        client_registry,
        env_vars,
        collectors,
    } = BamlFunctionArguments::from_c_buffer(encoded_args, length)?;

    let ctx = runtime.create_ctx_manager(BamlValue::String("cffi".to_string()), None);

    // Spawn an async task to await the future and call the callback when done.
    // Ensure that a Tokio runtime is running in your application.
    let rt = RUNTIME.clone();
    rt.spawn(async move {
        let (result, _) = runtime
            .call_function(
                func_name,
                &kwargs,
                &ctx,
                None,
                client_registry.as_ref(),
                collectors.map(|c| c.iter().map(|c| c.deref().clone()).collect()),
                env_vars,
            )
            .await;
        safe_trigger_callback(id, true, result, runtime);
    });

    Ok(())
}

/// Extern "C" function that returns immediately, scheduling the async call.
/// Once the asynchronous function completes, the provided callback is invoked.
#[no_mangle]
pub extern "C" fn call_function_stream_from_c(
    runtime: *const libc::c_void,
    function_name: *const c_char,
    encoded_args: *const libc::c_char,
    length: usize,
    id: u32,
) -> *const libc::c_void {
    match call_function_stream_from_c_inner(runtime, function_name, encoded_args, length, id) {
        Ok(_) => null(),
        Err(e) => {
            Box::into_raw(Box::new(CString::new(e.to_string()).unwrap())) as *const libc::c_void
        }
    }
}

fn call_function_stream_from_c_inner(
    runtime: *const libc::c_void,
    function_name: *const c_char,
    encoded_args: *const libc::c_char,
    length: usize,
    id: u32,
) -> Result<()> {
    // Safety: assume that the pointers provided are valid.
    let runtime = unsafe { &*(runtime as *const BamlRuntime) };

    // Convert the function name.
    let func_name = match unsafe { CStr::from_ptr(function_name) }.to_str() {
        Ok(s) => s.to_owned(),
        Err(_) => {
            return Err(anyhow::anyhow!("Failed to convert function name to string"));
        }
    };

    // Convert keyword arguments.
    let BamlFunctionArguments {
        kwargs,
        client_registry,
        env_vars,
        collectors,
    } = BamlFunctionArguments::from_c_buffer(encoded_args, length)?;

    let ctx = runtime.create_ctx_manager(BamlValue::String("cffi".to_string()), None);
    let mut stream = match runtime.stream_function(
        func_name,
        &kwargs,
        &ctx,
        None,
        client_registry.as_ref(),
        collectors.map(|c| c.iter().map(|c| c.deref().clone()).collect()),
        env_vars,
    ) {
        Ok(stream) => stream,
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to stream function: {}", e));
        }
    };

    let ctx = runtime.create_ctx_manager(BamlValue::String("cffi".to_string()), None);

    RUNTIME.spawn(async move {
        let (result, _) = stream
            .run(
                Some(|r| on_event(id, r, runtime)),
                &ctx,
                None,
                None,
                HashMap::new(),
            )
            .await;
        safe_trigger_callback(id, true, result, runtime);
    });

    Ok(())
}

fn on_event(id: u32, result: FunctionResult, runtime: &BamlRuntime) {
    safe_trigger_callback(id, false, Ok(result), runtime);
}

#[no_mangle]
pub extern "C" fn call_collector_function(
    object: *const libc::c_void,
    object_type: *const c_char,
    function_name: *const c_char,
) -> *const libc::c_void {
    match call_collector_function_inner(object, object_type, function_name) {
        Ok(result) => result,
        Err(e) => {
            Box::into_raw(Box::new(CString::new(e.to_string()).unwrap())) as *const libc::c_void
        }
    }
}

fn call_collector_function_inner(
    object: *const libc::c_void,
    object_type: *const c_char,
    function_name: *const c_char,
) -> Result<*const libc::c_void> {
    let object_type = match unsafe { CStr::from_ptr(object_type) }.to_str() {
        Ok(s) => s.to_owned(),
        Err(_) => {
            return Err(anyhow::anyhow!("Failed to convert object type to string"));
        }
    };

    let function_name = match unsafe { CStr::from_ptr(function_name) }.to_str() {
        Ok(s) => s.to_owned(),
        Err(_) => {
            return Err(anyhow::anyhow!("Failed to convert function name to string"));
        }
    };

    if object.is_null() {
        return match (object_type.as_str(), function_name.as_str()) {
            ("collector", "new") => {
                let collector = Collector::new(None);
                Ok(CollectorWrapper::from_object(collector).send())
            }
            _ => Err(anyhow::anyhow!(
                "Failed to call collector function: {}",
                function_name
            )),
        };
    }

    match object_type.as_str() {
        "collector" => {
            let collector = CollectorWrapper::from_raw(object, true);

            match function_name.as_str() {
                "destroy" => {
                    collector.destroy();
                    // collector goes out of scope here
                    Ok(null())
                }
                "usage" => {
                    let usage = collector.usage();
                    Ok(UsageWrapper::from_object(usage).send())
                }
                _ => Err(anyhow::anyhow!(
                    "Failed to call function: {} on object type: {}",
                    function_name,
                    object_type
                )),
            }
        }
        "usage" => {
            let usage = UsageWrapper::from_raw(object, true);
            match function_name.as_str() {
                "destroy" => {
                    usage.destroy();
                    Ok(null())
                }
                "input_tokens" => Ok(usage.input_tokens.unwrap_or_default() as *mut libc::c_void),
                "output_tokens" => Ok(usage.output_tokens.unwrap_or_default() as *mut libc::c_void),
                _ => Err(anyhow::anyhow!(
                    "Failed to call function: {} on object type: {}",
                    function_name,
                    object_type
                )),
            }
        }
        _ => Err(anyhow::anyhow!(
            "Failed to call function: {} on object type: {}",
            function_name,
            object_type
        )),
    }
}
