//! `Buffer` type for returning owned byte data across the FFI boundary.

/// Owned byte buffer returned by the BAML runtime.
///
/// The bytes are not NUL-terminated. A zero length may have either a null or
/// non-null pointer. The receiver must pass the original pair exactly once to
/// the `free_buffer` function in the same API table that allocated it. The
/// pointer is invalid after release. Never use a host allocator or a different
/// loaded BAML library instance to release it.
#[repr(C)]
pub struct Buffer {
    pub ptr: *const i8,
    pub len: usize,
}

impl Buffer {
    pub fn from(data: Vec<u8>) -> Self {
        let data = data.into_boxed_slice();
        let ptr = data.as_ptr() as *const i8;
        let len = data.len();
        std::mem::forget(data);
        Buffer { ptr, len }
    }

    pub fn as_ptr(&self) -> *const i8 {
        self.ptr
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// Free a buffer returned by an exported bridge function.
#[unsafe(no_mangle)]
pub extern "C" fn free_buffer(buf: Buffer) {
    if !buf.ptr.is_null() {
        unsafe {
            // Buffer is created from a boxed slice, so length and capacity match.
            let _ = Vec::from_raw_parts(buf.ptr as *mut u8, buf.len, buf.len);
        }
    }
}
