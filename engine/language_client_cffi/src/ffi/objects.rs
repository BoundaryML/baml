use libc::size_t;

use super::*;
use crate::{
    ctypes::{
        object_args_decode::{BamlMethodArguments, BamlObjectConstructorArgs},
        object_response_encode::BamlObjectResponse,
        EncodeToBuffer,
    },
    raw_ptr_wrapper::{CallMethod, RawPtrType},
};

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
        result.encode_to_c_buffer(&BasicLookup, baml_types::StreamingMode::NonStreaming)
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
        result.encode_to_c_buffer(&BasicLookup, baml_types::StreamingMode::NonStreaming)
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
