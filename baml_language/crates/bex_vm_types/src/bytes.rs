//! Heap-neutral byte operations shared with the interpreter.
use crate::errors::{VmBamlError, VmPanic, VmRustFnError};
use baml_type::sequence::resolve_slice_bound;
fn resolve_index(index: i64, len: usize) -> Option<usize> {
    let len_i64 = i64::try_from(len).ok()?;
    usize::try_from(if index < 0 { index + len_i64 } else { index })
        .ok()
        .filter(|&i| i < len)
}

#[allow(clippy::cast_possible_wrap)]
pub fn length(uint8array: &[u8]) -> i64 {
    uint8array.len() as i64
}

pub fn at(uint8array: &[u8], index: i64) -> Option<i64> {
    resolve_index(index, uint8array.len()).map(|i| i64::from(uint8array[i]))
}

#[allow(clippy::cast_possible_wrap)]
pub fn push(uint8array: &mut Vec<u8>, item: i64) -> i64 {
    // Clamp to u8 range per JS TypedArray behavior.
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let byte = (item & 0xFF) as u8;
    uint8array.push(byte);
    uint8array.len() as i64
}

pub fn pop(uint8array: &mut Vec<u8>) -> Option<i64> {
    uint8array.pop().map(i64::from)
}

pub fn concat(uint8array: &[u8], other: &[u8]) -> Vec<u8> {
    uint8array.iter().chain(other).copied().collect()
}

pub fn includes(uint8array: &[u8], item: i64) -> bool {
    let Ok(item) = u8::try_from(item) else {
        return false;
    };
    uint8array.contains(&item)
}

#[allow(clippy::cast_possible_wrap)]
pub fn index_of(uint8array: &[u8], item: i64) -> Option<i64> {
    let item = u8::try_from(item).ok()?;
    uint8array
        .iter()
        .position(|byte| *byte == item)
        .map(|index| index as i64)
}

pub fn reverse(uint8array: &[u8]) -> Vec<u8> {
    uint8array.iter().copied().rev().collect()
}

pub fn slice(uint8array: &[u8], start: i64, end: i64) -> Vec<u8> {
    let start = resolve_slice_bound(start, uint8array.len());
    // An `end` resolving before `start` yields an empty slice.
    let end = resolve_slice_bound(end, uint8array.len()).max(start);
    uint8array[start..end].to_vec()
}

pub fn zeroes(size: i64) -> Result<Vec<u8>, VmRustFnError> {
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
pub fn from_array(array: &[i64]) -> Result<Vec<u8>, VmRustFnError> {
    let mut result = Vec::with_capacity(array.len());
    for (i, val) in array.iter().enumerate() {
        let n = *val;
        let byte = u8::try_from(n).map_err(|_| VmBamlError::InvalidArgument {
            message: format!("Value {n} at index {i} is out of range 0..=255"),
        })?;
        result.push(byte);
    }
    Ok(result)
}

pub fn to_array(uint8array: &[u8]) -> Vec<i64> {
    uint8array.iter().map(|&b| i64::from(b)).collect()
}

pub fn from_hex(hex: &str) -> Result<Vec<u8>, VmRustFnError> {
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

pub fn to_hex(uint8array: &[u8]) -> bex_str::BexStr {
    use std::fmt::Write;
    let mut s = String::with_capacity(uint8array.len() * 2);
    for &b in uint8array {
        let Ok(()) = write!(s, "{b:02x}") else {
            unreachable!("write!() to `String` should never fail");
        };
    }
    bex_str::BexStr::from(s)
}

pub fn from_base64(base64_str: &str) -> Result<Vec<u8>, VmRustFnError> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(base64_str)
        .map_err(|e| VmBamlError::InvalidArgument {
            message: format!("failed to decode base64: {e}"),
        })
        .map_err(VmRustFnError::BamlError)
}

pub fn to_base64(uint8array: &[u8]) -> bex_str::BexStr {
    use base64::Engine;
    bex_str::BexStr::from(base64::engine::general_purpose::STANDARD.encode(uint8array))
}

pub fn _to_string_impl(uint8array: &[u8]) -> bex_str::BexStr {
    bex_str::BexStr::from(String::from_utf8_lossy(uint8array).into_owned())
}

#[allow(clippy::unused_unit)]
pub fn sort(uint8array: &mut Vec<u8>) -> () {
    uint8array.sort_unstable();
}
