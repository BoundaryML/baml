use bex_vm_types::types::Value;

use super::{BamlClassString, PackageBamlImpl};
use crate::{
    BexVm,
    errors::{VmBamlError, VmRustFnError},
};

impl BamlClassString for PackageBamlImpl {
    fn to_json(vm: &mut BexVm, string: &str) -> Value {
        // `string` is already a valid `json` arm — BAML's `json` type alias
        // includes `string` as one of its union members.  Wrap the Rust `&str`
        // back into a heap-allocated `Value::Object(Object::String(...))`.
        vm.alloc_string(string.to_string())
    }

    #[allow(clippy::cast_possible_wrap)]
    fn length(string: &str) -> i64 {
        string.len() as i64
    }

    #[allow(clippy::cast_possible_wrap)]
    fn char_count(string: &str) -> i64 {
        string.chars().count() as i64
    }

    fn to_lower_case(string: &str) -> String {
        string.to_lowercase()
    }

    fn to_upper_case(string: &str) -> String {
        string.to_uppercase()
    }

    fn trim(string: &str) -> String {
        string.trim().to_string()
    }

    fn trim_start(string: &str) -> String {
        string.trim_start().to_string()
    }

    fn trim_end(string: &str) -> String {
        string.trim_end().to_string()
    }

    fn includes(string: &str, search: &str) -> bool {
        string.contains(search)
    }

    fn starts_with(string: &str, prefix: &str) -> bool {
        string.starts_with(prefix)
    }

    fn ends_with(string: &str, suffix: &str) -> bool {
        string.ends_with(suffix)
    }

    fn split(string: &str, delimiter: &str) -> Vec<String> {
        string.split(delimiter).map(str::to_string).collect()
    }

    fn lines(string: &str) -> Vec<String> {
        string.lines().map(str::to_string).collect()
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    fn substring(string: &str, start: i64, end: i64) -> Result<String, VmRustFnError> {
        let len = string.len();
        // Clamp negatives to 0; out-of-range to len.
        let start = start.max(0) as usize;
        let end = end.max(0) as usize;
        let start = start.min(len);
        let end = end.min(len).max(start);
        if !string.is_char_boundary(start) {
            return Err(VmBamlError::InvalidArgument {
                message: format!(
                    "substring: start byte offset {start} is not a UTF-8 character boundary"
                ),
            }
            .into());
        }
        if !string.is_char_boundary(end) {
            return Err(VmBamlError::InvalidArgument {
                message: format!(
                    "substring: end byte offset {end} is not a UTF-8 character boundary"
                ),
            }
            .into());
        }
        Ok(string[start..end].to_string())
    }

    fn replace(string: &str, search: &str, replacement: &str) -> String {
        string.replacen(search, replacement, 1)
    }

    #[allow(clippy::cast_possible_wrap)]
    fn index_of(string: &str, search: &str) -> i64 {
        string.find(search).map(|i| i as i64).unwrap_or(-1)
    }

    fn char_at(string: &str, index: i64) -> Result<String, VmRustFnError> {
        let len = string.len();
        let Ok(index) = usize::try_from(index) else {
            return Err(VmBamlError::InvalidArgument {
                message: format!("char_at: byte offset {index} is negative"),
            }
            .into());
        };
        if index == len {
            return Ok(String::new());
        }
        if index > len {
            return Err(VmBamlError::InvalidArgument {
                message: format!("char_at: byte offset {index} is beyond the string length {len}"),
            }
            .into());
        }
        if !string.is_char_boundary(index) {
            return Err(VmBamlError::InvalidArgument {
                message: format!("char_at: byte offset {index} is not a UTF-8 character boundary"),
            }
            .into());
        }
        // Safe because `index` is < len and on a char boundary.
        let ch = string[index..].chars().next().unwrap_or_else(|| {
            unreachable!("char_at: char boundary at index < len must yield a char")
        });
        Ok(ch.to_string())
    }

    fn code_point_at(string: &str, index: i64) -> Result<i64, VmRustFnError> {
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
        if !string.is_char_boundary(index) {
            return Err(VmBamlError::InvalidArgument {
                message: format!(
                    "code_point_at: byte offset {index} is not a UTF-8 character boundary"
                ),
            }
            .into());
        }
        let ch = string[index..].chars().next().unwrap_or_else(|| {
            unreachable!("code_point_at: char boundary at index < len must yield a char")
        });
        Ok(i64::from(ch as u32))
    }

    fn matches(string: &str, pattern: &str) -> bool {
        string.contains(pattern)
    }

    fn replace_all(string: &str, search: &str, replacement: &str) -> String {
        string.replace(search, replacement)
    }

    // ── Unicode-aware predicates ──────────────────────────────────────────────
    //
    // All predicates use `chars().all(...)`, which returns true on the empty
    // string per the universal-quantifier convention.

    fn is_numeric(string: &str) -> bool {
        string.chars().all(char::is_numeric)
    }

    fn is_alphabetic(string: &str) -> bool {
        string.chars().all(char::is_alphabetic)
    }

    fn is_alphanumeric(string: &str) -> bool {
        string.chars().all(char::is_alphanumeric)
    }

    fn is_uppercase(string: &str) -> bool {
        string.chars().all(char::is_uppercase)
    }

    fn is_lowercase(string: &str) -> bool {
        string.chars().all(char::is_lowercase)
    }

    fn is_whitespace(string: &str) -> bool {
        string.chars().all(char::is_whitespace)
    }

    fn is_control(string: &str) -> bool {
        string.chars().all(char::is_control)
    }

    /// "Graphic" = visible / printing character: not control, not whitespace.
    /// Note this is a convenience definition — Unicode does not have a single
    /// `Graphic` property. We exclude `Cc` (control) and `White_Space`.
    fn is_graphic(string: &str) -> bool {
        string
            .chars()
            .all(|c| !c.is_control() && !c.is_whitespace())
    }

    // ── ASCII-only predicates ─────────────────────────────────────────────────

    fn is_ascii(string: &str) -> bool {
        string.is_ascii()
    }

    fn is_ascii_numeric(string: &str) -> bool {
        string.chars().all(|c| c.is_ascii_digit())
    }

    fn is_ascii_alphabetic(string: &str) -> bool {
        string.chars().all(|c| c.is_ascii_alphabetic())
    }

    fn is_ascii_alphanumeric(string: &str) -> bool {
        string.chars().all(|c| c.is_ascii_alphanumeric())
    }

    fn is_ascii_uppercase(string: &str) -> bool {
        string.chars().all(|c| c.is_ascii_uppercase())
    }

    fn is_ascii_lowercase(string: &str) -> bool {
        string.chars().all(|c| c.is_ascii_lowercase())
    }

    fn is_ascii_whitespace(string: &str) -> bool {
        string.chars().all(|c| c.is_ascii_whitespace())
    }

    fn is_ascii_control(string: &str) -> bool {
        string.chars().all(|c| c.is_ascii_control())
    }

    fn is_ascii_graphic(string: &str) -> bool {
        string.chars().all(|c| c.is_ascii_graphic())
    }

    fn is_ascii_hex(string: &str) -> bool {
        string.chars().all(|c| c.is_ascii_hexdigit())
    }

    fn to_utf8(string: &str) -> Vec<u8> {
        string.as_bytes().to_vec()
    }

    fn from_utf8(utf8: &[u8]) -> Result<String, VmRustFnError> {
        std::str::from_utf8(utf8).map(str::to_string).map_err(|e| {
            VmBamlError::InvalidArgument {
                message: format!(
                    "string.from_utf8: invalid UTF-8 at byte {}: {e}",
                    e.valid_up_to()
                ),
            }
            .into()
        })
    }

    fn from_code_points(unicode: &[Value]) -> Result<String, VmRustFnError> {
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
        Ok(result)
    }
}
