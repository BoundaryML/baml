/// cbindgen:ignore
mod ctypes;

mod raw_ptr_wrapper;
use std::{collections::HashMap, ffi::CStr, ops::Deref, ptr::null, sync::Arc};

use anyhow::Result;
use baml_runtime::{tracingv2::storage::storage::Collector, BamlRuntime, FunctionResult};
use once_cell::sync::{Lazy, OnceCell};

use crate::{
    ctypes::EncodeToBuffer,
    raw_ptr_wrapper::{SSEResponseWrapper, StreamTimingWrapper},
};

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

use baml_types::BamlValue;

use crate::{
    ctypes::{BamlFunctionArguments, DecodeFromBuffer},
    raw_ptr_wrapper::{
        CollectorWrapper, FunctionLogWrapper, HTTPBodyWrapper, HTTPRequestWrapper,
        HTTPResponseWrapper, LLMCallKindWrapper, TimingWrapper, UsageWrapper,
    },
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
        eprintln!("Error setting up BAML_LOG logging: {e}");
    }
    let env = env_logger::Env::new().filter("BAML_INTERNAL_LOG");
    let log_setup = env_logger::try_init_from_env(env);
    if let Err(e) = log_setup {
        eprintln!("Error setting up BAML_INTERNAL_LOG logging: {e}");
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
                    let meta = content.0.map_meta(|f| {
                        // Top level types in streaming always have `not_null` set to true.
                        let mut result_type = f.3.clone();
                        result_type.meta_mut().streaming_behavior.needed = true;
                        ctypes::EncodeMeta {
                            field_type: result_type.to_streaming_type(runtime.inner.ir.as_ref()),
                            checks: &f.1,
                        }
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
    encoded_args: *const libc::c_char,
    length: usize,
) -> *const libc::c_void {
    match call_collector_function_inner(object, object_type, function_name, encoded_args, length) {
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
    encoded_args: *const libc::c_char,
    length: usize,
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

    // Parse kwargs if provided
    let kwargs = if !encoded_args.is_null() && length > 0 {
        match BamlFunctionArguments::from_c_buffer(encoded_args, length) {
            Ok(args) => Some(args.kwargs),
            Err(_) => None,
        }
    } else {
        None
    };

    baml_log::trace!("{}::{}({:?}", object_type, function_name, kwargs);

    if object.is_null() {
        return match (object_type.as_str(), function_name.as_str()) {
            ("collector", "new") => {
                let name = kwargs
                    .as_ref()
                    .and_then(|kw| kw.get("name"))
                    .and_then(|v| v.as_str().map(|s| s.to_string()));
                let collector = Collector::new(name);
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
                "name" => {
                    let name = collector.name();
                    let c_string = CString::new(name).unwrap();
                    println!("cffi name: {c_string:?} {name:?}", name = collector.name());
                    Ok(c_string.into_raw() as *const libc::c_void)
                }
                "logs_count" => {
                    let logs = collector.function_logs();
                    Ok(logs.len() as *const libc::c_void)
                }
                "log_at" => {
                    let index = kwargs
                        .as_ref()
                        .and_then(|kw| kw.get("index"))
                        .and_then(|v| v.as_int())
                        .ok_or_else(|| anyhow::anyhow!("log_at requires index parameter"))?;

                    let logs = collector.function_logs();
                    if index >= 0 && (index as usize) < logs.len() {
                        Ok(FunctionLogWrapper::from_object(
                            logs.into_iter().nth(index as usize).unwrap(),
                        )
                        .send())
                    } else {
                        Ok(null())
                    }
                }
                "last" => match collector.last_function_log() {
                    Some(log) => Ok(FunctionLogWrapper::from_object(log).send()),
                    None => Ok(null()),
                },
                "clear" => {
                    // For now, we'll implement a simple clear by creating a new collector
                    // In a real implementation, we'd need to untrack all function IDs
                    // Since there's no built-in clear, we'll just return success
                    // The actual clearing would need to be implemented in the storage layer
                    Ok(null())
                }
                "id" => {
                    let _function_id = kwargs
                        .as_ref()
                        .and_then(|kw| kw.get("function_id"))
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                        .ok_or_else(|| {
                            anyhow::anyhow!("id lookup requires function_id parameter")
                        })?;

                    // Parse the function_id string to FunctionCallId
                    // For now, we'll just return null as we need to handle FunctionCallId parsing
                    Ok(null())
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
        "function_log" => {
            let function_log = FunctionLogWrapper::from_raw(object, true);
            match function_name.as_str() {
                "destroy" => {
                    function_log.destroy();
                    Ok(null())
                }
                "id" => {
                    let id = function_log.id().to_string();
                    let c_string = CString::new(id).unwrap();
                    Ok(c_string.into_raw() as *const libc::c_void)
                }
                "function_name" => {
                    // Create a mutable clone from the Arc
                    let mut log_clone = (function_log.as_ref()).clone();
                    let name = log_clone.function_name();
                    let c_string = CString::new(name).unwrap();
                    Ok(c_string.into_raw() as *const libc::c_void)
                }
                "log_type" => {
                    let mut log_clone = (function_log.as_ref()).clone();
                    let log_type = log_clone.log_type();
                    let c_string = CString::new(log_type).unwrap();
                    Ok(c_string.into_raw() as *const libc::c_void)
                }
                "timing" => {
                    let mut log_clone = (function_log.as_ref()).clone();
                    let timing = log_clone.timing();
                    Ok(TimingWrapper::from_object(timing).send())
                }
                "usage" => {
                    let mut log_clone = (function_log.as_ref()).clone();
                    let usage = log_clone.usage();
                    Ok(UsageWrapper::from_object(usage).send())
                }
                "raw_llm_response" => {
                    let mut log_clone = (function_log.as_ref()).clone();
                    match log_clone.raw_llm_response() {
                        Some(response) => {
                            let c_string = CString::new(response).unwrap();
                            Ok(c_string.into_raw() as *const libc::c_void)
                        }
                        None => Ok(null()),
                    }
                }
                "calls_count" => {
                    let mut log_clone = (function_log.as_ref()).clone();
                    let calls = log_clone.calls();
                    Ok(calls.len() as *const libc::c_void)
                }
                "metadata" => {
                    let mut log_clone = (function_log.as_ref()).clone();
                    let metadata = log_clone.metadata();
                    let json_str = serde_json::to_string(&metadata).unwrap_or_default();
                    let c_string = CString::new(json_str).unwrap();
                    Ok(c_string.into_raw() as *const libc::c_void)
                }
                "call_at" => {
                    let index = kwargs
                        .as_ref()
                        .and_then(|kw| kw.get("index"))
                        .and_then(|v| v.as_int())
                        .ok_or_else(|| anyhow::anyhow!("call_at requires index parameter"))?;

                    let mut log_clone = (function_log.as_ref()).clone();
                    let calls = log_clone.calls();
                    if index >= 0 && (index as usize) < calls.len() {
                        let call = calls.into_iter().nth(index as usize).unwrap();
                        Ok(LLMCallKindWrapper::from_object(call).send())
                    } else {
                        Ok(null())
                    }
                }
                "selected_call" => {
                    let mut log_clone = (function_log.as_ref()).clone();
                    let calls = log_clone.calls();

                    // Find the selected call (where selected = true)
                    for call in calls {
                        let selected = match &call {
                            baml_runtime::tracingv2::storage::storage::LLMCallKind::Basic(c) => {
                                c.selected
                            }
                            baml_runtime::tracingv2::storage::storage::LLMCallKind::Stream(c) => {
                                c.selected
                            }
                        };

                        if selected {
                            return Ok(LLMCallKindWrapper::from_object(call).send());
                        }
                    }

                    // No selected call found
                    Ok(null())
                }
                _ => Err(anyhow::anyhow!(
                    "Failed to call function: {} on object type: {}",
                    function_name,
                    object_type
                )),
            }
        }
        "timing" => {
            let timing = TimingWrapper::from_raw(object, true);
            match function_name.as_str() {
                "destroy" => {
                    timing.destroy();
                    Ok(null())
                }
                "start_time_utc_ms" => Ok(timing.start_time_utc_ms as *const libc::c_void),
                "duration_ms" => Ok(timing.duration_ms.unwrap_or_default() as *const libc::c_void),
                _ => Err(anyhow::anyhow!(
                    "Failed to call function: {} on object type: {}",
                    function_name,
                    object_type
                )),
            }
        }
        "llm_call" => {
            let llm_call = LLMCallKindWrapper::from_raw(object, true);
            match function_name.as_str() {
                "destroy" => {
                    llm_call.destroy();
                    Ok(null())
                }
                "client_name" => {
                    let client_name = match llm_call.deref().as_ref() {
                        baml_runtime::tracingv2::storage::storage::LLMCallKind::Basic(call) => {
                            &call.client_name
                        }
                        baml_runtime::tracingv2::storage::storage::LLMCallKind::Stream(call) => {
                            &call.client_name
                        }
                    };
                    let c_string = CString::new(client_name.clone()).unwrap();
                    Ok(c_string.into_raw() as *const libc::c_void)
                }
                "provider" => {
                    let provider = match llm_call.deref().as_ref() {
                        baml_runtime::tracingv2::storage::storage::LLMCallKind::Basic(call) => {
                            &call.provider
                        }
                        baml_runtime::tracingv2::storage::storage::LLMCallKind::Stream(_call) => {
                            &_call.provider
                        }
                    };
                    let c_string = CString::new(provider.clone()).unwrap();
                    Ok(c_string.into_raw() as *const libc::c_void)
                }
                "selected" => {
                    let selected = match llm_call.deref().as_ref() {
                        baml_runtime::tracingv2::storage::storage::LLMCallKind::Basic(call) => {
                            call.selected
                        }
                        baml_runtime::tracingv2::storage::storage::LLMCallKind::Stream(call) => {
                            call.selected
                        }
                    };
                    Ok(if selected { 1 } else { 0 } as *const libc::c_void)
                }
                "timing" => match llm_call.deref().as_ref() {
                    baml_runtime::tracingv2::storage::storage::LLMCallKind::Basic(call) => {
                        Ok(TimingWrapper::from_object(call.timing.clone()).send())
                    }
                    baml_runtime::tracingv2::storage::storage::LLMCallKind::Stream(call) => {
                        Ok(StreamTimingWrapper::from_object(call.timing.clone()).send())
                    }
                },
                "usage" => {
                    let usage = match llm_call.deref().as_ref() {
                        baml_runtime::tracingv2::storage::storage::LLMCallKind::Basic(call) => {
                            &call.usage
                        }
                        baml_runtime::tracingv2::storage::storage::LLMCallKind::Stream(call) => {
                            &call.usage
                        }
                    };
                    if let Some(usage) = usage {
                        Ok(UsageWrapper::from_object(usage.clone()).send())
                    } else {
                        Ok(null())
                    }
                }
                "http_request" => {
                    let request = match llm_call.deref().as_ref() {
                        baml_runtime::tracingv2::storage::storage::LLMCallKind::Basic(call) => {
                            call.request.clone()
                        }
                        baml_runtime::tracingv2::storage::storage::LLMCallKind::Stream(call) => {
                            call.request.clone()
                        }
                    };
                    if let Some(request) = request {
                        Ok(HTTPRequestWrapper::from_object(request.as_ref().clone()).send())
                    } else {
                        Ok(null())
                    }
                }
                "http_response" => {
                    let response = match llm_call.deref().as_ref() {
                        baml_runtime::tracingv2::storage::storage::LLMCallKind::Basic(call) => {
                            call.response.clone()
                        }
                        baml_runtime::tracingv2::storage::storage::LLMCallKind::Stream(call) => {
                            call.response.clone()
                        }
                    };
                    if let Some(response) = response {
                        Ok(HTTPResponseWrapper::from_object(response.as_ref().clone()).send())
                    } else {
                        Ok(null())
                    }
                }
                "sse_responses_count" => {
                    match llm_call.deref().as_ref() {
                        baml_runtime::tracingv2::storage::storage::LLMCallKind::Basic(_call) => {
                            // Basic calls don't have SSE responses
                            Ok(0 as *const libc::c_void)
                        }
                        baml_runtime::tracingv2::storage::storage::LLMCallKind::Stream(call) => {
                            if let Some(sse_chunks) = &call.sse_chunks {
                                Ok(sse_chunks.event.len() as *const libc::c_void)
                            } else {
                                Ok(0 as *const libc::c_void)
                            }
                        }
                    }
                }
                "sse_response_at" => {
                    let index = kwargs
                        .as_ref()
                        .and_then(|kw| kw.get("index"))
                        .and_then(|v| v.as_int())
                        .ok_or_else(|| {
                            anyhow::anyhow!("sse_response_at requires index parameter")
                        })?;

                    match llm_call.deref().as_ref() {
                        baml_runtime::tracingv2::storage::storage::LLMCallKind::Basic(_call) => {
                            // Basic calls don't have SSE responses
                            Ok(null())
                        }
                        baml_runtime::tracingv2::storage::storage::LLMCallKind::Stream(call) => {
                            if let Some(sse_chunks) = &call.sse_chunks {
                                if index >= 0 && (index as usize) < sse_chunks.event.len() {
                                    let sse_event = &sse_chunks.event[index as usize];
                                    Ok(SSEResponseWrapper::from_object(sse_event.as_ref().clone())
                                        .send())
                                } else {
                                    Ok(null())
                                }
                            } else {
                                Ok(null())
                            }
                        }
                    }
                }
                _ => Err(anyhow::anyhow!(
                    "Failed to call function: {} on object type: {}",
                    function_name,
                    object_type
                )),
            }
        }
        "http_request" => {
            let http_request = HTTPRequestWrapper::from_raw(object, true);
            match function_name.as_str() {
                "destroy" => {
                    http_request.destroy();
                    Ok(null())
                }
                "id" => {
                    let id = http_request.id().to_string();
                    let c_string = CString::new(id).unwrap();
                    Ok(c_string.into_raw() as *const libc::c_void)
                }
                "url" => {
                    let c_string = CString::new(http_request.url()).unwrap();
                    Ok(c_string.into_raw() as *const libc::c_void)
                }
                "method" => {
                    let c_string = CString::new(http_request.method()).unwrap();
                    Ok(c_string.into_raw() as *const libc::c_void)
                }
                "headers" => {
                    let headers = http_request.headers();
                    let json_str = serde_json::to_string(headers).unwrap_or_default();
                    let c_string = CString::new(json_str).unwrap();
                    Ok(c_string.into_raw() as *const libc::c_void)
                }
                "body" => Ok(HTTPBodyWrapper::from_object(http_request.body().clone()).send()),
                _ => Err(anyhow::anyhow!(
                    "Failed to call function: {} on object type: {}",
                    function_name,
                    object_type
                )),
            }
        }
        "http_response" => {
            let http_response = HTTPResponseWrapper::from_raw(object, true);
            match function_name.as_str() {
                "destroy" => {
                    http_response.destroy();
                    Ok(null())
                }
                "status" => Ok(http_response.status as *const libc::c_void),
                "headers" => {
                    if let Some(headers) = http_response.headers() {
                        let json_str = serde_json::to_string(headers).unwrap_or_default();
                        let c_string = CString::new(json_str).unwrap();
                        Ok(c_string.into_raw() as *const libc::c_void)
                    } else {
                        Ok(null())
                    }
                }
                "body" => Ok(HTTPBodyWrapper::from_object(http_response.body.clone()).send()),
                _ => Err(anyhow::anyhow!(
                    "Failed to call function: {} on object type: {}",
                    function_name,
                    object_type
                )),
            }
        }
        "http_body" => {
            let http_body = HTTPBodyWrapper::from_raw(object, true);
            match function_name.as_str() {
                "destroy" => {
                    http_body.destroy();
                    Ok(null())
                }
                "text" => match http_body.text() {
                    Ok(text) => {
                        let c_string = CString::new(text).unwrap();
                        Ok(c_string.into_raw() as *const libc::c_void)
                    }
                    Err(_) => Ok(null()),
                },
                "json" => match http_body.json() {
                    Ok(json_value) => {
                        let json_str = serde_json::to_string(&json_value).unwrap_or_default();
                        let c_string = CString::new(json_str).unwrap();
                        Ok(c_string.into_raw() as *const libc::c_void)
                    }
                    Err(_) => Ok(null()),
                },
                _ => Err(anyhow::anyhow!(
                    "Failed to call function: {} on object type: {}",
                    function_name,
                    object_type
                )),
            }
        }
        "sse_response" => {
            let sse_response = SSEResponseWrapper::from_raw(object, true);
            match function_name.as_str() {
                "destroy" => {
                    sse_response.destroy();
                    Ok(null())
                }
                "text" => {
                    let c_string = CString::new(sse_response.data.clone()).unwrap();
                    Ok(c_string.into_raw() as *const libc::c_void)
                }
                "json" => match serde_json::from_str::<serde_json::Value>(&sse_response.data) {
                    Ok(json_value) => {
                        let c_string = CString::new(sse_response.data.clone()).unwrap();
                        Ok(c_string.into_raw() as *const libc::c_void)
                    }
                    Err(_) => Ok(null()),
                },
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
