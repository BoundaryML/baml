//! Object operations FFI entry points.

use prost::Message;

use crate::{
    Buffer,
    baml::cffi::{InvocationResponse, invocation_response::Response as CResponse},
};

/// Encode a success response.
#[allow(dead_code)] // Reserved for future object operations
fn encode_success_response() -> Buffer {
    let msg = InvocationResponse { response: None };
    Buffer::from(msg.encode_to_vec())
}

/// Encode an error response.
fn encode_error_response(error: &str) -> Buffer {
    let msg = InvocationResponse {
        response: Some(CResponse::Error(error.to_string())),
    };
    Buffer::from(msg.encode_to_vec())
}

/// Construct an object (e.g., TypeBuilder, Collector).
/// Currently returns error as these are not supported.
#[unsafe(no_mangle)]
pub extern "C" fn call_object_constructor(
    _encoded_args: *const libc::c_char,
    _length: usize,
) -> Buffer {
    // TODO: Implement when bex_engine supports object construction
    encode_error_response("Object construction not implemented in bridge_cffi")
}

/// Call a method on an object.
/// Currently returns error as these are not supported.
#[unsafe(no_mangle)]
pub extern "C" fn call_object_method(
    _handle: *const libc::c_void,
    _encoded_args: *const libc::c_char,
    _length: usize,
) -> Buffer {
    // TODO: Implement when bex_engine supports object methods
    encode_error_response("Object methods not implemented in bridge_cffi")
}

/// Free a buffer returned by FFI functions.
#[unsafe(no_mangle)]
pub extern "C" fn free_buffer(buf: Buffer) {
    if !buf.ptr.is_null() {
        unsafe {
            // Buffer was created from boxed slice, so len == cap
            let _ = Vec::from_raw_parts(buf.ptr as *mut u8, buf.len, buf.len);
        }
    }
}
