//! Buffer encoding/decoding utilities.

use prost::Message;

use crate::error::CtypesError;

/// Trait for decoding from a C buffer (protobuf bytes).
#[allow(unsafe_code)]
pub trait DecodeFromBuffer: Sized {
    /// # Safety
    /// Caller must ensure `buffer` points to a valid, readable byte slice of at least `length` bytes.
    unsafe fn from_c_buffer(buffer: *const u8, length: usize) -> Result<Self, CtypesError>;
}

/// Generic implementation for prost Message types.
#[allow(unsafe_code)]
impl<T: Message + Default> DecodeFromBuffer for T {
    unsafe fn from_c_buffer(buffer: *const u8, length: usize) -> Result<Self, CtypesError> {
        if buffer.is_null() {
            return Err(CtypesError::NullBuffer);
        }
        let slice = unsafe { std::slice::from_raw_parts(buffer, length) };
        T::decode(slice).map_err(CtypesError::from)
    }
}
