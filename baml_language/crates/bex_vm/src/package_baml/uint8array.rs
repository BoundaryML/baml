use super::{BamlClassUint8Array, PackageBamlImpl};
use crate::errors::{RuntimeError, VmError};

impl BamlClassUint8Array for PackageBamlImpl {
    #[allow(clippy::cast_possible_wrap)]
    fn length(uint8array: &[u8]) -> i64 {
        uint8array.len() as i64
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    fn at(uint8array: &[u8], index: i64) -> Option<i64> {
        if index < 0 {
            return None;
        }
        uint8array.get(index as usize).map(|&b| i64::from(b))
    }

    fn zeroes(size: i64) -> Result<Vec<u8>, VmError> {
        let size = usize::try_from(size).map_err(|_| {
            VmError::from(RuntimeError::Other(format!(
                "uint8array.zeroes: invalid size {size}"
            )))
        })?;
        let mut v = Vec::new();
        v.try_reserve(size).map_err(|_| {
            VmError::from(RuntimeError::Other(format!(
                "uint8array.zeroes: allocation of {size} bytes failed"
            )))
        })?;
        v.resize(size, 0u8);
        Ok(v)
    }
}
