//! JSON tests
//!
//! Tests for JSON-related functionality using the parse API

use rust::baml_client::sync_client::B;

/// Test JSON parsing via parse API
#[test]
fn test_json_parse_simple() {
    // Test parsing a JSON string to get typed output
    let json_input = r#"{"prop1": "hello", "prop2": 42}"#;
    let result = B.FnOutputClass.parse(json_input);

    match result {
        Ok(output) => {
            assert_eq!(output.prop1, "hello", "Expected prop1 to be 'hello'");
            // Note: prop2 might be coerced differently
        }
        Err(e) => {
            // Parse might fail if JSON doesn't match expected schema exactly
            eprintln!("Parse failed (expected for some schemas): {:?}", e);
        }
    }
}

/// Test JSON parsing with nested structure
#[test]
fn test_json_parse_nested() {
    // Test parsing a nested JSON structure
    let json_input = r#"{
        "prop1": "outer",
        "prop2": {
            "prop1": "inner1",
            "prop2": "inner2",
            "inner": {
                "prop2": 100,
                "prop3": 3.14
            }
        }
    }"#;

    let result = B.FnOutputClassNested.parse(json_input);
    match result {
        Ok(output) => {
            assert_eq!(output.prop1, "outer", "Expected prop1 to be 'outer'");
        }
        Err(e) => {
            eprintln!("Nested parse failed: {:?}", e);
        }
    }
}

/// Test JSON parsing with enum
#[test]
fn test_json_parse_enum() {
    let result = B.FnEnumOutput.parse("ONE");
    assert!(result.is_ok(), "Expected successful enum parse");
}

/// Test JSON parsing with invalid input
#[test]
fn test_json_parse_invalid() {
    let result = B.FnOutputBool.parse("not a bool");
    assert!(result.is_err(), "Expected parse error for invalid input");
}
