mod json_collection;
mod json_parse_state;

use anyhow::Result;
use baml_types::CompletionState;

use self::json_parse_state::JsonParseState;
use super::ParseOptions;
use crate::jsonish::{value::Fixes, Value};

/// Which quote codepoints participate in the `unescaped_quote_count` parity
/// check that gates closing an ASCII-quoted string on `,`.
///
/// `AsciiOnly` preserves today's behaviour: only ASCII `"` increments the
/// counter. `AllUnicode` additionally increments for unicode quotes
/// (see `UNICODE_QUOTE_CHARS`). The rest of the parser — opener selection,
/// structural-delimiter branches, escapes — does not consult this mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteParityMode {
    AsciiOnly,
    AllUnicode,
}

/// Unicode quotes that count toward `unescaped_quote_count` under
/// `QuoteParityMode::AllUnicode`.
///
/// Only marks that function as *primary* (double-quote-level) delimiters
/// across languages are included. Single-quote-role marks — curly singles
/// (U+2018 / U+2019), the single low-9 and angle variants (U+201A, U+2039,
/// U+203A), and the CJK white corner brackets (U+300E / U+300F) — are
/// deliberately excluded:
///
/// 1. Parity counting exists to detect an unbalanced opener at the *outer*
///    delimiter level that would make a stray ASCII `"` look like a
///    closer. Single-role marks appear only *nested* inside a primary
///    quote, so they don't function at that level and counting them does
///    not disambiguate.
/// 2. U+2019 RIGHT SINGLE QUOTATION MARK is the standard typographic
///    apostrophe ("It's"). Counting it inside an ASCII-quoted string
///    makes common text like `"It's fine", …` look unbalanced and
///    prevents the real ASCII `"` from closing. CJK has the same
///    double/single distinction (「」 primary vs 『』 nested); 300E/300F
///    are excluded for the same reason.
///
/// | Language                          | Delimiters      | Codes            | Example                      |
/// |-----------------------------------|-----------------|------------------|------------------------------|
/// | English (US/UK)                   | `“ ”`           | 201C / 201D      | He said: “hello.”            |
/// | German — Gänsefüßchen             | `„ “`           | 201E / 201C      | Er sagte: „hallo.“           |
/// | German — Chevrons                 | `» «`           | 00BB / 00AB      | Er sagte: »hallo«.           |
/// | Polish                            | `„ ”`           | 201E / 201D      | Powiedział: „cześć”.         |
/// | Czech / Slovak                    | `„ “`           | 201E / 201C      | Řekl: „ahoj.“                |
/// | Hungarian                         | `„ ”`           | 201E / 201D      | Azt mondta: „szia”.          |
/// | French                            | `« »`           | 00AB / 00BB      | Il a dit : « bonjour ».      |
/// | Russian                           | `« »`           | 00AB / 00BB      | Он сказал: «привет».         |
/// | Spanish / Italian / Swiss / Greek | `« »`           | 00AB / 00BB      | Dijo: «hola».                |
/// | Swedish / Finnish                 | `” ”`           | 201D / 201D      | Han sade: ”hej.”             |
/// | Danish / Norwegian / Dutch        | `“ ”`           | 201C / 201D      | Han sagde: “hej.”            |
/// | Chinese (CN)                      | `“ ”`           | 201C / 201D      | 他说：“你好。”                 |
/// | Japanese / Chinese (TW/HK)        | `「 」`         | 300C / 300D      | 彼は「こんにちは」と言った。    |
/// | Korean                            | `“ ”` or `「 」` | 201C/D or 300C/D | 그는 “안녕”이라고 말했다.      |
/// | Hebrew                            | `״`             | 05F4             | הוא אמר: ״שלום״.              |
/// | Arabic                            | `« »`           | 00AB / 00BB      | قال: «مرحبا».                |
pub(crate) const UNICODE_QUOTE_CHARS: &[char] = &[
    '\u{00AB}', // «  LEFT-POINTING DOUBLE ANGLE QUOTATION MARK
    '\u{00BB}', // »  RIGHT-POINTING DOUBLE ANGLE QUOTATION MARK
    '\u{201C}', // "  LEFT DOUBLE QUOTATION MARK
    '\u{201D}', // "  RIGHT DOUBLE QUOTATION MARK
    '\u{201E}', // „  DOUBLE LOW-9 QUOTATION MARK
    '\u{300C}', // 「 LEFT CORNER BRACKET
    '\u{300D}', // 」 RIGHT CORNER BRACKET
    '\u{05F4}', // ״  HEBREW PUNCTUATION GERSHAYIM
];

/// Returns `true` if `s` contains at least one codepoint in
/// `UNICODE_QUOTE_CHARS`. Used by the entry cascade to skip the
/// `AllUnicode` parse pass when it would be a no-op (pure-ASCII input).
pub fn contains_unicode_quote_char(s: &str) -> bool {
    s.chars().any(|c| UNICODE_QUOTE_CHARS.contains(&c))
}

pub fn parse(
    str: &str,
    _options: &ParseOptions,
    quote_parity: QuoteParityMode,
) -> Result<Vec<(Value, Vec<Fixes>)>> {
    // Try to fix some common JSON issues
    // - Unquoted single word strings
    // - Single quoted strings
    // - Double quoted strings with badly escaped characters
    // - Numbers
    // - Numbers starting with a .
    // - Booleans
    // - Null
    // - Arrays
    // - Objects
    // - Comments
    // - Trailing commas
    // - Leading commas
    // - Unterminated comments
    // - Unterminated arrays
    // - Unterminated objects
    // - Unterminated strings

    let mut state = JsonParseState::new();

    let mut chars = str.char_indices().peekable();
    while let Some((count, c)) = chars.next() {
        let peekable = str[count + c.len_utf8()..].char_indices().peekable();
        match state.process_token(c, peekable, quote_parity) {
            Ok(increments) => {
                for _ in 0..increments {
                    chars.next();
                }
            }
            Err(e) => {
                return Err(e);
            }
        }
    }

    // If we still have a collection open, close it
    while !state.collection_stack.is_empty() {
        state.complete_collection(CompletionState::Incomplete);
    }

    // Determine what to return.

    match state.completed_values.len() {
        0 => Err(anyhow::anyhow!("No JSON objects found")),
        1 => state
            .completed_values
            .pop()
            .map(|(_name, value, fixes)| Ok(vec![(value, fixes)]))
            .unwrap_or(Err(anyhow::anyhow!("Failed to pop completed value"))),
        _ => {
            if state.completed_values.iter().all(|f| f.0 == "string") {
                // If all the values are strings, return them as an array of strings
                Ok(vec![(
                    Value::Array(
                        state
                            .completed_values
                            .into_iter()
                            .map(|f| {
                                let completion_state = f.1.completion_state().clone();
                                Value::FixedJson(f.1.into(), f.2)
                            })
                            .collect(),
                        CompletionState::Incomplete, // TODO: Is it complete?
                    ),
                    vec![Fixes::InferredArray],
                )])
            } else {
                // Filter for only objects and arrays
                let values: Vec<(Value, Vec<Fixes>)> = state
                    .completed_values
                    .into_iter()
                    .filter_map(|f| {
                        if f.0 == "Object" || f.0 == "Array" {
                            Some((f.1, f.2))
                        } else {
                            None
                        }
                    })
                    .collect();
                match values.len() {
                    0 => Err(anyhow::anyhow!("No JSON objects found")),
                    _ => Ok(values),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsonish::{ParseOptions, Value};

    #[test]
    fn test_partial_array() {
        let opts = ParseOptions::default();
        let vals = parse("[12", &opts, QuoteParityMode::AsciiOnly).unwrap();

        match vals[0].0.clone() {
            Value::Array(xs, array_cmplt) => {
                assert_eq!(xs.len(), 1);
                assert_eq!(array_cmplt, CompletionState::Incomplete);
                match &xs[0] {
                    Value::Number(n, n_cmplt) => {
                        assert_eq!(n, &serde_json::Number::from(12));
                        assert_eq!(n_cmplt, &CompletionState::Incomplete);
                    }
                    _ => panic!("Expected number"),
                }
            }
            _ => panic!("Expected array"),
        }
    }

    #[test]
    fn test_partial_object() {
        let opts = ParseOptions::default();
        let vals = parse(r#"{"a": 11, "b": 22"#, &opts, QuoteParityMode::AsciiOnly).unwrap();
        match &vals[0].0 {
            Value::Object(fields, obj_cmplt) => {
                assert_eq!(fields.len(), 2);
                assert_eq!(obj_cmplt, &CompletionState::Incomplete);
                match (&fields[0], &fields[1]) {
                    ((key_a, Value::Number(a, a_cmplt)), (key_b, Value::Number(b, b_cmplt))) => {
                        assert_eq!(key_a.as_str(), "a");
                        assert_eq!(key_b.as_str(), "b");
                        assert_eq!(a, &serde_json::Number::from(11));
                        assert_eq!(b, &serde_json::Number::from(22));
                        assert_eq!(a_cmplt, &CompletionState::Complete);
                        assert_eq!(b_cmplt, &CompletionState::Incomplete);
                    }
                    _ => panic!("Expected two numbers."),
                }
            }
            _ => panic!("Expected object"),
        }
    }

    #[test]
    fn test_partial_object_newlines() {
        let opts = ParseOptions::default();
        let vals = parse(
            "{\n \"a\": 11, \n \"b\": 22",
            &opts,
            QuoteParityMode::AsciiOnly,
        )
        .unwrap();
        match &vals[0].0 {
            Value::Object(fields, obj_cmplt) => {
                assert_eq!(fields.len(), 2);
                assert_eq!(obj_cmplt, &CompletionState::Incomplete);
                match (&fields[0], &fields[1]) {
                    ((key_a, Value::Number(a, a_cmplt)), (key_b, Value::Number(b, b_cmplt))) => {
                        assert_eq!(key_a.as_str(), "a");
                        assert_eq!(key_b.as_str(), "b");
                        assert_eq!(a, &serde_json::Number::from(11));
                        assert_eq!(b, &serde_json::Number::from(22));
                        assert_eq!(a_cmplt, &CompletionState::Complete);
                        assert_eq!(b_cmplt, &CompletionState::Incomplete);
                    }
                    _ => panic!("Expected two numbers."),
                }
            }
            _ => panic!("Expected object"),
        }
    }

    // Regression tests for the off-by-one fix in should_close_unescaped_string.
    // When the iterator exhausts without finding a structural delimiter, the
    // counter must account for the last consumed character to prevent the outer
    // loop from re-processing it. These tests fail without the fix.
    //
    // Note: The InObjectValue branch has the same bug but it only manifests
    // during streaming (multiple parse() calls on successive chunks), not in
    // single-pass parsing, so it cannot be tested at this level.

    #[test]
    fn test_partial_unquoted_key_no_char_duplication() {
        // InObjectKey: unquoted key "mykey", stream ends before ':' is found.
        // Without the fix, the off-by-one causes the last char to be
        // re-processed, creating a spurious key-value pair.
        let opts = ParseOptions::default();
        let vals = parse(r#"{mykey"#, &opts, QuoteParityMode::AsciiOnly).unwrap();
        match &vals[0].0 {
            Value::Object(fields, _) => {
                assert_eq!(fields.len(), 0, "No complete key-value pair yet");
            }
            _ => panic!("Expected object"),
        }
    }

    #[test]
    fn test_partial_unquoted_toplevel_no_char_duplication() {
        // InNothing: unquoted string at top level, stream ends without '{' or '['.
        // Without the fix, the off-by-one corrupts parsing so no value is produced.
        let opts = ParseOptions::default();
        let vals = parse("foobar", &opts, QuoteParityMode::AsciiOnly).unwrap();
        match &vals[0].0 {
            Value::String(s, cmplt) => {
                assert_eq!(
                    s.as_str(),
                    "foobar",
                    "Top-level string should be 'foobar' without duplicated chars"
                );
                assert_eq!(cmplt, &CompletionState::Incomplete);
            }
            _ => panic!("Expected string, got: {:?}", vals[0].0),
        }
    }
}
