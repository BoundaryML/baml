//! Basic function call tests - ported from test_functions_basic_test.go
//!
//! Tests for basic function calls including:
//! - Simple function calls
//! - Boolean output
//! - String list output
//! - Multiple arguments
//! - Enum input/output
//! - Float and Int input
//! - Literal types
//! - Optional string input

use rust::baml_client::sync_client::B;
use rust::baml_client::types::*;

/// Test basic function call - Go: TestSyncFunctionCall
#[test]
fn test_basic_function_call() {
    let arg = NamedArgsSingleClass {
        key: "key".to_string(),
        key_two: true,
        key_three: 52,
    };
    let result = B.TestFnNamedArgsSingleClass.call(&arg);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    // Go: assert.Contains(t, result, "52")
    assert!(
        output.contains("52"),
        "Expected output to contain '52', got: {}",
        output
    );
}

/// Test single bool input - Go: TestSingleBoolInput
#[test]
fn test_single_bool_input() {
    let result = B.TestFnNamedArgsSingleBool.call(true);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    // Go: assert.True(t, result == "true")
    assert_eq!(output, "true", "Expected 'true' output");
}

/// Test boolean output
#[test]
fn test_boolean_output() {
    let result = B.FnOutputBool.call("test input");
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    // Go: assert.True(t, boolResult)
    assert!(output, "Expected true output");
}

/// Test string list input - Go: TestSingleStringListInput
#[test]
fn test_string_list_input() {
    // Test with items
    let items = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let result = B.TestFnNamedArgsSingleStringList.call(&items);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    // Go: assert.Contains(t, result, "a"), etc.
    // output is Vec<String>, check if any element contains the substring
    let output_str = output.join(" ");
    assert!(output_str.contains("a"), "Expected output to contain 'a'");
    assert!(output_str.contains("b"), "Expected output to contain 'b'");
    assert!(output_str.contains("c"), "Expected output to contain 'c'");

    // Test empty list
    let empty: Vec<String> = vec![];
    let result_empty = B.TestFnNamedArgsSingleStringList.call(&empty);
    assert!(
        result_empty.is_ok(),
        "Expected successful call with empty list, got {:?}",
        result_empty
    );
    let output_empty = result_empty.unwrap();
    // Go: assert.Empty(t, result)
    assert!(
        output_empty.is_empty(),
        "Expected empty output for empty list"
    );
}

/// Test string list output
#[test]
fn test_string_list_output() {
    let result = B.FnOutputStringList.call("apple, banana, cherry");
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert!(!output.is_empty(), "Expected non-empty list");
}

/// Test multiple arguments - Go: TestMultipleArgsFunction
#[test]
fn test_multiple_args() {
    let arg = NamedArgsSingleClass {
        key: "key".to_string(),
        key_two: true,
        key_three: 52,
    };
    let arg2 = NamedArgsSingleClass {
        key: "key".to_string(),
        key_two: true,
        key_three: 64,
    };
    let result = B.TestMulticlassNamedArgs.call(&arg, &arg2);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    // Go: assert.Contains(t, result, "52") and assert.Contains(t, result, "64")
    assert!(
        output.contains("52"),
        "Expected output to contain '52', got: {}",
        output
    );
    assert!(
        output.contains("64"),
        "Expected output to contain '64', got: {}",
        output
    );
}

/// Test enum input list - Go: TestSingleEnumInput
#[test]
fn test_enum_input_list() {
    let result = B
        .TestFnNamedArgsSingleEnumList
        .call(&[NamedArgsSingleEnumList::TWO]);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    // Go: assert.Contains(t, result, "TWO")
    assert!(
        output.contains("TWO"),
        "Expected output to contain 'TWO', got: {}",
        output
    );
}

/// Test enum output
#[test]
fn test_enum_output() {
    let result = B.FnEnumOutput.call("pick the last option");
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    // Go: assert.NotEmpty(t, enumResult)
    // Output should be a valid enum variant
    match output {
        EnumOutput::ONE | EnumOutput::TWO | EnumOutput::THREE => {}
    }
}

/// Test enum input
#[test]
fn test_enum_input() {
    let result = B.FnTestNamedArgsSingleEnum.call(&NamedArgsSingleEnum::ONE);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
}

/// Test float input - Go: TestSingleFloat
#[test]
fn test_float_input() {
    let result = B.TestFnNamedArgsSingleFloat.call(3.12);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    // Go: assert.Contains(t, result, "3.12")
    assert!(
        output.contains("3.12"),
        "Expected output to contain '3.12', got: {}",
        output
    );
}

/// Test int input - Go: TestSingleInt
#[test]
fn test_int_input() {
    let result = B.TestFnNamedArgsSingleInt.call(3566);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    // Go: assert.Contains(t, result, "3566")
    assert!(
        output.contains("3566"),
        "Expected output to contain '3566', got: {}",
        output
    );
}

/// Test int output - Go: TestAllOutputTypes
#[test]
fn test_int_output() {
    let result = B.FnOutputInt.call("test input");
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    // Go: assert.Equal(t, int64(5), intResult)
    assert_eq!(output, 5, "Expected 5 output");
}

/// Test literal int input - Go: TestSingleLiteralInt
#[test]
fn test_literal_int_input() {
    let result = B.TestNamedArgsLiteralInt.call(1);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    // Go: assert.Contains(t, result, "1")
    assert!(
        output.contains("1"),
        "Expected output to contain '1', got: {}",
        output
    );
}

/// Test literal bool input - Go: TestSingleLiteralBool
#[test]
fn test_literal_bool_input() {
    let result = B.TestNamedArgsLiteralBool.call(true);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    // Go: assert.Contains(t, result, "true")
    assert!(
        output.contains("true"),
        "Expected output to contain 'true', got: {}",
        output
    );
}

/// Test literal string input - Go: TestSingleLiteralString
#[test]
fn test_literal_string_input() {
    let result = B.TestNamedArgsLiteralString.call("My String");
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    // Go: assert.Contains(t, result, "My String")
    assert!(
        output.contains("My String"),
        "Expected output to contain 'My String', got: {}",
        output
    );
}

/// Test optional string input with Some value - Go: TestOptionalStringInput
#[test]
fn test_optional_string_input_some() {
    let result = B.FnNamedArgsSingleStringOptional.call(Some("test value"));
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    // Go: assert.Contains(t, result, "test value")
    assert!(
        output.contains("test value"),
        "Expected output to contain 'test value', got: {}",
        output
    );
}

/// Test optional string input with None value - Go: TestOptionalStringInput
#[test]
fn test_optional_string_input_none() {
    let result = B.FnNamedArgsSingleStringOptional.call(None::<&str>);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    // Go: assert.NotEmpty(t, result) -- should still return some result when passed nil
    assert!(
        !output.is_empty(),
        "Expected non-empty output for None input"
    );
}

/// Test class output - Go: TestAllOutputTypes
#[test]
fn test_class_output() {
    let result = B.FnOutputClass.call("test input");
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    // Go: assert.NotEmpty(t, classResult.Prop1)
    assert!(!output.prop1.is_empty(), "Expected non-empty prop1");
    // Go: assert.Equal(t, int64(540), classResult.Prop2)
    assert_eq!(output.prop2, 540, "Expected prop2 to be 540");
}

/// Test class list output - Go: TestAllOutputTypes
#[test]
fn test_class_list_output() {
    let result = B.FnOutputClassList.call("test input");
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    // Go: assert.NotEmpty(t, classListResult)
    assert!(!output.is_empty(), "Expected non-empty class list");
    // Go: assert.NotEmpty(t, classListResult[0].Prop1)
    assert!(
        !output[0].prop1.is_empty(),
        "Expected non-empty prop1 in first item"
    );
}

/// Test literal int output - Go: TestAllOutputTypes
#[test]
fn test_literal_int_output() {
    let result = B.FnOutputLiteralInt.call("test input");
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    // Go: assert.Equal(t, int64(5), literalIntResult)
    assert_eq!(output, 5, "Expected literal int 5");
}

/// Test literal bool output - Go: TestAllOutputTypes
#[test]
fn test_literal_bool_output() {
    let result = B.FnOutputLiteralBool.call("test input");
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    // Go: assert.False(t, literalBoolResult)
    assert!(!output, "Expected literal bool false");
}

/// Test literal string output - Go: TestAllOutputTypes
#[test]
fn test_literal_string_output() {
    let result = B.FnOutputLiteralString.call("test input");
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    // Go: assert.Equal(t, "example output", literalStringResult)
    assert_eq!(output, "example output", "Expected 'example output'");
}

/// Test enum list output - Go: TestAllOutputTypes
#[test]
fn test_enum_list_output() {
    let result = B.FnEnumListOutput.call("pick 2 at random");
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    // Go: assert.Len(t, enumListResult, 2)
    assert_eq!(output.len(), 2, "Expected 2 enum values");
}
