//! Tests for `testing.Registry.new` via `call_function` with `copy_objects` flag.
//!
//! Verifies that:
//! - `copy_objects: true` deep-extracts the Registry as a `BexExternalValue::Instance`
//! - `copy_objects: false` returns a `BexExternalValue::Handle` (a live GC-rooted reference)

mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use common::compile_for_engine;
use sys_native::SysOpsExt;

/// The source used in both tests below: a minimal program with no user code,
/// just the stdlib's `testing.Registry.new` available as a top-level function.
fn registry_source() -> &'static str {
    // No user declarations needed; testing.Registry.new is from the stdlib.
    ""
}

/// Calling `testing.Registry.new` with `copy_objects: true` should deep-extract
/// the Registry instance and return a `BexExternalValue::Instance` with
/// `class_name == "testing.Registry"`.
#[tokio::test]
async fn registry_new_copy_objects_true_returns_instance() {
    let snapshot = compile_for_engine(registry_source());
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            None,
        )
        .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "testing.Registry.new",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true, // copy_objects: deep-extract to BexExternalValue
        )
        .await
        .expect("testing.Registry.new should succeed");

    match &result {
        BexExternalValue::Instance { class_name, fields } => {
            assert_eq!(
                class_name, "testing.Registry",
                "expected class_name 'testing.Registry', got '{class_name}'"
            );
            // Registry has two fields: tests (empty array) and testsets (empty array)
            assert!(
                fields.contains_key("tests"),
                "expected 'tests' field in Registry instance"
            );
            assert!(
                fields.contains_key("testsets"),
                "expected 'testsets' field in Registry instance"
            );
        }
        other => panic!("expected Instance for testing.Registry.new, got: {other:?}"),
    }
}

/// Calling `testing.Registry.new` with `copy_objects: false` should return a
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
        )
        .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "testing.Registry.new",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            false, // copy_objects: return Handle instead of deep-extracting
        )
        .await
        .expect("testing.Registry.new should succeed");

    assert!(
        matches!(result, BexExternalValue::Handle(_)),
        "expected Handle for testing.Registry.new with copy_objects=false, got: {result:?}"
    );

    // The handle should survive a GC cycle (GC-rooted).
    let _stats = engine.collect_garbage().await;
    assert!(
        matches!(result, BexExternalValue::Handle(_)),
        "handle should still be valid after GC"
    );
}
