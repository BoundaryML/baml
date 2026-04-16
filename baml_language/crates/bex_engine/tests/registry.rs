//! Tests for `testing.TestCollector.new` via `call_function` with `copy_objects` flag.
//!
//! Verifies that:
//! - `copy_objects: true` deep-extracts the `TestCollector` as a `BexExternalValue::Instance`
//! - `copy_objects: false` returns a `BexExternalValue::Handle` (a live GC-rooted reference)

mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use common::compile_for_engine;
use sys_native::SysOpsExt;

/// The source used in both tests below: a minimal program with no user code,
/// just the stdlib's `testing.TestCollector.new` available as a top-level function.
fn registry_source() -> &'static str {
    // No user declarations needed; testing.TestCollector.new is from the stdlib.
    ""
}

/// Calling `testing.TestCollector.new` with `copy_objects: true` should deep-extract
/// the `TestCollector` instance and return a `BexExternalValue::Instance` with
/// `class_name == "testing.TestCollector"`.
#[tokio::test]
async fn registry_new_copy_objects_true_returns_instance() {
    let snapshot = compile_for_engine(registry_source());
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "testing.TestCollector.new",
            vec![BexExternalValue::String(String::new())],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true, // copy_objects: deep-extract to BexExternalValue
        )
        .await
        .expect("testing.TestCollector.new should succeed");

    match &result {
        BexExternalValue::Instance { class_name, fields } => {
            assert_eq!(
                class_name, "testing.TestCollector",
                "expected class_name 'testing.TestCollector', got '{class_name}'"
            );
            // TestCollector has three fields: prefix (string), tests (empty array), testsets (empty array)
            assert!(
                fields.contains_key("tests"),
                "expected 'tests' field in TestCollector instance"
            );
            assert!(
                fields.contains_key("testsets"),
                "expected 'testsets' field in TestCollector instance"
            );
            assert!(
                fields.contains_key("prefix"),
                "expected 'prefix' field in TestCollector instance"
            );
        }
        other => panic!("expected Instance for testing.TestCollector.new, got: {other:?}"),
    }
}

/// Calling `testing.TestCollector.new` with `copy_objects: false` should return a
/// `BexExternalValue::Handle` — a live GC-rooted reference to the heap object.
/// This is the path `collect_tests` will use.
#[tokio::test]
async fn registry_new_copy_objects_false_returns_handle() {
    let snapshot = compile_for_engine(registry_source());
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "testing.TestCollector.new",
            vec![BexExternalValue::String(String::new())],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            false, // copy_objects: return Handle instead of deep-extracting
        )
        .await
        .expect("testing.TestCollector.new should succeed");

    assert!(
        matches!(result, BexExternalValue::Handle(_)),
        "expected Handle for testing.TestCollector.new with copy_objects=false, got: {result:?}"
    );

    // The handle should survive a GC cycle (GC-rooted).
    let _stats = engine.collect_garbage().await;
    assert!(
        matches!(result, BexExternalValue::Handle(_)),
        "handle should still be valid after GC"
    );
}
