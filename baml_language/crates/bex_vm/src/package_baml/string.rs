use bex_vm_types::{BexStr, types::Value};

use super::{BamlClassString, PackageBamlImpl};
use crate::{
    BexVm,
    errors::{VmBamlError, VmRustFnError},
};

impl BamlClassString for PackageBamlImpl {
    fn to_json(vm: &mut BexVm, string: &BexStr) -> Value {
        // `string` is already a valid `json` arm — BAML's `json` type alias
        // includes `string` as one of its union members.  Wrap the BexStr
        // back into a heap-allocated `Value::Object(Object::String(...))`.
        vm.alloc_string(string.clone())
    }

    #[allow(clippy::cast_possible_wrap)]
    fn length(string: &BexStr) -> i64 {
        string.len() as i64
    }

    #[allow(clippy::cast_possible_wrap)]
    fn char_count(string: &BexStr) -> i64 {
        string.chars().count() as i64
    }

    fn to_lower_case(string: &BexStr) -> BexStr {
        BexStr::from(string.to_lowercase())
    }

    fn to_upper_case(string: &BexStr) -> BexStr {
        BexStr::from(string.to_uppercase())
    }

    fn trim(string: &BexStr) -> BexStr {
        let s = string.as_str();
        let trimmed = s.trim();
        if trimmed.len() == s.len() {
            return string.clone();
        }
        let start = s.len() - s.trim_start().len();
        let end = start + trimmed.len();
        string.substring(start, end)
    }

    fn trim_start(string: &BexStr) -> BexStr {
        let s = string.as_str();
        let trimmed = s.trim_start();
        if trimmed.len() == s.len() {
            return string.clone();
        }
        let start = s.len() - trimmed.len();
        string.substring(start, s.len())
    }

    fn trim_end(string: &BexStr) -> BexStr {
        let s = string.as_str();
        let trimmed = s.trim_end();
        if trimmed.len() == s.len() {
            return string.clone();
        }
        string.substring(0, trimmed.len())
    }

    fn includes(string: &BexStr, search: &BexStr) -> bool {
        string.contains(search.as_str())
    }

    fn starts_with(string: &BexStr, prefix: &BexStr) -> bool {
        str::starts_with(string, prefix.as_str())
    }

    fn ends_with(string: &BexStr, suffix: &BexStr) -> bool {
        str::ends_with(string, suffix.as_str())
    }

    fn split(string: &BexStr, delimiter: &BexStr) -> Vec<BexStr> {
        let s = string.as_str();
        let d = delimiter.as_str();
        // Use zero-copy substring for each split segment
        s.match_indices(d)
            .scan(0usize, |start, (idx, _)| {
                let seg_start = *start;
                *start = idx + d.len();
                Some((seg_start, idx))
            })
            .chain(std::iter::once({
                // Find the start of the last segment
                let last_start = s
                    .rmatch_indices(d)
                    .next()
                    .map(|(idx, _)| idx + d.len())
                    .unwrap_or(0);
                (last_start, s.len())
            }))
            .map(|(start, end)| string.substring(start, end))
            .collect()
    }

    fn lines(string: &BexStr) -> Vec<BexStr> {
        let s = string.as_str();
        let mut result = Vec::new();
        let mut start = 0;
        for (idx, _) in s.match_indices('\n') {
            let end = if idx > start && s.as_bytes()[idx - 1] == b'\r' {
                idx - 1
            } else {
                idx
            };
            result.push(string.substring(start, end));
            start = idx + 1;
        }
        // Only add the final segment if there's content after the last \n,
        // or if there was no \n at all
        if start < s.len() {
            result.push(string.substring(start, s.len()));
        }
        result
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    fn substring(string: &BexStr, start: i64, end: i64) -> Result<BexStr, VmRustFnError> {
        let len = string.len();
        // Clamp negatives to 0; out-of-range to len.
        let start = start.max(0) as usize;
        let end = end.max(0) as usize;
        let start = start.min(len);
        let end = end.min(len).max(start);
        let s = string.as_str();
        if !s.is_char_boundary(start) {
            return Err(VmBamlError::InvalidArgument {
                message: format!(
                    "substring: start byte offset {start} is not a UTF-8 character boundary"
                ),
            }
            .into());
        }
        if !s.is_char_boundary(end) {
            return Err(VmBamlError::InvalidArgument {
                message: format!(
                    "substring: end byte offset {end} is not a UTF-8 character boundary"
                ),
            }
            .into());
        }
        Ok(string.substring(start, end))
    }

    fn replace(string: &BexStr, search: &BexStr, replacement: &BexStr) -> BexStr {
        BexStr::from(string.replacen(search.as_str(), replacement, 1))
    }

    #[allow(clippy::cast_possible_wrap)]
    fn index_of(string: &BexStr, search: &BexStr) -> i64 {
        string.find(search.as_str()).map(|i| i as i64).unwrap_or(-1)
    }

    fn char_at(string: &BexStr, index: i64) -> Result<BexStr, VmRustFnError> {
        let len = string.len();
        let Ok(index) = usize::try_from(index) else {
            return Err(VmBamlError::InvalidArgument {
                message: format!("char_at: byte offset {index} is negative"),
            }
            .into());
        };
        if index == len {
            return Ok(BexStr::empty());
        }
        if index > len {
            return Err(VmBamlError::InvalidArgument {
                message: format!("char_at: byte offset {index} is beyond the string length {len}"),
            }
            .into());
        }
        let s = string.as_str();
        if !s.is_char_boundary(index) {
            return Err(VmBamlError::InvalidArgument {
                message: format!("char_at: byte offset {index} is not a UTF-8 character boundary"),
            }
            .into());
        }
        // Safe because `index` is < len and on a char boundary.
        let ch = s[index..].chars().next().unwrap_or_else(|| {
            unreachable!("char_at: char boundary at index < len must yield a char")
        });
        let end = index + ch.len_utf8();
        Ok(string.substring(index, end))
    }

    fn code_point_at(string: &BexStr, index: i64) -> Result<i64, VmRustFnError> {
        let len = string.len();
        let Ok(index) = usize::try_from(index) else {
            return Err(VmBamlError::InvalidArgument {
                message: format!("code_point_at: byte offset {index} is negative"),
            }
            .into());
        };
        if index >= len {
            return Err(VmBamlError::InvalidArgument {
                message: format!(
                    "code_point_at: byte offset {index} is beyond the last code point (length {len})"
                ),
            }
            .into());
        }
        let s = string.as_str();
        if !s.is_char_boundary(index) {
            return Err(VmBamlError::InvalidArgument {
                message: format!(
                    "code_point_at: byte offset {index} is not a UTF-8 character boundary"
                ),
            }
            .into());
        }
        let ch = s[index..].chars().next().unwrap_or_else(|| {
            unreachable!("code_point_at: char boundary at index < len must yield a char")
        });
        Ok(i64::from(ch as u32))
    }

    fn matches(string: &BexStr, pattern: &BexStr) -> bool {
        string.contains(pattern.as_str())
    }

    fn replace_all(string: &BexStr, search: &BexStr, replacement: &BexStr) -> BexStr {
        BexStr::from(string.replace(search.as_str(), replacement))
    }

    // ── Unicode-aware predicates ──────────────────────────────────────────────
    //
    // All predicates use `chars().all(...)`, which returns true on the empty
    // string per the universal-quantifier convention.

    fn is_numeric(string: &BexStr) -> bool {
        string.chars().all(char::is_numeric)
    }

    fn is_alphabetic(string: &BexStr) -> bool {
        string.chars().all(char::is_alphabetic)
    }

    fn is_alphanumeric(string: &BexStr) -> bool {
        string.chars().all(char::is_alphanumeric)
    }

    fn is_uppercase(string: &BexStr) -> bool {
        string.chars().all(char::is_uppercase)
    }

    fn is_lowercase(string: &BexStr) -> bool {
        string.chars().all(char::is_lowercase)
    }

    fn is_whitespace(string: &BexStr) -> bool {
        string.chars().all(char::is_whitespace)
    }

    fn is_control(string: &BexStr) -> bool {
        string.chars().all(char::is_control)
    }

    /// "Graphic" = visible / printing character: not control, not whitespace.
    /// Note this is a convenience definition — Unicode does not have a single
    /// `Graphic` property. We exclude `Cc` (control) and `White_Space`.
    fn is_graphic(string: &BexStr) -> bool {
        string
            .chars()
            .all(|c| !c.is_control() && !c.is_whitespace())
    }

    // ── ASCII-only predicates ─────────────────────────────────────────────────

    fn is_ascii(string: &BexStr) -> bool {
        string.is_ascii()
    }

    fn is_ascii_numeric(string: &BexStr) -> bool {
        string.chars().all(|c| c.is_ascii_digit())
    }

    fn is_ascii_alphabetic(string: &BexStr) -> bool {
        string.chars().all(|c| c.is_ascii_alphabetic())
    }

    fn is_ascii_alphanumeric(string: &BexStr) -> bool {
        string.chars().all(|c| c.is_ascii_alphanumeric())
    }

    fn is_ascii_uppercase(string: &BexStr) -> bool {
        string.chars().all(|c| c.is_ascii_uppercase())
    }

    fn is_ascii_lowercase(string: &BexStr) -> bool {
        string.chars().all(|c| c.is_ascii_lowercase())
    }

    fn is_ascii_whitespace(string: &BexStr) -> bool {
        string.chars().all(|c| c.is_ascii_whitespace())
    }

    fn is_ascii_control(string: &BexStr) -> bool {
        string.chars().all(|c| c.is_ascii_control())
    }

    fn is_ascii_graphic(string: &BexStr) -> bool {
        string.chars().all(|c| c.is_ascii_graphic())
    }

    fn is_ascii_hex(string: &BexStr) -> bool {
        string.chars().all(|c| c.is_ascii_hexdigit())
    }

    fn to_utf8(string: &BexStr) -> Vec<u8> {
        string.as_bytes().to_vec()
    }

    fn from_utf8(utf8: &[u8]) -> Result<BexStr, VmRustFnError> {
        std::str::from_utf8(utf8).map(BexStr::from).map_err(|e| {
            VmBamlError::InvalidArgument {
                message: format!(
                    "string.from_utf8: invalid UTF-8 at byte {}: {e}",
                    e.valid_up_to()
                ),
            }
            .into()
        })
    }

    fn from_code_points(unicode: &[Value]) -> Result<BexStr, VmRustFnError> {
        let mut result = String::with_capacity(unicode.len());
        for (i, val) in unicode.iter().enumerate() {
            let Value::Int(n) = val else {
                return Err(VmBamlError::InvalidArgument {
                    message: format!(
                        "string.from_code_points: element at index {i} is not an `int`"
                    ),
                }
                .into());
            };
            let cp = u32::try_from(*n).ok().and_then(char::from_u32).ok_or_else(|| {
                VmBamlError::InvalidArgument {
                    message: format!(
                        "string.from_code_points: value {n} at index {i} is not a valid Unicode code point (must be in [0, 0x10FFFF] and not a surrogate)"
                    ),
                }
            })?;
            result.push(cp);
        }
        Ok(BexStr::from(result))
    }
}
