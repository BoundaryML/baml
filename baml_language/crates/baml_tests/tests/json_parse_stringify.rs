//! Phase 2 runtime integration tests for `baml.json.parse`, `stringify`, and
//! `stringify_pretty`.
//!
//! These tests verify:
//! - `baml.json.parse(s)` parses a JSON string and returns a `json`-typed value.
//! - `baml.json.stringify(j)` round-trips a parsed value back to a JSON string.
//! - `baml.json.stringify_pretty(j)` returns indented JSON output.
//! - `baml.json.parse("{[")` throws a `JsonParseError` (escapes as `EngineError`).
//! - `parse("1")` produces an `int`-arm value; `parse("1.0")` produces `float`.
//! - Pattern-matching against a parsed value works.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ─── 2.1 Parse an array of ints ──────────────────────────────────────────────

#[tokio::test]
async fn parse_array_of_ints() {
    let source = r#"
        function main() -> json {
            baml.json.parse("[1, 2, 3]")
        }
    "#;
    let output = baml_test!(source);
    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> baml.json.json {
        load_const "[1, 2, 3]"
        call baml.json.parse
        return
    }
    "#);
    assert!(
        matches!(
            &output.result,
            Ok(BexExternalValue::Array { items, .. }) if items.len() == 3
        ),
        "expected array of 3, got {:?}",
        output.result
    );
}

// ─── 2.2 Parse then match: array arm ─────────────────────────────────────────

#[tokio::test]
async fn parse_then_match_array() {
    let source = r#"
        function main() -> int {
            let j: json = baml.json.parse("[1, 2, 3]")
            match (j) {
                let arr: json[] => arr.length()
                _ => -1
            }
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

// ─── 2.3 Roundtrip: stringify(parse(s)) ──────────────────────────────────────

#[tokio::test]
async fn roundtrip_stringify_parse() {
    // serde_json normalizes whitespace and sorts nothing — compact output.
    let source = r#"
        function main() -> string {
            baml.json.stringify(baml.json.parse("[1,2,3]"))
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("[1,2,3]".to_string()))
    );
}

// ─── 2.4 Int/float disambiguation ────────────────────────────────────────────

#[tokio::test]
async fn parse_int_disambiguation() {
    let source = r#"
        function main() -> bool {
            match (baml.json.parse("1")) {
                let n: int => true
                _ => false
            }
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn parse_float_disambiguation() {
    let source = r#"
        function main() -> bool {
            match (baml.json.parse("1.0")) {
                let f: float => true
                _ => false
            }
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

// ─── 2.5 Parse throws on garbage input ───────────────────────────────────────

#[tokio::test]
async fn parse_throws_on_garbage() {
    let source = r#"
        function main() -> json {
            baml.json.parse("{[")
        }
    "#;
    let output = baml_test!(source);
    // The parse should fail and throw a JsonParseError (uncaught → EngineError).
    assert!(
        output.result.is_err(),
        "expected parse to throw, got {:?}",
        output.result
    );
    // The error message should mention the thrown class.
    let err_str = output.result.unwrap_err().to_string();
    assert!(
        err_str.contains("JsonParseError") || err_str.contains("baml.json"),
        "error should mention JsonParseError: {err_str}"
    );
}

// ─── 2.6 stringify_pretty ────────────────────────────────────────────────────

#[tokio::test]
async fn stringify_pretty_multiline() {
    let source = r#"
        function main() -> string {
            baml.json.stringify_pretty(baml.json.parse("{\"a\":1}"))
        }
    "#;
    let output = baml_test!(source);
    // The pretty-printed output should contain a newline.
    match &output.result {
        Ok(BexExternalValue::String(s)) => {
            assert!(s.contains('\n'), "expected multi-line output, got: {s:?}");
        }
        other => panic!("expected String result, got {other:?}"),
    }
}

// ─── 2.7 Roundtrip a nested object ───────────────────────────────────────────

#[tokio::test]
async fn roundtrip_nested_object() {
    let source = r#"
        function main() -> string {
            let s: string = "{\"x\":1,\"y\":[2,3]}"
            baml.json.stringify(baml.json.parse(s))
        }
    "#;
    let output = baml_test!(source);
    // serde_json compact output preserves key order (preserve_order feature).
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(r#"{"x":1,"y":[2,3]}"#.to_string()))
    );
}
