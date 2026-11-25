//! Reading JSON text into a `json` value, and following a selector into
//! one.
//!
//! The scanner accepts what the interpreter's accepts: RFC 8259, nested
//! less than [`MAX_DEPTH`] deep, every number with a fraction or exponent
//! a finite `float`. A scan that fails says only why it stopped: the
//! message is `serde_json`'s, found by reading the text again, as the
//! interpreter finds it.

use super::{MAX_DEPTH, Node, object_get, out::Out, string_keyed};
use crate::{
    describe::Types,
    heap::{Value, load},
    layout::{ARRAY_LENGTH, TAG_BOX_BOOL, TAG_BOX_FLOAT, TAG_BOX_INT},
};

/// Why a scan stopped.
pub(super) enum Stop {
    /// The text is not JSON.
    Syntax,
    /// A number with a fraction or exponent is past `float`: where its
    /// token ends.
    Float(usize),
}

struct Parser<'t> {
    src: &'t str,
    pos: usize,
    depth: usize,
    out: Out,
    /// The decoded text of the last string that held an escape.
    scratch: String,
    /// The values of the containers being read, innermost last.
    items: Vec<usize>,
}

/// Build the `json` value `text` holds in `out`: the value, zero while
/// counting, and `out` as the build left it.
pub(super) fn parse(text: &str, out: Out) -> Result<(usize, Out), Stop> {
    let mut parser = Parser {
        src: text,
        pos: 0,
        depth: 0,
        out,
        scratch: String::new(),
        items: Vec::new(),
    };
    let value = parser.value()?;
    parser.skip_ws();
    if parser.pos != text.len() {
        return Err(Stop::Syntax);
    }
    Ok((value, parser.out))
}

impl Parser<'_> {
    fn bytes(&self) -> &[u8] {
        self.src.as_bytes()
    }

    fn byte(&self, at: usize) -> Option<u8> {
        self.bytes().get(at).copied()
    }

    fn skip_ws(&mut self) {
        while let Some(b' ' | b'\n' | b'\t' | b'\r') = self.byte(self.pos) {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_ws();
        self.byte(self.pos)
    }

    fn literal(&mut self, word: &str) -> Result<(), Stop> {
        if self.bytes()[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            Ok(())
        } else {
            Err(Stop::Syntax)
        }
    }

    fn open(&mut self) -> Result<(), Stop> {
        self.depth += 1;
        if self.depth >= MAX_DEPTH {
            return Err(Stop::Syntax);
        }
        self.pos += 1;
        Ok(())
    }

    fn value(&mut self) -> Result<usize, Stop> {
        match self.peek().ok_or(Stop::Syntax)? {
            b'"' => {
                let span = self.string_span()?;
                Ok(self.string(span))
            }
            b'[' => {
                self.open()?;
                let start = self.items.len();
                loop {
                    let first = self.items.len() == start;
                    match self.peek() {
                        Some(b']') => break,
                        Some(b',') if !first => self.pos += 1,
                        Some(_) if first => {}
                        _ => return Err(Stop::Syntax),
                    }
                    let item = self.value()?;
                    self.items.push(item);
                }
                self.pos += 1;
                self.depth -= 1;
                // SAFETY: each item is zero or an object built here.
                let items = self.items[start..]
                    .iter()
                    .map(|item| unsafe { Value::reference(*item) });
                let array = self.out.array(items);
                self.items.truncate(start);
                Ok(array)
            }
            b'{' => {
                self.open()?;
                let start = self.items.len();
                loop {
                    let first = self.items.len() == start;
                    match self.peek() {
                        Some(b'}') => break,
                        Some(b',') if !first => self.pos += 1,
                        Some(b'"') if first => {}
                        _ => return Err(Stop::Syntax),
                    }
                    if self.peek() != Some(b'"') {
                        return Err(Stop::Syntax);
                    }
                    let span = self.string_span()?;
                    if self.peek() != Some(b':') {
                        return Err(Stop::Syntax);
                    }
                    self.pos += 1;
                    let key = self.string(span);
                    self.items.push(key);
                    let value = self.value()?;
                    self.items.push(value);
                }
                self.pos += 1;
                self.depth -= 1;
                // SAFETY: each value is zero or an object built here.
                let entries = self.items[start..]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|[key, value]| (*key, unsafe { Value::reference(*value) }));
                let map = self.out.map(entries);
                self.items.truncate(start);
                Ok(map)
            }
            b't' => {
                self.literal("true")?;
                Ok(self.out.boxed(TAG_BOX_BOOL, 1))
            }
            b'f' => {
                self.literal("false")?;
                Ok(self.out.boxed(TAG_BOX_BOOL, 0))
            }
            b'n' => self.literal("null").map(|()| 0),
            _ => self.number(),
        }
    }

    /// The string a span holds, as an object.
    fn string(&mut self, span: Option<(usize, usize)>) -> usize {
        match span {
            Some((start, end)) => self.out.string(&self.src[start..end]),
            None => self.out.string(&self.scratch),
        }
    }

    /// Read a string, from its opening quote: where its text is when it
    /// holds no escape, and otherwise `None`, the text decoded in
    /// `scratch`.
    fn string_span(&mut self) -> Result<Option<(usize, usize)>, Stop> {
        self.pos += 1;
        let start = self.pos;
        loop {
            match self.byte(self.pos) {
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(Some((start, self.pos - 1)));
                }
                Some(b'\\') => break,
                Some(0..=0x1f) | None => return Err(Stop::Syntax),
                Some(_) => self.pos += 1,
            }
        }
        self.scratch.clear();
        self.scratch.push_str(&self.src[start..self.pos]);
        loop {
            match self.byte(self.pos) {
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(None);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    self.escape()?;
                }
                Some(0..=0x1f) | None => return Err(Stop::Syntax),
                Some(_) => {
                    let run = self.pos;
                    while let Some(byte) = self.byte(self.pos)
                        && !matches!(byte, b'"' | b'\\' | 0..=0x1f)
                    {
                        self.pos += 1;
                    }
                    self.scratch.push_str(&self.src[run..self.pos]);
                }
            }
        }
    }

    fn escape(&mut self) -> Result<(), Stop> {
        let byte = self.byte(self.pos).ok_or(Stop::Syntax)?;
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
                0xDC00..=0xDFFF => return Err(Stop::Syntax),
                lead @ 0xD800..=0xDBFF => {
                    if !self.bytes()[self.pos..].starts_with(b"\\u") {
                        return Err(Stop::Syntax);
                    }
                    self.pos += 2;
                    let trail = self.hex4()?;
                    if !(0xDC00..=0xDFFF).contains(&trail) {
                        return Err(Stop::Syntax);
                    }
                    char::from_u32(0x1_0000 + ((lead - 0xD800) << 10 | (trail - 0xDC00)))
                        .ok_or(Stop::Syntax)?
                }
                code => char::from_u32(code).ok_or(Stop::Syntax)?,
            },
            _ => return Err(Stop::Syntax),
        };
        self.scratch.push(decoded);
        Ok(())
    }

    fn hex4(&mut self) -> Result<u32, Stop> {
        let digits = self
            .bytes()
            .get(self.pos..self.pos + 4)
            .ok_or(Stop::Syntax)?;
        let mut code = 0;
        for &digit in digits {
            code = code * 16 + char::from(digit).to_digit(16).ok_or(Stop::Syntax)?;
        }
        self.pos += 4;
        Ok(code)
    }

    fn digits(&mut self) -> bool {
        let from = self.pos;
        while let Some(b'0'..=b'9') = self.byte(self.pos) {
            self.pos += 1;
        }
        self.pos > from
    }

    fn number(&mut self) -> Result<usize, Stop> {
        let start = self.pos;
        if self.byte(self.pos) == Some(b'-') {
            self.pos += 1;
        }
        match self.byte(self.pos) {
            Some(b'0') => {
                self.pos += 1;
                // One leading zero, and only alone.
                if let Some(b'0'..=b'9') = self.byte(self.pos) {
                    return Err(Stop::Syntax);
                }
            }
            Some(b'1'..=b'9') => {
                self.digits();
            }
            _ => return Err(Stop::Syntax),
        }
        let mut integral = true;
        if self.byte(self.pos) == Some(b'.') {
            integral = false;
            self.pos += 1;
            if !self.digits() {
                return Err(Stop::Syntax);
            }
        }
        if let Some(b'e' | b'E') = self.byte(self.pos) {
            integral = false;
            self.pos += 1;
            if let Some(b'+' | b'-') = self.byte(self.pos) {
                self.pos += 1;
            }
            if !self.digits() {
                return Err(Stop::Syntax);
            }
        }
        let token = &self.src[start..self.pos];
        if integral {
            // One past an `int` is a `bigint`, as in the interpreter.
            return Ok(match token.parse::<i64>() {
                Ok(int) => self.out.boxed(TAG_BOX_INT, int as usize),
                Err(_) => self.out.bigint(&token.parse().map_err(|_| Stop::Syntax)?),
            });
        }
        match token.parse::<f64>() {
            Ok(float) if float.is_finite() => {
                Ok(self.out.boxed(TAG_BOX_FLOAT, float.to_bits() as usize))
            }
            _ => Err(Stop::Float(self.pos)),
        }
    }
}

/// A JSON value read and dropped. Unlike `serde::de::IgnoredAny`, which
/// `serde_json` skips a value for without decoding its strings or counting
/// its depth, this visits every value, so every syntax error is found.
struct Check;

impl<'de> serde::Deserialize<'de> for Check {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(Check)
    }
}

impl<'de> serde::de::Visitor<'de> for Check {
    type Value = Check;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("any JSON value")
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
        while map.next_entry::<Check, Check>()?.is_some() {}
        Ok(Check)
    }
}

/// The message of the `ParseError` for `text`, which a scan refused.
pub(super) fn syntax_message(text: &str, stop: &Stop) -> String {
    if let Err(error) = serde_json::from_str::<Check>(text) {
        return error.to_string();
    }
    match stop {
        Stop::Float(end) => {
            let before = &text[..(*end).min(text.len())];
            let line_start = before.rfind('\n').map_or(0, |newline| newline + 1);
            let line = before.matches('\n').count() + 1;
            let column = before[line_start..].chars().count();
            format!("number out of range at line {line} column {column}")
        }
        // `serde_json` refuses all the scanner does.
        Stop::Syntax => "invalid JSON".to_owned(),
    }
}

/// The node `selector` picks out of the `json` value `json`, as the slot
/// that holds it, or the message of the `PathError`.
///
/// # Safety
///
/// `json` is `null` or refers to a live object, and nothing is allocated
/// while the result is in use.
pub(super) unsafe fn select(types: &Types, json: Value, selector: &str) -> Result<Value, String> {
    if !(selector.starts_with('.') || selector.starts_with('[')) {
        return Err(format!(
            "selector must start with '.' or '[' (jq style), got '{selector}'"
        ));
    }
    let field = |current: Value, key: &str| {
        // SAFETY: the caller's contract.
        let found = unsafe {
            match Node::of(types, current) {
                Node::Object(map) if string_keyed(map) => Some(object_get(map, key)),
                _ => None,
            }
        };
        match found {
            Some(Some(value)) => Ok(value),
            Some(None) => Err(format!("missing field '{key}'")),
            None => Err(format!("field access '.{key}' on a non-object")),
        }
    };
    let chars: Vec<char> = selector.chars().collect();
    let mut current = json;
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '.' {
            i += 1;
            let start = i;
            while i < chars.len() && chars[i] != '.' && chars[i] != '[' {
                i += 1;
            }
            if i > start {
                let key: String = chars[start..i].iter().collect();
                current = field(current, &key)?;
            }
            continue;
        }
        if chars[i] != '[' {
            return Err(format!("expected '.' or '[' at offset {i}"));
        }
        i += 1;
        let Some(&first) = chars.get(i) else {
            return Err("unterminated '[' segment".to_owned());
        };
        if first == '"' || first == '\'' {
            i += 1;
            let mut key = String::new();
            let mut closed = false;
            while let Some(&ch) = chars.get(i) {
                if ch == '\\' {
                    let Some(&escaped) = chars.get(i + 1) else {
                        return Err("unterminated escape in quoted bracket key".to_owned());
                    };
                    key.push(escaped);
                    i += 2;
                    continue;
                }
                i += 1;
                if ch == first {
                    closed = true;
                    break;
                }
                key.push(ch);
            }
            if !closed || chars.get(i) != Some(&']') {
                return Err("unterminated quoted bracket key".to_owned());
            }
            i += 1;
            current = field(current, &key)?;
            continue;
        }
        let start = i;
        while i < chars.len() && chars[i] != ']' {
            i += 1;
        }
        if i >= chars.len() {
            return Err("unterminated '[' segment".to_owned());
        }
        let token: String = chars[start..i].iter().collect();
        i += 1;
        let Ok(index) = token.parse::<i64>() else {
            return Err(format!("bad bracket token '[{token}]'"));
        };
        // SAFETY: the caller's contract.
        let Node::Array(array) = (unsafe { Node::of(types, current) }) else {
            return Err(format!("index [{index}] on a non-array"));
        };
        // SAFETY: a live array has its length, and storage for it.
        let length = unsafe { load(array + ARRAY_LENGTH) };
        match usize::try_from(index) {
            Ok(at) if at < length => {
                // SAFETY: element `at` is inside the storage.
                current = unsafe { Value::load(crate::array::element(array, at)) };
            }
            _ => return Err(format!("index [{index}] out of bounds (length {length})")),
        }
    }
    Ok(current)
}
