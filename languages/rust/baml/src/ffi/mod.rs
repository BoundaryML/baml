mod bindings;
pub mod callbacks;

pub(crate) use bindings::*;
use crate::proto::baml_cffi_v1::{invocation_response::Response, InvocationResponse};
use baml_sys::Buffer;
use prost::Message;

/// RAII guard that frees Buffer on drop
pub(crate) struct BufferGuard(pub Buffer);

impl Drop for BufferGuard {
    fn drop(&mut self) {
        // Safety: Buffer was returned from FFI and needs to be freed
        // We create a copy of the Buffer struct to pass to free_buffer
        #[allow(unsafe_code)]
        let buf_copy = Buffer {
            ptr: self.0.ptr,
            len: self.0.len,
        };
        let _ = unsafe { baml_sys::free_buffer(buf_copy) };
    }
}

/// Decode Buffer containing InvocationResponse, return Ok(()) on success or error message.
///
/// The buffer is automatically freed when this function returns.
pub(crate) fn decode_async_response(buf: Buffer) -> Result<(), String> {
    let _guard = BufferGuard(buf);

    // Empty buffer = success
    if _guard.0.ptr.is_null() || _guard.0.len == 0 {
        return Ok(());
    }

    // Safety: Buffer pointer and length are valid as returned from FFI
    #[allow(unsafe_code)]
    let bytes = unsafe { std::slice::from_raw_parts(_guard.0.ptr as *const u8, _guard.0.len) };

    let response =
        InvocationResponse::decode(bytes).map_err(|e| format!("failed to decode response: {e}"))?;

    match response.response {
        Some(Response::Error(msg)) => Err(msg),
        _ => Ok(()),
    }
}
