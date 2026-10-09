//! `string`: the value type and the operations the language defines on it.
//!
//! [`Str`] wraps [`bex_str::BexStr`], the VM's own string: inline for short
//! text, otherwise reference counted with a cached code-point count, and with
//! deferred (rope) concatenation. The helpers here pin down the semantics the
//! language exposes: `length` counts Unicode code points, comparison is by
//! UTF-8 bytes.

use std::{cmp::Ordering, fmt, ops::Deref};

use bex_str::BexStr;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{Int63, int_from_usize};

/// A BAML `string`. Dereferences to [`BexStr`] (`as_str`, `len`,
/// `char_count`, ...). `Display` prints the raw text; `Debug` prints it as a
/// quoted, escaped Rust string literal. `Ord` is byte order.
#[repr(transparent)]
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Str(BexStr);

impl Str {
    /// `""`.
    #[inline]
    pub fn empty() -> Self {
        Self(BexStr::empty())
    }
}

impl Default for Str {
    fn default() -> Self {
        Self::empty()
    }
}

impl Deref for Str {
    type Target = BexStr;

    #[inline]
    fn deref(&self) -> &BexStr {
        &self.0
    }
}

impl AsRef<str> for Str {
    #[inline]
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for Str {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.as_str())
    }
}

impl fmt::Debug for Str {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.0.as_str(), f)
    }
}

impl From<String> for Str {
    #[inline]
    fn from(text: String) -> Self {
        Self(BexStr::from(text))
    }
}

impl From<&str> for Str {
    #[inline]
    fn from(text: &str) -> Self {
        Self(BexStr::from(text))
    }
}

impl From<BexStr> for Str {
    #[inline]
    fn from(inner: BexStr) -> Self {
        Self(inner)
    }
}

impl Serialize for Str {
    #[inline]
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0.as_str())
    }
}

impl<'de> Deserialize<'de> for Str {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::from)
    }
}

// ─── Operations ──────────────────────────────────────────────────────────────

/// A string constant from the program text.
#[inline]
pub fn from_literal(text: &'static str) -> Str {
    Str::from(text)
}

/// `left + right`. O(1) when either side is long: `BexStr::concat` builds a
/// rope that flattens on first byte access, as the VM's `Add` opcode does.
#[inline]
pub fn concat(left: &Str, right: &Str) -> Str {
    Str(BexStr::concat(left.0.clone(), right.0.clone()))
}

/// `left == right`: equal UTF-8 bytes.
#[inline]
pub fn eq(left: &Str, right: &Str) -> bool {
    left == right
}

/// `<`, `<=`, `>`, `>=`: lexicographic order on UTF-8 bytes, which is also
/// code-point order. Not locale aware.
#[inline]
pub fn cmp(left: &Str, right: &Str) -> Ordering {
    left.cmp(right)
}

/// `string.length()` and `char_count()`: the number of Unicode code points,
/// as Python's `len` counts (JavaScript's `length` counts UTF-16 units and
/// disagrees).
#[inline]
pub fn length(text: &Str) -> Int63 {
    int_from_usize(text.char_count())
}

/// `string.is_ascii()`: every byte is below `0x80`; true for `""`.
#[inline]
pub fn is_ascii(text: &Str) -> bool {
    text.as_str().is_ascii()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(value: i64) -> Int63 {
        Int63::new(value).unwrap()
    }

    #[test]
    fn length_counts_code_points_not_bytes() {
        assert_eq!(length(&from_literal("hello")), int(5));
        assert_eq!(length(&from_literal("héllo")), int(5));
        assert_eq!(length(&from_literal("😀")), int(1));
        assert_eq!(length(&from_literal("")), int(0));
        // Long enough to leave the inline representation.
        let long = Str::from("é".repeat(100));
        assert_eq!(length(&long), int(100));
        assert_eq!(long.len(), 200, "bytes");
    }

    #[test]
    fn concat_and_eq_round_trip() {
        let a = from_literal("foo");
        let b = Str::from("bar".to_owned());
        let ab = concat(&a, &b);
        assert_eq!(ab.as_str(), "foobar");
        assert!(eq(&ab, &from_literal("foobar")));
        assert!(!eq(&ab, &a));
        assert!(eq(&concat(&a, &Str::empty()), &a));
        let long = Str::from("x".repeat(80));
        let longer = concat(&long, &long);
        assert_eq!(length(&longer), int(160));
        assert!(eq(&longer, &Str::from("x".repeat(160))));
    }

    #[test]
    fn cmp_is_byte_order() {
        let s = |t: &'static str| from_literal(t);
        assert_eq!(cmp(&s("a"), &s("b")), Ordering::Less);
        assert_eq!(cmp(&s("b"), &s("a")), Ordering::Greater);
        assert_eq!(cmp(&s("a"), &s("a")), Ordering::Equal);
        assert_eq!(cmp(&s(""), &s("a")), Ordering::Less);
        assert_eq!(cmp(&s("a"), &s("ab")), Ordering::Less);
        // Uppercase sorts before lowercase in ASCII; `é` (0xC3 0xA9) after `z`.
        assert_eq!(cmp(&s("Z"), &s("a")), Ordering::Less);
        assert_eq!(cmp(&s("z"), &s("é")), Ordering::Less);
        // Byte order is code-point order: U+FF5E (3 bytes) < U+1F600 (4 bytes).
        assert_eq!(cmp(&s("～"), &s("😀")), Ordering::Less);
    }

    #[test]
    fn ascii_predicate() {
        assert!(is_ascii(&from_literal("")));
        assert!(is_ascii(&from_literal("abc 123\n")));
        assert!(!is_ascii(&from_literal("héllo")));
    }

    #[test]
    fn std_traits() {
        let s = Str::from(String::from("a \"q\"\n"));
        assert_eq!(s.to_string(), "a \"q\"\n");
        assert_eq!(format!("{s:?}"), r#""a \"q\"\n""#);
        assert_eq!(s.as_ref(), "a \"q\"\n");
        assert_eq!(Str::default(), Str::empty());
        assert_eq!(Str::from(BexStr::from("z")).as_str(), "z");
        let mut sorted = vec![Str::from("b"), Str::from("a")];
        sorted.sort();
        assert_eq!(sorted, [Str::from("a"), Str::from("b")]);
    }
}
