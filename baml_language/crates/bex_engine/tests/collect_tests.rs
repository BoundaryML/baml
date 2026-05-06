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
            Vec::new(),
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

/// Helper: serialize a registry via the registry's `serialize` method.
async fn serialize_registry(
    engine: &Arc<BexEngine>,
    registry: BexExternalValue,
) -> Result<BexExternalValue, bex_engine::EngineError> {
    engine
        .call_function(
            "testing.TestRegistry.serialize",
            vec![registry],
            bex_engine::FunctionCallContextBuilder::new(CallId::next()).build(),
            true,
        )
        .await
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

/// Helper: run a named test via the registry's `run_test` method.
async fn run_named_test(
    engine: &Arc<BexEngine>,
    registry: BexExternalValue,
    name: &str,
) -> Result<BexExternalValue, bex_engine::EngineError> {
    engine
        .call_function(
            "testing.TestRegistry.run_test",
            vec![registry, BexExternalValue::String(name.to_string())],
            bex_engine::FunctionCallContextBuilder::new(CallId::next()).build(),
            true,
        )
        .await
}

#[tokio::test]
async fn collect_tests_run_test_catches_typed_throwing_body() {
    let source = r#"
        function risky() -> void throws string {
            throw "boom"
        }

        test "throws become failure" {
            risky()
        }
    "#;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests("user", CallId::next(), CancellationToken::default())
        .await
        .expect("collect_tests should succeed");

    let report = run_named_test(&engine, registry, "throws become failure")
        .await
        .expect("run_test should normalize typed body throws into a report");

    let BexExternalValue::Instance { class_name, fields } = report else {
        panic!("expected TestReport instance, got: {report:?}");
    };
    assert_eq!(class_name, "testing.TestReport");

    match fields.get("outcome") {
        Some(BexExternalValue::String(outcome)) => assert_eq!(outcome, "fail"),
        Some(BexExternalValue::Union { value, .. }) => match value.as_ref() {
            BexExternalValue::String(outcome) => assert_eq!(outcome, "fail"),
            other => panic!("expected string outcome inside union, got: {other:?}"),
        },
        other => panic!("expected string outcome field, got: {other:?}"),
    }

    match fields.get("runs") {
        Some(BexExternalValue::Array { items, .. }) => {
            assert_eq!(items.len(), 1, "expected exactly one run record");
        }
        other => panic!("expected runs array field, got: {other:?}"),
    }
}

/// A testset with an inline for loop should expand successfully and find tests.
/// This is the "working" case from the bug report.
#[tokio::test]
async fn collect_tests_testset_inline_for_loop_expands() {
    let source = r#"
        testset "dynamic" {
            for (let case in ["a", "b", "c"]) {
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
#[tokio::test]
async fn collect_tests_testset_let_then_for_loop_expands() {
    let source = r#"
        testset "dynamic" {
            let cases: string[] = ["a", "b", "c"];
            for (let case in cases) {
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
    let result = expand_testset(&engine, registry, "dynamic")
        .await
        .expect("expand_set should succeed for let+for testset");
    assert!(
        !matches!(result, BexExternalValue::Null),
        "expected non-null result after expanding testset with let+for loop"
    );
}

/// Let-bound array indexing in testset body works.
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
            while (i < names.length()) {
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

    // Should have 3 tests registered (one per iteration: i=0,1,2)
    let result = expand_testset(&engine, registry, "whileloop")
        .await
        .expect("expand_set should succeed for let+while testset");
    let repr = format!("{result:?}");
    assert!(
        repr.contains("whileloop/item"),
        "expected tests registered from while loop: {repr}"
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

    let result = expand_testset(&engine, registry, "concat")
        .await
        .expect("expand_set should succeed for let+concat testset");
    let repr = format!("{result:?}");
    assert!(
        repr.contains("concat/hello_world"),
        "expected 'concat/hello_world': {repr}"
    );
}

/// If-condition reading a let-bound bool in a testset body.
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

    // The test "gated" should be registered since enabled=true
    let result = expand_testset(&engine, registry, "ifcond")
        .await
        .expect("expand_set should succeed for let+if testset");
    let repr = format!("{result:?}");
    assert!(
        repr.contains("ifcond/gated"),
        "expected test 'ifcond/gated' to be registered (enabled=true): {repr}"
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

/// Serializing a registry with unexpanded testsets should work (the match on
/// nullable `expansions.get(name)` should hit the `null` arm without error).
#[tokio::test]
async fn serialize_registry_with_unexpanded_testsets() {
    let source = r#"
        test "top_test" { null }
        testset "lazy_suite" {
            test "inner" { null }
        }
    "#;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests("user", CallId::next(), CancellationToken::default())
        .await
        .expect("collect_tests should succeed");

    // Serialize without expanding the testset first — this exercises the `null`
    // arm of the `match (expanded)` in `TestRegistry.serialize`.
    let serialized = serialize_registry(&engine, registry)
        .await
        .expect("serialize should succeed even with unexpanded testsets");
    assert!(
        !matches!(serialized, BexExternalValue::Null),
        "expected non-null serialized result, got: {serialized:?}"
    );
}

/// Testset with `for` loop and nested `testset <variable>` blocks.
/// Reproduces the pattern from the user's failing code:
/// ```
/// testset "vibes" {
///     let topics = ["happy", "sad"];
///     for (let sentiments in topics) {
///         testset sentiments { ... }
///     }
/// }
/// ```
#[tokio::test]
async fn collect_tests_testset_for_loop_with_nested_testset_variable_name() {
    let source = r#"
        testset "vibes" {
            let topics: string[] = ["happy", "sad"];
            for (let topic in topics) {
                testset topic {
                    test "check" {
                        assert.not_null(topic)
                    }
                }
            }
        }
    "#;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests("user", CallId::next(), CancellationToken::default())
        .await
        .expect("collect_tests should succeed");

    // Expand the outer testset
    let result = expand_testset(&engine, registry.clone(), "vibes").await;
    match &result {
        Ok(v) => {
            assert!(
                !matches!(v, BexExternalValue::Null),
                "expected non-null result after expanding 'vibes' testset"
            );
        }
        Err(e) => {
            panic!("expand_set failed: {e}");
        }
    }
}

/// Testset with for loop over array, test block uses loop variable in assertion.
/// Tests basic for-loop + test body with captured variable.
#[tokio::test]
async fn collect_tests_testset_for_loop_test_uses_loop_var() {
    let source = r#"
        testset "suite" {
            let items: string[] = ["a", "b"];
            for (let item in items) {
                test item {
                    assert.not_null(item)
                }
            }
        }
    "#;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests("user", CallId::next(), CancellationToken::default())
        .await
        .expect("collect_tests should succeed");

    let result = expand_testset(&engine, registry, "suite").await;
    match &result {
        Ok(v) => {
            assert!(
                !matches!(v, BexExternalValue::Null),
                "expected non-null after expanding 'suite'"
            );
        }
        Err(e) => {
            panic!("expand_set for 'suite' failed: {e}");
        }
    }
}

/// Top-level testset "test" with a nested testset using string concat name.
/// This mimics the user's `testset "test" { ... }` pattern.
#[tokio::test]
async fn collect_tests_top_level_testset_with_string_concat_name() {
    let source = r#"
        testset "test" {
            let topics: string[] = ["happy", "sad"];
            for (let topic in topics) {
                testset topic {
                    test "basic " + topic {
                        assert.not_null(topic)
                    }
                }
            }
        }
    "#;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests("user", CallId::next(), CancellationToken::default())
        .await
        .expect("collect_tests should succeed");

    let result = expand_testset(&engine, registry, "test").await;
    match &result {
        Ok(v) => {
            assert!(
                !matches!(v, BexExternalValue::Null),
                "expected non-null after expanding 'test'"
            );
        }
        Err(e) => {
            panic!("expand_set for 'test' failed: {e}");
        }
    }
}

/// Testset with function call + assert.equal in test body.
/// Reproduces the user's pattern where test body calls a function
/// and uses `assert.equal(result.feeling, "positive")`.
#[tokio::test]
async fn collect_tests_testset_with_function_call_and_field_access() {
    let source = r##"
        class Sentiment {
            feeling string
            confidence float
        }

        function ClassifySentiment(text: string) -> Sentiment {
            client GPT4o
            prompt #"classify {{ text }}"#
        }

        client<llm> GPT4o {
            provider openai
            options {
                model "gpt-4o"
                api_key env.OPENAI_API_KEY
            }
        }

        testset "vibes" {
            let topics: string[] = ["happy", "sad"];
            for (let topic in topics) {
                testset topic {
                    test "basic" {
                        let result = ClassifySentiment("hi");
                        assert.equal(result.feeling, "positive");
                    }
                }
            }
        }
    "##;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests("user", CallId::next(), CancellationToken::default())
        .await
        .expect("collect_tests should succeed");

    let result = expand_testset(&engine, registry, "vibes").await;
    result.expect("expand_set for vibes should succeed");
}

/// Reproduces the user's EXACT file — "vibes" is nested INSIDE "test".
/// Includes union-return-type function (`Foo() -> string | image`) to check
/// if Discriminant instructions cause "expected variant, got any" errors.
#[tokio::test]
async fn collect_tests_user_exact_file_full_lifecycle() {
    // NOTE: "vibes" is INSIDE "test" — the closing brace of "test" comes
    // AFTER "vibes". This is the exact structure from the user's file.
    let source = r##"
        client<llm> GPT4o {
            provider openai
            options {
                model "gpt-4o"
                api_key env.OPENAI_API_KEY
            }
        }

        class Sentiment {
            feeling string @description("The detected sentiment")
            confidence float @description("Confidence score between 0 and 1")
            reasoning string @description("Brief explanation")
        }

        function ClassifySentiment(text: string) -> Sentiment {
            client GPT4o
            prompt #"
                {{ _.role('system') }}
                Classify the sentiment of the following text.
                {{ ctx.output_format }}
                {{ _.role('assistant') }}
                Text: {{ text }}
            "#
        }

        testset "test" {
            let topics = ["happy", "sad"];
            for (let sentiments in topics) {
                testset sentiments {
                    let req = baml.http.fetch("http://localhost:8000/" + sentiments);
                    let data = req.text();
                    let tests = GenerateTests$parse(data);
                    for (let ex in tests) {
                        test ex {
                            let result = ClassifySentiment("hi");
                            assert.equal(result.feeling, "positive");
                        }
                    }
                }
            }

            testset "vibes" {
                let topics = ["happy", "sad"];
                for (let sentiments in topics) {
                    testset sentiments {
                        let tests = GenerateTests(5, sentiments);
                        for (let ex in tests) {
                            test ex {
                                let result = ClassifySentiment("hi");
                                assert.equal(result.feeling, "positive");
                            }
                        }
                    }
                }
            }
        }

        function Foo() -> string | image {
            let x = "foo";
            let y = 2;
            for (let i = 0; i < y; i += 1) {
                x = "hi:  " + x
            }
            x
        }

        function ClassifySentiment2(text: string) -> string {
            client GPT4o
            prompt #"classify {{ text }}"#
        }

        function GenerateTests(count: int, topic: string) -> string[] {
            client GPT4o
            prompt #"generate {{ count }} tests about {{ topic }}"#
        }
    "##;

    let engine = make_engine(source);

    // Step 1: Collect tests
    let registry = engine
        .collect_tests("user", CallId::next(), CancellationToken::default())
        .await
        .expect("collect_tests should succeed");

    // Step 2: Serialize before expansion
    serialize_registry(&engine, registry.clone())
        .await
        .expect("serialize before expansion should succeed");

    // Step 3: Expand "test" testset (runs collector lambda with for loops + nested testsets)
    expand_testset(&engine, registry.clone(), "test")
        .await
        .expect("expand 'test' should succeed");

    // Step 4: Serialize after expanding "test"
    serialize_registry(&engine, registry.clone())
        .await
        .expect("serialize after expanding 'test' should succeed");

    // Step 5: Expand "test/vibes" (second-level nested testset)
    expand_testset(&engine, registry.clone(), "test/vibes")
        .await
        .expect("expand 'test/vibes' should succeed");

    // Step 6: Expand third-level testsets.
    // "test/vibes/happy" collector lambda calls GenerateTests(5, "happy") which
    // is an LLM function. Without OPENAI_API_KEY, the expansion fails with a
    // runtime error — but crucially NOT "expected variant, got any" (which was
    // the bug). The expected error is about the missing env var.
    for name in &[
        "test/happy",
        "test/sad",
        "test/vibes/happy",
        "test/vibes/sad",
    ] {
        let result = expand_testset(&engine, registry.clone(), name).await;
        if let Err(e) = &result {
            let err_str = format!("{e}");
            // The "expected variant, got any" error was the bug we fixed.
            // Any other error (like missing env var) is acceptable.
            assert!(
                !err_str.contains("expected variant, got any"),
                "BUG: enum variant not resolved for testset '{name}': {e}"
            );
        }
    }

    // Step 7: Final serialize
    serialize_registry(&engine, registry)
        .await
        .expect("final serialize should succeed");
}

/// Expansion of a nested testset that itself contains async operations.
/// After expanding the outer testset, expand the inner ones to exercise
/// the full depth of the testset tree.
#[tokio::test]
async fn collect_tests_expand_nested_testsets_full_depth() {
    let source = r#"
        testset "outer" {
            let items: string[] = ["alpha", "beta"];
            for (let item in items) {
                testset item {
                    test "check" {
                        assert.not_null(item)
                    }
                }
            }
        }
    "#;

    let engine = make_engine(source);
    let registry = engine
        .collect_tests("user", CallId::next(), CancellationToken::default())
        .await
        .expect("collect_tests should succeed");

    // Expand outer
    expand_testset(&engine, registry.clone(), "outer")
        .await
        .expect("expand 'outer' should succeed");

    // Serialize after outer expansion
    serialize_registry(&engine, registry.clone())
        .await
        .expect("serialize after outer expand should succeed");

    // The inner testsets are registered with full path names ("outer/alpha", "outer/beta").
    // Expand them to exercise full-depth async expansion.
    expand_testset(&engine, registry.clone(), "outer/alpha")
        .await
        .expect("expand 'outer/alpha' should succeed");

    expand_testset(&engine, registry.clone(), "outer/beta")
        .await
        .expect("expand 'outer/beta' should succeed");

    // Final serialize
    let final_ser = serialize_registry(&engine, registry).await;
    final_ser.expect("final serialize should succeed");
}
