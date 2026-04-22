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

// Raw strings must NOT unescape — they should round-trip byte-for-byte.
function raw_keeps_backslash_n() -> string { #"a\nb"# }
function raw_keeps_quote() -> string { ##"""##  }
"####;

    macro_rules! run_str {
        ($entry:expr) => {
            match baml_test!(baml: SOURCE, entry: $entry).result {
                Ok(BexExternalValue::String(s)) => s,
                other => panic!("expected string result from {}, got {:?}", $entry, other),
            }
        };
    }

    macro_rules! run_int {
        ($entry:expr) => {
            match baml_test!(baml: SOURCE, entry: $entry).result {
                Ok(BexExternalValue::Int(n)) => n,
                other => panic!("expected int result from {}, got {:?}", $entry, other),
            }
        };
    }

    #[tokio::test]
    async fn quoted_string_escape_sequences_decode_to_exact_bytes() {
        // Exact-value checks (not just length) — a length of 3 could match
        // by accident with a different decoding bug.
        assert_eq!(run_str!("escaped_newline"), "a\nb");
        assert_eq!(run_str!("lone_newline"), "\n");
        assert_eq!(run_str!("escaped_tab"), "a\tb");
        assert_eq!(run_str!("escaped_cr"), "a\rb");
        assert_eq!(run_str!("escaped_backslash"), "a\\b");
        assert_eq!(run_str!("escaped_quote"), "a\"b");
    }

    #[tokio::test]
    async fn quoted_string_escape_lengths_match() {
        // Pain-report repros: these are what users saw at the CLI.
        assert_eq!(run_int!("escaped_newline_length"), 3);
        assert_eq!(run_int!("lone_newline_length"), 1);
        assert_eq!(run_int!("escaped_tab_length"), 3);
        assert_eq!(run_int!("escaped_backslash_length"), 3);
        assert_eq!(run_int!("escaped_quote_length"), 3);
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

    #[tokio::test]
    async fn raw_strings_do_not_decode_escapes() {
        // Raw strings are the documented workaround; they must keep
        // backslashes and quotes verbatim or the workaround breaks too.
        assert_eq!(run_str!("raw_keeps_backslash_n"), "a\\nb");
        assert_eq!(run_str!("raw_keeps_quote"), "\"");
    }
}
