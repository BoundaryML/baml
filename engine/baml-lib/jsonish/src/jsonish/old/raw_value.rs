use std::fmt::Debug;

use super::parse_state::ParseState;

#[derive(PartialEq, Eq)]
pub(super) enum JsonishValue<'a> {
    // Any container type that may have other ish values inside.
    Stringish(&'a str),

    // Once parsed, it will be one of the following
    Null(&'a str),
    Bool(&'a str, bool),
    String(&'a str, String), // This is an escaped string (i.e. \\n -> \n)
    Number(&'a str, N),
    Array(&'a str, Vec<&'a JsonishValue<'a>>),
    Object(&'a str, Vec<(&'a JsonishValue<'a>, &'a JsonishValue<'a>)>),
}

impl<'a> Debug for JsonishValue<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsonishValue::Stringish(raw) => write!(f, "Stringish(raw: `{}`)", raw),
            JsonishValue::Null(raw) => write!(f, "Null(raw: `{}`)", raw),
            JsonishValue::Bool(raw, b) => write!(f, "Bool(raw: `{}`, {})", raw, b),
            JsonishValue::String(raw, s) => write!(f, "String({})", s),
            JsonishValue::Number(raw, n) => write!(f, "Number(raw: `{}`, {:?})", raw, n),
            JsonishValue::Array(raw, arr) => write!(
                f,
                "Array({}, [\n{}\n])",
                raw,
                arr.iter()
                    .enumerate()
                    .map(|(idx, i)| format!("  {idx} - {:?}", i))
                    .collect::<Vec<_>>()
                    .join(",\n")
            ),
            JsonishValue::Object(raw, obj) => write!(
                f,
                "Object({}, {{\n{}\n}})",
                raw,
                obj.iter()
                    .map(|(k, v)| format!("  {:?}: {:?}", k, v))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) enum N {
    PosInt(u64),
    // Always less than 0
    NegInt(i64),
    Float(f64),
}

impl PartialEq for N {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (N::PosInt(a), N::PosInt(b)) => a == b,
            (N::NegInt(a), N::NegInt(b)) => a == b,
            (N::Float(a), N::Float(b)) => a == b,
            _ => false,
        }
    }
}

// Implementing Eq is fine since any float values are always finite.
impl Eq for N {}

impl<'a> JsonishValue<'a> {
    pub fn is_ish(&self) -> bool {
        matches!(self, JsonishValue::Stringish(_))
    }

    pub fn as_null(&'a self, state: &'a ParseState<'a>) -> Option<()> {
        match state.resolve_ish_value(self) {
            JsonishValue::Null(_) => Some(()),
            _ => None,
        }
    }

    pub fn as_bool(&'a self, state: &'a ParseState<'a>) -> Option<bool> {
        match state.resolve_ish_value(self) {
            JsonishValue::Bool(_, b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_number(&'a self, state: &'a ParseState<'a>) -> Option<&N> {
        match state.resolve_ish_value(self) {
            JsonishValue::Number(_, n) => Some(n),
            _ => None,
        }
    }

    pub fn as_array(&'a self, state: &'a ParseState<'a>) -> Option<&[&JsonishValue]> {
        // Resolve the value if it's an ish value
        match state.resolve_ish_value(self) {
            JsonishValue::Array(_, arr) => Some(arr),
            _ => None,
        }
    }

    pub fn as_object(
        &'a self,
        state: &'a ParseState<'a>,
    ) -> Option<&Vec<(&JsonishValue, &JsonishValue)>> {
        // Resolve the value if it's an ish value
        match state.resolve_ish_value(self) {
            JsonishValue::Object(_, obj) => Some(obj),
            _ => None,
        }
    }

    pub(super) fn as_string(&'a self, state: &'a ParseState<'a>) -> Option<String> {
        // Resolve the value if it's an ish value
        match state.resolve_ish_value(self) {
            JsonishValue::String(_, parsed) => Some(parsed.clone()),
            _ => None,
        }
    }

    pub(super) fn raw_len(&self) -> usize {
        self.raw().len()
    }

    pub(super) fn raw(&self) -> &str {
        match self {
            JsonishValue::Stringish(raw) => raw,
            JsonishValue::Null(raw) => raw,
            JsonishValue::Bool(raw, _) => raw,
            JsonishValue::String(raw, _) => raw,
            JsonishValue::Number(raw, _) => raw,
            JsonishValue::Array(raw, _) => raw,
            JsonishValue::Object(raw, _) => raw,
        }
    }

    pub(super) fn from_str(s: &str) -> JsonishValue {
        let trimmed = s.trim();

        // Attempt to match boolean values or null without converting the entire string
        match trimmed.len() {
            4 => {
                let lower = &trimmed[0..1].to_lowercase(); // Convert only the first character
                match lower.as_str() {
                    "t" => {
                        if trimmed.eq_ignore_ascii_case("true") {
                            return JsonishValue::Bool(trimmed, true);
                        }
                    }
                    "n" => {
                        if trimmed.eq_ignore_ascii_case("null") {
                            return JsonishValue::Null(trimmed);
                        }
                    }
                    _ => (),
                }
            }
            5 => {
                if trimmed.eq_ignore_ascii_case("false") {
                    return JsonishValue::Bool(trimmed, false);
                }
            }
            _ => (),
        }
        // Attempt to parse as number
        if let Ok(n) = trimmed.parse::<u64>() {
            return JsonishValue::Number(trimmed, N::PosInt(n));
        } else if let Ok(n) = trimmed.parse::<i64>() {
            return JsonishValue::Number(trimmed, N::NegInt(n));
        } else if let Ok(n) = trimmed.parse::<f64>() {
            return JsonishValue::Number(trimmed, N::Float(n));
        }

        // Instead of doing fancy parsing on the string preemptively, just return it as a string. Later, we can search that string for the actual value.
        JsonishValue::Stringish(s)
    }

    // Entry point for parsing a JSON string
    pub fn from_json_str(
        s: &'a str,
        state: &'a ParseState<'a>,
    ) -> Result<&'a JsonishValue<'a>, JsonParseError> {
        let trimmed = s.trim();

        // Skip whitespace at the start (if any)
        if trimmed.is_empty() {
            return Err(JsonParseError::EmptyString);
        }

        match trimmed.chars().next() {
            Some('"') => parse_quoted_string(trimmed, state),
            Some('[') => parse_array(trimmed, state),
            Some('{') => parse_object(trimmed, state),
            // Number detection could be refined
            Some('-') => parse_unquoted_single_word(trimmed, state),
            Some(x) if x.is_alphanumeric() => parse_unquoted_single_word(trimmed, state),
            Some(_) => Err(JsonParseError::InvalidSyntax(
                "JSON is not parsable".into(),
            )),
            None => Err(JsonParseError::InvalidSyntax(
                "JSON has empty content".into(),
            )),
        }
    }
}

#[derive(Debug)]
pub enum JsonParseError {
    InvalidSyntax(String),
    EmptyString,
    InvalidBool(String),
    InvalidNull(String),
    InvalidNumber(String),
    InvalidEscape,
    UnfinishedString,
    InvalidUnicodeEscape,
    MissingValue,
    MissingKey,
}

impl From<&str> for JsonParseError {
    fn from(s: &str) -> Self {
        JsonParseError::InvalidSyntax(s.to_owned())
    }
}

fn skip_iter<I, IIter>(iter: &mut std::iter::Peekable<IIter>, val: &JsonishValue)
where
    IIter: Iterator<Item = I>,
{
    let count = val.raw_len();
    for _ in 1..count {
        iter.next();
    }
}

fn is_terminal(c: char) -> bool {
    c.is_whitespace() || c == ',' || c == ']' || c == '}' || c == ':'
}

fn unquoted_word_to_value<'a>(s: &'a str, state: &'a ParseState<'a>) -> &'a JsonishValue<'a> {
    match s {
        "true" => state.record_bool(s, true),
        "false" => state.record_bool(s, false),
        "null" => state.record_null(s),
        // Check if its a number
        _ => {
            if let Ok(n) = s.parse::<u64>() {
                state.record_number(s, N::PosInt(n))
            } else if let Ok(n) = s.parse::<i64>() {
                state.record_number(s, N::NegInt(n))
            } else if let Ok(n) = s.parse::<f64>() {
                state.record_number(s, N::Float(n))
            } else {
                state.record_string(s, s.to_string())
            }
        }
    }
}

fn parse_unquoted_single_word<'a>(
    s: &'a str,
    state: &'a ParseState<'a>,
) -> Result<&'a JsonishValue<'a>, JsonParseError> {
    let mut chars = s.char_indices().peekable();

    while let Some((i, c)) = chars.next() {
        match chars.peek() {
            Some((_, next)) if is_terminal(*next) => {
                return Ok(unquoted_word_to_value(&s[0..(i + 1)], state));
            }
            Some(_) => {}
            None => return Ok(unquoted_word_to_value(s, state)),
        }
    }

    Err(JsonParseError::InvalidSyntax(s.into()))
}

fn parse_quoted_string<'a>(
    s: &'a str,
    state: &'a ParseState<'a>,
) -> Result<&'a JsonishValue<'a>, JsonParseError> {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    let mut count = 0;

    chars.next(); // Skip the initial quote
    count += 1;

    while let Some(c) = chars.next() {
        count += 1;
        match c {
            '"' => {
                // Find the first non-space character after the quote
                // Common case, the string is finished
                let mut peekable = s[count..].chars().peekable();
                let mut next_char = peekable.peek();

                // Check for spaces and peek beyond them
                while let Some(' ') = next_char {
                    peekable.next(); // Consume the space
                    next_char = peekable.peek(); // Peek next character after spaces
                }

                // Common case, the string is finished
                // Heuristic for attempting to detect and fix unescaped quotes
                match next_char {
                    Some('\n') | Some(',') | Some('}') | Some(']') | Some(':') | None => {
                        return Ok(state.record_string(&s[0..count], result));
                    }
                    Some('/') => {
                        // Potential start of a comment
                        // Check the next character to determine if it's "//" or "/*"
                        peekable.next(); // Move past the '/'
                        match peekable.peek() {
                            Some('/') | Some('*') => {
                                return Ok(state.record_string(&s[0..count], result));
                            }
                            _ => {
                                // Not a comment start, treat as unescaped quote
                                result.push('"');
                            }
                        }
                    }
                    _ => {
                        // Almost definitely a mistake by the LLM which forgot to escape the quote. So we'll just fix it.
                        result.push('"');
                    }
                }
            }
            '\n' => {
                // Newline is not allowed in JSON strings so fix it
                result.push_str("\\n");
            }
            '\\' => {
                let next_char = chars.next().ok_or(JsonParseError::UnfinishedString)?;
                count += 1;
                match next_char {
                    '"' => result.push('"'),
                    '\\' => result.push('\\'),
                    '/' => result.push('/'),
                    'b' => result.push('\x08'),
                    'f' => result.push('\x0c'),
                    'n' => result.push('\n'),
                    'r' => result.push('\r'),
                    't' => result.push('\t'),
                    'u' => {
                        let unicode_seq = chars.by_ref().take(4).collect::<String>();
                        if unicode_seq.len() != 4 {
                            return Err(JsonParseError::InvalidUnicodeEscape);
                        }
                        let code_point = u16::from_str_radix(&unicode_seq, 16)
                            .map_err(|_| JsonParseError::InvalidUnicodeEscape)?;
                        let character = char::from_u32(code_point as u32)
                            .ok_or(JsonParseError::InvalidUnicodeEscape)?;
                        result.push(character);
                    }
                    _ => return Err(JsonParseError::InvalidEscape),
                }
            }
            _ => result.push(c),
        }
    }

    Err(JsonParseError::UnfinishedString)
}

fn parse_array<'a>(
    s: &'a str,
    state: &'a ParseState<'a>,
) -> Result<&'a JsonishValue<'a>, JsonParseError> {
    let mut elements = Vec::new();
    let mut chars = s.char_indices().peekable();
    if chars.peek().map(|&(_, c)| c) != Some('[') {
        return Err(JsonParseError::InvalidSyntax(
            "Invalid JSON array: Expected a '['".into(),
        ));
    }
    // Skip the opening bracket
    chars.next();

    let mut expect_comma = false;

    while let Some((i, c)) = chars.next() {
        match c {
            '"' => match parse_quoted_string(&s[i..], state) {
                Ok(v) => {
                    skip_iter(&mut chars, &v);
                    elements.push(v);
                    expect_comma = true;
                }
                Err(e) => return Err(e),
            },
            '[' => match parse_array(&s[i..], state) {
                Ok(v) => {
                    skip_iter(&mut chars, &v);
                    elements.push(v);
                    expect_comma = true;
                }
                Err(e) => return Err(e),
            },
            '{' => match parse_object(&s[i..], state) {
                Ok(v) => {
                    skip_iter(&mut chars, &v);
                    elements.push(v);
                    expect_comma = true;
                }
                Err(e) => return Err(e),
            },
            ']' => {
                return Ok(state.record_array(&s[0..(i + 1)], elements));
            }
            ',' => {
                if expect_comma {
                    expect_comma = false;
                } else {
                    return Err(JsonParseError::InvalidSyntax(
                        "Invalid JSON array: Expected a comma".into(),
                    ));
                }
            }
            _ if c.is_whitespace() => {
                // Do nothing
            }
            _ => {
                // assume this is some unquoted string, number, or boolean
                match parse_unquoted_single_word(&s[i..], state) {
                    Ok(v) => {
                        skip_iter(&mut chars, &v);
                        elements.push(v);
                        expect_comma = true;
                    }
                    Err(e) => return Err(e),
                }
            }
        }
    }

    Err(JsonParseError::InvalidSyntax("Invalid JSON array".into()))
}

enum ObjectExpectation {
    Key,
    Colon,
    Value,
    Comma,
}

fn get_next_expectation(e: &ObjectExpectation) -> ObjectExpectation {
    match e {
        ObjectExpectation::Key => ObjectExpectation::Colon,
        ObjectExpectation::Colon => ObjectExpectation::Value,
        ObjectExpectation::Value => ObjectExpectation::Comma,
        ObjectExpectation::Comma => ObjectExpectation::Key,
    }
}

fn parse_object<'a>(
    s: &'a str,
    state: &'a ParseState<'a>,
) -> Result<&'a JsonishValue<'a>, JsonParseError> {
    let mut keys = Vec::new();
    let mut values = Vec::new();

    let mut chars = s.char_indices().peekable();

    assert_eq!(
        chars.peek().map(|&(_, c)| c),
        Some('{'),
        "Object must start with '{{'"
    );
    // Skip the opening brace
    chars.next();

    let mut expecting = ObjectExpectation::Key;

    while let Some((i, c)) = chars.next() {
        match c {
            ',' => {
                if matches!(&expecting, ObjectExpectation::Comma) {
                    expecting = ObjectExpectation::Key;
                } else {
                    return Err(JsonParseError::InvalidSyntax(
                        "Invalid JSON object: Expected a comma".into(),
                    ));
                }
            }
            ':' => {
                if matches!(&expecting, ObjectExpectation::Colon) {
                    expecting = ObjectExpectation::Value;
                } else {
                    return Err(JsonParseError::InvalidSyntax(
                        "Invalid JSON object: Expected a colon".into(),
                    ));
                }
            }
            '"' => match parse_quoted_string(&s[i..], state) {
                Ok(v) => {
                    skip_iter(&mut chars, &v);
                    match expecting {
                        ObjectExpectation::Key => keys.push(v),
                        ObjectExpectation::Value => values.push(v),
                        _ => {
                            return Err(JsonParseError::InvalidSyntax(
                                "Invalid JSON object: Unexpected string".into(),
                            ))
                        }
                    }
                    expecting = get_next_expectation(&expecting);
                }
                Err(e) => return Err(e),
            },
            '[' => match parse_array(&s[i..], state) {
                Ok(v) => {
                    skip_iter(&mut chars, &v);
                    match expecting {
                        ObjectExpectation::Key => keys.push(v),
                        ObjectExpectation::Value => values.push(v),
                        _ => {
                            return Err(JsonParseError::InvalidSyntax(
                                "Invalid JSON object: Unexpected array".into(),
                            ))
                        }
                    }
                    expecting = get_next_expectation(&expecting);
                }
                Err(e) => return Err(e),
            },
            '{' => match parse_object(&s[i..], state) {
                Ok(v) => {
                    skip_iter(&mut chars, &v);
                    match expecting {
                        ObjectExpectation::Key => keys.push(v),
                        ObjectExpectation::Value => values.push(v),
                        _ => {
                            return Err(JsonParseError::InvalidSyntax(
                                "Invalid JSON object: Unexpected object".into(),
                            ))
                        }
                    }
                    expecting = get_next_expectation(&expecting);
                }
                Err(e) => return Err(e),
            },
            '}' => {
                // Ensure elements are even, i.e. key-value pairs
                return Ok(state.record_object(
                    &s[0..(i + 1)],
                    keys.into_iter().zip(values.into_iter()).collect(),
                ));
            }
            _ if c.is_whitespace() => {
                // Do nothing
            }
            _ => {
                // assume this is some unquoted string, number, or boolean
                match parse_unquoted_single_word(&s[i..], state) {
                    Ok(v) => {
                        skip_iter(&mut chars, &v);
                        match expecting {
                            ObjectExpectation::Key => keys.push(v),
                            ObjectExpectation::Value => values.push(v),
                            _ => {
                                return Err(JsonParseError::InvalidSyntax(
                                    "Invalid JSON object: Unexpected value".into(),
                                ))
                            }
                        }
                        expecting = get_next_expectation(&expecting);
                    }
                    Err(e) => return Err(e),
                }
            }
        }
    }

    // Handle the case of an improperly closed object or other syntax issues
    Err(JsonParseError::InvalidSyntax(
        "Invalid JSON object test".into(),
    ))
}
