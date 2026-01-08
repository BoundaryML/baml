//! Parser tests - ported from test_parser_test.go
//!
//! Tests for parsing LLM responses including:
//! - Basic parsing
//! - Synchronous parsing
//! - Streaming parsing
//! - JSON extraction from text
//! - Complex structures
//! - Error handling

use rust::baml_client::sync_client::B;
use rust::baml_client::types::*;

/// Test basic parsing
#[test]
fn test_basic_parsing() {
    let response = r#"{"prop1": "value1", "prop2": 42}"#;
    let result = B.FnOutputClass.parse(response);

    assert!(
        result.is_ok(),
        "Expected successful parse, got {:?}",
        result
    );
    let output = result.unwrap();
    assert_eq!(output.prop1, "value1");
    assert_eq!(output.prop2, 42);
}

/// Test sync parsing
#[test]
fn test_sync_parsing() {
    let response = "true";
    let result = B.FnOutputBool.parse(response);

    assert!(
        result.is_ok(),
        "Expected successful parse, got {:?}",
        result
    );
    let output = result.unwrap();
    assert!(output, "Expected true");
}

/// Test stream parsing
#[test]
fn test_stream_parsing() {
    let response = r#"{"prop1": "partial", "prop2": 10}"#;
    let result = B.FnOutputClass.parse_stream(response);

    assert!(
        result.is_ok(),
        "Expected successful stream parse, got {:?}",
        result
    );
}

/// Test JSON extraction from text
#[test]
fn test_json_extraction() {
    let response = r#"
        Here is the result:
        {"prop1": "extracted", "prop2": 100}
        Some trailing text.
    "#;

    let result = B.FnOutputClass.parse(response);
    assert!(
        result.is_ok(),
        "Expected successful extraction, got {:?}",
        result
    );
}

/// Test parsing complex structures
#[test]
fn test_complex_structure_parsing() {
    let response = r#"{
        "prop1": "outer",
        "prop2": {
            "prop1": "inner1",
            "prop2": "inner2",
            "inner": {
                "prop2": 42,
                "prop3": 3.14
            }
        }
    }"#;

    let result = B.FnOutputClassNested.parse(response);
    assert!(
        result.is_ok(),
        "Expected successful complex parse, got {:?}",
        result
    );
}

/// Test parsing error handling
#[test]
fn test_parsing_error() {
    let invalid_response = "not valid json at all";
    let result = B.FnOutputClass.parse(invalid_response);

    // Should fail to parse
    assert!(result.is_err(), "Expected parse error with invalid input");
}

/// Test parsing partial streaming response
#[test]
fn test_partial_streaming_parse() {
    // Partial response (incomplete)
    let partial = r#"{"prop1": "partial"#;

    let result = B.FnOutputClass.parse_stream(partial);
    // Partial parsing may succeed with partial data or fail
    let _ = result;
}

/// Test parsing different formats
#[test]
fn test_different_formats() {
    // Test integer parsing
    let int_response = "42";
    let int_result = B.FnOutputInt.parse(int_response);
    assert!(int_result.is_ok(), "Expected successful int parse");

    // Test string list parsing
    let list_response = r#"["apple", "banana", "cherry"]"#;
    let list_result = B.FnOutputStringList.parse(list_response);
    assert!(list_result.is_ok(), "Expected successful list parse");

    // Test enum parsing
    let enum_response = "ONE";
    let enum_result = B.FnEnumOutput.parse(enum_response);
    assert!(enum_result.is_ok(), "Expected successful enum parse");
}

/// Test parsing union types
#[test]
fn test_union_type_parsing() {
    // This depends on the specific union type structure
    let response = r#"{"prop1": "string_value", "prop2": [true, false], "prop3": [1, 2, 3]}"#;

    let result = B.UnionTest_Function.parse(response);
    assert!(
        result.is_ok(),
        "Expected successful union parse, got {:?}",
        result
    );
}
