/// cbindgen:ignore
mod ctypes;

use std::{collections::HashMap, ffi::CStr, ptr::null, sync::Arc};

use anyhow::Result;
use baml_runtime::{BamlRuntime, FunctionResult};
use once_cell::sync::{Lazy, OnceCell};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[no_mangle]
pub extern "C" fn version() -> *const libc::c_char {
    let version = CString::new(VERSION).unwrap();
    version.into_raw() as *const libc::c_char
}

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

use std::ffi::CString;
use std::os::raw::c_char;

use baml_types::{BamlMap, BamlValue};

/// Convert Flatbuffers encoded arguments to a BamlMap<String, BamlValue>
fn ckwargs_to_map(
    encoded_args: *const libc::c_char,
    length: usize,
) -> Result<BamlMap<String, BamlValue>> {
    let buffer = unsafe { std::slice::from_raw_parts(encoded_args as *const u8, length) };
    let value = ctypes::buffer_to_cffi_value_holder(buffer)?;
    if let Some(map) = value.as_map_owned() {
        Ok(map)
    } else {
        Err(anyhow::anyhow!("Invalid encoded arguments"))
    }
}

pub type CallbackFn = extern "C" fn(call_id: u32, is_done: bool, content: *const i8, length: usize);

/// cbindgen:ignore
static RESULT_CALLBACK_FN: OnceCell<CallbackFn> = OnceCell::new();

/// cbindgen:ignore
static ERROR_CALLBACK_FN: OnceCell<CallbackFn> = OnceCell::new();

#[no_mangle]
extern "C" fn register_callbacks(callback_fn: CallbackFn, error_callback_fn: CallbackFn) {
    println!("Registering callbacks");
    let _ = baml_log::init();
    let _ = env_logger::try_init_from_env(env_logger::Env::new().filter("BAML_INTERNAL_LOG"));

    // Create a global runtime or pass it along as needed.
    unsafe {
        let _ = RESULT_CALLBACK_FN.set(std::mem::transmute(callback_fn));
        let _ = ERROR_CALLBACK_FN.set(std::mem::transmute(error_callback_fn));
    }
}

fn safe_trigger_callback(id: u32, is_done: bool, result: Result<FunctionResult>) {
    let callback_fn = RESULT_CALLBACK_FN
        .get()
        .expect("expected callback function to be set. Did you call register_callbacks?");

    let error_callback_fn = ERROR_CALLBACK_FN
        .get()
        .expect("expected error callback function to be set. Did you call register_callbacks?");

    match result {
        Ok(result) => match result.parsed() {
            Some(Ok(content)) => {
                let mut builder = flatbuffers::FlatBufferBuilder::new();
                let content = ctypes::serialize_baml_value_with_meta(&content.0, &mut builder);
                callback_fn(id, is_done, content.as_ptr() as *const i8, content.len());
            }
            Some(Err(e)) => {
                // let c_message = CString::new(e.to_string()).unwrap();
                let message = e.to_string();
                error_callback_fn(
                    id,
                    true,
                    message.as_ptr() as *const libc::c_char,
                    message.len(),
                );
            }
            None => {
                let message = "No result from baml".to_string();
                error_callback_fn(
                    id,
                    true,
                    message.as_ptr() as *const libc::c_char,
                    message.len(),
                );
            }
        },
        Err(e) => {
            let message = format!("Error: {}", e);
            error_callback_fn(
                id,
                true,
                message.as_ptr() as *const libc::c_char,
                message.len(),
            );
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
    let keyword_args = ckwargs_to_map(encoded_args, length)?;

    let ctx = runtime.create_ctx_manager(BamlValue::String("cffi".to_string()), None);

    // Spawn an async task to await the future and call the callback when done.
    // Ensure that a Tokio runtime is running in your application.
    let rt = RUNTIME.clone();
    rt.spawn(async move {
        let (result, _) = runtime
            .call_function(func_name, &keyword_args, &ctx, None, None, None)
            .await;
        safe_trigger_callback(id, true, result);
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
    let keyword_args = ckwargs_to_map(encoded_args, length)?;

    let ctx = runtime.create_ctx_manager(BamlValue::String("cffi".to_string()), None);
    let mut stream = match runtime.stream_function(func_name, &keyword_args, &ctx, None, None, None)
    {
        Ok(stream) => stream,
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to stream function: {}", e));
        }
    };

    let ctx = runtime.create_ctx_manager(BamlValue::String("cffi".to_string()), None);

    RUNTIME.spawn(async move {
        let (result, _) = stream
            .run(Some(|r| on_event(id, r)), &ctx, None, None)
            .await;
        safe_trigger_callback(id, false, result);
    });

    Ok(())
}

fn on_event(id: u32, result: FunctionResult) {
    safe_trigger_callback(id, true, Ok(result));
}
