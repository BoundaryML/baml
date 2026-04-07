//! Integration tests for `BexEngine::collect_tests`.
//!
//! Verifies that:
//! - A project with `test` blocks produces a `Handle` (non-null) `BexExternalValue`
//! - A project with no tests produces `BexExternalValue::Null`
//! - A project with a `testset` block produces a non-null registry

mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, CallId, CancellationToken};
use common::compile_for_engine;
use sys_native::SysOpsExt;

/// Build an engine from BAML source.
fn make_engine(source: &str) -> Arc<BexEngine> {
    let snapshot = compile_for_engine(source);
    Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            None,
        )
        .expect("Failed to create engine"),
    )
}

/// A project with two top-level tests should produce a non-null registry Handle.
#[tokio::test]
async fn collect_tests_returns_registry_handle() {
    let source = r#"
        test "foo" {
            null
        }

        test "bar" {
            null
        }
    "#;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests("user", CallId::next(), CancellationToken::default())
        .await
        .expect("collect_tests should succeed");

    assert!(
        matches!(registry, BexExternalValue::Handle(_)),
        "expected Handle for project with tests, got: {registry:?}"
    );
}

/// A project with no test blocks should produce `BexExternalValue::Null`.
#[tokio::test]
async fn collect_tests_no_tests_returns_null() {
    // No test or testset blocks — $init_test will not exist.
    let source = "";

    let engine = make_engine(source);
    let registry = engine
        .collect_tests("user", CallId::next(), CancellationToken::default())
        .await
        .expect("collect_tests should succeed");

    assert!(
        matches!(registry, BexExternalValue::Null),
        "expected Null for empty project, got: {registry:?}"
    );
}

/// A project with a testset should produce a non-null registry Handle.
#[tokio::test]
async fn collect_tests_with_testset_returns_handle() {
    let source = r#"
        testset "suite" {
            test "inner" {
                null
            }
        }
    "#;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests("user", CallId::next(), CancellationToken::default())
        .await
        .expect("collect_tests should succeed");

    assert!(
        matches!(registry, BexExternalValue::Handle(_)),
        "expected Handle for project with testset, got: {registry:?}"
    );
}

/// A project with nested testsets should produce a non-null registry Handle.
#[tokio::test]
async fn collect_tests_nested_testset_returns_handle() {
    let source = r#"
        testset "outer" {
            testset "inner" {
                test "deep" { null }
            }
        }
    "#;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests("user", CallId::next(), CancellationToken::default())
        .await
        .expect("collect_tests should succeed");

    assert!(
        matches!(registry, BexExternalValue::Handle(_)),
        "expected Handle for project with nested testsets, got: {registry:?}"
    );
}

/// A project with dynamic test names should produce a non-null registry Handle.
#[tokio::test]
async fn collect_tests_dynamic_name_returns_handle() {
    let source = r#"
        testset "dynamic" {
            test "case_" + "a" {
                null
            }
        }
    "#;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests("user", CallId::next(), CancellationToken::default())
        .await
        .expect("collect_tests should succeed");

    assert!(
        matches!(registry, BexExternalValue::Handle(_)),
        "expected Handle for project with dynamic test names, got: {registry:?}"
    );
}

/// A project with multiple testsets should produce a non-null registry Handle.
#[tokio::test]
async fn collect_tests_multiple_testsets_returns_handle() {
    let source = r#"
        testset "group_a" {
            test "a1" { null }
            test "a2" { null }
        }
        testset "group_b" {
            test "b1" { null }
        }
    "#;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests("user", CallId::next(), CancellationToken::default())
        .await
        .expect("collect_tests should succeed");

    assert!(
        matches!(registry, BexExternalValue::Handle(_)),
        "expected Handle for project with multiple testsets, got: {registry:?}"
    );
}
