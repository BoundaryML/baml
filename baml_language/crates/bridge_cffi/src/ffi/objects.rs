//! Object operations FFI entry points.

use crate::Buffer;

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
