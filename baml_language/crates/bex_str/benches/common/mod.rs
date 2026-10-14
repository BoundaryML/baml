//! Shared by `v8_benches` and `bun_benches`: the BAML `String` methods the
//! ports call, and fixture helpers. Each bench target uses a subset.
#![allow(dead_code)]

use bex_str::BexStr;

/// BAML `String` methods, mirroring `bex_vm/src/package_baml/string.rs` and
/// `bex_vm/src/array_index.rs` (which `bex_str` cannot depend on).
pub mod baml {
    use bex_str::BexStr;

    fn resolve_index(index: i64, len: usize) -> Option<usize> {
        let len_i64 = i64::try_from(len).ok()?;
        let i = if index < 0 { index + len_i64 } else { index };
        usize::try_from(i).ok().filter(|&i| i < len)
    }

    fn resolve_slice_bound(bound: i64, len: usize) -> usize {
        let len_i64 = i64::try_from(len).unwrap_or(i64::MAX);
        let resolved = if bound < 0 { bound + len_i64 } else { bound };
        usize::try_from(resolved.clamp(0, len_i64)).unwrap_or(len)
    }

    /// `s.length()`
    pub fn length(s: &BexStr) -> i64 {
        s.codepoint_len() as i64
    }

    /// `s.at(i)`
    pub fn at(s: &BexStr, i: i64) -> Option<BexStr> {
        let i = if i < 0 {
            resolve_index(i, s.codepoint_len())?
        } else {
            usize::try_from(i).ok()?
        };
        s.char_get(i)
    }

    /// `s.code_point_at(i)`
    pub fn code_point_at(s: &BexStr, i: i64) -> Option<i64> {
        let i = if i < 0 {
            resolve_index(i, s.codepoint_len())?
        } else {
            usize::try_from(i).ok()?
        };
        s.char_at(i).map(|c| i64::from(u32::from(c)))
    }

    /// `s.slice(start, end)`
    pub fn slice(s: &BexStr, start: i64, end: i64) -> BexStr {
        let len = s.codepoint_len();
        let start = resolve_slice_bound(start, len);
        let end = resolve_slice_bound(end, len).max(start);
        s.char_slice(start..end)
    }

    /// `s.index_of(x)`
    pub fn index_of(s: &BexStr, x: &BexStr) -> Option<i64> {
        s.char_find(x.as_str()).map(|i| i as i64)
    }

    /// `baml.regex._index_of_from(s, x, from)` (behind `index_of` with a start)
    pub fn index_of_from(s: &BexStr, x: &BexStr, from: i64) -> Option<i64> {
        let from = usize::try_from(from).ok()?;
        s.char_find_from(x.as_str(), from).map(|i| i as i64)
    }

    /// `s.last_index_of(x)`
    pub fn last_index_of(s: &BexStr, x: &BexStr) -> Option<i64> {
        s.char_rfind(x.as_str()).map(|i| i as i64)
    }

    pub fn includes(s: &BexStr, x: &BexStr) -> bool {
        s.as_str().contains(x.as_str())
    }

    pub fn starts_with(s: &BexStr, x: &BexStr) -> bool {
        s.as_str().starts_with(x.as_str())
    }

    pub fn ends_with(s: &BexStr, x: &BexStr) -> bool {
        s.as_str().ends_with(x.as_str())
    }

    fn char_substrings(s: &BexStr) -> Vec<BexStr> {
        s.as_str()
            .char_indices()
            .map(|(start, ch)| s.byte_slice(start..start + ch.len_utf8()))
            .collect()
    }

    /// `s.chars()`
    pub fn chars(s: &BexStr) -> Vec<BexStr> {
        char_substrings(s)
    }

    /// `s.split(d)`
    pub fn split(s: &BexStr, d: &BexStr) -> Vec<BexStr> {
        let text = s.as_str();
        let d = d.as_str();
        if d.is_empty() {
            return char_substrings(s);
        }
        let base = text.as_ptr() as usize;
        text.split(d)
            .map(|part| {
                let start = part.as_ptr() as usize - base;
                s.byte_slice(start..start + part.len())
            })
            .collect()
    }

    pub fn to_lower_case(s: &BexStr) -> BexStr {
        BexStr::from(s.as_str().to_lowercase())
    }

    pub fn to_upper_case(s: &BexStr) -> BexStr {
        BexStr::from(s.as_str().to_uppercase())
    }

    pub fn repeat(s: &BexStr, n: i64) -> BexStr {
        s.repeat(usize::try_from(n.max(0)).unwrap_or(0))
    }

    /// `s.to_utf8()`
    pub fn to_utf8(s: &BexStr) -> Vec<u8> {
        s.as_bytes().to_vec()
    }

    /// `string.from_utf8(bytes)`
    pub fn from_utf8(bytes: &[u8]) -> Option<BexStr> {
        std::str::from_utf8(bytes).ok().map(BexStr::from)
    }

    /// `string.from_code_points([n])`; `None` where BAML throws (surrogates).
    pub fn from_code_point(n: i64) -> Option<BexStr> {
        let c = u32::try_from(n).ok().and_then(char::from_u32)?;
        let mut s = String::with_capacity(4);
        s.push(c);
        Some(BexStr::from(s))
    }

    /// `a + b`
    pub fn add(a: &BexStr, b: &BexStr) -> BexStr {
        BexStr::concat(a.clone(), b.clone())
    }
}

pub fn s(x: &str) -> BexStr {
    BexStr::from(x)
}
