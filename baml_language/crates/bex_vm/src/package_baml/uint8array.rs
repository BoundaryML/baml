use bex_vm_types::Value;

use super::{BamlClassUint8Array, PackageBamlImpl, json::raise_serialize_no_path};
use crate::{
    BexVm, VmPanic,
    errors::{VmBamlError, VmRustFnError},
};

impl BamlClassUint8Array for PackageBamlImpl {
    fn to_json(vm: &mut BexVm, _uint8array: &[u8]) -> Result<Value, VmRustFnError> {
        // BEP §"non-representable": `uint8array` has no canonical JSON form.
        // Callers must explicitly encode to base64 or hex before serializing.
        Err(raise_serialize_no_path(
            vm,
            "uint8array requires explicit encoding (use to_base64() or to_hex())",
            "uint8array",
        ))
    }

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
        let start = start.clamp(0, len);
        let end = end.clamp(start, len) as usize;
        let start = start as usize;
        uint8array[start..end].to_vec()
    }

    fn zeroes(size: i64) -> Result<Vec<u8>, VmRustFnError> {
        let size = usize::try_from(size).map_err(|_| VmBamlError::InvalidArgument {
            message: format!("Invalid size {size} for uint8array"),
        })?;
        let mut v = Vec::new();
        v.try_reserve(size).map_err(|_| VmPanic::AllocFailure {
            message: format!("Allocation of {size} bytes for new uint8array failed"),
        })?;
        v.resize(size, 0u8);
        Ok(v)
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    fn from_array(array: &[Value]) -> Result<Vec<u8>, VmRustFnError> {
        let mut result = Vec::with_capacity(array.len());
        for (i, val) in array.iter().enumerate() {
            let Value::Int(n) = val else {
                return Err(VmBamlError::InvalidArgument {
                    message: format!("Element at index {i} is not an `int`"),
                }
                .into());
            };
            let byte = u8::try_from(*n).map_err(|_| VmBamlError::InvalidArgument {
                message: format!("Value {n} at index {i} is out of range 0..=255"),
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

    fn from_hex(hex: &str) -> Result<Vec<u8>, VmRustFnError> {
        #[inline]
        const fn parse_hex_digit(c: u8) -> Option<u8> {
            match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'a'..=b'f' | b'A'..=b'F' => Some((c & 0x1F) + 9),
                _ => None,
            }
        }
        let (chunks, &[]) = hex.as_bytes().as_chunks::<2>() else {
            return Err(VmBamlError::InvalidArgument {
                message: "hex string must have even length".to_string(),
            }
            .into());
        };
        chunks
            .iter()
            .enumerate()
            .map(|(i, &[hi, lo]): (usize, &[u8; 2])| {
                let hi = parse_hex_digit(hi).ok_or(VmBamlError::InvalidArgument {
                    message: format!(
                        "uint8array.from_hex: invalid hex digit at position {}",
                        i * 2
                    ),
                })?;
                let lo = parse_hex_digit(lo).ok_or(VmBamlError::InvalidArgument {
                    message: format!(
                        "uint8array.from_hex: invalid hex digit at position {}",
                        i * 2 + 1
                    ),
                })?;
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

    fn from_base64(base64_str: &str) -> Result<Vec<u8>, VmRustFnError> {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(base64_str)
            .map_err(|e| VmBamlError::InvalidArgument {
                message: format!("failed to decode base64: {e}"),
            })
            .map_err(VmRustFnError::BamlError)
    }

    fn to_base64(uint8array: &[u8]) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(uint8array)
    }

    fn to_string(uint8array: &[u8]) -> String {
        String::from_utf8_lossy(uint8array).into_owned()
    }

    #[allow(clippy::unused_unit)]
    fn sort(uint8array: &mut Vec<u8>) -> () {
        uint8array.sort_unstable();
    }
}
