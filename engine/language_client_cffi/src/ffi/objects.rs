use std::{
    fs::OpenOptions,
    io::Write,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use libc::size_t;

use super::*;
use crate::{
    ctypes::{
        object_args_decode::{BamlMethodArguments, BamlObjectConstructorArgs},
        object_response_encode::{BamlObjectResponse, BamlObjectResponseWrapper},
        EncodeToBuffer,
    },
    raw_ptr_wrapper::{CallMethod, RawPtrType},
};

// Buffer tracking logging (uses same BAML_FFI_LOG env var as raw_ptr_wrapper)
fn ffi_log_file() -> Option<&'static str> {
    static FILE: OnceLock<Option<String>> = OnceLock::new();
    FILE.get_or_init(|| std::env::var("BAML_FFI_LOG").ok())
        .as_deref()
}

fn ffi_log_mutex() -> &'static Mutex<()> {
    static MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
    MUTEX.get_or_init(|| Mutex::new(()))
}

fn timestamp_micros() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros())
        .unwrap_or(0)
}

fn write_ffi_log(msg: &str) {
    if let Some(path) = ffi_log_file() {
        let _guard = ffi_log_mutex().lock().unwrap();
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{}", msg);
        }
    }
}

macro_rules! ffi_log {
    ($($arg:tt)*) => {
        if ffi_log_file().is_some() {
            let ts = timestamp_micros();
            let msg = format!($($arg)*);
            let msg = if msg.starts_with('[') {
                let bracket_end = msg.find(']').unwrap_or(0);
                format!("{} ts={}{}", &msg[..bracket_end], ts, &msg[bracket_end..])
            } else {
                format!("ts={} {}", ts, msg)
            };
            write_ffi_log(&msg);
        }
    };
}

struct BasicLookup;
impl baml_types::baml_value::TypeLookups for BasicLookup {
    fn expand_recursive_type(&self, _: &str) -> anyhow::Result<&baml_types::TypeIR> {
        anyhow::bail!("Not implemented");
    }
}

#[repr(C)]
pub struct Buffer {
    ptr: *const i8,
    len: size_t,
}

impl Buffer {
    pub fn from(buf: Vec<u8>) -> Self {
        let ptr = buf.as_ptr() as *const i8;
        let len = buf.len();
        std::mem::forget(buf); // Prevent Rust from freeing the buffer
        ffi_log!("[FFI_BUF_ALLOC] ptr={:#x} len={}", ptr as usize, len);
        Buffer { ptr, len }
    }
}

#[no_mangle]
pub extern "C" fn call_object_constructor(
    encoded_args: *const libc::c_char,
    length: usize,
) -> Buffer {
    let result = call_object_constructor_impl(encoded_args, length);

    let buf_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        BamlObjectResponseWrapper(result)
            .encode_to_c_buffer(&BasicLookup, baml_types::StreamingMode::NonStreaming)
    }));

    let buf = match buf_result {
        Ok(buf) => buf,
        Err(panic_info) => {
            let error_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                format!("Object constructor encoding panicked: {s}")
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                format!("Object constructor encoding panicked: {s}")
            } else {
                "Object constructor encoding panicked with unknown error".to_string()
            };

            eprintln!("Error: {error_msg}");
            // Return a simple error message as bytes without going through encode_to_c_buffer
            error_msg.into_bytes()
        }
    };

    Buffer::from(buf)
}

fn call_object_constructor_impl(
    encoded_args: *const libc::c_char,
    length: usize,
) -> BamlObjectResponse {
    let BamlObjectConstructorArgs {
        object_type,
        kwargs,
    } = match BamlObjectConstructorArgs::from_c_buffer(encoded_args, length) {
        Ok(args) => args,
        Err(e) => {
            return Err(format!("Failed to parse arguments: {e}"));
        }
    };
    baml_log::trace!("{}::new({:?})", object_type.as_str_name(), kwargs);
    RawPtrType::new_from(object_type, &kwargs)
}

#[no_mangle]
pub extern "C" fn free_buffer(buf: Buffer) {
    ffi_log!("[FFI_BUF_FREE] ptr={:#x} len={}", buf.ptr as usize, buf.len);
    // Rebuild the Vec so Rust can drop it safely
    unsafe { Vec::from_raw_parts(buf.ptr as *mut u8, buf.len, buf.len) };
}

#[no_mangle]
pub extern "C" fn call_object_method(
    runtime: *const libc::c_void,
    encoded_args: *const libc::c_char,
    length: usize,
) -> Buffer {
    let runtime = unsafe { &*(runtime as *const baml_runtime::BamlRuntime) };

    let result = call_object_method_impl(runtime, encoded_args, length);

    let buf_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        BamlObjectResponseWrapper(result)
            .encode_to_c_buffer(&BasicLookup, baml_types::StreamingMode::NonStreaming)
    }));

    let buf = match buf_result {
        Ok(buf) => buf,
        Err(panic_info) => {
            let error_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                format!("Object method encoding panicked: {s}")
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                format!("Object method encoding panicked: {s}")
            } else {
                "Object method encoding panicked with unknown error".to_string()
            };

            eprintln!("Error: {error_msg}");
            // Return a simple error message as bytes without going through encode_to_c_buffer
            error_msg.into_bytes()
        }
    };

    Buffer::from(buf)
}

fn call_object_method_impl(
    runtime: &baml_runtime::BamlRuntime,
    encoded_args: *const libc::c_char,
    length: usize,
) -> BamlObjectResponse {
    let BamlMethodArguments {
        object,
        method_name,
        kwargs,
    } = match BamlMethodArguments::from_c_buffer(encoded_args, length) {
        Ok(args) => args,
        Err(e) => {
            return Err(format!("Failed to parse arguments: {e}"));
        }
    };
    baml_log::trace!("{}::{}({:?})", object.name(), method_name, kwargs);
    let result = object.call_method(runtime, method_name.as_str(), &kwargs);
    baml_log::trace!("-> {:?}", result);
    result
}
