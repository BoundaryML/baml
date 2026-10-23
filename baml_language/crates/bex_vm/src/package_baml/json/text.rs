//! Reading JSON text: the scanner every text path shares, and `parse`, which
//! builds `json` values from the text in one pass.
//!
//! The scanner accepts exactly what `serde_json::from_str` accepts (RFC 8259,
//! with `serde_json`'s nesting limit), and on top of that rejects an integral
//! number past the bigint limit, as `parse` always has, and a number with a
//! fraction or exponent past `f64` (`1e400`), which JSON cannot mean as a
//! finite number. A scan that fails reports only that it failed: the error
//! message is recovered by [`syntax_error`], which re-reads the text with
//! `serde_json` so the message (and its line and column) stays the one
//! `parse` has always given.

use std::sync::Arc;

use bex_heap::TlabHolder;
use bex_vm_types::{
    MapData, Object, RealizedTy, Value,
    types::{Array, Map},
};
use rustc_hash::FxHashMap;

use crate::{BexVm, errors::VmRustFnError};

/// How deep arrays and objects may nest: an array or object 128 levels deep is
/// a syntax error. `serde_json`'s limit, which `parse` has always had.
const MAX_DEPTH: u32 = 128;

/// A decimal integer with at most this many digits always fits the bigint
/// limit (a decimal digit needs fewer than 4 bits), so only longer ones are
/// checked by parsing.
const BIGINT_SAFE_DIGITS: usize = (baml_type::MAX_BIGINT_BITS / 4) as usize;

/// The text is not valid JSON, or does not fit what the reader was asked for.
#[derive(Debug)]
pub(super) struct Stop;

/// A position to come back to: where a union member that did not match began.
#[derive(Clone, Copy)]
pub(super) struct Mark {
    pos: usize,
    depth: u32,
}

/// A cursor over JSON text.
pub(super) struct Reader<'a> {
    src: &'a str,
    pos: usize,
    depth: u32,
    /// The decoded text of the last string that held an escape.
    scratch: String,
}

/// A number token, as written.
#[derive(Clone, Copy)]
pub(super) struct Number<'a> {
    pub(super) text: &'a str,
    /// No fraction and no exponent.
    pub(super) integral: bool,
}

impl Number<'_> {
    /// The integer the token writes, when it is integral and fits `i64`.
    pub(super) fn as_i64(self) -> Option<i64> {
        if !self.integral {
            return None;
        }
        let digits = self.text.strip_prefix('-').unwrap_or(self.text);
        // Up to 18 digits always fit, so they skip the checked parse.
        if digits.len() <= 18 {
            let magnitude = digits
                .bytes()
                .fold(0i64, |acc, digit| acc * 10 + i64::from(digit - b'0'));
            return Some(if digits.len() == self.text.len() {
                magnitude
            } else {
                -magnitude
            });
        }
        self.text.parse().ok()
    }

    /// The number as a finite `f64`, or `None` when it overflows.
    pub(super) fn as_f64(self) -> Option<f64> {
        self.text
            .parse::<f64>()
            .ok()
            .filter(|float| float.is_finite())
    }

    /// The integer the token writes as a bigint, when it is integral and
    /// within the bigint limit.
    pub(super) fn as_bigint(self) -> Option<num_bigint::BigInt> {
        if !self.integral {
            return None;
        }
        match self.as_i64() {
            Some(int) => Some(num_bigint::BigInt::from(int)),
            None => super::parse_decimal_bigint(self.text),
        }
    }

    /// Whether `parse` accepts the token: an integral one must fit the bigint
    /// limit, and any other must be a finite `f64`.
    fn within_limits(self) -> bool {
        if !self.integral {
            return self.as_f64().is_some();
        }
        self.text.len() <= BIGINT_SAFE_DIGITS || super::parse_decimal_bigint(self.text).is_some()
    }
}

impl<'a> Reader<'a> {
    pub(super) fn new(src: &'a str) -> Self {
        Self {
            src,
            pos: 0,
            depth: 0,
            scratch: String::new(),
        }
    }

    pub(super) fn mark(&self) -> Mark {
        Mark {
            pos: self.pos,
            depth: self.depth,
        }
    }

    pub(super) fn reset(&mut self, mark: Mark) {
        self.pos = mark.pos;
        self.depth = mark.depth;
    }

    fn bytes(&self) -> &'a [u8] {
        self.src.as_bytes()
    }

    fn skip_ws(&mut self) {
        let bytes = self.bytes();
        while let Some(b' ' | b'\n' | b'\t' | b'\r') = bytes.get(self.pos) {
            self.pos += 1;
        }
    }

    /// The next byte after whitespace, not consumed.
    pub(super) fn peek(&mut self) -> Option<u8> {
        self.skip_ws();
        self.bytes().get(self.pos).copied()
    }

    /// The end of the document: only whitespace may follow.
    pub(super) fn finish(&mut self) -> Result<(), Stop> {
        self.skip_ws();
        if self.pos == self.src.len() {
            Ok(())
        } else {
            Err(Stop)
        }
    }

    /// Consume `word` (`true`, `false`, or `null`) at the cursor.
    fn literal(&mut self, word: &str) -> Result<(), Stop> {
        if self.src[self.pos..].starts_with(word) {
            self.pos += word.len();
            Ok(())
        } else {
            Err(Stop)
        }
    }

    pub(super) fn null(&mut self) -> Result<(), Stop> {
        match self.peek() {
            Some(b'n') => self.literal("null"),
            _ => Err(Stop),
        }
    }

    pub(super) fn bool(&mut self) -> Result<bool, Stop> {
        match self.peek() {
            Some(b't') => self.literal("true").map(|()| true),
            Some(b'f') => self.literal("false").map(|()| false),
            _ => Err(Stop),
        }
    }

    /// Consume the `[` or `{` at the cursor.
    fn open(&mut self) -> Result<(), Stop> {
        self.depth += 1;
        if self.depth >= MAX_DEPTH {
            return Err(Stop);
        }
        self.pos += 1;
        Ok(())
    }

    /// Consume the `[` that opens an array.
    pub(super) fn open_array(&mut self) -> Result<(), Stop> {
        match self.peek() {
            Some(b'[') => self.open(),
            _ => Err(Stop),
        }
    }

    /// Whether another element follows, consuming the `,` before it or the
    /// closing `]`. `first` is whether no element has been read yet.
    pub(super) fn next_element(&mut self, first: bool) -> Result<bool, Stop> {
        match self.peek() {
            Some(b']') => {
                self.pos += 1;
                self.depth -= 1;
                Ok(false)
            }
            Some(b',') if !first => {
                self.pos += 1;
                Ok(true)
            }
            Some(_) if first => Ok(true),
            _ => Err(Stop),
        }
    }

    /// Consume the `{` that opens an object.
    pub(super) fn open_object(&mut self) -> Result<(), Stop> {
        match self.peek() {
            Some(b'{') => self.open(),
            _ => Err(Stop),
        }
    }

    /// The next key and its `:`, or `None` at the closing `}`. `first` is
    /// whether no entry has been read yet.
    pub(super) fn next_key(&mut self, first: bool) -> Result<Option<&str>, Stop> {
        match self.peek() {
            Some(b'}') => {
                self.pos += 1;
                self.depth -= 1;
                return Ok(None);
            }
            Some(b',') if !first => self.pos += 1,
            Some(b'"') if first => {}
            _ => return Err(Stop),
        }
        if self.peek() != Some(b'"') {
            return Err(Stop);
        }
        let start = self.string_span()?;
        if self.peek() != Some(b':') {
            return Err(Stop);
        }
        self.pos += 1;
        Ok(Some(self.span_str(start)))
    }

    /// The string at the cursor, decoded.
    pub(super) fn string(&mut self) -> Result<&str, Stop> {
        if self.peek() != Some(b'"') {
            return Err(Stop);
        }
        let start = self.string_span()?;
        Ok(self.span_str(start))
    }

    /// Consume the string at the cursor (its opening quote is at the cursor).
    /// Returns where its text began, or `None` when it held an escape and its
    /// decoded text is in `scratch`.
    fn string_span(&mut self) -> Result<Option<(usize, usize)>, Stop> {
        let bytes = self.bytes();
        self.pos += 1;
        let start = self.pos;
        loop {
            match bytes.get(self.pos) {
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(Some((start, self.pos - 1)));
                }
                Some(b'\\') => break,
                Some(0..=0x1f) | None => return Err(Stop),
                Some(_) => self.pos += 1,
            }
        }
        self.scratch.clear();
        self.scratch.push_str(&self.src[start..self.pos]);
        loop {
            match bytes.get(self.pos) {
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(None);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    self.escape()?;
                }
                Some(0..=0x1f) | None => return Err(Stop),
                Some(_) => {
                    let run = self.pos;
                    while let Some(byte) = bytes.get(self.pos)
                        && !matches!(byte, b'"' | b'\\' | 0..=0x1f)
                    {
                        self.pos += 1;
                    }
                    self.scratch.push_str(&self.src[run..self.pos]);
                }
            }
        }
    }

    fn span_str(&self, span: Option<(usize, usize)>) -> &str {
        match span {
            Some((start, end)) => &self.src[start..end],
            None => &self.scratch,
        }
    }

    /// Decode the escape after a `\`.
    fn escape(&mut self) -> Result<(), Stop> {
        let byte = *self.bytes().get(self.pos).ok_or(Stop)?;
        self.pos += 1;
        let decoded = match byte {
            b'"' => '"',
            b'\\' => '\\',
            b'/' => '/',
            b'b' => '\u{8}',
            b'f' => '\u{c}',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'u' => match self.hex4()? {
                0xDC00..=0xDFFF => return Err(Stop),
                lead @ 0xD800..=0xDBFF => {
                    if !self.src[self.pos..].starts_with("\\u") {
                        return Err(Stop);
                    }
                    self.pos += 2;
                    let trail = self.hex4()?;
                    if !(0xDC00..=0xDFFF).contains(&trail) {
                        return Err(Stop);
                    }
                    char::from_u32(0x1_0000 + ((lead - 0xD800) << 10 | (trail - 0xDC00)))
                        .ok_or(Stop)?
                }
                code => char::from_u32(code).ok_or(Stop)?,
            },
            _ => return Err(Stop),
        };
        self.scratch.push(decoded);
        Ok(())
    }

    fn hex4(&mut self) -> Result<u32, Stop> {
        let digits = self.bytes().get(self.pos..self.pos + 4).ok_or(Stop)?;
        let mut code = 0;
        for &digit in digits {
            code = code * 16 + char::from(digit).to_digit(16).ok_or(Stop)?;
        }
        self.pos += 4;
        Ok(code)
    }

    /// The number at the cursor.
    pub(super) fn number(&mut self) -> Result<Number<'a>, Stop> {
        self.skip_ws();
        let bytes = self.bytes();
        let start = self.pos;
        let digits = |reader: &mut Self| {
            let from = reader.pos;
            while let Some(b'0'..=b'9') = bytes.get(reader.pos) {
                reader.pos += 1;
            }
            reader.pos > from
        };
        if bytes.get(self.pos) == Some(&b'-') {
            self.pos += 1;
        }
        match bytes.get(self.pos) {
            Some(b'0') => {
                self.pos += 1;
                // One leading zero, and only alone.
                if let Some(b'0'..=b'9') = bytes.get(self.pos) {
                    return Err(Stop);
                }
            }
            Some(b'1'..=b'9') => {
                digits(self);
            }
            _ => return Err(Stop),
        }
        let mut integral = true;
        if bytes.get(self.pos) == Some(&b'.') {
            integral = false;
            self.pos += 1;
            if !digits(self) {
                return Err(Stop);
            }
        }
        if let Some(b'e' | b'E') = bytes.get(self.pos) {
            integral = false;
            self.pos += 1;
            if let Some(b'+' | b'-') = bytes.get(self.pos) {
                self.pos += 1;
            }
            if !digits(self) {
                return Err(Stop);
            }
        }
        Ok(Number {
            text: &self.src[start..self.pos],
            integral,
        })
    }

    /// The keys of the object at the cursor, sorted and without repeats, its
    /// values checked and skipped.
    pub(super) fn object_keys(&mut self) -> Result<Vec<String>, Stop> {
        self.open_object()?;
        let mut keys = Vec::new();
        while let Some(key) = self.next_key(keys.is_empty())? {
            keys.push(key.to_owned());
            self.skip_value()?;
        }
        keys.sort_unstable();
        keys.dedup();
        Ok(keys)
    }

    /// Consume one value without building it, checking it as `parse` would.
    pub(super) fn skip_value(&mut self) -> Result<(), Stop> {
        match self.peek().ok_or(Stop)? {
            b'"' => {
                self.string_span()?;
            }
            b'[' => {
                self.open()?;
                let mut first = true;
                while self.next_element(first)? {
                    first = false;
                    self.skip_value()?;
                }
            }
            b'{' => {
                self.open()?;
                let mut first = true;
                while self.next_key(first)?.is_some() {
                    first = false;
                    self.skip_value()?;
                }
            }
            b't' => self.literal("true")?,
            b'f' => self.literal("false")?,
            b'n' => self.literal("null")?,
            _ => {
                if !self.number()?.within_limits() {
                    return Err(Stop);
                }
            }
        }
        Ok(())
    }
}

// ─── parse ───────────────────────────────────────────────────────────────────

/// Build the `json` value `text` holds, or [`Stop`] when it is not valid JSON
/// (see [`syntax_error`] for the message).
pub(super) fn parse(vm: &mut BexVm, text: &str) -> Result<Value, Stop> {
    let mut parser = Parser {
        reader: Reader::new(text),
        json: super::json_alias_ty(vm),
    };
    let value = parser.value(vm)?;
    parser.reader.finish()?;
    Ok(value)
}

/// Builds `json` values. Nothing it allocates can move until the native
/// returns: the collector only runs while every VM is yielded.
pub(super) struct Parser<'a> {
    pub(super) reader: Reader<'a>,
    /// The `json` alias, the element and value type of every container built.
    pub(super) json: RealizedTy,
}

impl Parser<'_> {
    /// The value at the cursor.
    pub(super) fn value(&mut self, vm: &mut BexVm) -> Result<Value, Stop> {
        let reader = &mut self.reader;
        match reader.peek().ok_or(Stop)? {
            b'"' => {
                let text = reader.string()?;
                Ok(Value::object(vm.alloc_string(text)))
            }
            b'[' => {
                reader.open()?;
                let mut items = Vec::new();
                while self.reader.next_element(items.is_empty())? {
                    items.push(self.value(vm)?);
                }
                Ok(Value::object(vm.alloc(Object::Array(Array::new(
                    self.json.clone(),
                    items,
                )))))
            }
            b'{' => {
                reader.open()?;
                let mut entries = Entries::default();
                while let Some(key) = self.reader.next_key(entries.is_empty())? {
                    let key = bex_vm_types::BexStr::from(key);
                    let value = self.value(vm)?;
                    entries.insert(key, value);
                }
                Ok(entries.alloc(vm, RealizedTy::String, self.json.clone()))
            }
            b't' | b'f' => reader.bool().map(Value::bool),
            b'n' => reader.null().map(|()| Value::NULL),
            _ => {
                let number = reader.number()?;
                number_value(vm, number)
            }
        }
    }
}

/// A number token as an untyped `json` value: an `int` when it is integral
/// and fits BAML's 63-bit `int`, a `bigint` when it is integral and larger, a
/// `float` otherwise. A float past `f64`, and an integral token past the
/// bigint limit, are [`Stop`].
pub(super) fn number_value(vm: &mut BexVm, number: Number<'_>) -> Result<Value, Stop> {
    if !number.integral {
        let float = number.as_f64().ok_or(Stop)?;
        return Ok(Value::object(vm.alloc_float(float)));
    }
    if let Some(int) = number.as_i64().and_then(Value::try_int) {
        return Ok(int);
    }
    let bigint = number.as_bigint().ok_or(Stop)?;
    Ok(Value::object(vm.alloc(Object::Bigint(Arc::new(bigint)))))
}

/// The entries of a JSON object being built, keyed by text: a repeated key
/// keeps its first position and takes its last value, as it always has.
#[derive(Default)]
pub(super) struct Entries {
    entries: Vec<(u64, bex_vm_types::BexStr, Value)>,
    /// Positions by key hash, once there are enough entries that a scan for
    /// a repeated key costs more than the index.
    index: Option<FxHashMap<u64, Vec<usize>>>,
}

/// Entries an object holds before repeated keys are found through an index.
const ENTRIES_SCANNED: usize = 32;

impl Entries {
    pub(super) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(super) fn insert(&mut self, key: bex_vm_types::BexStr, value: Value) {
        let hash = bex_vm_types::map_string_hash(&key);
        let entries = &mut self.entries;
        let seen = match &self.index {
            None => entries
                .iter()
                .position(|(seen, seen_key, _)| *seen == hash && *seen_key == key),
            Some(index) => index
                .get(&hash)
                .and_then(|positions| positions.iter().copied().find(|&i| entries[i].1 == key)),
        };
        if let Some(position) = seen {
            entries[position].2 = value;
            return;
        }
        entries.push((hash, key, value));
        if let Some(index) = &mut self.index {
            index.entry(hash).or_default().push(entries.len() - 1);
        } else if entries.len() > ENTRIES_SCANNED {
            let mut index: FxHashMap<u64, Vec<usize>> = FxHashMap::default();
            for (position, (hash, _, _)) in entries.iter().enumerate() {
                index.entry(*hash).or_default().push(position);
            }
            self.index = Some(index);
        }
    }

    /// The map of these entries, typed `map<key, value>`.
    pub(super) fn alloc(self, vm: &mut BexVm, key: RealizedTy, value: RealizedTy) -> Value {
        let mut data = MapData::new();
        for (hash, key, value) in self.entries {
            let key = Value::object(vm.alloc_string(key));
            data.insert_hashed(hash, key, value);
        }
        Value::object(vm.alloc(Object::Map(Map::new(key, value, data))))
    }
}

// ─── Errors ──────────────────────────────────────────────────────────────────

/// The `ParseError` for `text`, which a scan refused. The message is
/// `serde_json`'s (`expected value at line 1 column 3`), or names the number
/// past a limit when that is all that is wrong.
pub(super) fn syntax_error(vm: &mut BexVm, text: &str) -> VmRustFnError {
    let message = match refusal(text) {
        Refusal::Syntax(message) | Refusal::OutOfRange(message) => message,
        Refusal::BigintLimit => bigint_limit_message(),
    };
    VmRustFnError::thrown_fresh(super::throw_json_parse_error(vm, message))
}

/// Why a scan refused some text.
pub(super) enum Refusal {
    /// It is not JSON: `serde_json`'s message.
    Syntax(String),
    /// An integral number is past the bigint limit.
    BigintLimit,
    /// A number with a fraction or exponent is past `f64`: the message, with
    /// its line and column.
    OutOfRange(String),
}

/// Why a scan refused `text`.
pub(super) fn refusal(text: &str) -> Refusal {
    if let Err(message) = check(text) {
        return Refusal::Syntax(message);
    }
    // The text is JSON, so a number past a limit stopped the scan, right
    // after its token.
    let mut reader = Reader::new(text);
    if reader.skip_value().is_ok() {
        return Refusal::BigintLimit;
    }
    let end = reader.pos;
    let token = text[..end]
        .rfind(|c: char| !matches!(c, '0'..='9' | '-' | '+' | '.' | 'e' | 'E'))
        .map_or(&text[..end], |before| &text[before + 1..end]);
    if !token.contains(['.', 'e', 'E']) {
        return Refusal::BigintLimit;
    }
    let line_start = text[..end].rfind('\n').map_or(0, |newline| newline + 1);
    let line = text[..end].matches('\n').count() + 1;
    let column = text[line_start..end].chars().count();
    Refusal::OutOfRange(format!(
        "number out of range at line {line} column {column}"
    ))
}

/// `serde_json`'s message for the first syntax error in `text`, if it has one.
pub(super) fn check(text: &str) -> Result<(), String> {
    serde_json::from_str::<Check>(text)
        .map(|Check| ())
        .map_err(|error| error.to_string())
}

/// The message for an integral number past the bigint limit.
pub(super) fn bigint_limit_message() -> String {
    format!(
        "integral JSON number exceeds the {}-bit bigint limit",
        baml_type::MAX_BIGINT_BITS
    )
}

/// Reads a document and keeps nothing, so `serde_json` reports its first syntax
/// error (and its nesting limit) without building a tree.
struct Check;

impl<'de> serde::Deserialize<'de> for Check {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(Check)
    }
}

impl<'de> serde::de::Visitor<'de> for Check {
    type Value = Check;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E>(self, _: bool) -> Result<Check, E> {
        Ok(Check)
    }

    fn visit_i64<E>(self, _: i64) -> Result<Check, E> {
        Ok(Check)
    }

    fn visit_u64<E>(self, _: u64) -> Result<Check, E> {
        Ok(Check)
    }

    fn visit_f64<E>(self, _: f64) -> Result<Check, E> {
        Ok(Check)
    }

    fn visit_str<E>(self, _: &str) -> Result<Check, E> {
        Ok(Check)
    }

    fn visit_unit<E>(self) -> Result<Check, E> {
        Ok(Check)
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Check, A::Error> {
        while seq.next_element::<Check>()?.is_some() {}
        Ok(Check)
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Check, A::Error> {
        while map.next_key::<Check>()?.is_some() {
            map.next_value::<Check>()?;
        }
        Ok(Check)
    }
}

#[cfg(test)]
mod tests {
    use super::{Number, Reader, Refusal, check, refusal};

    /// Whether the scanner accepts `text` as one JSON document.
    fn scans(text: &str) -> bool {
        let mut reader = Reader::new(text);
        reader.skip_value().is_ok() && reader.finish().is_ok()
    }

    /// Whether every number in `value` with a fraction or exponent is a
    /// finite `f64`: what the scanner asks on top of `serde_json`.
    fn finite(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Number(number) => {
                let text = number.to_string();
                !text.contains(['.', 'e', 'E']) || number.as_f64().is_some()
            }
            serde_json::Value::Array(items) => items.iter().all(finite),
            serde_json::Value::Object(entries) => entries.values().all(finite),
            _ => true,
        }
    }

    fn agrees(text: &str) {
        let serde = serde_json::from_str::<serde_json::Value>(text);
        assert_eq!(
            scans(text),
            serde.as_ref().is_ok_and(finite),
            "scanner and serde_json disagree on {text:?}"
        );
        assert_eq!(
            check(text).is_ok(),
            serde.is_ok(),
            "check and serde_json disagree on {text:?}"
        );
    }

    #[test]
    fn refuses_a_number_past_f64() {
        for (text, message) in [
            ("1e400", "number out of range at line 1 column 5"),
            ("[1, -2.5E+999]", "number out of range at line 1 column 13"),
            (
                "{\n  \"a\": [0.5,\n    1e-2, 9e9999]}",
                "number out of range at line 3 column 16",
            ),
        ] {
            assert!(!scans(text), "{text:?}");
            match refusal(text) {
                Refusal::OutOfRange(found) => assert_eq!(found, message, "{text:?}"),
                _ => panic!("{text:?} is out of range"),
            }
        }
        // A tiny number is zero, and a huge integral one is a bigint.
        assert!(scans("1e-400"));
        assert!(scans(&"9".repeat(400)));
        assert!(matches!(refusal(&"9".repeat(2000)), Refusal::BigintLimit));
    }

    #[test]
    fn accepts_what_serde_json_accepts() {
        let cases = [
            "null",
            "true",
            "false",
            "0",
            "-0",
            "1",
            "-1",
            "1.5",
            "1e5",
            "1E+5",
            "1e-5",
            "-0.0e0",
            "1.0e+100",
            "123456789012345678901234567890",
            "\"\"",
            "\"a\"",
            "\"\\u00e9\"",
            "\"\\ud83d\\ude00\"",
            "\"\\\"\\\\\\/\\b\\f\\n\\r\\t\"",
            "\"é\"",
            "[]",
            "[1,2]",
            "{}",
            "{\"a\":1}",
            " [ 1 , { \"a\" : [ ] } ] ",
            "{\"a\":1,\"a\":2}",
            "\u{7f}",
            "\"\u{7f}\"",
            "",
            " ",
            "01",
            "-",
            "+1",
            "1.",
            ".5",
            "1e",
            "1e+",
            "--1",
            "[1,]",
            "[,1]",
            "{,}",
            "{\"a\":1,}",
            "{\"a\"}",
            "{\"a\":}",
            "{a:1}",
            "[1 2]",
            "\"\\x\"",
            "\"\\u12\"",
            "\"\\ud800\"",
            "\"\\udc00\"",
            "\"\\ud800\\u0041\"",
            "\"\\ud800x\"",
            "\"\u{1}\"",
            "\"\t\"",
            "tru",
            "nul",
            "true false",
            "[1]]",
            "{\"a\":1}}",
            "NaN",
            "Infinity",
            "'a'",
            "\"abc",
            "[",
            "{",
            "{\"a\":1",
            "\"\\",
            "\u{feff}1",
            "1 ",
            "\n1\n",
            "[\"a\"\n,\n\"b\"]",
            "-01",
            "0.0",
            "00",
            "1e01",
            "[{\"a\":[{\"b\":null}]}]",
            "{\"\":\"\"}",
            "[true,false,null]",
            "[-]",
            "[1e]",
        ];
        for case in cases {
            agrees(case);
        }
    }

    #[test]
    fn nests_as_deep_as_serde_json() {
        for depth in [126, 127, 128, 129] {
            agrees(&format!("{}{}", "[".repeat(depth), "]".repeat(depth)));
            agrees(&format!(
                "{}1{}",
                "{\"a\":".repeat(depth),
                "}".repeat(depth)
            ));
        }
    }

    /// Random documents and their one-byte mutations, from a seeded LCG.
    #[test]
    fn agrees_on_generated_documents() {
        const PIECES: &[&str] = &[
            "[",
            "]",
            "{",
            "}",
            ",",
            ":",
            " ",
            "\n",
            "\"",
            "\"k\"",
            "\"v\"",
            "\\",
            "\\n",
            "\\u",
            "00e9",
            "d83d",
            "1",
            "0",
            "-",
            ".",
            "e",
            "E",
            "+",
            "true",
            "false",
            "null",
            "7",
            "\"a\\\"b\"",
            "12345678901234567890",
        ];
        let mut seed: u64 = 7;
        let mut next = |n: usize| {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            usize::try_from(seed >> 33).unwrap_or(0) % n
        };
        for _ in 0..20_000 {
            let length = 1 + next(12);
            let text: String = (0..length).map(|_| PIECES[next(PIECES.len())]).collect();
            agrees(&text);
        }
        let documents = [
            "{\"a\":[1,2.5,-3e2,{\"b\":\"c\\n\"}],\"d\":null,\"e\":true}",
            "[\"\\ud83d\\ude00\",0,-0.5,{\"\":[]}]",
        ];
        for document in documents {
            for i in 0..document.len() {
                if !document.is_char_boundary(i) {
                    continue;
                }
                let deleted = format!("{}{}", &document[..i], &document[i + 1..]);
                agrees(&deleted);
                for piece in ["\"", ",", "]", "}", "x", "0", "\\"] {
                    agrees(&format!("{}{piece}{}", &document[..i], &document[i..]));
                }
            }
        }
    }

    /// The scanner reads a number token as `serde_json`'s arbitrary-precision
    /// `Number` does.
    #[test]
    fn numbers_read_as_serde_json_reads_them() {
        let tokens = [
            "0",
            "-0",
            "1",
            "-1",
            "9223372036854775807",
            "-9223372036854775808",
            "9223372036854775808",
            "4611686018427387903",
            "4611686018427387904",
            "123456789012345678",
            "1234567890123456789",
            "1.5",
            "1e5",
            "-2.5E-3",
            "1e400",
            "-1e400",
            "0.1",
            "100000000000000000000000",
        ];
        for token in tokens {
            let mut reader = Reader::new(token);
            let number: Number<'_> = reader.number().expect("a number token");
            let serde: serde_json::Number = token.parse().expect("serde reads the token");
            assert_eq!(number.as_i64(), serde.as_i64(), "as_i64 of {token}");
            assert_eq!(number.as_f64(), serde.as_f64(), "as_f64 of {token}");
        }
    }
}
