//! Data types tests - ported from test_functions_data_types_test.go
//!
//! Tests for various data types including:
//! - Classes with literal properties
//! - Map types
//! - Union aliases
//! - Optional lists and maps
//! - Dynamic types

use rust::baml_client::sync_client::B;
use rust::baml_client::types::*;
use std::collections::HashMap;

/// Test class with literal property
#[test]
fn test_class_with_literal_property() {
    let input = LiteralClassHello {
        prop: "hello".to_string(),
    };
    let result = B.FnLiteralClassInputOutput.call(&input);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output.prop, "hello", "Expected literal value 'hello'");
}

/// Test literal class with union properties
#[test]
fn test_literal_class_with_union() {
    let input = Union2LiteralClassOneOrLiteralClassTwo::LiteralClassOne(LiteralClassOne {
        prop: "one".to_string(),
    });
    let result = B.FnLiteralUnionClassInputOutput.call(&input);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
}

/// Test map string to string
#[test]
fn test_map_string_to_string() {
    let mut input = HashMap::new();
    input.insert("key1".to_string(), "value1".to_string());
    input.insert("key2".to_string(), "value2".to_string());

    let result = B.TestFnNamedArgsSingleMapStringToString.call(&input);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert!(!output.is_empty(), "Expected non-empty map output");
}

/// Test map string to class
#[test]
fn test_map_string_to_class() {
    let mut input = HashMap::new();
    input.insert(
        "entry1".to_string(),
        StringToClassEntry {
            word: "hello".to_string(),
        },
    );
    input.insert(
        "entry2".to_string(),
        StringToClassEntry {
            word: "world".to_string(),
        },
    );

    let result = B.TestFnNamedArgsSingleMapStringToClass.call(&input);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
}

/// Test map string to map
#[test]
fn test_map_string_to_map() {
    let mut inner = HashMap::new();
    inner.insert("inner_key".to_string(), "inner_value".to_string());

    let mut input = HashMap::new();
    input.insert("outer_key".to_string(), inner);

    let result = B.TestFnNamedArgsSingleMapStringToMap.call(&input);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
}

/// Test literal string keys in maps
#[test]
fn test_literal_string_keys_in_map() {
    let mut input = HashMap::new();
    input.insert("key".to_string(), "value".to_string());

    let result = B.InOutSingleLiteralStringMapKey.call(&input);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
}

/// Test primitive union alias
#[test]
fn test_primitive_union_alias() {
    let input = Union4BoolOrFloatOrIntOrString::Int(42);
    let result = B.PrimitiveAlias.call(&input);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
}

/// Test map alias
#[test]
fn test_map_alias() {
    let mut input = HashMap::new();
    input.insert(
        "key".to_string(),
        vec!["value1".to_string(), "value2".to_string()],
    );

    let result = B.MapAlias.call(&input);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
}

/// Test nested alias
#[test]
fn test_nested_alias() {
    let input = Union6BoolOrFloatOrIntOrListStringOrMapStringKeyListStringValueOrString::String(
        "test".to_string(),
    );
    let result = B.NestedAlias.call(&input);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
}

/// Test optional lists and maps
#[test]
fn test_optional_lists_and_maps() {
    let input = OptionalListAndMap {
        p: Some(vec!["item1".to_string(), "item2".to_string()]),
        q: Some({
            let mut map = HashMap::new();
            map.insert("key".to_string(), "value".to_string());
            map
        }),
    };

    let result = B.AllowedOptionals.call(&input);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
}

/// Test optional lists and maps with None
#[test]
fn test_optional_lists_and_maps_none() {
    let input = OptionalListAndMap { p: None, q: None };

    let result = B.AllowedOptionals.call(&input);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
}

/// Test literal union returns
#[test]
fn test_literal_union_returns() {
    let result = B.LiteralUnionsTest.call("return string_output");
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
}

/// Test JSON type alias cycle
#[test]
fn test_json_type_alias_cycle() {
    // Create a HashMap<String, JsonValue> which is JsonObject
    let mut map = HashMap::new();
    map.insert("key".to_string(), JsonValue::String("value".to_string()));
    let input = JsonValue::JsonObject(map);

    let result = B.JsonTypeAliasCycle.call(&input);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
}

/// Test differentiate unions (dynamic types)
#[test]
fn test_differentiate_unions() {
    let result = B.DifferentiateUnions.call();
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
}
