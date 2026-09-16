// Regression tests for a \uXXXX escape inside a still-open JSON string fed
// to the partial (streaming) parser. The fixing parser used to re-read the
// peeked 'u' as the first hex digit, dropping the 4th digit and emitting the
// raw text "\u270" instead of the decoded "✅".

class SapPartialUnicodeResult {
    answer: string,
}

// The JSON is built from raw Unicode code points (not a string literal) so
// the compiler cannot pre-decode the ✅ escape before the exact byte
// sequence reaches the partial parser.

function sap_partial_unicode_incomplete_escaped() -> json {
    // {"answer":"✅ still streaming   — string and object left open,
    // the way a real LLM token stream looks mid-message.
    let cache = baml.sap._new_parse_cache<SapPartialUnicodeResult, SapPartialUnicodeResult>();
    let chunk = string.from_code_points([
        123, 34, 97, 110, 115, 119, 101, 114, 34, 58, 34,
        92, 117, 50, 55, 48, 53,
        32, 115, 116, 105, 108, 108, 32, 115, 116, 114, 101, 97, 109, 105, 110, 103,
    ]);
    let parsed = cache._parse_partial(chunk);
    match (parsed) {
        let result: SapPartialUnicodeResult => result.answer,
        _ => "no_yield",
    }
}

test "incomplete chunk decodes \\uXXXX escape intact" {
    assert.equal(sap_partial_unicode_incomplete_escaped(), "✅ still streaming");
}

function sap_partial_unicode_complete_escaped() -> json {
    // Same escape, but the string and object are properly closed, simulating
    // the final chunk of a stream (this takes the serde path, not the fixer).
    let cache = baml.sap._new_parse_cache<SapPartialUnicodeResult, SapPartialUnicodeResult>();
    let chunk = string.from_code_points([
        123, 34, 97, 110, 115, 119, 101, 114, 34, 58, 34,
        92, 117, 50, 55, 48, 53,
        32, 115, 116, 105, 108, 108, 32, 115, 116, 114, 101, 97, 109, 105, 110, 103,
        34, 125,
    ]);
    let parsed = cache._parse_partial(chunk);
    match (parsed) {
        let result: SapPartialUnicodeResult => result.answer,
        _ => "no_yield",
    }
}

test "complete chunk decodes \\uXXXX escape intact" {
    assert.equal(sap_partial_unicode_complete_escaped(), "✅ still streaming");
}

function sap_partial_unicode_incomplete_literal() -> json {
    // Same open-ended JSON shape, but with the literal ✅ character
    // (code point 9989) instead of an escape sequence.
    let cache = baml.sap._new_parse_cache<SapPartialUnicodeResult, SapPartialUnicodeResult>();
    let chunk = string.from_code_points([
        123, 34, 97, 110, 115, 119, 101, 114, 34, 58, 34,
        9989,
        32, 115, 116, 105, 108, 108, 32, 115, 116, 114, 101, 97, 109, 105, 110, 103,
    ]);
    let parsed = cache._parse_partial(chunk);
    match (parsed) {
        let result: SapPartialUnicodeResult => result.answer,
        _ => "no_yield",
    }
}

test "incomplete chunk keeps literal checkmark intact" {
    assert.equal(sap_partial_unicode_incomplete_literal(), "✅ still streaming");
}
