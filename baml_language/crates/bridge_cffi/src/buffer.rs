//! `Buffer` type for returning owned byte data across the FFI boundary.

/// Buffer type for returning data across FFI boundary.
/// Caller must free with `free_buffer()`.
///
/// This matches the Buffer struct expected by baml-sys.
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
