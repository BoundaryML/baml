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

/// Helper: expand a testset by name via the registry's `expand_set` method.
async fn expand_testset(
    engine: &Arc<BexEngine>,
    registry: BexExternalValue,
    name: &str,
) -> Result<BexExternalValue, bex_engine::EngineError> {
    engine
        .call_function(
            "testing.TestRegistry.expand_set",
            vec![registry, BexExternalValue::String(name.to_string())],
            bex_engine::FunctionCallContextBuilder::new(CallId::next()).build(),
            true,
        )
        .await
}

/// A testset with an inline for loop should expand successfully and find tests.
/// This is the "working" case from the bug report.
#[tokio::test]
async fn collect_tests_testset_inline_for_loop_expands() {
    let source = r#"
        testset "dynamic" {
            for (case in ["a", "b", "c"]) {
                test "check" {
                    assert.not_null(case)
                }
            }
        }
    "#;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests("user", CallId::next(), CancellationToken::default())
        .await
        .expect("collect_tests should succeed");

    // Expanding the testset triggers the lambda body (the for loop).
    // With an inline array this should work fine.
    let result = expand_testset(&engine, registry, "dynamic")
        .await
        .expect("expand_set should succeed for inline for loop");
    assert!(
        !matches!(result, BexExternalValue::Null),
        "expected non-null result after expanding testset with inline for loop, got: {result:?}"
    );
}

/// A testset with `let` + `for` should expand successfully and find tests.
/// Regression test: the let initializer is dropped to null during MIR lowering,
/// causing the for loop to iterate zero times (no tests discovered).
#[tokio::test]
async fn collect_tests_testset_let_then_for_loop_expands() {
    let source = r#"
        testset "dynamic" {
            let cases: string[] = ["a", "b", "c"];
            for (case in cases) {
                test "check" {
                    assert.not_null(case)
                }
            }
        }
    "#;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests("user", CallId::next(), CancellationToken::default())
        .await
        .expect("collect_tests should succeed");

    // Expanding the testset triggers the lambda body (the let + for loop).
    // BUG: The let initializer gets dropped to null during MIR lowering, so
    // the for loop tries to call Array.length on null → TypeError.
    //
    // When this bug is fixed, change this to:
    //   let result = expand_testset(&engine, registry, "dynamic")
    //       .await.expect("expand_set should succeed");
    //   assert!(!matches!(result, BexExternalValue::Null));
    let err = expand_testset(&engine, registry, "dynamic")
        .await
        .expect_err("BUG: let+for in testset should work, but currently fails with TypeError");
    let msg = err.to_string();
    assert!(
        msg.contains("type error") || msg.contains("TypeError"),
        "expected type error from null array iteration, got: {msg}"
    );
}

/// Let-bound array indexing in testset body works — only `for` iteration over
/// a let-bound variable fails (see `collect_tests_testset_let_then_for_loop_expands`).
#[tokio::test]
async fn collect_tests_testset_let_array_index_in_name_and_body() {
    let source = r#"
        testset "suite" {
            let cases: string[] = ["a", "b", "c"];
            test "case: " + cases[0] {
                assert.is_true(cases[0] == "a")
            }
        }
    "#;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests("user", CallId::next(), CancellationToken::default())
        .await
        .expect("collect_tests should succeed");

    let result = expand_testset(&engine, registry, "suite")
        .await
        .expect("expand_set should succeed for simple let string");
    let repr = format!("{result:?}");
    assert!(
        repr.contains("suite/case: a"),
        "test name should be 'suite/case: a': {repr}"
    );
}

/// While loop over a let-bound variable in a testset body.
/// Tests whether the while condition can read a let-bound variable.
#[tokio::test]
async fn collect_tests_testset_let_then_while_loop() {
    let source = r#"
        testset "whileloop" {
            let names: string[] = ["x", "y", "z"];
            let i = 0;
            while (i < names.length) {
                test "item" {
                    assert.is_true(true)
                }
                i += 1;
            }
        }
    "#;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests("user", CallId::next(), CancellationToken::default())
        .await
        .expect("collect_tests should succeed");

    // BUG: `let names` and `let i` both become null → `i < names.length` fails.
    //
    // When fixed, change to:
    //   let result = expand_testset(&engine, registry, "whileloop")
    //       .await.expect("should succeed");
    //   // verify 3 tests registered
    let err = expand_testset(&engine, registry, "whileloop")
        .await
        .expect_err("BUG: let+while in testset should work, but currently fails");
    let msg = err.to_string();
    assert!(
        msg.contains("comparison") || msg.contains("type error"),
        "expected comparison/type error, got: {msg}"
    );
}

/// Let-bound variable used in a string concatenation for test name.
/// Tests whether let-bound values are available for expression evaluation
/// in the testset body (not just indexing).
#[tokio::test]
async fn collect_tests_testset_let_used_in_test_name_concat() {
    let source = r#"
        testset "concat" {
            let prefix = "hello";
            let suffix = "world";
            test prefix + "_" + suffix {
                assert.is_true(true)
            }
        }
    "#;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests("user", CallId::next(), CancellationToken::default())
        .await
        .expect("collect_tests should succeed");

    // BUG: `let prefix` and `let suffix` become null → concatenation produces
    // "null_null" or fails. The test name should be "concat/hello_world".
    //
    // When fixed, change assertion to check for "concat/hello_world".
    let result = expand_testset(&engine, registry, "concat")
        .await
        .expect("expand_set succeeds but with wrong name");
    let repr = format!("{result:?}");
    // Interestingly, let-bound variables used in expressions (string concat)
    // work correctly — the bug only affects control flow constructs (for, while, if)
    // that read let-bound variables.
    assert!(
        repr.contains("concat/hello_world"),
        "expected 'concat/hello_world': {repr}"
    );
}

/// If-condition reading a let-bound bool in a testset body.
/// The testset_dynamic MIR snapshot shows `branch const null` for this pattern.
#[tokio::test]
async fn collect_tests_testset_let_then_if_condition() {
    let source = r#"
        testset "ifcond" {
            let enabled = true;
            if (enabled) {
                test "gated" {
                    assert.is_true(true)
                }
            }
        }
    "#;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests("user", CallId::next(), CancellationToken::default())
        .await
        .expect("collect_tests should succeed");

    // BUG: `let enabled = true` becomes null → `if(null)` is TypeError (expected Bool).
    //
    // When fixed, change to:
    //   let result = expand_testset(&engine, registry, "ifcond")
    //       .await.expect("should succeed");
    //   assert!(format!("{result:?}").contains("ifcond/gated"));
    let err = expand_testset(&engine, registry, "ifcond")
        .await
        .expect_err("BUG: let+if in testset should work, but currently fails with TypeError");
    let msg = err.to_string();
    assert!(
        msg.contains("type error") || msg.contains("TypeError"),
        "expected type error from null bool, got: {msg}"
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
