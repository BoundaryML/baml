//! Unified tests for array construction and methods.

use baml_tests::baml_test;
use baml_type::Ty;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn array_literal() {
    let output = baml_test!(
        "
        function main() -> int[] {
            [1, 2, 3]
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int[] {
        load_const 1
        load_const 2
        load_const 3
        alloc_array 3
        return
    }
    ");

    assert_eq!(
        output.result,
        Ok(BexExternalValue::Array {
            element_type: Ty::int(),
            items: vec![
                BexExternalValue::Int(1),
                BexExternalValue::Int(2),
                BexExternalValue::Int(3),
            ],
        })
    );
}

#[tokio::test]
async fn array_assign_to_variable() {
    let output = baml_test!(
        "
        function main() -> int[] {
            let a = [1, 2, 3];
            a
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int[] {
        load_const 1
        load_const 2
        load_const 3
        alloc_array 3
        return
    }
    ");

    assert_eq!(
        output.result,
        Ok(BexExternalValue::Array {
            element_type: Ty::int(),
            items: vec![
                BexExternalValue::Int(1),
                BexExternalValue::Int(2),
                BexExternalValue::Int(3),
            ],
        })
    );
}

#[tokio::test]
async fn array_push() {
    let output = baml_test!(
        "
        function main() -> int[] {
            let a: int[] = [1, 2, 3];
            a.push(4);
            a
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int[] {
        load_const 1
        load_const 2
        load_const 3
        alloc_array 3
        store_var a
        load_var a
        load_const 4
        call baml.Array.push
        pop 1
        load_var a
        return
    }
    ");

    assert_eq!(
        output.result,
        Ok(BexExternalValue::Array {
            element_type: Ty::int(),
            items: vec![
                BexExternalValue::Int(1),
                BexExternalValue::Int(2),
                BexExternalValue::Int(3),
                BexExternalValue::Int(4),
            ],
        })
    );
}

// ============================================================================
// Array.map tests
// ============================================================================

/// array.map with a simple doubling function (no captures).
/// [1, 2, 3].map(x -> x * 2) returns [2, 4, 6].
#[tokio::test]
async fn array_map_simple_double() {
    let output = baml_test!(
        "
        function main() -> int[] {
            let items: int[] = [1, 2, 3]
            items.map((x: int) -> int { x * 2 })
        }
    "
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Array {
            element_type: Ty::int(),
            items: vec![
                BexExternalValue::Int(2),
                BexExternalValue::Int(4),
                BexExternalValue::Int(6),
            ],
        })
    );
}

/// array.map on an empty array returns [].
#[tokio::test]
async fn array_map_empty_array() {
    let output = baml_test!(
        "
        function main() -> int[] {
            let items: int[] = []
            items.map((x: int) -> int { x * 2 })
        }
    "
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Array {
            element_type: Ty::int(),
            items: vec![],
        })
    );
}

/// array.map with a single-element array.
/// [42].map(x -> x + 1) returns [43].
#[tokio::test]
async fn array_map_single_element() {
    let output = baml_test!(
        "
        function main() -> int[] {
            let items: int[] = [42]
            items.map((x: int) -> int { x + 1 })
        }
    "
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Array {
            element_type: Ty::int(),
            items: vec![BexExternalValue::Int(43)],
        })
    );
}

/// array.map with a closure capturing multiple variables.
/// [1, 2, 3].map(x -> x * scale + bias) where scale=10, bias=5 returns [15, 25, 35].
#[tokio::test]
async fn array_map_closure_captures_multiple_variables() {
    let output = baml_test!(
        "
        function main() -> int[] {
            let scale = 10
            let bias = 5
            let items: int[] = [1, 2, 3]
            items.map((x: int) -> int { x * scale + bias })
        }
    "
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Array {
            element_type: Ty::int(),
            items: vec![
                BexExternalValue::Int(15),
                BexExternalValue::Int(25),
                BexExternalValue::Int(35),
            ],
        })
    );
}

/// array.map where the callback throws — verify exception propagates through
/// the CPS trampoline and can be caught.
#[tokio::test]
async fn array_map_callback_throws() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let items: int[] = [1, 2, 3]
            items.map((x: int) -> int {
                if (x == 2) { throw "bad value" }
                x
            }) catch (e) {
                _ => "caught"
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::String("caught".into())));
}

/// array.map over string[] — exercises heap-object paths in MapContinuation
/// (gc_roots, apply_forwarding) that int[] tests don't cover.
#[tokio::test]
async fn array_map_string_elements() {
    let output = baml_test!(
        r#"
        function main() -> string[] {
            let items: string[] = ["a", "b", "c"]
            items.map((s: string) -> string { s + "!" })
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Array {
            element_type: Ty::String {
                attr: baml_base::TyAttr::default()
            },
            items: vec![
                BexExternalValue::String("a!".into()),
                BexExternalValue::String("b!".into()),
                BexExternalValue::String("c!".into()),
            ],
        })
    );
}
