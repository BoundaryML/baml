use std::ops::Range;

use bex_str::BexStr;
use bex_vm_types::{
    layout::{Fault, ListOf, Lists, ObjPtr, RawWord, Strings},
    types::Value,
};

use super::{BamlClassString, PackageBamlImpl};
use crate::{
    array_index::{int_len, resolve_index, slice_bounds},
    errors::{VmBamlError, VmRustFnError},
};

impl BamlClassString for PackageBamlImpl {
    #[expect(clippy::cast_possible_wrap)]
    fn byte_length(string: &BexStr) -> i64 {
        string.byte_len() as i64
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
        string.byte_slice(start..start + trimmed.len())
    }

    fn trim_start(string: &BexStr) -> BexStr {
        let s = string.as_str();
        let trimmed = s.trim_start();
        if trimmed.len() == s.len() {
            return string.clone();
        }
        let start = trimmed.as_ptr() as usize - s.as_ptr() as usize;
        string.byte_slice(start..string.byte_len())
    }

    fn trim_end(string: &BexStr) -> BexStr {
        let s = string.as_str();
        let trimmed = s.trim_end();
        if trimmed.len() == s.len() {
            return string.clone();
        }
        string.byte_slice(0..trimmed.len())
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
    fn _replace_literal(string: &BexStr, search: &BexStr, replacement: &BexStr) -> BexStr {
        BexStr::from(
            string
                .as_str()
                .replacen(search.as_str(), replacement.as_str(), 1),
        )
    }

    #[expect(clippy::cast_possible_wrap)]
    fn index_of(string: &BexStr, search: &BexStr) -> Option<i64> {
        string.char_find(search.as_str()).map(|i| i as i64)
    }

    #[expect(clippy::cast_possible_wrap)]
    fn last_index_of(string: &BexStr, search: &BexStr) -> Option<i64> {
        string.char_rfind(search.as_str()).map(|i| i as i64)
    }

    fn at(string: &BexStr, index: i64) -> Option<BexStr> {
        // Codepoint-indexed, not byte-indexed. A negative index counts from the
        // end; an index still outside the string after that yields `null`, so a
        // non-null result is always exactly one codepoint.
        resolve_index(index, string.char_len()).map(|i| string.char_slice(i..i + 1))
    }

    fn code_point_at(string: &BexStr, index: i64) -> Option<i64> {
        // The numeric counterpart of `at`: same codepoint-indexing and bounds
        // rules (negative counts from the end, out of range yields `null`), but
        // yields the code point's value rather than a one-character string. A
        // `char` is always in `[0, 0x10FFFF]`, so the widening to `i64` is
        // lossless.
        resolve_index(index, string.char_len())
            .and_then(|i| string.char_at(i))
            .map(|c| i64::from(u32::from(c)))
    }

    fn repeat(string: &BexStr, count: i64) -> BexStr {
        let count = usize::try_from(count.max(0)).unwrap_or(0);
        string.repeat(count)
    }

    fn _replace_all_literal(string: &BexStr, search: &BexStr, replacement: &BexStr) -> BexStr {
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

// ── Bodies shared by the interpreter and the shared-format adapters ──────
//
// The interpreter's glue runs them over `value_model::ValueModel`; generated
// adapters run them over any runtime. Substrings share the receiver's
// storage where the runtime allows (`Strings::str_substring`).
pub(crate) mod shared {
    #![allow(
        clippy::unnecessary_wraps,
        reason = "one calling convention for every body: generated glue and adapters call \
                  each `fn(&mut R, ...) -> Result<_, Fault>` alike"
    )]

    use super::{Fault, ListOf, Lists, ObjPtr, Range, RawWord, Strings, int_len, slice_bounds};

    /// The byte range of `part` within `text`, which it borrows from.
    fn byte_range(text: &str, part: &str) -> Range<usize> {
        let start = part.as_ptr() as usize - text.as_ptr() as usize;
        start..start + part.len()
    }

    /// The byte range of each character of `text`.
    fn char_ranges(text: &str) -> Vec<Range<usize>> {
        text.char_indices()
            .map(|(start, ch)| start..start + ch.len_utf8())
            .collect()
    }

    /// A `string[]` of the substrings of `string` at `ranges`.
    fn substrings<R: Strings + Lists<ObjPtr>>(
        rt: &mut R,
        string: ObjPtr,
        ranges: Vec<Range<usize>>,
    ) -> ObjPtr {
        let parts: Vec<ObjPtr> = ranges
            .into_iter()
            .map(|range| rt.str_substring(string, range))
            .collect();
        rt.list_alloc(ListOf::Strings, &parts)
    }

    pub(crate) fn baml_string_char_count<R: Strings>(
        rt: &mut R,
        string: ObjPtr,
    ) -> Result<i64, Fault> {
        Ok(int_len(rt.str_char_len(string)))
    }

    /// An empty delimiter splits into characters.
    pub(crate) fn baml_string_split<R: Strings + Lists<ObjPtr>>(
        rt: &mut R,
        string: ObjPtr,
        delimiter: ObjPtr,
    ) -> Result<ObjPtr, Fault> {
        let text = rt.str_text(string);
        let delimiter = rt.str_text(delimiter);
        let ranges = if delimiter.is_empty() {
            char_ranges(text)
        } else {
            text.split(delimiter)
                .map(|part| byte_range(text, part))
                .collect()
        };
        Ok(substrings(rt, string, ranges))
    }

    pub(crate) fn baml_string_chars<R: Strings + Lists<ObjPtr>>(
        rt: &mut R,
        string: ObjPtr,
    ) -> Result<ObjPtr, Fault> {
        let ranges = char_ranges(rt.str_text(string));
        Ok(substrings(rt, string, ranges))
    }

    pub(crate) fn baml_string_lines<R: Strings + Lists<ObjPtr>>(
        rt: &mut R,
        string: ObjPtr,
    ) -> Result<ObjPtr, Fault> {
        let text = rt.str_text(string);
        let ranges = text.lines().map(|line| byte_range(text, line)).collect();
        Ok(substrings(rt, string, ranges))
    }

    /// Codepoint-indexed, not byte-indexed; a negative index counts from the end.
    pub(crate) fn baml_string_slice<R: Strings>(
        rt: &mut R,
        string: ObjPtr,
        start: i64,
        end: i64,
    ) -> Result<ObjPtr, Fault> {
        let chars = slice_bounds(start, end, rt.str_char_len(string));
        let text = rt.str_text(string);
        let byte_at = |char_index: usize| {
            text.char_indices()
                .nth(char_index)
                .map_or(text.len(), |(byte, _)| byte)
        };
        let bytes = byte_at(chars.start)..byte_at(chars.end);
        Ok(rt.str_substring(string, bytes))
    }

    pub(crate) fn baml_string_to_utf8<R: Strings>(
        rt: &mut R,
        string: ObjPtr,
    ) -> Result<ObjPtr, Fault> {
        let bytes = rt.str_text(string).as_bytes().to_vec();
        Ok(rt.bytes_alloc(&bytes))
    }

    /// One `int` per character: the inverse of `from_code_points`.
    pub(crate) fn baml_string_to_code_points<R: Strings + Lists<RawWord>>(
        rt: &mut R,
        string: ObjPtr,
    ) -> Result<ObjPtr, Fault> {
        let points: Vec<RawWord> = rt
            .str_text(string)
            .chars()
            .map(|ch| RawWord::from_int(i64::from(u32::from(ch))))
            .collect();
        Ok(rt.list_alloc(ListOf::Ints, &points))
    }
}
