//! Buffer encoding/decoding utilities.

use prost::Message;

use crate::error::CtypesError;

/// Trait for decoding from a C buffer (protobuf bytes).
pub trait DecodeFromBuffer: Sized {
    fn from_c_buffer(buffer: *const u8, length: usize) -> Result<Self, CtypesError>;
}

/// Generic implementation for prost Message types.
#[allow(unsafe_code, clippy::not_unsafe_ptr_arg_deref)]
impl<T: Message + Default> DecodeFromBuffer for T {
    fn from_c_buffer(buffer: *const u8, length: usize) -> Result<Self, CtypesError> {
        if buffer.is_null() {
            return Err(CtypesError::NullBuffer);
        }
        // SAFETY: Caller must ensure (buffer, length) is a valid read; used for C FFI buffers.
        let slice = unsafe { std::slice::from_raw_parts(buffer, length) };
        T::decode(slice).map_err(CtypesError::from)
    }
}
