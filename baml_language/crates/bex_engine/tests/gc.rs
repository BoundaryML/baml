//! GC integration tests for handle-as-GC-root behavior.
//!
//! These tests verify that handles returned from `call_function` properly
//! protect their referenced objects from garbage collection.

mod common;

use std::sync::Arc;

use ::bex_heap::CollectionLevel;
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use common::compile_for_engine;
use sys_native::SysOpsExt;

/// Test that a handle prevents the referenced object from being collected.
///
/// The test goes beyond checking the local `result` binding: it passes the
/// value *back through the engine* after GC to verify the engine can still
/// round-trip it correctly — something that would fail if the underlying
/// heap object had been collected or its memory corrupted during GC.
#[tokio::test]
async fn test_handle_prevents_gc_collection() {
    let source = r#"
        function return_string() -> string {
            "hello world"
        }
        function echo_string(s: string) -> string {
            s
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .unwrap(),
    );

    // Get a handle to a string object
    let result = engine
        .call_function(
            "return_string",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .unwrap();
    assert!(
        matches!(result, BexExternalValue::String(_)),
        "Expected String, got {result:?}"
    );

    // Trigger GC
    let _stats = engine.collect_garbage(CollectionLevel::Major).await;

    // Value should still be correct after GC (basic local-binding check)
    assert_eq!(
        result.clone(),
        BexExternalValue::String("hello world".to_string().into())
    );

    // Strengthen: pass the value back through the engine after GC.
    // If GC had corrupted or collected the underlying data, the engine
    // would either panic or return a wrong value here.
    let echoed = engine
        .call_function(
            "echo_string",
            vec![result],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(
        echoed,
        BexExternalValue::String("hello world".to_string().into()),
        "Engine must round-trip the string correctly after GC"
    );
}

/// Test that handles to arrays preserve the entire structure.
#[tokio::test]
async fn test_array_preserved_through_gc() {
    let source = r#"
        function return_array() -> string[] {
            let items = ["a", "b", "c", "d", "e"];
            items
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .unwrap(),
    );

    // Get a handle to the array
    let result = engine
        .call_function(
            "return_array",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .unwrap();
    assert!(
        matches!(result, BexExternalValue::Array { .. }),
        "Expected Array, got {result:?}"
    );

    // Trigger GC
    let _stats = engine.collect_garbage(CollectionLevel::Major).await;

    // Array and all its elements should be preserved
    match result {
        BexExternalValue::Array { items, .. } => {
            assert_eq!(items.len(), 5);
            assert_eq!(items[0], BexExternalValue::String("a".to_string().into()));
            assert_eq!(items[4], BexExternalValue::String("e".to_string().into()));
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
            let a = "first";
            let b = "second";
            let c = "third";
            let arr = [a, b, c];
            arr
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .unwrap(),
    );

    // Create objects
    let result = engine
        .call_function(
            "create_objects",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .unwrap();

    // Trigger multiple GC cycles to ensure forwarding works
    for _ in 0..3 {
        let _stats = engine.collect_garbage(CollectionLevel::Major).await;
    }

    // Objects should still be accessible with correct values
    match result {
        BexExternalValue::Array { items, .. } => {
            assert_eq!(items.len(), 3);
            assert_eq!(
                items[0],
                BexExternalValue::String("first".to_string().into())
            );
            assert_eq!(
                items[1],
                BexExternalValue::String("second".to_string().into())
            );
            assert_eq!(
                items[2],
                BexExternalValue::String("third".to_string().into())
            );
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
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .unwrap(),
    );

    // Create multiple handles
    let h1 = engine
        .call_function(
            "make_string",
            vec!["hello".into()],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .unwrap();
    let h2 = engine
        .call_function(
            "make_string",
            vec!["world".into()],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .unwrap();
    let h3 = engine
        .call_function(
            "make_string",
            vec!["test".into()],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .unwrap();

    // Trigger GC
    let _stats = engine.collect_garbage(CollectionLevel::Major).await;

    // All handles should still be valid
    assert_eq!(h1, BexExternalValue::String("hello".to_string().into()));
    assert_eq!(h2, BexExternalValue::String("world".to_string().into()));
    assert_eq!(h3, BexExternalValue::String("test".to_string().into()));
}

/// Test that nested class instances (Outer → Inner) survive GC and remain
/// correctly accessible through the engine.
#[tokio::test]
async fn test_gc_with_nested_class_instances() {
    let source = r#"
        class Inner {
            value int
        }
        class Outer {
            inner Inner
            name string
        }
        function make_nested() -> Outer {
            Outer { inner: Inner { value: 42 }, name: "outer" }
        }
        function get_inner_value(o: Outer) -> int {
            o.inner.value
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .unwrap(),
    );

    let outer = engine
        .call_function(
            "user.make_nested",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .unwrap();

    // Force GC to move objects and test that forwarding works correctly.
    let _stats = engine.collect_garbage(CollectionLevel::Major).await;

    // Access nested field through the GC'd handle.  If Gen2 fixup is broken
    // the inner pointer will be stale and the engine will return wrong data.
    let result = engine
        .call_function(
            "user.get_inner_value",
            vec![outer],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .unwrap();

    assert_eq!(result, BexExternalValue::Int(42));
}

/// Test that an enum variant survives GC and can be echoed back through
/// the engine correctly.
#[tokio::test]
async fn test_gc_with_enum_variant_round_trip() {
    let source = r#"
        enum Color {
            Red
            Green
            Blue
        }
        function make_color() -> Color {
            Color.Green
        }
        function echo_color(c: Color) -> Color {
            c
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .unwrap(),
    );

    let color = engine
        .call_function(
            "user.make_color",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .unwrap();

    // Force GC.
    let _stats = engine.collect_garbage(CollectionLevel::Major).await;

    let echoed = engine
        .call_function(
            "user.echo_color",
            vec![color.clone()],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .unwrap();

    assert_eq!(echoed, color);
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
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .unwrap(),
    );

    // Int should be BexExternalValue::Int
    let result = engine
        .call_function(
            "return_int",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .unwrap();
    assert!(matches!(result, BexExternalValue::Int(42)));

    // Null should be BexExternalValue::Null
    let result = engine
        .call_function(
            "return_null",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .unwrap();
    assert!(matches!(result, BexExternalValue::Null));

    // Bool should be BexExternalValue::Bool
    let result = engine
        .call_function(
            "return_bool",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .unwrap();
    assert!(matches!(result, BexExternalValue::Bool(true)));
}
