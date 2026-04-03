//! Integration tests for `BexEngine::collect_tests`.
//!
//! Verifies that:
//! - A project with `test` blocks produces a populated `TestRegistry`
//! - A project with no tests produces an empty `TestRegistry`
//! - A project with a `testset` block produces a `TestRegistry` with testset names

mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, CancellationToken};
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

/// A project with two top-level tests should produce a registry with both names.
#[tokio::test]
async fn collect_tests_returns_two_tests() {
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
        .collect_tests("user", CancellationToken::default())
        .await
        .expect("collect_tests should succeed");

    assert_eq!(
        registry.tests.len(),
        2,
        "expected 2 top-level tests, got {}",
        registry.tests.len()
    );

    let names = registry.all_test_names();
    assert!(
        names.contains(&"foo".to_string()),
        "expected 'foo' in test names, got: {names:?}"
    );
    assert!(
        names.contains(&"bar".to_string()),
        "expected 'bar' in test names, got: {names:?}"
    );
}

/// A project with no test blocks should produce an empty registry.
#[tokio::test]
async fn collect_tests_no_tests_returns_empty() {
    // No test or testset blocks — $init_test will not exist.
    let source = "";

    let engine = make_engine(source);
    let registry = engine
        .collect_tests("user", CancellationToken::default())
        .await
        .expect("collect_tests should succeed");

    assert_eq!(
        registry.tests.len(),
        0,
        "expected 0 tests for empty project, got {}",
        registry.tests.len()
    );
    assert_eq!(
        registry.testsets.len(),
        0,
        "expected 0 testsets for empty project, got {}",
        registry.testsets.len()
    );
    assert!(
        registry.all_test_names().is_empty(),
        "expected empty all_test_names() for empty project"
    );
}

/// A project with a testset should produce a registry with the testset name.
#[tokio::test]
async fn collect_tests_with_testset_returns_testset_name() {
    let source = r#"
        testset "suite" {
            test "inner" {
                null
            }
        }
    "#;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests("user", CancellationToken::default())
        .await
        .expect("collect_tests should succeed");

    assert_eq!(
        registry.testsets.len(),
        1,
        "expected 1 testset, got {}",
        registry.testsets.len()
    );
    assert_eq!(
        registry.testsets[0].name, "suite",
        "expected testset name 'suite', got '{}'",
        registry.testsets[0].name
    );
}
