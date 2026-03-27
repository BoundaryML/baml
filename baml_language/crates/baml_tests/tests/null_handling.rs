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

/// Unwrap an optional (Union) wrapper to get the inner value for simpler assertions.
fn unwrap_optional(val: BexExternalValue) -> BexExternalValue {
    match val {
        BexExternalValue::Union {
            value, metadata, ..
        } if metadata.is_optional && metadata.is_single_pattern => *value,
        other => other,
    }
}

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

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string? {
        load_const null
        call user.run
        return
    }

    function run(user: User?) -> string? {
        load_var user
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L0
        jump L2

      L0:
        load_var user
        load_field .1
        store_var _2
        load_var _2
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L1
        jump L2

      L1:
        load_var _2
        load_const "updated"
        store_field .0

      L2:
        load_var user
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L3
        jump L5

      L3:
        load_var user
        load_field .1
        store_var _5
        load_var _5
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L4
        jump L5

      L4:
        load_var _5
        load_field .0
        jump L6

      L5:
        load_const null

      L6:
        return
    }
    "#);

    assert_eq!(output.result.map(unwrap_optional), Ok(BexExternalValue::Null));
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

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string? {
        alloc_instance User
        copy 0
        load_const 1
        store_field .id
        copy 0
        alloc_instance Profile
        copy 0
        load_const "old"
        store_field .name
        copy 0
        load_const null
        store_field .address
        copy 0
        load_const null
        store_field .scores
        copy 0
        load_const null
        store_field .settings
        store_field .profile
        call user.run
        return
    }

    function run(user: User?) -> string? {
        load_var user
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L0
        jump L2

      L0:
        load_var user
        load_field .1
        store_var _2
        load_var _2
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L1
        jump L2

      L1:
        load_var _2
        load_const "updated"
        store_field .0

      L2:
        load_var user
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L3
        jump L5

      L3:
        load_var user
        load_field .1
        store_var _5
        load_var _5
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L4
        jump L5

      L4:
        load_var _5
        load_field .0
        jump L6

      L5:
        load_const null

      L6:
        return
    }
    "#);

    assert_eq!(output.result.map(unwrap_optional), Ok(BexExternalValue::String("updated".into())));
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int? {
        load_const null
        call user.run
        return
    }

    function run(user: User?) -> int? {
        load_var user
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_var user
        load_var user
        load_field .0
        load_const 1
        bin_op +
        store_field .0

      L1:
        load_var user
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L2
        jump L3

      L2:
        load_var user
        load_field .0
        jump L4

      L3:
        load_const null

      L4:
        return
    }
    ");

    assert_eq!(output.result.map(unwrap_optional), Ok(BexExternalValue::Null));
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int? {
        alloc_instance User
        copy 0
        load_const 10
        store_field .id
        copy 0
        load_const null
        store_field .profile
        call user.run
        return
    }

    function run(user: User?) -> int? {
        load_var user
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_var user
        load_var user
        load_field .0
        load_const 1
        bin_op +
        store_field .0

      L1:
        load_var user
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L2
        jump L3

      L2:
        load_var user
        load_field .0
        jump L4

      L3:
        load_const null

      L4:
        return
    }
    ");

    assert_eq!(output.result.map(unwrap_optional), Ok(BexExternalValue::Int(11)));
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int? {
        load_const null
        call user.run
        return
    }

    function run(items: int[]?) -> int? {
        load_var items
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_var items
        load_const 0
        load_const 42
        store_array_element

      L1:
        load_var items
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L2
        jump L3

      L2:
        load_var items
        load_const 0
        load_array_element
        jump L4

      L3:
        load_const null

      L4:
        return
    }
    ");

    assert_eq!(output.result.map(unwrap_optional), Ok(BexExternalValue::Null));
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int? {
        load_const 0
        load_const 1
        load_const 2
        alloc_array 3
        call user.run
        return
    }

    function run(items: int[]?) -> int? {
        load_var items
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_var items
        load_const 0
        load_const 42
        store_array_element

      L1:
        load_var items
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L2
        jump L3

      L2:
        load_var items
        load_const 0
        load_array_element
        jump L4

      L3:
        load_const null

      L4:
        return
    }
    ");

    assert_eq!(output.result.map(unwrap_optional), Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
#[ignore = "assignment-as-expression return value not yet implemented"]
async fn safe_assign_return_value_null() {
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
            let result = (user?.profile?.name = "new_name");
            result
        }
        function main() -> string? {
            run(null)
        }
    "#,
        entry: "main"
    };

    insta::assert_snapshot!(output.bytecode, @"");

    assert_eq!(output.result.map(unwrap_optional), Ok(BexExternalValue::Null));
}

#[tokio::test]
#[ignore = "assignment-as-expression return value not yet implemented"]
async fn safe_assign_return_value_non_null() {
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
            let result = (user?.profile?.name = "new_name");
            result
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

    insta::assert_snapshot!(output.bytecode, @"");

    assert_eq!(output.result.map(unwrap_optional), Ok(BexExternalValue::String("new_name".into())));
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 42
        call user.run
        return
    }

    function run(value: int?) -> int {
        load_var ?2
        make_cell
        store_var ?2
        load_const 0
        store_deref ?2
        load_var value
        load_var value
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L0
        load_var counter
        make_closure .<lambda(run, 0)>, 1
        call_indirect

      L0:
        load_const 10
        bin_op *
        load_deref ?2
        bin_op +
        return
    }
    ");

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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const null
        call user.run
        return
    }

    function run(value: int?) -> int {
        load_var ?2
        make_cell
        store_var ?2
        load_const 0
        store_deref ?2
        load_var value
        load_var value
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L0
        load_var counter
        make_closure .<lambda(run, 0)>, 1
        call_indirect

      L0:
        load_const 10
        bin_op *
        load_deref ?2
        bin_op +
        return
    }
    ");

    assert_eq!(output.result.map(unwrap_optional), Ok(BexExternalValue::Int(11)));
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int? {
        alloc_instance Node
        copy 0
        load_const 10
        store_field .value
        copy 0
        load_const null
        store_field .next
        copy 0
        load_const null
        store_field .children
        copy 0
        load_const null
        store_field .labels
        load_const null
        call user.run
        return
    }

    function run(node: Node?, items: int[]?) -> int? {
        load_var node
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L0
        jump L5

      L0:
        load_var items
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L1
        jump L2

      L1:
        load_var items
        load_const 0
        load_array_element
        store_var _6
        jump L3

      L2:
        load_const null
        store_var _6

      L3:
        load_var _6
        store_var _5
        load_var _6
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L4
        load_const 1
        store_var _5

      L4:
        load_var node
        load_var node
        load_field .0
        load_var _5
        bin_op +
        store_field .0

      L5:
        load_var node
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L6
        jump L7

      L6:
        load_var node
        load_field .0
        jump L8

      L7:
        load_const null

      L8:
        return
    }
    ");

    assert_eq!(output.result.map(unwrap_optional), Ok(BexExternalValue::Int(11)));
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

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int? {
        load_const null
        load_const 5
        alloc_array 1
        call user.run
        return
    }

    function run(node: Node?, items: int[]?) -> int? {
        load_var node
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L0
        jump L5

      L0:
        load_var items
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L1
        jump L2

      L1:
        load_var items
        load_const 0
        load_array_element
        store_var _6
        jump L3

      L2:
        load_const null
        store_var _6

      L3:
        load_var _6
        store_var _5
        load_var _6
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L4
        load_const 1
        store_var _5

      L4:
        load_var node
        load_var node
        load_field .0
        load_var _5
        bin_op +
        store_field .0

      L5:
        load_var node
        type_tag
        load_const 3
        cmp_op ==
        pop_jump_if_false L6
        jump L7

      L6:
        load_var node
        load_field .0
        jump L8

      L7:
        load_const null

      L8:
        return
    }
    ");

    assert_eq!(output.result.map(unwrap_optional), Ok(BexExternalValue::Null));
}
