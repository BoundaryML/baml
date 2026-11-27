//! `baml.Uint8Array`.
//!
//! A native reads the bytes by copying them, one at a time or all at once.
//! The three that lend them as a slice instead say why where they do.

use base64::Engine as _;

use super::{
    Arr, ArrRef, Bytes, BytesRef, Int, NativeBamlUint8Array, Natives, Root, Scope, Str, Thrown,
    array_size,
    string::{index_in, slice_bound, text_size},
};
use crate::layout::BYTES_MAX;

const HEX: &[u8; 16] = b"0123456789abcdef";

fn hex_digit(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' | b'A'..=b'F' => Some((digit & 0x1F) + 9),
        _ => None,
    }
}

impl NativeBamlUint8Array for Natives {
    fn length(uint8array: BytesRef<'_>) -> i64 {
        uint8array.len().cast_signed() as i64
    }

    fn at(uint8array: BytesRef<'_>, index: i64) -> Option<i64> {
        uint8array
            .get(index_in(index, uint8array.len())?)
            .map(i64::from)
    }

    fn concat<'s>(
        scope: &mut Scope<'s>,
        uint8array: Root<'s, Bytes>,
        other: Root<'s, Bytes>,
    ) -> Root<'s, Bytes> {
        let (first, second) = (uint8array.get(scope).len(), other.get(scope).len());
        let joined = scope.alloc_zeroed_bytes(first + second);
        let into = joined.get(scope);
        into.copy_from_bytes(0, uint8array.get(scope), 0, first);
        into.copy_from_bytes(first, other.get(scope), 0, second);
        joined
    }

    fn includes(uint8array: BytesRef<'_>, item: i64) -> bool {
        // A scan that answers yes or no: a copy would allocate and move
        // the whole array to give it.
        u8::try_from(item).is_ok_and(|item| uint8array.as_slice().contains(&item))
    }

    fn index_of(uint8array: BytesRef<'_>, item: i64) -> Option<i64> {
        let item = u8::try_from(item).ok()?;
        // A scan, as in `includes`.
        uint8array
            .as_slice()
            .iter()
            .position(|byte| *byte == item)
            .map(|index| index.cast_signed() as i64)
    }

    fn reverse<'s>(scope: &mut Scope<'s>, uint8array: Root<'s, Bytes>) -> Root<'s, Bytes> {
        let mut bytes = uint8array.get(scope).to_vec();
        bytes.reverse();
        scope.alloc_bytes(&bytes)
    }

    fn slice<'s>(
        scope: &mut Scope<'s>,
        uint8array: Root<'s, Bytes>,
        start: i64,
        end: i64,
    ) -> Root<'s, Bytes> {
        let length = uint8array.get(scope).len();
        let start = slice_bound(start, length);
        // An end before the start is an empty slice.
        let end = slice_bound(end, length).max(start);
        let part = scope.alloc_zeroed_bytes(end - start);
        part.get(scope)
            .copy_from_bytes(0, uint8array.get(scope), start, end - start);
        part
    }

    fn zeroes<'s>(scope: &mut Scope<'s>, size: i64) -> Result<Root<'s, Bytes>, Thrown<'s>> {
        let Ok(size) = usize::try_from(size) else {
            return Err(scope.invalid_argument(&format!("Invalid size {size} for uint8array")));
        };
        if size > BYTES_MAX {
            return Err(scope.alloc_failure(&format!(
                "Allocation of {size} bytes for new uint8array failed"
            )));
        }
        Ok(scope.alloc_zeroed_bytes(size))
    }

    fn from_array<'s>(
        scope: &mut Scope<'s>,
        array: Root<'s, Arr<Int>>,
    ) -> Result<Root<'s, Bytes>, Thrown<'s>> {
        let values: ArrRef<'_, Int> = array.get(scope);
        let mut bytes = Vec::with_capacity(values.len());
        let mut invalid = None;
        for (index, value) in values.iter().enumerate() {
            match u8::try_from(value) {
                Ok(byte) => bytes.push(byte),
                Err(_) => {
                    invalid = Some((index, value));
                    break;
                }
            }
        }
        if let Some((index, value)) = invalid {
            return Err(scope.invalid_argument(&format!(
                "Value {value} at index {index} is out of range 0..=255"
            )));
        }
        Ok(scope.alloc_bytes(&bytes))
    }

    fn to_array<'s>(scope: &mut Scope<'s>, uint8array: Root<'s, Bytes>) -> Root<'s, Arr<Int>> {
        let length = uint8array.get(scope).len();
        let reservation = scope.reserve(array_size(length));
        let array = reservation.array::<Int>(length);
        let bytes = reservation.get(uint8array);
        for index in 0..length {
            array.set(index, bytes.get(index).map_or(0, i64::from));
        }
        let array = array.keep();
        reservation.finish(array)
    }

    fn from_hex<'s>(
        scope: &mut Scope<'s>,
        hex: Root<'s, Str>,
    ) -> Result<Root<'s, Bytes>, Thrown<'s>> {
        let digits = hex.get(scope).as_str().as_bytes();
        let (pairs, []) = digits.as_chunks::<2>() else {
            return Err(scope.invalid_argument("hex string must have even length"));
        };
        let mut bytes = Vec::with_capacity(pairs.len());
        let mut invalid = None;
        for (index, &[high, low]) in pairs.iter().enumerate() {
            match (hex_digit(high), hex_digit(low)) {
                (Some(high), Some(low)) => bytes.push(high << 4 | low),
                (high, _) => {
                    invalid = Some(index * 2 + usize::from(high.is_some()));
                    break;
                }
            }
        }
        if let Some(position) = invalid {
            return Err(scope.invalid_argument(&format!(
                "uint8array.from_hex: invalid hex digit at position {position}"
            )));
        }
        Ok(scope.alloc_bytes(&bytes))
    }

    fn to_hex<'s>(scope: &mut Scope<'s>, uint8array: Root<'s, Bytes>) -> Root<'s, Str> {
        let length = uint8array.get(scope).len();
        let reservation = scope.reserve(text_size(length * 2));
        let mut hex = reservation.new_str(length * 2, length * 2);
        let bytes = reservation.get(uint8array);
        for index in 0..length {
            let byte = bytes.get(index).unwrap_or_default();
            hex.push_char(char::from(HEX[usize::from(byte >> 4)]));
            hex.push_char(char::from(HEX[usize::from(byte & 0xF)]));
        }
        let hex = hex.finish().keep();
        reservation.finish(hex)
    }

    fn from_base64<'s>(
        scope: &mut Scope<'s>,
        base64: Root<'s, Str>,
    ) -> Result<Root<'s, Bytes>, Thrown<'s>> {
        let decoded = base64::engine::general_purpose::STANDARD.decode(base64.get(scope).as_str());
        match decoded {
            Ok(bytes) => Ok(scope.alloc_bytes(&bytes)),
            Err(error) => Err(scope.invalid_argument(&format!("failed to decode base64: {error}"))),
        }
    }

    fn to_base64<'s>(scope: &mut Scope<'s>, uint8array: Root<'s, Bytes>) -> Root<'s, Str> {
        let bytes = uint8array.get(scope).to_vec();
        scope.alloc_str(&base64::engine::general_purpose::STANDARD.encode(bytes))
    }

    fn _to_string_impl<'s>(scope: &mut Scope<'s>, uint8array: Root<'s, Bytes>) -> Root<'s, Str> {
        let bytes = uint8array.get(scope).to_vec();
        scope.alloc_str(&String::from_utf8_lossy(&bytes))
    }

    fn sort(uint8array: BytesRef<'_>) {
        // In place, as the declaration says: sorting a copy would have to
        // write it back over the array anyway.
        // SAFETY: no other slice of the array is alive in this native.
        unsafe { uint8array.as_mut_slice() }.sort_unstable();
    }
}
