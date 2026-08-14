use bex_str::BexStr;
use bex_vm_types::types::Value;

use super::{BamlClassString, PackageBamlImpl};
use crate::{
    array_index::{resolve_index, resolve_slice_bound},
    errors::{VmBamlError, VmRustFnError},
};

fn char_substrings(string: &BexStr) -> Vec<BexStr> {
    string
        .as_str()
        .char_indices()
        .map(|(start, ch)| string.substring(start, start + ch.len_utf8()))
        .collect()
}

impl BamlClassString for PackageBamlImpl {
    #[expect(clippy::cast_possible_wrap)]
    fn length(string: &BexStr) -> i64 {
        string.char_count() as i64
    }

    #[expect(clippy::cast_possible_wrap)]
    fn char_count(string: &BexStr) -> i64 {
        string.char_count() as i64
    }

    #[expect(clippy::cast_possible_wrap)]
    fn byte_length(string: &BexStr) -> i64 {
        string.len() as i64
    }

    fn to_lower_case(string: &BexStr) -> BexStr {
        BexStr::from(string.as_str().to_lowercase())
    }

    fn to_upper_case(string: &BexStr) -> BexStr {
        BexStr::from(string.as_str().to_uppercase())
    }

    // Zero-copy: returns a Slice into the original string when whitespace is trimmed.
    fn trim(string: &BexStr) -> BexStr {
        let s = string.as_str();
        let trimmed = s.trim();
        if trimmed.len() == s.len() {
            return string.clone();
        }
        let start = trimmed.as_ptr() as usize - s.as_ptr() as usize;
        string.substring(start, start + trimmed.len())
    }

    fn trim_start(string: &BexStr) -> BexStr {
        let s = string.as_str();
        let trimmed = s.trim_start();
        if trimmed.len() == s.len() {
            return string.clone();
        }
        let start = trimmed.as_ptr() as usize - s.as_ptr() as usize;
        string.substring(start, string.len())
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
        string.as_str().contains(search.as_str())
    }

    fn starts_with(string: &BexStr, prefix: &BexStr) -> bool {
        string.as_str().starts_with(prefix.as_str())
    }

    fn ends_with(string: &BexStr, suffix: &BexStr) -> bool {
        string.as_str().ends_with(suffix.as_str())
    }

    // Zero-copy: each segment is a zero-copy substring Slice into the original.
    fn split(string: &BexStr, delimiter: &BexStr) -> Vec<BexStr> {
        let s = string.as_str();
        let d = delimiter.as_str();
        if d.is_empty() {
            return char_substrings(string);
        }
        let base = s.as_ptr() as usize;
        s.split(d)
            .map(|part| {
                let start = part.as_ptr() as usize - base;
                string.substring(start, start + part.len())
            })
            .collect()
    }

    fn chars(string: &BexStr) -> Vec<BexStr> {
        char_substrings(string)
    }

    fn lines(string: &BexStr) -> Vec<BexStr> {
        let s = string.as_str();
        let base = s.as_ptr() as usize;
        s.lines()
            .map(|line| {
                let start = line.as_ptr() as usize - base;
                string.substring(start, start + line.len())
            })
            .collect()
    }

    fn slice(string: &BexStr, start: i64, end: i64) -> BexStr {
        // Codepoint-indexed, not byte-indexed; a negative index counts from the end.
        let len = string.char_count();
        let start = resolve_slice_bound(start, len);
        // An `end` resolving before `start` yields an empty string.
        let end = resolve_slice_bound(end, len).max(start);
        string.substring_by_char(start, end)
    }

    fn replace(string: &BexStr, search: &BexStr, replacement: &BexStr) -> BexStr {
        BexStr::from(
            string
                .as_str()
                .replacen(search.as_str(), replacement.as_str(), 1),
        )
    }

    #[expect(clippy::cast_possible_wrap)]
    fn index_of(string: &BexStr, search: &BexStr) -> Option<i64> {
        string.char_index_of(search.as_str()).map(|i| i as i64)
    }

    #[expect(clippy::cast_possible_wrap)]
    fn last_index_of(string: &BexStr, search: &BexStr) -> Option<i64> {
        string.char_last_index_of(search.as_str()).map(|i| i as i64)
    }

    fn at(string: &BexStr, index: i64) -> Option<BexStr> {
        // Codepoint-indexed, not byte-indexed. A negative index counts from the
        // end; an index still outside the string after that yields `null`, so a
        // non-null result is always exactly one codepoint.
        resolve_index(index, string.char_count()).and_then(|i| string.char_at_codepoint(i))
    }

    fn code_point_at(string: &BexStr, index: i64) -> Option<i64> {
        // The numeric counterpart of `at`: same codepoint-indexing and bounds
        // rules (negative counts from the end, out of range yields `null`), but
        // yields the code point's value rather than a one-character string. A
        // `char` is always in `[0, 0x10FFFF]`, so the widening to `i64` is
        // lossless.
        resolve_index(index, string.char_count())
            .and_then(|i| string.as_str().chars().nth(i))
            .map(|c| i64::from(u32::from(c)))
    }

    fn repeat(string: &BexStr, count: i64) -> BexStr {
        let count = usize::try_from(count.max(0)).unwrap_or(0);
        string.repeat(count)
    }

    fn replace_all(string: &BexStr, search: &BexStr, replacement: &BexStr) -> BexStr {
        BexStr::from(
            string
                .as_str()
                .replace(search.as_str(), replacement.as_str()),
        )
    }

    // ── Unicode-aware predicates ──────────────────────────────────────────────
    //
    // All predicates use `chars().all(...)`, which returns true on the empty
    // string per the universal-quantifier convention.

    fn is_numeric(string: &BexStr) -> bool {
        string.as_str().chars().all(char::is_numeric)
    }

    fn is_alphabetic(string: &BexStr) -> bool {
        string.as_str().chars().all(char::is_alphabetic)
    }

    fn is_alphanumeric(string: &BexStr) -> bool {
        string.as_str().chars().all(char::is_alphanumeric)
    }

    fn is_uppercase(string: &BexStr) -> bool {
        string.as_str().chars().all(char::is_uppercase)
    }

    fn is_lowercase(string: &BexStr) -> bool {
        string.as_str().chars().all(char::is_lowercase)
    }

    fn is_whitespace(string: &BexStr) -> bool {
        string.as_str().chars().all(char::is_whitespace)
    }

    fn is_control(string: &BexStr) -> bool {
        string.as_str().chars().all(char::is_control)
    }

    /// "Graphic" = visible / printing character: not control, not whitespace.
    /// Note this is a convenience definition — Unicode does not have a single
    /// `Graphic` property. We exclude `Cc` (control) and `White_Space`.
    fn is_graphic(string: &BexStr) -> bool {
        string
            .as_str()
            .chars()
            .all(|c| !c.is_control() && !c.is_whitespace())
    }

    // ── ASCII-only predicates ─────────────────────────────────────────────────

    fn is_ascii(string: &BexStr) -> bool {
        string.as_str().is_ascii()
    }

    fn is_ascii_numeric(string: &BexStr) -> bool {
        string.as_str().chars().all(|c| c.is_ascii_digit())
    }

    fn is_ascii_alphabetic(string: &BexStr) -> bool {
        string.as_str().chars().all(|c| c.is_ascii_alphabetic())
    }

    fn is_ascii_alphanumeric(string: &BexStr) -> bool {
        string.as_str().chars().all(|c| c.is_ascii_alphanumeric())
    }

    fn is_ascii_uppercase(string: &BexStr) -> bool {
        string.as_str().chars().all(|c| c.is_ascii_uppercase())
    }

    fn is_ascii_lowercase(string: &BexStr) -> bool {
        string.as_str().chars().all(|c| c.is_ascii_lowercase())
    }

    fn is_ascii_whitespace(string: &BexStr) -> bool {
        string.as_str().chars().all(|c| c.is_ascii_whitespace())
    }

    fn is_ascii_control(string: &BexStr) -> bool {
        string.as_str().chars().all(|c| c.is_ascii_control())
    }

    fn is_ascii_graphic(string: &BexStr) -> bool {
        string.as_str().chars().all(|c| c.is_ascii_graphic())
    }

    fn is_ascii_hex(string: &BexStr) -> bool {
        string.as_str().chars().all(|c| c.is_ascii_hexdigit())
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

    fn to_code_points(string: &BexStr) -> Vec<Value> {
        // The exact inverse of `from_code_points`: one `int` per character. Each
        // `char` is in `[0, 0x10FFFF]`, so `Value::int` is always in range.
        string
            .as_str()
            .chars()
            .map(|c| Value::int(i64::from(u32::from(c))))
            .collect()
    }

    fn from_code_points(unicode: &[Value]) -> Result<BexStr, VmRustFnError> {
        let mut result = String::with_capacity(unicode.len());
        for (i, val) in unicode.iter().enumerate() {
            let Some(n) = val.as_int() else {
                return Err(VmBamlError::InvalidArgument {
                    message: format!(
                        "string.from_code_points: element at index {i} is not an `int`"
                    ),
                }
                .into());
            };
            let cp = u32::try_from(n).ok().and_then(char::from_u32).ok_or_else(|| {
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
