//! `string`: the value type and the operations the language defines on it.
//!
//! [`Str`] wraps [`bex_str::BexStr`], the VM's own string: inline for short
//! text, otherwise reference counted with a cached code-point count, and with
//! deferred (rope) concatenation. The helpers here pin down the semantics the
//! language exposes — `length` counts Unicode code points, comparison is by
//! UTF-8 bytes — and are thin wrappers so there is one implementation.

use std::{borrow::Borrow, cmp::Ordering, fmt, ops::Deref};

use bex_str::BexStr;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{Int63, int_from_usize};

/// A BAML `string`. Dereferences to [`BexStr`] (`as_str`, `len`,
/// `char_count`, ...). `Display` prints the raw text; `Debug` prints it as a
/// quoted, escaped Rust string literal. `Ord` is byte order.
#[repr(transparent)]
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Str(BexStr);

impl Default for Str {
    fn default() -> Self {
        Self::empty()
    }
}

impl Str {
    /// `""`.
    #[inline]
    pub fn empty() -> Self {
        Self(BexStr::empty())
    }

    /// Wrap a VM string.
    #[inline]
    pub fn new(inner: BexStr) -> Self {
        Self(inner)
    }

    /// The VM string.
    #[inline]
    pub fn as_bex(&self) -> &BexStr {
        &self.0
    }

    /// Unwrap into the VM string.
    #[inline]
    pub fn into_bex(self) -> BexStr {
        self.0
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

impl Borrow<str> for Str {
    #[inline]
    fn borrow(&self) -> &str {
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

impl From<&String> for Str {
    #[inline]
    fn from(text: &String) -> Self {
        Self(BexStr::from(text.as_str()))
    }
}

impl From<BexStr> for Str {
    #[inline]
    fn from(inner: BexStr) -> Self {
        Self(inner)
    }
}

impl From<Str> for BexStr {
    #[inline]
    fn from(text: Str) -> Self {
        text.0
    }
}

impl From<Str> for String {
    #[inline]
    fn from(text: Str) -> Self {
        text.0.as_str().to_owned()
    }
}

impl PartialEq<str> for Str {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.0.as_str() == other
    }
}

impl PartialEq<&str> for Str {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        self.0.as_str() == *other
    }
}

impl PartialEq<String> for Str {
    #[inline]
    fn eq(&self, other: &String) -> bool {
        self.0.as_str() == other.as_str()
    }
}

impl Serialize for Str {
    #[inline]
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0.as_str())
    }
}

/// A JSON string; any other JSON type fails with the VM's `expected string`.
impl<'de> Deserialize<'de> for Str {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(StrVisitor)
    }
}

struct StrVisitor;

const EXPECTED_STRING: &str = "expected string";

fn not_a_string<E: de::Error, T>() -> Result<T, E> {
    Err(E::custom(EXPECTED_STRING))
}

impl<'de> de::Visitor<'de> for StrVisitor {
    type Value = Str;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Str, E> {
        Ok(Str::from(value))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Str, E> {
        Ok(Str::from(value))
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Str, E> {
        not_a_string()
    }

    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Str, E> {
        not_a_string()
    }

    fn visit_i128<E: de::Error>(self, _: i128) -> Result<Str, E> {
        not_a_string()
    }

    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Str, E> {
        not_a_string()
    }

    fn visit_u128<E: de::Error>(self, _: u128) -> Result<Str, E> {
        not_a_string()
    }

    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Str, E> {
        not_a_string()
    }

    fn visit_char<E: de::Error>(self, _: char) -> Result<Str, E> {
        not_a_string()
    }

    fn visit_bytes<E: de::Error>(self, _: &[u8]) -> Result<Str, E> {
        not_a_string()
    }

    fn visit_none<E: de::Error>(self) -> Result<Str, E> {
        not_a_string()
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Str, D::Error> {
        deserializer.deserialize_any(self)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Str, E> {
        not_a_string()
    }

    fn visit_newtype_struct<D: Deserializer<'de>>(self, deserializer: D) -> Result<Str, D::Error> {
        deserializer.deserialize_any(self)
    }

    fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Str, A::Error> {
        while seq.next_element::<de::IgnoredAny>()?.is_some() {}
        not_a_string()
    }

    fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Str, A::Error> {
        while map
            .next_entry::<de::IgnoredAny, de::IgnoredAny>()?
            .is_some()
        {}
        not_a_string()
    }

    fn visit_enum<A: de::EnumAccess<'de>>(self, _: A) -> Result<Str, A::Error> {
        not_a_string()
    }
}

// ─── Operations ──────────────────────────────────────────────────────────────

/// A string constant from the program text.
#[inline]
pub fn from_literal(text: &'static str) -> Str {
    Str::from(text)
}

/// Adopt a Rust `String`.
#[inline]
pub fn from_std(text: String) -> Str {
    Str::from(text)
}

/// Copy into a Rust `String`.
#[inline]
pub fn to_std(text: &Str) -> String {
    text.as_str().to_owned()
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

/// `string.length()`: the number of Unicode code points, as Python's `len`
/// counts (JavaScript's `length` counts UTF-16 units and disagrees).
#[inline]
pub fn length(text: &Str) -> Int63 {
    int_from_usize(text.char_count())
}

/// `string.char_count()`, an alias of [`length`].
#[inline]
pub fn char_count(text: &Str) -> Int63 {
    length(text)
}

/// `string.byte_length()`: the UTF-8 length.
#[inline]
pub fn byte_length(text: &Str) -> Int63 {
    int_from_usize(text.len())
}

/// `string.is_ascii()`: every byte is below `0x80`; true for `""`.
#[inline]
pub fn is_ascii(text: &Str) -> bool {
    text.as_str().is_ascii()
}

/// `string.is_empty()`.
#[inline]
pub fn is_empty(text: &Str) -> bool {
    text.0.is_empty()
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
        assert_eq!(byte_length(&from_literal("héllo")), int(6));
        assert_eq!(length(&from_literal("😀")), int(1));
        assert_eq!(byte_length(&from_literal("😀")), int(4));
        assert_eq!(length(&from_literal("")), int(0));
        assert_eq!(char_count(&from_literal("日本語")), int(3));
        // Long enough to leave the inline representation.
        let long = from_std("é".repeat(100));
        assert_eq!(length(&long), int(100));
        assert_eq!(byte_length(&long), int(200));
    }

    #[test]
    fn concat_and_eq_round_trip() {
        let a = from_literal("foo");
        let b = from_std("bar".to_owned());
        let ab = concat(&a, &b);
        assert_eq!(to_std(&ab), "foobar");
        assert!(eq(&ab, &from_literal("foobar")));
        assert!(!eq(&ab, &a));
        assert!(eq(&concat(&a, &Str::empty()), &a));
        let long = from_std("x".repeat(80));
        let longer = concat(&long, &long);
        assert_eq!(length(&longer), int(160));
        assert!(eq(&longer, &from_std("x".repeat(160))));
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
    fn ascii_and_empty_predicates() {
        assert!(is_ascii(&from_literal("")));
        assert!(is_ascii(&from_literal("abc 123\n")));
        assert!(!is_ascii(&from_literal("héllo")));
        assert!(is_empty(&Str::empty()));
        assert!(!is_empty(&from_literal(" ")));
    }

    #[test]
    fn std_traits() {
        let s = Str::from(String::from("a \"q\"\n"));
        assert_eq!(s.to_string(), "a \"q\"\n");
        assert_eq!(format!("{s:?}"), r#""a \"q\"\n""#);
        assert_eq!(s, "a \"q\"\n");
        assert_eq!(s, String::from("a \"q\"\n"));
        assert_eq!(Str::from(&String::from("x")), Str::from("x"));
        assert_eq!(String::from(Str::from("y")), "y");
        assert_eq!(Str::default(), Str::empty());
        assert_eq!(Str::new(BexStr::from("z")).into_bex(), BexStr::from("z"));
        assert_eq!(s.as_bex().as_str(), s.as_str());
        let mut sorted = vec![Str::from("b"), Str::from("a")];
        sorted.sort();
        assert_eq!(sorted, [Str::from("a"), Str::from("b")]);
        // `Borrow<str>` lets a `Str` key be looked up by `&str`.
        assert_eq!(<Str as Borrow<str>>::borrow(&sorted[0]), "a");
    }
}
