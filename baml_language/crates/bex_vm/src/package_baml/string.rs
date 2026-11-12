use std::ops::Range;

use bex_heap::TlabHolder as _;
use bex_vm_types::{
    StrRef,
    layout::{Fault, ListOf, Lists, ObjPtr, RawWord, Strings},
    types::Value,
};

use super::{BamlClassString, NativeCallResult, PackageBamlImpl};
use crate::{
    BexVm,
    array_index::{int_len, resolve_index, slice_bounds},
    errors::{VmBamlError, VmRustFnError},
};

/// The receiver trimmed by `trim`, sharing its storage: a view when the
/// kept range is long, a flat copy otherwise (`Tlab::substring`). The
/// generated glue would copy the native's `String` out every time.
fn trimmed(vm: &mut BexVm, args: &[Value], trim: fn(&str) -> &str) -> NativeCallResult {
    vm.flatten_str(&args[0]);
    let (string, kept) = match vm.as_string(&args[0]) {
        Ok(string) => {
            let text = string.as_str();
            let part = trim(text);
            let start = part.as_ptr() as usize - text.as_ptr() as usize;
            (string.ptr(), start..start + part.len())
        }
        Err(error) => return NativeCallResult::Error(error.into()),
    };
    NativeCallResult::Done(Value::object(vm.substring(string, kept)))
}

impl BamlClassString for PackageBamlImpl {
    #[expect(clippy::cast_possible_wrap)]
    fn byte_length(string: &str) -> i64 {
        string.len() as i64
    }

    fn to_lower_case(string: &str) -> String {
        string.to_lowercase()
    }

    fn to_upper_case(string: &str) -> String {
        string.to_uppercase()
    }

    fn trim(string: &str) -> String {
        string.trim().to_owned()
    }

    fn trim_start(string: &str) -> String {
        string.trim_start().to_owned()
    }

    fn trim_end(string: &str) -> String {
        string.trim_end().to_owned()
    }

    // The glue for the three is overridden to share the receiver's storage
    // (see `trimmed`); the bodies above stay the trait's required methods.

    fn __glue_trim(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
        trimmed(vm, args, str::trim)
    }

    fn __glue_trim_start(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
        trimmed(vm, args, str::trim_start)
    }

    fn __glue_trim_end(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
        trimmed(vm, args, str::trim_end)
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

    fn _replace_literal(string: &str, search: &str, replacement: &str) -> String {
        string.replacen(search, replacement, 1)
    }

    // The char-indexed methods take the cell (`//baml:str_ref`): its lengths
    // make the ASCII case O(1).

    #[expect(clippy::cast_possible_wrap)]
    fn index_of(string: StrRef<'_>, search: StrRef<'_>) -> Option<i64> {
        string.char_find(search.as_str()).map(|i| i as i64)
    }

    #[expect(clippy::cast_possible_wrap)]
    fn last_index_of(string: StrRef<'_>, search: StrRef<'_>) -> Option<i64> {
        string.char_rfind(search.as_str()).map(|i| i as i64)
    }

    fn at(string: StrRef<'_>, index: i64) -> Option<String> {
        // Codepoint-indexed, not byte-indexed. A negative index counts from the
        // end; an index still outside the string after that yields `null`, so a
        // non-null result is always exactly one codepoint.
        resolve_index(index, string.char_len()).map(|i| string.char_slice(i..i + 1).to_owned())
    }

    fn code_point_at(string: StrRef<'_>, index: i64) -> Option<i64> {
        // The numeric counterpart of `at`: same codepoint-indexing and bounds
        // rules (negative counts from the end, out of range yields `null`), but
        // yields the code point's value rather than a one-character string. A
        // `char` is always in `[0, 0x10FFFF]`, so the widening to `i64` is
        // lossless.
        resolve_index(index, string.char_len())
            .and_then(|i| string.char_at(i))
            .map(|c| i64::from(u32::from(c)))
    }

    fn repeat(string: &str, count: i64) -> String {
        let count = usize::try_from(count.max(0)).unwrap_or(0);
        string.repeat(count)
    }

    fn _replace_all_literal(string: &str, search: &str, replacement: &str) -> String {
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
        string.chars().all(|c| !c.is_control() && !c.is_whitespace())
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
    fn from_utf8(utf8: &[u8]) -> Result<String, VmRustFnError> {
        std::str::from_utf8(utf8).map(str::to_owned).map_err(|e| {
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
        Ok(result)
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
        let bytes = rt.str_char_bytes(string, chars);
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
