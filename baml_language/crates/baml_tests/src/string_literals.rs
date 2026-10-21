//! Regression tests for BAML string-literal escape decoding.
//!
//! Context: "..." strings used to pass backslash sequences through literally,
//! so `"\n".length()` returned 2 and `"{\"foo\":1}"` produced wire bytes
//! containing an actual backslash before the quote (breaking JSON for any
//! HTTP callee). These tests compile and execute BAML at runtime and check
//! exact string values so any regression in the escape decoder fails here.
#[cfg(test)]
mod tests {
    use bex_engine::BexExternalValue;

    const SOURCE: &str = r####"
function escaped_newline() -> string { "a\nb" }
function lone_newline() -> string { "\n" }
function escaped_tab() -> string { "a\tb" }
function escaped_cr() -> string { "a\rb" }
function escaped_backslash() -> string { "a\\b" }
function escaped_quote() -> string { "a\"b" }

function escaped_newline_length() -> int { "a\nb".length() }
function lone_newline_length() -> int { "\n".length() }
function escaped_tab_length() -> int { "a\tb".length() }
function escaped_backslash_length() -> int { "a\\b".length() }
function escaped_quote_length() -> int { "a\"b".length() }

// Original pain-point: building a JSON body must yield bytes the wire side
// can parse. The `\"` in source must decode to a real double-quote byte.
function json_body() -> string {
  "{\"input\":\"hello\\nworld\",\"model\":\"m\"}"
}

// Escaped backslash at string boundary — the minimal repro for the \\-before-closing-quote bug.
function lone_backslash() -> string { "\\" }
function lone_backslash_length() -> int { "\\".length() }
function double_backslash() -> string { "\\\\" }
function double_backslash_length() -> int { "\\\\".length() }
function trailing_double_backslash() -> string { "a\\\\" }
function trailing_double_backslash_length() -> int { "a\\\\".length() }
function replace_backslash() -> string { "a\\b\\c".replace_all("\\", "/") }

"####;

    macro_rules! run_str {
        ($entry:expr) => {
            match baml_test!(baml: SOURCE, entry: $entry).result {
                Ok(BexExternalValue::String(s)) => s,
                other => panic!("expected string result from {}, got {:?}", $entry, other),
            }
        };
    }

    #[tokio::test]
    async fn json_body_is_parseable_by_serde() {
        // Surefire regression test for the OpenAI pain point: the string
        // produced by a `"..."` literal must be valid JSON on the wire.
        // If `\"` or `\\` regress, serde_json::from_str will fail.
        let body = run_str!("json_body");
        let parsed: serde_json::Value =
            serde_json::from_str(&body).expect("body must be valid JSON");
        assert_eq!(parsed["input"], "hello\nworld");
        assert_eq!(parsed["model"], "m");
    }
}
