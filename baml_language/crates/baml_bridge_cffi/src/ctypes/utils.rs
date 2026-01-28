//! Buffer encoding/decoding utilities.

use prost::Message;

use crate::error::BridgeError;

/// Trait for decoding from a C buffer (protobuf bytes).
pub trait DecodeFromBuffer: Sized {
    fn from_c_buffer(buffer: *const u8, length: usize) -> Result<Self, BridgeError>;
}

/// Generic implementation for prost Message types.
impl<T: Message + Default> DecodeFromBuffer for T {
    fn from_c_buffer(buffer: *const u8, length: usize) -> Result<Self, BridgeError> {
        if buffer.is_null() {
            return Err(BridgeError::NullBuffer);
        }
        let slice = unsafe { std::slice::from_raw_parts(buffer, length) };
        T::decode(slice).map_err(BridgeError::from)
    }
}
