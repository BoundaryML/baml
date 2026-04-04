//! Integration tests for `BexEngine::collect_tests`.
//!
//! Verifies that:
//! - A project with `test` blocks produces a populated `TestRegistry`
//! - A project with no tests produces an empty `TestRegistry`
//! - A project with a `testset` block produces a `TestRegistry` with testset names
//! - Timeout-based lazy loading: slow testsets become `Lazy` placeholders
//! - `expand_lazy_testset` can expand a lazy placeholder on demand
//! - `skip_testsets` skips named testsets without invoking their collectors

mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, CancellationToken, TestSetResult};
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

/// Helper: unwrap a `TestSetResult::Expanded` or panic.
fn unwrap_expanded(result: &TestSetResult) -> &bex_engine::TestSetInfo {
    match result {
        TestSetResult::Expanded(ts) => ts,
        TestSetResult::Lazy { name, .. } => {
            panic!("expected Expanded testset, got Lazy({name})")
        }
    }
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
        .collect_tests(
            "user",
            CancellationToken::default(),
            None,
            std::collections::HashSet::new(),
        )
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
        .collect_tests(
            "user",
            CancellationToken::default(),
            None,
            std::collections::HashSet::new(),
        )
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

/// A project with a testset should produce a registry with the testset name and nested tests.
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
        .collect_tests(
            "user",
            CancellationToken::default(),
            None,
            std::collections::HashSet::new(),
        )
        .await
        .expect("collect_tests should succeed");

    assert_eq!(
        registry.testsets.len(),
        1,
        "expected 1 testset, got {}",
        registry.testsets.len()
    );
    let ts = unwrap_expanded(&registry.testsets[0]);
    assert_eq!(
        ts.name, "suite",
        "expected testset name 'suite', got '{}'",
        ts.name
    );
    // Nested test should now be populated by collector closure invocation
    assert_eq!(
        ts.tests.len(),
        1,
        "expected 1 nested test in 'suite', got {}",
        ts.tests.len()
    );
    assert_eq!(
        ts.tests[0].name, "suite/inner",
        "expected nested test 'suite/inner', got '{}'",
        ts.tests[0].name
    );
}

/// Two tests in a testset (no nesting).
#[tokio::test]
async fn collect_tests_testset_two_children() {
    let source = r#"
        testset "group" {
            test "a" { null }
            test "b" { null }
        }
    "#;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests(
            "user",
            CancellationToken::default(),
            None,
            std::collections::HashSet::new(),
        )
        .await
        .expect("collect_tests should succeed");

    assert_eq!(registry.testsets.len(), 1);
    let group = unwrap_expanded(&registry.testsets[0]);
    assert_eq!(group.name, "group");
    assert_eq!(
        group.tests.len(),
        2,
        "expected 2 tests in 'group', got {} tests",
        group.tests.len()
    );
}

/// Dynamic test name using string concatenation (expression name).
#[tokio::test]
async fn collect_tests_dynamic_name_expr() {
    let source = r#"
        testset "dynamic" {
            test "case_" + "a" {
                null
            }
        }
    "#;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests(
            "user",
            CancellationToken::default(),
            None,
            std::collections::HashSet::new(),
        )
        .await
        .expect("collect_tests should succeed");

    assert_eq!(registry.testsets.len(), 1);
    let ts = unwrap_expanded(&registry.testsets[0]);
    assert_eq!(ts.name, "dynamic");
    assert_eq!(
        ts.tests.len(),
        1,
        "expected 1 test, got {} tests",
        ts.tests.len()
    );
    assert_eq!(
        ts.tests[0].name, "dynamic/case_a",
        "expected 'dynamic/case_a', got '{}'",
        ts.tests[0].name
    );
}

/// Nested testset inside a testset.
#[tokio::test]
async fn collect_tests_nested_testset() {
    let source = r#"
        testset "outer" {
            testset "inner" {
                test "deep" { null }
            }
        }
    "#;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests(
            "user",
            CancellationToken::default(),
            None,
            std::collections::HashSet::new(),
        )
        .await
        .expect("collect_tests should succeed");

    assert_eq!(registry.testsets.len(), 1);
    let outer = unwrap_expanded(&registry.testsets[0]);
    assert_eq!(outer.name, "outer");
    assert_eq!(
        outer.testsets.len(),
        1,
        "expected 1 nested testset, got {} testsets",
        outer.testsets.len()
    );
    let inner = unwrap_expanded(&outer.testsets[0]);
    assert_eq!(inner.name, "outer/inner");
    assert_eq!(inner.tests.len(), 1, "expected 1 test in 'outer/inner'");
    assert_eq!(inner.tests[0].name, "outer/inner/deep");
}

// ============================================================================
// Phase 2: Timeout-based lazy loading tests
// ============================================================================

/// Helper: unwrap a `TestSetResult::Lazy` or panic.
fn unwrap_lazy(result: &TestSetResult) -> (&str, &bex_engine::BexExternalValue) {
    match result {
        TestSetResult::Lazy {
            name,
            collector_closure,
        } => (name.as_str(), collector_closure.as_ref()),
        TestSetResult::Expanded(ts) => {
            panic!("expected Lazy testset, got Expanded({})", ts.name)
        }
    }
}

/// A slow testset (1s sleep in its body) with a 1ms timeout should become Lazy.
#[tokio::test]
async fn collect_tests_slow_testset_becomes_lazy() {
    // The testset body calls baml.sys.sleep(1000) — far longer than 1ms timeout.
    let source = r#"
        testset "slow" {
            baml.sys.sleep(1000)
            test "never reached" { null }
        }
    "#;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests(
            "user",
            CancellationToken::default(),
            Some(std::time::Duration::from_millis(1)),
            std::collections::HashSet::new(),
        )
        .await
        .expect("collect_tests should succeed");

    assert_eq!(registry.testsets.len(), 1, "expected 1 testset");
    let (name, _closure) = unwrap_lazy(&registry.testsets[0]);
    assert_eq!(name, "slow", "expected lazy testset named 'slow'");
    // Lazy testsets don't contribute to all_test_names
    assert!(
        registry.all_test_names().is_empty(),
        "lazy testset should not appear in all_test_names: {:?}",
        registry.all_test_names()
    );
}

/// A fast testset under a timeout should still be Expanded normally.
#[tokio::test]
async fn collect_tests_fast_testset_still_expanded_with_timeout() {
    let source = r#"
        testset "fast" {
            test "quick" { null }
        }
    "#;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests(
            "user",
            CancellationToken::default(),
            Some(std::time::Duration::from_secs(10)), // generous timeout
            std::collections::HashSet::new(),
        )
        .await
        .expect("collect_tests should succeed");

    assert_eq!(registry.testsets.len(), 1);
    let ts = unwrap_expanded(&registry.testsets[0]);
    assert_eq!(ts.name, "fast");
    assert_eq!(ts.tests.len(), 1);
    assert_eq!(ts.tests[0].name, "fast/quick");
}

/// A testset in `skip_testsets` becomes Lazy without invoking its collector.
#[tokio::test]
async fn collect_tests_skip_testsets_returns_lazy() {
    // Even though "expensive" has only fast tests, we skip it by name.
    let source = r#"
        testset "expensive" {
            test "a" { null }
            test "b" { null }
        }
        testset "cheap" {
            test "c" { null }
        }
    "#;

    let mut skip = std::collections::HashSet::new();
    skip.insert("expensive".to_string());

    let engine = make_engine(source);
    let registry = engine
        .collect_tests("user", CancellationToken::default(), None, skip)
        .await
        .expect("collect_tests should succeed");

    assert_eq!(registry.testsets.len(), 2);

    // "expensive" should be Lazy (skipped)
    let (lazy_name, _) = unwrap_lazy(&registry.testsets[0]);
    assert_eq!(lazy_name, "expensive");

    // "cheap" should be Expanded
    let cheap = unwrap_expanded(&registry.testsets[1]);
    assert_eq!(cheap.name, "cheap");
    assert_eq!(cheap.tests.len(), 1);

    // Only "cheap/c" appears in all_test_names
    let names = registry.all_test_names();
    assert_eq!(names, vec!["cheap/c".to_string()]);
}

/// `expand_lazy_testset` on a Lazy placeholder fully expands it.
#[tokio::test]
async fn expand_lazy_testset_fully_expands() {
    // Use a 1ms timeout to force "slow" lazy, then expand it manually.
    let source = r#"
        testset "slow" {
            baml.sys.sleep(1000)
            test "hidden" { null }
        }
    "#;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests(
            "user",
            CancellationToken::default(),
            Some(std::time::Duration::from_millis(1)),
            std::collections::HashSet::new(),
        )
        .await
        .expect("collect_tests should succeed (with lazy)");

    assert_eq!(registry.testsets.len(), 1);
    let (name, closure) = unwrap_lazy(&registry.testsets[0]);
    assert_eq!(name, "slow");

    // Now expand the lazy testset — no timeout (user explicitly clicked "load")
    let expanded = engine
        .expand_lazy_testset(name, closure.clone(), CancellationToken::default(), None)
        .await
        .expect("expand_lazy_testset should succeed");

    assert_eq!(expanded.name, "slow");
    assert_eq!(
        expanded.tests.len(),
        1,
        "expected 1 test after expansion, got {}",
        expanded.tests.len()
    );
    assert_eq!(expanded.tests[0].name, "slow/hidden");
}

/// Fast parent with a slow nested child → parent is Expanded, child is Lazy.
#[tokio::test]
async fn collect_tests_fast_parent_slow_child_is_lazy() {
    let source = r#"
        testset "parent" {
            test "fast_test" { null }
            testset "slow_child" {
                baml.sys.sleep(1000)
                test "deep" { null }
            }
        }
    "#;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests(
            "user",
            CancellationToken::default(),
            Some(std::time::Duration::from_millis(1)),
            std::collections::HashSet::new(),
        )
        .await
        .expect("collect_tests should succeed");

    assert_eq!(registry.testsets.len(), 1);
    // "parent" itself is fast — it should be expanded
    let parent = unwrap_expanded(&registry.testsets[0]);
    assert_eq!(parent.name, "parent");
    assert_eq!(parent.tests.len(), 1, "parent should have 1 direct test");
    assert_eq!(parent.tests[0].name, "parent/fast_test");

    // "parent/slow_child" should be lazy (timed out)
    assert_eq!(parent.testsets.len(), 1);
    let (child_name, _) = unwrap_lazy(&parent.testsets[0]);
    assert_eq!(child_name, "parent/slow_child");
}
