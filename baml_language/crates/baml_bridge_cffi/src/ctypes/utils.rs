//! Buffer encoding/decoding utilities.

use anyhow::Result;
use prost::Message;

/// Trait for decoding from a C buffer (protobuf bytes).
pub trait DecodeFromBuffer: Sized {
    fn from_c_buffer(buffer: *const u8, length: usize) -> Result<Self>;
}

/// Trait for encoding to a C buffer (protobuf bytes).
pub trait EncodeToBuffer {
    fn to_c_buffer(&self) -> Result<Vec<u8>>;
}

/// Generic implementation for prost Message types.
impl<T: Message + Default> DecodeFromBuffer for T {
    fn from_c_buffer(buffer: *const u8, length: usize) -> Result<Self> {
        if buffer.is_null() {
            anyhow::bail!("Null buffer pointer");
        }
        let slice = unsafe { std::slice::from_raw_parts(buffer, length) };
        T::decode(slice).map_err(|e| anyhow::anyhow!("Protobuf decode error: {}", e))
    }
}

impl<T: Message> EncodeToBuffer for T {
    fn to_c_buffer(&self) -> Result<Vec<u8>> {
        Ok(self.encode_to_vec())
    }
}
