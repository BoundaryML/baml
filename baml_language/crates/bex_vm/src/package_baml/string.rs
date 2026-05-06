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

    fn to_lower_case(string: &str) -> String {
        string.to_lowercase()
    }

    fn to_upper_case(string: &str) -> String {
        string.to_uppercase()
    }

    fn trim(string: &str) -> String {
        string.trim().to_string()
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

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
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
        let ch = string[index..]
            .chars()
            .next()
            .expect("char_at: char boundary at index < len must yield a char");
        Ok(ch.to_string())
    }

    fn matches(string: &str, pattern: &str) -> bool {
        string.contains(pattern)
    }

    fn replace_all(string: &str, search: &str, replacement: &str) -> String {
        string.replace(search, replacement)
    }

    fn to_bytes(string: &str) -> Vec<u8> {
        string.as_bytes().to_vec()
    }
}
