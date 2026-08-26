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
        assert_eq!(run_str!("escaped_newline").as_str(), "a\nb");
        assert_eq!(run_str!("lone_newline").as_str(), "\n");
        assert_eq!(run_str!("escaped_tab").as_str(), "a\tb");
        assert_eq!(run_str!("escaped_cr").as_str(), "a\rb");
        assert_eq!(run_str!("escaped_backslash").as_str(), "a\\b");
        assert_eq!(run_str!("escaped_quote").as_str(), "a\"b");
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

    // ── Escaped backslash at string boundary ────────────────────────────

    #[tokio::test]
    async fn escaped_backslash_at_string_boundary() {
        // "\\" must parse as a single backslash character.
        assert_eq!(run_str!("lone_backslash").as_str(), "\\");
        // "\\\\" must parse as two backslashes.
        assert_eq!(run_str!("double_backslash").as_str(), "\\\\");
        // "a\\\\" must parse as 'a' followed by two backslashes.
        assert_eq!(run_str!("trailing_double_backslash").as_str(), "a\\\\");
    }

    #[tokio::test]
    async fn escaped_backslash_boundary_lengths() {
        assert_eq!(run_int!("lone_backslash_length"), 1);
        assert_eq!(run_int!("double_backslash_length"), 2);
        assert_eq!(run_int!("trailing_double_backslash_length"), 3);
    }

    #[tokio::test]
    async fn replace_all_with_backslash_arguments() {
        // replace_all("\\", "/") should replace each backslash with a forward slash.
        assert_eq!(run_str!("replace_backslash").as_str(), "a/b/c");
    }

    // ── Negative tests: invalid strings must still produce errors ───────

    #[test]
    fn invalid_strings_still_produce_errors() {
        use baml_compiler_diagnostics::Severity;
        use baml_db::{collect_diagnostics, testing::setup_test_db};

        let cases = [
            // Unterminated string — no closing quote at all
            (
                r#"function f() -> string { "hello }"#,
                "unterminated string",
            ),
            // Backslash at EOF — backslash escapes nothing, string never closes
            (r#"function f() -> string { "hello\"#, "backslash at EOF"),
            // Escaped quote with no closing quote — \" eats the quote
            (
                r#"function f() -> string { "hello\"}"#,
                "escaped quote eats closing quote",
            ),
        ];

        for (source, label) in &cases {
            let db = setup_test_db(source);
            let diagnostics = collect_diagnostics(&db);
            let has_error = diagnostics
                .iter()
                .any(|d| matches!(d.severity, Severity::Error));
            assert!(
                has_error,
                "Expected compilation error for case '{}', but got none.\nDiagnostics: {:?}",
                label,
                diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
            );
        }
    }
}
