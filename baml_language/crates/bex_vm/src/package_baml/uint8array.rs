use bex_vm_types::Value;

use super::{BamlClassUint8Array, PackageBamlImpl};
use crate::errors::VmError;

impl BamlClassUint8Array for PackageBamlImpl {
    #[allow(clippy::cast_possible_wrap)]
    fn length(uint8array: &[u8]) -> i64 {
        uint8array.len() as i64
    }

    fn at(uint8array: &[u8], index: i64) -> Option<i64> {
        let Ok(index) = usize::try_from(index) else {
            return None;
        };
        uint8array.get(index).map(|&b| i64::from(b))
    }

    #[allow(clippy::cast_possible_wrap)]
    fn push(uint8array: &mut Vec<u8>, item: i64) -> i64 {
        // Clamp to u8 range per JS TypedArray behavior.
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let byte = (item & 0xFF) as u8;
        uint8array.push(byte);
        uint8array.len() as i64
    }

    fn pop(uint8array: &mut Vec<u8>) -> Option<i64> {
        uint8array.pop().map(i64::from)
    }

    fn concat(uint8array: &[u8], other: &[u8]) -> Vec<u8> {
        uint8array.iter().chain(other).copied().collect()
    }

    fn includes(uint8array: &[u8], item: i64) -> bool {
        let Ok(item) = u8::try_from(item) else {
            return false;
        };
        uint8array.contains(&item)
    }

    fn reverse(uint8array: &[u8]) -> Vec<u8> {
        uint8array.iter().copied().rev().collect()
    }

    #[allow(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap
    )]
    fn slice(uint8array: &[u8], start: i64, end: i64) -> Vec<u8> {
        let len = uint8array.len() as i64;
        let start = start.max(0).min(len) as usize;
        let end = end.max(0).min(len) as usize;
        let end = end.max(start);
        uint8array[start..end].to_vec()
    }

    fn zeroes(size: i64) -> Result<Vec<u8>, VmError> {
        let size = usize::try_from(size).map_err(|_| {
            VmError::InternalError(format!("uint8array.zeroes: invalid size {size}"))
        })?;
        let mut v = Vec::new();
        v.try_reserve(size).map_err(|_| {
            VmError::InternalError(format!(
                "uint8array.zeroes: allocation of {size} bytes failed"
            ))
        })?;
        v.resize(size, 0u8);
        Ok(v)
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    fn from_array(array: &[Value]) -> Result<Vec<u8>, VmError> {
        let mut result = Vec::with_capacity(array.len());
        for (i, val) in array.iter().enumerate() {
            let Value::Int(n) = val else {
                return Err(VmError::InternalError(format!(
                    "uint8array.from_array: element at index {i} is not an integer"
                )));
            };
            let byte = u8::try_from(*n).map_err(|_| {
                VmError::InternalError(format!(
                    "uint8array.from_array: value {n} at index {i} is out of range 0..=255"
                ))
            })?;
            result.push(byte);
        }
        Ok(result)
    }

    fn to_array(uint8array: &[u8]) -> Vec<Value> {
        uint8array
            .iter()
            .map(|&b| Value::Int(i64::from(b)))
            .collect()
    }

    fn from_hex(hex: &str) -> Result<Vec<u8>, VmError> {
        #[inline]
        fn parse_hex_digit(c: u8) -> Option<u8> {
            match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'a'..=b'f' => Some(c - b'a' + 10),
                b'A'..=b'F' => Some(c - b'A' + 10),
                _ => None,
            }
        }
        let (chunks, &[]) = hex.as_bytes().as_chunks::<2>() else {
            return Err(VmError::InternalError(
                "uint8array.from_hex: hex string must have even length".to_string(),
            ));
        };
        chunks
            .iter()
            .enumerate()
            .map(|(i, &[hi, lo]): (usize, &[u8; 2])| {
                let hi = parse_hex_digit(hi).ok_or(VmError::InternalError(format!(
                    "uint8array.from_hex: invalid hex at position {}",
                    i * 2
                )))?;
                let lo = parse_hex_digit(lo).ok_or(VmError::InternalError(format!(
                    "uint8array.from_hex: invalid hex at position {}",
                    i * 2 + 1
                )))?;
                Ok(hi << 4 | lo)
            })
            .collect()
    }

    fn to_hex(uint8array: &[u8]) -> String {
        use std::fmt::Write;
        let mut s = String::with_capacity(uint8array.len() * 2);
        for &b in uint8array {
            let Ok(()) = write!(s, "{b:02x}") else {
                unreachable!("write!() to `String` should never fail");
            };
        }
        s
    }

    fn from_base64(base64_str: &str) -> Result<Vec<u8>, VmError> {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(base64_str)
            .map_err(|e| VmError::InternalError(format!("uint8array.from_base64: {e}")))
    }

    fn to_base64(uint8array: &[u8]) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(uint8array)
    }

    #[allow(clippy::unused_unit)]
    fn sort(uint8array: &mut Vec<u8>) -> () {
        uint8array.sort_unstable();
    }
}
