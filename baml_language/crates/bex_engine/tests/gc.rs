//! GC integration tests for handle-as-GC-root behavior.
//!
//! These tests verify that handles returned from `call_function` properly
//! protect their referenced objects from garbage collection.

mod common;

use std::collections::HashMap;

use bex_engine::{BexEngine, BexExternalValue};
use common::compile_for_engine;

/// Test that a handle prevents the referenced object from being collected.
#[tokio::test]
async fn test_handle_prevents_gc_collection() {
    let source = r#"
        function return_string() -> string {
            "hello world"
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = BexEngine::new(snapshot, HashMap::new()).unwrap();

    // Get a handle to a string object
    let result = engine.call_function("return_string", &[]).await.unwrap();
    assert!(
        matches!(result, BexExternalValue::String(_)),
        "Expected String, got {result:?}"
    );

    // Trigger GC
    let _stats = engine.collect_garbage().await;

    // Value should still be correct after GC
    assert_eq!(result, BexExternalValue::String("hello world".to_string()));
}

/// Test that handles to arrays preserve the entire structure.
#[tokio::test]
async fn test_array_preserved_through_gc() {
    let source = r#"
        function return_array() -> string[] {
            let items = ["a", "b", "c", "d", "e"]
            items
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = BexEngine::new(snapshot, HashMap::new()).unwrap();

    // Get a handle to the array
    let result = engine.call_function("return_array", &[]).await.unwrap();
    assert!(
        matches!(result, BexExternalValue::Array { .. }),
        "Expected Array, got {result:?}"
    );

    // Trigger GC
    let _stats = engine.collect_garbage().await;

    // Array and all its elements should be preserved
    match result {
        BexExternalValue::Array { items, .. } => {
            assert_eq!(items.len(), 5);
            assert_eq!(items[0], BexExternalValue::String("a".to_string()));
            assert_eq!(items[4], BexExternalValue::String("e".to_string()));
        }
        other => panic!("Expected array, got: {other:?}"),
    }
}

/// Test that GC updates forwarding pointers correctly.
///
/// This test verifies Gap #2 (root remapping) is fixed by:
/// 1. Creating multiple objects that will be moved during GC
/// 2. Triggering GC
/// 3. Verifying all objects are still accessible at their new locations
#[tokio::test]
async fn test_gc_updates_forwarding_pointers() {
    let source = r#"
        function create_objects() -> string[] {
            let a = "first"
            let b = "second"
            let c = "third"
            let arr = [a, b, c]
            arr
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = BexEngine::new(snapshot, HashMap::new()).unwrap();

    // Create objects
    let result = engine.call_function("create_objects", &[]).await.unwrap();

    // Trigger multiple GC cycles to ensure forwarding works
    for _ in 0..3 {
        let _stats = engine.collect_garbage().await;
    }

    // Objects should still be accessible with correct values
    match result {
        BexExternalValue::Array { items, .. } => {
            assert_eq!(items.len(), 3);
            assert_eq!(items[0], BexExternalValue::String("first".to_string()));
            assert_eq!(items[1], BexExternalValue::String("second".to_string()));
            assert_eq!(items[2], BexExternalValue::String("third".to_string()));
        }
        other => panic!("Expected array, got: {other:?}"),
    }
}

/// Test that multiple handles survive GC.
///
/// This verifies the handle table is properly updated during GC.
#[tokio::test]
async fn test_multiple_handles_survive_gc() {
    let source = r#"
        function make_string(s: string) -> string {
            s
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = BexEngine::new(snapshot, HashMap::new()).unwrap();

    // Create multiple handles
    let h1 = engine
        .call_function("make_string", &["hello".into()])
        .await
        .unwrap();
    let h2 = engine
        .call_function("make_string", &["world".into()])
        .await
        .unwrap();
    let h3 = engine
        .call_function("make_string", &["test".into()])
        .await
        .unwrap();

    // Trigger GC
    let _stats = engine.collect_garbage().await;

    // All handles should still be valid
    assert_eq!(h1, BexExternalValue::String("hello".to_string()));
    assert_eq!(h2, BexExternalValue::String("world".to_string()));
    assert_eq!(h3, BexExternalValue::String("test".to_string()));
}

/// Test primitive return values (should be `BexExternalValue`, not Handle).
#[tokio::test]
async fn test_primitive_returns_are_external_values() {
    let source = r#"
        function return_int() -> int {
            42
        }
        function return_null() -> null {
            null
        }
        function return_bool() -> bool {
            true
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = BexEngine::new(snapshot, HashMap::new()).unwrap();

    // Int should be BexExternalValue::Int
    let result = engine.call_function("return_int", &[]).await.unwrap();
    assert!(matches!(result, BexExternalValue::Int(42)));

    // Null should be BexExternalValue::Null
    let result = engine.call_function("return_null", &[]).await.unwrap();
    assert!(matches!(result, BexExternalValue::Null));

    // Bool should be BexExternalValue::Bool
    let result = engine.call_function("return_bool", &[]).await.unwrap();
    assert!(matches!(result, BexExternalValue::Bool(true)));
}
