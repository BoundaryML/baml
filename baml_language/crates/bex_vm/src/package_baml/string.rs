use bex_str::BexStr;
use bex_vm_types::types::Value;

use super::{BamlClassString, PackageBamlImpl};
use crate::{
    array_index::{resolve_index, resolve_slice_bound},
    errors::{VmBamlError, VmRustFnError},
};

/// Whether `byte` is a whitespace character by `char::is_whitespace`. The
/// ASCII whitespace that `u8::is_ascii_whitespace` leaves out is the vertical
/// tab (`\x0B`), which Unicode counts.
fn is_ascii_whitespace_char(byte: u8) -> bool {
    matches!(byte, b'\t' | b'\n' | b'\x0B' | b'\x0C' | b'\r' | b' ')
}

/// Byte offset where `s.trim_start()` begins. Skips ASCII whitespace a byte
/// at a time and hands the rest to `str` only at a non-ASCII byte, which may
/// start Unicode whitespace.
fn trim_start_offset(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut start = 0;
    while start < bytes.len() && is_ascii_whitespace_char(bytes[start]) {
        start += 1;
    }
    if start < bytes.len() && !bytes[start].is_ascii() {
        let rest = &s[start..];
        start += rest.len() - rest.trim_start().len();
    }
    start
}

/// Byte offset where `s[start..].trim_end()` ends, the mirror of
/// [`trim_start_offset`].
fn trim_end_offset(s: &str, start: usize) -> usize {
    let bytes = s.as_bytes();
    let mut end = bytes.len();
    while end > start && is_ascii_whitespace_char(bytes[end - 1]) {
        end -= 1;
    }
    if end > start && !bytes[end - 1].is_ascii() {
        end = start + s[start..end].trim_end().len();
    }
    end
}

/// `string` without the bytes outside `start..end`: the string itself when
/// nothing was trimmed, otherwise a zero-copy slice of it.
fn trimmed(string: &BexStr, start: usize, end: usize) -> BexStr {
    if start == 0 && end == string.byte_len() {
        return string.clone();
    }
    string.byte_slice(start..end)
}

/// Push the segments of `string` between the non-overlapping delimiter
/// matches starting at `matches`, each `delimiter_len` bytes long.
fn split_at_matches(
    string: &BexStr,
    matches: impl Iterator<Item = usize>,
    delimiter_len: usize,
    parts: &mut Vec<BexStr>,
) {
    let mut start = 0;
    for end in matches {
        parts.push(string.byte_slice(start..end));
        start = end + delimiter_len;
    }
    parts.push(string.byte_slice(start..string.byte_len()));
}

fn char_substrings(string: &BexStr) -> Vec<BexStr> {
    string
        .as_str()
        .char_indices()
        .map(|(start, ch)| string.byte_slice(start..start + ch.len_utf8()))
        .collect()
}

impl BamlClassString for PackageBamlImpl {
    #[expect(clippy::cast_possible_wrap)]
    fn length(string: &BexStr) -> i64 {
        string.char_len() as i64
    }

    #[expect(clippy::cast_possible_wrap)]
    fn char_count(string: &BexStr) -> i64 {
        string.char_len() as i64
    }

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
        let start = trim_start_offset(s);
        trimmed(string, start, trim_end_offset(s, start))
    }

    fn trim_start(string: &BexStr) -> BexStr {
        trimmed(
            string,
            trim_start_offset(string.as_str()),
            string.byte_len(),
        )
    }

    fn trim_end(string: &BexStr) -> BexStr {
        trimmed(string, 0, trim_end_offset(string.as_str(), 0))
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
        let bytes = string.as_bytes();
        // A valid UTF-8 delimiter only matches at char boundaries, so the
        // byte searches below split exactly where `str::split` would.
        match *delimiter.as_bytes() {
            [] => char_substrings(string),
            // One byte (ASCII, so `" "`, `","`, `"\n"`): sized up front.
            [byte] => {
                let mut parts = Vec::with_capacity(1 + memchr::memchr_iter(byte, bytes).count());
                split_at_matches(string, memchr::memchr_iter(byte, bytes), 1, &mut parts);
                parts
            }
            ref needle => {
                let mut parts = Vec::new();
                let matches = memchr::memmem::find_iter(bytes, needle);
                split_at_matches(string, matches, needle.len(), &mut parts);
                parts
            }
        }
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
                string.byte_slice(start..start + line.len())
            })
            .collect()
    }

    fn slice(string: &BexStr, start: i64, end: i64) -> BexStr {
        // Codepoint-indexed, not byte-indexed; a negative index counts from the end.
        let len = string.char_len();
        let start = resolve_slice_bound(start, len);
        // An `end` resolving before `start` yields an empty string.
        let end = resolve_slice_bound(end, len).max(start);
        string.char_slice(start..end)
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

#[cfg(test)]
mod tests {
    use bex_str::BexStr;

    use super::{BamlClassString, PackageBamlImpl};

    /// Every whitespace character `char::is_whitespace` knows, ASCII first.
    fn whitespace() -> Vec<char> {
        (0..=u32::from(char::MAX))
            .filter_map(char::from_u32)
            .filter(|c| c.is_whitespace())
            .collect()
    }

    /// Inputs that mix ASCII and Unicode whitespace on either side of text
    /// that itself starts and ends with ASCII, non-ASCII and whitespace-like
    /// characters.
    fn trim_inputs() -> Vec<String> {
        let mut inputs = vec![
            String::new(),
            " ".to_string(),
            "x".to_string(),
            "é".to_string(),
            "\u{0B}\u{0C} vertical tab and form feed \u{0B}".to_string(),
            "\u{85}next line\u{85}".to_string(),
        ];
        for ws in whitespace() {
            for core in ["hi", "é", "a b", "\u{200B}zero width\u{200B}"] {
                inputs.push(format!("{ws}{core}{ws}"));
                inputs.push(format!("  {ws} \t{core}\n{ws}  "));
                inputs.push(format!("{ws}{ws}"));
                inputs.push(format!("{ws} {}", core.repeat(30)));
            }
        }
        inputs
    }

    #[test]
    fn trims_like_str() {
        for input in trim_inputs() {
            let string = BexStr::from(input.as_str());
            assert_eq!(
                PackageBamlImpl::trim(&string).as_str(),
                input.trim(),
                "{input:?}"
            );
            assert_eq!(
                PackageBamlImpl::trim_start(&string).as_str(),
                input.trim_start(),
                "{input:?}"
            );
            assert_eq!(
                PackageBamlImpl::trim_end(&string).as_str(),
                input.trim_end(),
                "{input:?}"
            );
        }
    }

    #[test]
    fn splits_like_str() {
        let long = "the quick brown fox jumps over the lazy dog ".repeat(4);
        let texts = [
            "",
            ",",
            ",,",
            "a,b,,c,",
            ",leading",
            "héllo,wörld,😀,",
            "no delimiter here",
            "aaaa",
            "aaa",
            long.as_str(),
        ];
        for text in texts {
            for delimiter in [",", " ", "o", "a", "aa", "é", "😀", ", ", "the"] {
                let string = BexStr::from(text);
                let parts = PackageBamlImpl::split(&string, &BexStr::from(delimiter));
                let parts: Vec<&str> = parts.iter().map(BexStr::as_str).collect();
                let expected: Vec<&str> = text.split(delimiter).collect();
                assert_eq!(parts, expected, "{text:?}.split({delimiter:?})");
            }
        }
    }
}
