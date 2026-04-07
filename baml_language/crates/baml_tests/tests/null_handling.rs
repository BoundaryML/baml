//! Runtime tests for null handling — optional chaining, safe assignment, null coalescing.
//!
//! Snapshot tests in `projects/null_handling/` cover compilation, type inference, and bytecode.
//! These tests cover **only** cases where correctness depends on which values are null at runtime:
//! short-circuit boundaries, safe-assignment skip-vs-write, and `??` lazy evaluation.
//!
//! Values are passed as function parameters to prevent the compiler from narrowing
//! nullable types at compile time (e.g., `let x: T? = null` narrows to `null` type).

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ============================================================================
// 1. LHS safe assignment: skip-vs-write
// ============================================================================

#[tokio::test]
async fn safe_assign_null_skips() {
    let output = baml_test! {
        baml: r#"
        class Profile {
            name string
            address string?
            scores int[]?
            settings map<string, string>?
        }
        class User {
            id int
            profile Profile?
        }
        function run(user: User?) -> string? {
            user?.profile?.name = "updated";
            user?.profile?.name
        }
        function main() -> string? {
            run(null)
        }
    "#,
        entry: "main"
    };

    assert_eq!(output.result, Ok(BexExternalValue::Null));
}

#[tokio::test]
async fn safe_assign_non_null_writes() {
    let output = baml_test! {
        baml: r#"
        class Profile {
            name string
            address string?
            scores int[]?
            settings map<string, string>?
        }
        class User {
            id int
            profile Profile?
        }
        function run(user: User?) -> string? {
            user?.profile?.name = "updated";
            user?.profile?.name
        }
        function main() -> string? {
            run(User {
                id: 1,
                profile: Profile { name: "old", address: null, scores: null, settings: null },
            })
        }
    "#,
        entry: "main"
    };

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("updated".into()))
    );
}

#[tokio::test]
async fn compound_assign_null_skips() {
    let output = baml_test! {
        baml: r#"
        class Profile {
            name string
            address string?
            scores int[]?
            settings map<string, string>?
        }
        class User {
            id int
            profile Profile?
        }
        function run(user: User?) -> int? {
            user?.id += 1;
            user?.id
        }
        function main() -> int? {
            run(null)
        }
    "#,
        entry: "main"
    };

    assert_eq!(output.result, Ok(BexExternalValue::Null));
}

#[tokio::test]
async fn compound_assign_non_null_increments() {
    let output = baml_test! {
        baml: r#"
        class Profile {
            name string
            address string?
            scores int[]?
            settings map<string, string>?
        }
        class User {
            id int
            profile Profile?
        }
        function run(user: User?) -> int? {
            user?.id += 1;
            user?.id
        }
        function main() -> int? {
            run(User { id: 10, profile: null })
        }
    "#,
        entry: "main"
    };

    assert_eq!(output.result, Ok(BexExternalValue::Int(11)));
}

#[tokio::test]
async fn safe_assign_index_null_skips() {
    let output = baml_test! {
        baml: r#"
        function run(items: int[]?) -> int? {
            items?.[0] = 42;
            items?.[0]
        }
        function main() -> int? {
            run(null)
        }
    "#,
        entry: "main"
    };

    assert_eq!(output.result, Ok(BexExternalValue::Null));
}

#[tokio::test]
async fn safe_assign_index_non_null_writes() {
    let output = baml_test! {
        baml: r#"
        function run(items: int[]?) -> int? {
            items?.[0] = 42;
            items?.[0]
        }
        function main() -> int? {
            run([0, 1, 2])
        }
    "#,
        entry: "main"
    };

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

// ============================================================================
// 2. ?? lazy RHS evaluation
// ============================================================================

#[tokio::test]
async fn null_coalesce_lazy_rhs_skips_side_effects() {
    let output = baml_test! {
        baml: r#"
        function run(value: int?) -> int {
            let counter = 0;
            let bump = () -> int { counter += 1; counter };
            let result = value ?? bump();
            // result should be 42 (LHS), counter should still be 0 (RHS never called)
            result * 10 + counter
        }
        function main() -> int {
            run(42)
        }
    "#,
        entry: "main"
    };

    assert_eq!(output.result, Ok(BexExternalValue::Int(420)));
}

#[tokio::test]
async fn null_coalesce_null_lhs_evaluates_rhs() {
    let output = baml_test! {
        baml: r#"
        function run(value: int?) -> int {
            let counter = 0;
            let bump = () -> int { counter += 1; counter };
            let result = value ?? bump();
            // result should be 1 (from bump), counter should be 1
            result * 10 + counter
        }
        function main() -> int {
            run(null)
        }
    "#,
        entry: "main"
    };

    assert_eq!(output.result, Ok(BexExternalValue::Int(11)));
}

// ============================================================================
// 3. Combined: compound assign through mixed chain with ??-provided fallback
// ============================================================================

#[tokio::test]
async fn combined_compound_assign_rhs_null_uses_fallback() {
    let output = baml_test! {
        baml: r#"
        class Node {
            value int
            next Node?
            children Node[]?
            labels map<string, Node>?
        }
        function run(node: Node?, items: int[]?) -> int? {
            node?.value += items?.[0] ?? 1;
            node?.value
        }
        function main() -> int? {
            run(
                Node { value: 10, next: null, children: null, labels: null },
                null,
            )
        }
    "#,
        entry: "main"
    };

    assert_eq!(output.result, Ok(BexExternalValue::Int(11)));
}

#[tokio::test]
async fn combined_compound_assign_lhs_null_skips() {
    let output = baml_test! {
        baml: r#"
        class Node {
            value int
            next Node?
            children Node[]?
            labels map<string, Node>?
        }
        function run(node: Node?, items: int[]?) -> int? {
            node?.value += items?.[0] ?? 1;
            node?.value
        }
        function main() -> int? {
            run(null, [5])
        }
    "#,
        entry: "main"
    };

    assert_eq!(output.result, Ok(BexExternalValue::Null));
}
