mod ctypes;

use std::{collections::HashMap, ffi::CStr, path::Path};

extern crate baml_runtime;
use anyhow::Result;
use baml_runtime::{BamlRuntime, FunctionResult, FunctionResultStream};

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
pub extern "C" fn invoke_runtime_cli(args: *const *const libc::c_char) {
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
    baml_cli::run_cli(
        args_vec,
        baml_runtime::RuntimeCliDefaults {
            output_type: baml_types::GeneratorOutputType::PythonPydantic,
        },
    )
    .unwrap();
}

use std::ffi::CString;
use std::os::raw::c_char;

use baml_types::{BamlMap, BamlValue};

/// Convert CKwargs to a BamlMap<String, BamlValue>
fn ckwargs_to_map(kwargs: *const libc::c_char) -> Result<BamlMap<String, BamlValue>> {
    let kwargs_str = unsafe { CStr::from_ptr(kwargs) };
    let kwargs_map = serde_json::from_str::<BamlMap<String, BamlValue>>(kwargs_str.to_str()?)?;
    Ok(kwargs_map)
}

static mut RESULT_CALLBACK_FN: Option<
    extern "C" fn(call_id: u32, is_done: bool, content: *const i8, length: usize),
> = None;
static mut ERROR_CALLBACK_FN: Option<
    extern "C" fn(call_id: u32, name: *const c_char, message: *const i8, length: usize),
> = None;

#[no_mangle]
extern "C" fn register_callbacks(
    callback_fn: *const libc::c_void,
    error_callback_fn: *const libc::c_void,
) {
    // Create a global runtime or pass it along as needed.
    let _rt = tokio::runtime::Runtime::new().unwrap();
    // Store _rt somewhere accessible if needed.
    unsafe {
        RUNTIME = Some(std::sync::Arc::new(_rt));
        RESULT_CALLBACK_FN = Some(std::mem::transmute(callback_fn));
        ERROR_CALLBACK_FN = Some(std::mem::transmute(error_callback_fn));
    }
}

fn safe_trigger_callback(id: u32, is_done: bool, result: Result<FunctionResult>) {
    let callback_fn = unsafe { RESULT_CALLBACK_FN.unwrap() };
    // let error_callback_fn = unsafe { ERROR_CALLBACK_FN.unwrap() };

    match result {
        Ok(result) => match result.parsed() {
            Some(Ok(content)) => {
                let mut builder = flatbuffers::FlatBufferBuilder::new();
                let content = ctypes::serialize_baml_value_with_meta(&content.0, &mut builder);
                callback_fn(
                    id,
                    is_done,
                    content.as_ptr() as *const libc::c_char,
                    content.len(),
                );
            }
            Some(Err(e)) => {
                println!("Error: {}", e);
                // let c_message = CString::new(e.to_string()).unwrap();
                // error_callback_fn(id, c_message.as_ptr() as *const libc::c_char);
            }
            None => {
                println!("No result");
                // error_callback_fn(id, c_message.as_ptr() as *const libc::c_char);
            }
        },
        Err(e) => {
            println!("Error: {}", e);
            // let c_message = CString::new(e.to_string()).unwrap();
            // error_callback_fn(id, c_message.as_ptr() as *const libc::c_char);
        }
    }
}

static mut RUNTIME: Option<std::sync::Arc<tokio::runtime::Runtime>> = None;

/// Extern "C" function that returns immediately, scheduling the async call.
/// Once the asynchronous function completes, the provided callback is invoked.
#[no_mangle]
pub extern "C" fn call_function_from_c(
    runtime: *const libc::c_void,
    function_name: *const c_char,
    kwargs: *const libc::c_char,
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
    let keyword_args = ckwargs_to_map(kwargs)?;

    let ctx = runtime.create_ctx_manager(BamlValue::String("cffi".to_string()), None);

    // Spawn an async task to await the future and call the callback when done.
    // Ensure that a Tokio runtime is running in your application.
    let rt = unsafe { RUNTIME.as_ref().unwrap() };
    rt.spawn(async move {
        let (result, _) = runtime
            .call_function(func_name, &keyword_args, &ctx, None, None)
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
    kwargs: *const libc::c_char,
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
    let keyword_args = ckwargs_to_map(kwargs)?;

    let ctx = runtime.create_ctx_manager(BamlValue::String("cffi".to_string()), None);
    let mut stream = match runtime.stream_function(func_name, &keyword_args, &ctx, None, None) {
        Ok(stream) => stream,
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to stream function: {}", e));
        }
    };

    let ctx = runtime.create_ctx_manager(BamlValue::String("cffi".to_string()), None);

    let rt = unsafe { RUNTIME.as_ref().unwrap() };

    rt.spawn(async move {
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
