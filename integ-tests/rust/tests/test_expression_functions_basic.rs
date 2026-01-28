//! Expression function tests - tests for functions with `return` body instead of `prompt` body
//!
//! These functions don't call LLMs, they just execute the expression and return the result.
//! Tests verify that the expression functions return the expected values.

use rust::baml_client::sync_client::B;
use rust::baml_client::types::*;
use rust::baml_client::{
    new_audio_from_url, new_image_from_url, new_pdf_from_url, new_video_from_url,
};
use std::collections::HashMap;

// =============================================================================
// Input Functions - Simple Types
// =============================================================================

/// Test bool input expression function
#[test]
fn test_expr_func_bool_input() {
    let result = B.TestFnNamedArgsSingleBoolExprFunc.call(true);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output, "Hello, world!", "Expected 'Hello, world!' output");
}

/// Test string input expression function
#[test]
fn test_expr_func_string_input() {
    let result = B.TestFnNamedArgsSingleStringExprFunc.call("test");
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output, "Hello, world!", "Expected 'Hello, world!' output");
}

/// Test int input expression function
#[test]
fn test_expr_func_int_input() {
    let result = B.TestFnNamedArgsSingleIntExprFunc.call(42);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output, "Hello, world!", "Expected 'Hello, world!' output");
}

/// Test float input expression function
#[test]
fn test_expr_func_float_input() {
    let result = B.TestFnNamedArgsSingleFloatExprFunc.call(3.14);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output, "Hello, world!", "Expected 'Hello, world!' output");
}

/// Test optional string input expression function with Some value
#[test]
fn test_expr_func_optional_string_input_some() {
    let result = B.FnNamedArgsSingleStringOptionalExprFunc.call(Some("test"));
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output, "Hello, world!", "Expected 'Hello, world!' output");
}

/// Test optional string input expression function with None value
#[test]
fn test_expr_func_optional_string_input_none() {
    let result = B.FnNamedArgsSingleStringOptionalExprFunc.call(None::<&str>);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output, "Hello, world!", "Expected 'Hello, world!' output");
}

// =============================================================================
// Input Functions - Literal Types
// =============================================================================

/// Test literal bool input expression function
#[test]
fn test_expr_func_literal_bool_input() {
    let result = B.TestFnNamedArgsLiteralBoolExprFunc.call(true);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output, "Hello, world!", "Expected 'Hello, world!' output");
}

/// Test literal int input expression function
#[test]
fn test_expr_func_literal_int_input() {
    let result = B.TestFnNamedArgsLiteralIntExprFunc.call(1);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output, "Hello, world!", "Expected 'Hello, world!' output");
}

/// Test literal string input expression function
#[test]
fn test_expr_func_literal_string_input() {
    let result = B.TestFnNamedArgsLiteralStringExprFunc.call("My String");
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output, "Hello, world!", "Expected 'Hello, world!' output");
}

// =============================================================================
// Input Functions - List Types
// =============================================================================

/// Test string list input expression function
#[test]
fn test_expr_func_string_list_input() {
    let items = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let result = B.TestFnNamedArgsSingleStringListExprFunc.call(&items);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(
        output,
        vec!["hello".to_string(), "world".to_string()],
        "Expected ['hello', 'world'] output"
    );
}

/// Test string array input expression function
#[test]
fn test_expr_func_string_array_input() {
    let items = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let result = B.TestFnNamedArgsSingleStringArrayExprFunc.call(&items);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output, "Hello, world!", "Expected 'Hello, world!' output");
}

// =============================================================================
// Input Functions - Class Types
// =============================================================================

/// Test class input expression function
#[test]
fn test_expr_func_class_input() {
    let arg = NamedArgsSingleClass {
        key: "key".to_string(),
        key_two: true,
        key_three: 52,
    };
    let result = B.TestFnNamedArgsSingleClassExprFunc.call(&arg);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output, "Hello, world!", "Expected 'Hello, world!' output");
}

/// Test literal class input/output expression function
#[test]
fn test_expr_func_literal_class_input_output() {
    let input = LiteralClassHello {
        prop: "hello".to_string(),
    };
    let result = B.FnLiteralClassInputOutputExprFunc.call(&input);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output.prop, "hello", "Expected prop to be 'hello'");
}

/// Test literal union class input/output expression function
#[test]
fn test_expr_func_literal_union_class_input_output() {
    let input = Union2LiteralClassOneOrLiteralClassTwo::LiteralClassOne(LiteralClassOne {
        prop: "one".to_string(),
    });
    let result = B.FnLiteralUnionClassInputOutputExprFunc.call(&input);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    match output {
        Union2LiteralClassOneOrLiteralClassTwo::LiteralClassOne(c) => {
            assert_eq!(c.prop, "one", "Expected prop to be 'one'");
        }
        _ => panic!("Expected LiteralClassOne variant"),
    }
}

// =============================================================================
// Input Functions - Enum Types
// =============================================================================

/// Test enum input expression function
#[test]
fn test_expr_func_enum_input() {
    let result = B
        .FnTestNamedArgsSingleEnumExprFunc
        .call(&NamedArgsSingleEnum::ONE);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output, "Hello, world!", "Expected 'Hello, world!' output");
}

/// Test enum list input expression function
#[test]
fn test_expr_func_enum_list_input() {
    let result = B
        .TestFnNamedArgsSingleEnumListExprFunc
        .call(&[NamedArgsSingleEnumList::ONE, NamedArgsSingleEnumList::TWO]);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output, "Hello, world!", "Expected 'Hello, world!' output");
}

// =============================================================================
// Input Functions - Map Types
// =============================================================================

/// Test map string to string input expression function
#[test]
fn test_expr_func_map_string_to_string_input() {
    let mut my_map: HashMap<String, String> = HashMap::new();
    my_map.insert("key1".to_string(), "value1".to_string());
    my_map.insert("key2".to_string(), "value2".to_string());
    let result = B
        .TestFnNamedArgsSingleMapStringToStringExprFunc
        .call(&my_map);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output.len(), 2, "Expected map with 2 entries");
    assert_eq!(
        output.get("key1"),
        Some(&"value1".to_string()),
        "Expected key1 -> value1"
    );
    assert_eq!(
        output.get("key2"),
        Some(&"value2".to_string()),
        "Expected key2 -> value2"
    );
}

/// Test map string to map input expression function
#[test]
fn test_expr_func_map_string_to_map_input() {
    let mut inner_map: HashMap<String, String> = HashMap::new();
    inner_map.insert("inner_key".to_string(), "inner_value".to_string());
    let mut my_map: HashMap<String, HashMap<String, String>> = HashMap::new();
    my_map.insert("outer_key".to_string(), inner_map);
    let result = B.TestFnNamedArgsSingleMapStringToMapExprFunc.call(&my_map);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output.len(), 1, "Expected map with 1 entry");
    let inner = output.get("outer_key").expect("Expected outer_key");
    assert_eq!(
        inner.get("inner_key"),
        Some(&"inner_value".to_string()),
        "Expected inner_key -> inner_value"
    );
}

/// Test map string to class input expression function
#[test]
fn test_expr_func_map_string_to_class_input() {
    let entry = StringToClassEntry {
        word: "test".to_string(),
    };
    let mut my_map: HashMap<String, StringToClassEntry> = HashMap::new();
    my_map.insert("key1".to_string(), entry);
    let result = B
        .TestFnNamedArgsSingleMapStringToClassExprFunc
        .call(&my_map);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output.len(), 1, "Expected map with 1 entry");
    let entry = output.get("key1").expect("Expected key1");
    assert_eq!(entry.word, "test", "Expected word to be 'test'");
}

// =============================================================================
// Input Functions - Media Types
// =============================================================================

/// Test image input expression function
#[test]
fn test_expr_func_image_input() {
    let img = new_image_from_url("https://example.com/image.png", None);
    let result = B.TestImageInputExprFunc.call(&img);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output, "Hello, world!", "Expected 'Hello, world!' output");
}

/// Test image list input expression function
#[test]
fn test_expr_func_image_list_input() {
    let img1 = new_image_from_url("https://example.com/image1.png", None);
    let img2 = new_image_from_url("https://example.com/image2.png", None);
    let result = B.TestImageListInputExprFunc.call(&[img1, img2]);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output, "Hello, world!", "Expected 'Hello, world!' output");
}

/// Test audio input expression function
#[test]
fn test_expr_func_audio_input() {
    let aud = new_audio_from_url("https://example.com/audio.mp3", None);
    let result = B.AudioInputExprFunc.call(&aud);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output, "Hello, world!", "Expected 'Hello, world!' output");
}

/// Test video input expression function
#[test]
fn test_expr_func_video_input() {
    let vid = new_video_from_url("https://example.com/video.mp4", None);
    let result = B.VideoInputExprFunc.call(&vid);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output, "Hello, world!", "Expected 'Hello, world!' output");
}

/// Test pdf input expression function
#[test]
fn test_expr_func_pdf_input() {
    let pdf = new_pdf_from_url("https://example.com/document.pdf", None);
    let result = B.PdfInputExprFunc.call(&pdf);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output, "Hello, world!", "Expected 'Hello, world!' output");
}

// =============================================================================
// Output Functions
// =============================================================================

/// Test bool output expression function
#[test]
fn test_expr_func_bool_output() {
    let result = B.FnOutputBoolExprFunc.call("any input");
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert!(output, "Expected true output");
}

/// Test int output expression function
#[test]
fn test_expr_func_int_output() {
    let result = B.FnOutputIntExprFunc.call("any input");
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output, 5, "Expected 5 output");
}

/// Test literal string output expression function
#[test]
fn test_expr_func_literal_string_output() {
    let result = B.FnOutputLiteralStringExprFunc.call("any input");
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output, "example output", "Expected 'example output'");
}

/// Test class output expression function
#[test]
fn test_expr_func_class_output() {
    let result = B.FnOutputClassExprFunc.call("any input");
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(
        output.prop1, "example string",
        "Expected prop1 to be 'example string'"
    );
    assert_eq!(output.prop2, 540, "Expected prop2 to be 540");
}

/// Test class list output expression function
#[test]
fn test_expr_func_class_list_output() {
    let result = B.FnOutputClassListExprFunc.call("any input");
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output.len(), 1, "Expected list with 1 element");
    assert_eq!(
        output[0].prop1, "example string",
        "Expected prop1 to be 'example string'"
    );
    assert_eq!(output[0].prop2, 540, "Expected prop2 to be 540");
}

/// Test nested class output expression function
#[test]
fn test_expr_func_nested_class_output() {
    let result = B.FnOutputClassNestedExprFunc.call("any input");
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(
        output.prop1, "example string",
        "Expected prop1 to be 'example string'"
    );
    assert_eq!(
        output.prop2.prop1, "example string",
        "Expected nested prop1 to be 'example string'"
    );
    assert_eq!(
        output.prop2.prop2, "example string",
        "Expected nested prop2 to be 'example string'"
    );
    assert_eq!(
        output.prop2.inner.prop2, 540,
        "Expected inner prop2 to be 540"
    );
    assert!(
        (output.prop2.inner.prop3 - 1.23).abs() < 0.01,
        "Expected inner prop3 to be approximately 1.23"
    );
}

/// Test null literal class output expression function
#[test]
fn test_expr_func_null_literal_class_output() {
    let result = B.NullLiteralClassHelloExprFunc.call("unused");
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output.a, "hi", "Expected a to be 'hi'");
}

/// Test allowed optionals expression function (returns input)
#[test]
fn test_expr_func_allowed_optionals() {
    let input = OptionalListAndMap {
        p: Some(vec!["item1".to_string(), "item2".to_string()]),
        q: None,
    };
    let result = B.AllowedOptionalsExprFunc.call(&input);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(
        output.p,
        Some(vec!["item1".to_string(), "item2".to_string()]),
        "Expected same list back"
    );
}
