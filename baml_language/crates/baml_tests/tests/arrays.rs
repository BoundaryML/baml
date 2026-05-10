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

/// array.map callback inside a catch base still resolves its own parameter,
/// even though the catch handler introduces a later lexical scope.
#[tokio::test]
async fn array_map_callback_in_catch_base_keeps_parameter_scope() {
    let output = baml_test!(
        r#"
        function main() -> int[] {
            let items: int[] = [1, 2, 3]
            items.map((x: int) -> int {
                if (x == 2) { x + 10 } else { x }
            }) catch (e) {
                _ => [0]
            }
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Array {
            element_type: Ty::int(),
            items: vec![
                BexExternalValue::Int(1),
                BexExternalValue::Int(12),
                BexExternalValue::Int(3),
            ],
        })
    );
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

// ─── Helpers for the BEP-043 mut-self method tests ────────────────────────────

fn int_arr(items: &[i64]) -> BexExternalValue {
    BexExternalValue::Array {
        element_type: Ty::int(),
        items: items.iter().copied().map(BexExternalValue::Int).collect(),
    }
}

async fn expect_throw(src: &str, expected_class: &str) {
    let output = baml_test!(baml: src, entry: "main");
    let Err(bex_engine::EngineError::UnhandledThrow { value, .. }) = &output.result else {
        panic!(
            "expected UnhandledThrow({expected_class}), got: {:?}",
            output.result
        );
    };
    let BexExternalValue::Instance { class_name, .. } = value.as_ref() else {
        panic!("expected throw Instance({expected_class}), got: {value:?}");
    };
    assert_eq!(class_name, expected_class);
}

// ─── clear ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn array_clear_non_empty() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let xs = [1, 2, 3];
            xs.clear();
            xs.length()
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn array_clear_already_empty_is_noop() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let xs: int[] = [];
            xs.clear();
            xs.length()
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn array_clear_returns_array_after() {
    let output = baml_test!(
        r#"
        function main() -> int[] {
            let xs = [1, 2, 3];
            xs.clear();
            xs
        }
    "#
    );
    assert_eq!(output.result, Ok(int_arr(&[])));
}

// ─── shift ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn array_shift_returns_first_element() {
    let output = baml_test!(
        r#"
        function main() -> int? {
            let xs = [10, 20, 30];
            xs.shift()
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(10)));
}

#[tokio::test]
async fn array_shift_modifies_array_in_place() {
    let output = baml_test!(
        r#"
        function main() -> int[] {
            let xs = [10, 20, 30];
            let _ = xs.shift();
            xs
        }
    "#
    );
    assert_eq!(output.result, Ok(int_arr(&[20, 30])));
}

#[tokio::test]
async fn array_shift_empty_returns_null() {
    let output = baml_test!(
        r#"
        function main() -> int? {
            let xs: int[] = [];
            xs.shift()
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Null));
}

#[tokio::test]
async fn array_shift_repeated_drains_array() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let xs = [1, 2, 3];
            let _ = xs.shift();
            let _ = xs.shift();
            let _ = xs.shift();
            xs.length()
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

// ─── unshift ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn array_unshift_returns_new_length() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let xs = [2, 3];
            xs.unshift(1)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn array_unshift_inserts_at_front() {
    let output = baml_test!(
        r#"
        function main() -> int[] {
            let xs = [2, 3];
            let _ = xs.unshift(1);
            xs
        }
    "#
    );
    assert_eq!(output.result, Ok(int_arr(&[1, 2, 3])));
}

#[tokio::test]
async fn array_unshift_empty_array() {
    let output = baml_test!(
        r#"
        function main() -> int[] {
            let xs: int[] = [];
            let _ = xs.unshift(42);
            xs
        }
    "#
    );
    assert_eq!(output.result, Ok(int_arr(&[42])));
}

// ─── insert ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn array_insert_in_middle() {
    let output = baml_test!(
        r#"
        function main() -> int[] {
            let xs = [1, 2, 4];
            let _ = xs.insert(3, 2);
            xs
        }
    "#
    );
    assert_eq!(output.result, Ok(int_arr(&[1, 2, 3, 4])));
}

#[tokio::test]
async fn array_insert_at_zero() {
    let output = baml_test!(
        r#"
        function main() -> int[] {
            let xs = [2, 3];
            let _ = xs.insert(1, 0);
            xs
        }
    "#
    );
    assert_eq!(output.result, Ok(int_arr(&[1, 2, 3])));
}

#[tokio::test]
async fn array_insert_at_length_appends() {
    let output = baml_test!(
        r#"
        function main() -> int[] {
            let xs = [1, 2];
            let _ = xs.insert(3, xs.length());
            xs
        }
    "#
    );
    assert_eq!(output.result, Ok(int_arr(&[1, 2, 3])));
}

#[tokio::test]
async fn array_insert_returns_new_length() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let xs = [1, 2, 4];
            xs.insert(3, 2)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(4)));
}

#[tokio::test]
async fn array_insert_empty_array_at_zero() {
    let output = baml_test!(
        r#"
        function main() -> int[] {
            let xs: int[] = [];
            let _ = xs.insert(99, 0);
            xs
        }
    "#
    );
    assert_eq!(output.result, Ok(int_arr(&[99])));
}

#[tokio::test]
async fn array_insert_negative_throws() {
    expect_throw(
        r#"
        function main() -> int {
            let xs = [1, 2, 3];
            xs.insert(0, -1)
        }
    "#,
        "baml.errors.InvalidArgument",
    )
    .await;
}

#[tokio::test]
async fn array_insert_past_length_throws() {
    expect_throw(
        r#"
        function main() -> int {
            let xs = [1, 2, 3];
            xs.insert(0, 100)
        }
    "#,
        "baml.errors.InvalidArgument",
    )
    .await;
}

// ─── splice ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn array_splice_replaces_range() {
    let output = baml_test!(
        r#"
        function main() -> int[] {
            let xs = [0, 1, 2, 3, 4];
            xs.splice(1, 2, [9, 9, 9]);
            xs
        }
    "#
    );
    assert_eq!(output.result, Ok(int_arr(&[0, 9, 9, 9, 3, 4])));
}

#[tokio::test]
async fn array_splice_pure_insertion() {
    let output = baml_test!(
        r#"
        function main() -> int[] {
            let xs = [1, 2, 3];
            xs.splice(0, 0, [-1]);
            xs
        }
    "#
    );
    assert_eq!(output.result, Ok(int_arr(&[-1, 1, 2, 3])));
}

#[tokio::test]
async fn array_splice_pure_removal() {
    let output = baml_test!(
        r#"
        function main() -> int[] {
            let xs = [0, 1, 2, 3];
            xs.splice(1, 2, []);
            xs
        }
    "#
    );
    assert_eq!(output.result, Ok(int_arr(&[0, 3])));
}

#[tokio::test]
async fn array_splice_count_clamped_to_length() {
    let output = baml_test!(
        r#"
        function main() -> int[] {
            let xs = [0, 1, 2];
            xs.splice(1, 99, [9]);
            xs
        }
    "#
    );
    assert_eq!(output.result, Ok(int_arr(&[0, 9])));
}

#[tokio::test]
async fn array_splice_at_length_appends() {
    let output = baml_test!(
        r#"
        function main() -> int[] {
            let xs = [0, 1];
            xs.splice(2, 0, [2, 3]);
            xs
        }
    "#
    );
    assert_eq!(output.result, Ok(int_arr(&[0, 1, 2, 3])));
}

#[tokio::test]
async fn array_splice_replace_with_longer() {
    let output = baml_test!(
        r#"
        function main() -> int[] {
            let xs = [0, 1, 2];
            xs.splice(1, 1, [10, 11, 12]);
            xs
        }
    "#
    );
    assert_eq!(output.result, Ok(int_arr(&[0, 10, 11, 12, 2])));
}

#[tokio::test]
async fn array_splice_negative_start_throws() {
    expect_throw(
        r#"
        function main() -> null {
            let xs = [1, 2, 3];
            xs.splice(-1, 0, [])
        }
    "#,
        "baml.errors.InvalidArgument",
    )
    .await;
}

#[tokio::test]
async fn array_splice_start_past_length_throws() {
    expect_throw(
        r#"
        function main() -> null {
            let xs = [1, 2, 3];
            xs.splice(99, 0, [])
        }
    "#,
        "baml.errors.InvalidArgument",
    )
    .await;
}

#[tokio::test]
async fn array_splice_negative_count_throws() {
    expect_throw(
        r#"
        function main() -> null {
            let xs = [1, 2, 3];
            xs.splice(0, -1, [])
        }
    "#,
        "baml.errors.InvalidArgument",
    )
    .await;
}

// ─── Helpers for callback scan tests ──────────────────────────────────────────

async fn run_int(src: &str) -> i64 {
    let output = baml_test!(baml: src, entry: "main");
    match output.result {
        Ok(BexExternalValue::Int(n)) => n,
        other => panic!("expected int, got {other:?}"),
    }
}

async fn run_bool(src: &str) -> bool {
    let output = baml_test!(baml: src, entry: "main");
    match output.result {
        Ok(BexExternalValue::Bool(b)) => b,
        other => panic!("expected bool, got {other:?}"),
    }
}

async fn run_int_array(src: &str) -> Vec<i64> {
    let output = baml_test!(baml: src, entry: "main");
    match output.result {
        Ok(BexExternalValue::Array { items, .. }) => items
            .into_iter()
            .map(|v| match v {
                BexExternalValue::Int(n) => n,
                other => panic!("expected int element, got {other:?}"),
            })
            .collect(),
        other => panic!("expected array, got {other:?}"),
    }
}

// ─── some ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn array_some_finds_match() {
    assert!(
        run_bool("function main() -> bool { [1, 2, 3].some((x: int) -> bool { x > 2 }) }",).await
    );
}

#[tokio::test]
async fn array_some_no_match() {
    assert!(
        !run_bool("function main() -> bool { [1, 2, 3].some((x: int) -> bool { x > 9 }) }",).await
    );
}

#[tokio::test]
async fn array_some_empty_is_false() {
    assert!(
        !run_bool(
            "function main() -> bool { let xs: int[] = []; xs.some((x: int) -> bool { true }) }",
        )
        .await
    );
}

#[tokio::test]
async fn array_some_short_circuits() {
    // Counts predicate invocations via a closure-captured counter. If `some`
    // short-circuits, the counter stops at the first match (idx 1).
    assert_eq!(
        run_int(
            r#"
            function main() -> int {
                let count = 0;
                let _ = [1, 5, 100, 200].some((x: int) -> bool {
                    count = count + 1;
                    x > 4
                });
                count
            }
            "#
        )
        .await,
        2
    );
}

#[tokio::test]
async fn array_some_propagates_error() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            [1, 2, 3].some((x: int) -> bool throws string {
                if (x == 2) { throw "boom" } else { false }
            }) catch (e) {
                string => true
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

// ─── every ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn array_every_all_match() {
    assert!(
        run_bool("function main() -> bool { [2, 4, 6].every((x: int) -> bool { x % 2 == 0 }) }",)
            .await
    );
}

#[tokio::test]
async fn array_every_one_fails() {
    assert!(
        !run_bool("function main() -> bool { [2, 4, 5].every((x: int) -> bool { x % 2 == 0 }) }",)
            .await
    );
}

#[tokio::test]
async fn array_every_empty_is_true() {
    assert!(
        run_bool(
            "function main() -> bool { let xs: int[] = []; xs.every((x: int) -> bool { false }) }"
        )
        .await
    );
}

#[tokio::test]
async fn array_every_short_circuits_on_false() {
    assert_eq!(
        run_int(
            r#"
            function main() -> int {
                let count = 0;
                let _ = [1, 2, 3, 4].every((x: int) -> bool {
                    count = count + 1;
                    x < 3
                });
                count
            }
            "#
        )
        .await,
        3 // visited 1, 2, 3; 3 < 3 = false → stop
    );
}

// ─── find / find_index ────────────────────────────────────────────────────────

#[tokio::test]
async fn array_find_returns_first_match() {
    assert_eq!(
        run_int("function main() -> int? { [1, 2, 3, 4].find((x: int) -> bool { x > 2 }) }",).await,
        3
    );
}

#[tokio::test]
async fn array_find_returns_null_when_no_match() {
    let output = baml_test!(
        r#"
        function main() -> int? {
            [1, 2, 3].find((x: int) -> bool { x > 9 })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Null));
}

#[tokio::test]
async fn array_find_empty_is_null() {
    let output = baml_test!(
        r#"
        function main() -> int? {
            let xs: int[] = [];
            xs.find((x: int) -> bool { true })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Null));
}

#[tokio::test]
async fn array_find_index_returns_first_index() {
    assert_eq!(
        run_int("function main() -> int? { [1, 2, 3, 4].find_index((x: int) -> bool { x > 2 }) }",)
            .await,
        2
    );
}

#[tokio::test]
async fn array_find_index_returns_null_when_no_match() {
    let output = baml_test!(
        r#"
        function main() -> int? {
            [1, 2].find_index((x: int) -> bool { x > 9 })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Null));
}

#[tokio::test]
async fn array_find_short_circuits() {
    assert_eq!(
        run_int(
            r#"
            function main() -> int {
                let count = 0;
                let _ = [10, 20, 30, 40].find((x: int) -> bool {
                    count = count + 1;
                    x >= 20
                });
                count
            }
            "#
        )
        .await,
        2
    );
}

// ─── find_last / find_last_index ──────────────────────────────────────────────

#[tokio::test]
async fn array_find_last_returns_last_match() {
    assert_eq!(
        run_int("function main() -> int? { [1, 2, 3, 4].find_last((x: int) -> bool { x > 1 }) }",)
            .await,
        4
    );
}

#[tokio::test]
async fn array_find_last_returns_null_when_no_match() {
    let output = baml_test!(
        r#"
        function main() -> int? {
            [1, 2, 3].find_last((x: int) -> bool { x > 9 })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Null));
}

#[tokio::test]
async fn array_find_last_index_returns_last_index() {
    // [1, 2, 1, 2] → last index where x == 1 is 2.
    assert_eq!(
        run_int(
            "function main() -> int? { [1, 2, 1, 2].find_last_index((x: int) -> bool { x == 1 }) }",
        )
        .await,
        2
    );
}

#[tokio::test]
async fn array_find_last_iterates_back_to_front() {
    // Verify iteration direction by counting visits.
    assert_eq!(
        run_int(
            r#"
            function main() -> int {
                let count = 0;
                let _ = [10, 20, 30, 40].find_last((x: int) -> bool {
                    count = count + 1;
                    x <= 20
                });
                count
            }
            "#
        )
        .await,
        3 // visited 40, 30, 20 — short-circuits on 20
    );
}

#[tokio::test]
async fn array_find_last_empty_is_null() {
    let output = baml_test!(
        r#"
        function main() -> int? {
            let xs: int[] = [];
            xs.find_last((x: int) -> bool { true })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Null));
}

// ─── includes / index_of / last_index_of ──────────────────────────────────────

#[tokio::test]
async fn array_includes_present() {
    assert!(run_bool("function main() -> bool { [1, 2, 3].includes(2) }").await);
}

#[tokio::test]
async fn array_includes_absent() {
    assert!(!run_bool("function main() -> bool { [1, 2, 3].includes(99) }").await);
}

#[tokio::test]
async fn array_includes_empty() {
    assert!(!run_bool("function main() -> bool { let xs: int[] = []; xs.includes(1) }").await);
}

#[tokio::test]
async fn array_includes_strings() {
    assert!(run_bool(r#"function main() -> bool { ["a", "b", "c"].includes("b") }"#).await);
}

#[tokio::test]
async fn array_index_of_present() {
    assert_eq!(
        run_int("function main() -> int? { [10, 20, 30].index_of(20) }").await,
        1
    );
}

#[tokio::test]
async fn array_index_of_first_match() {
    // Both 10s — index_of returns the first.
    assert_eq!(
        run_int("function main() -> int? { [10, 20, 10].index_of(10) }").await,
        0
    );
}

#[tokio::test]
async fn array_index_of_absent() {
    let output = baml_test!(
        r#"
        function main() -> int? {
            [10, 20].index_of(99)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Null));
}

#[tokio::test]
async fn array_last_index_of_returns_last_match() {
    assert_eq!(
        run_int("function main() -> int? { [10, 20, 10].last_index_of(10) }").await,
        2
    );
}

#[tokio::test]
async fn array_last_index_of_absent() {
    let output = baml_test!(
        r#"
        function main() -> int? {
            [10, 20].last_index_of(99)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Null));
}

// ─── reduce ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn array_reduce_sum() {
    assert_eq!(
        run_int(
            r#"
            function main() -> int {
                [1, 2, 3, 4].reduce((acc: int, x: int) -> int { acc + x }, 0)
            }
            "#
        )
        .await,
        10
    );
}

#[tokio::test]
async fn array_reduce_with_nonzero_initial() {
    assert_eq!(
        run_int(
            r#"
            function main() -> int {
                [1, 2, 3].reduce((acc: int, x: int) -> int { acc + x }, 100)
            }
            "#
        )
        .await,
        106
    );
}

#[tokio::test]
async fn array_reduce_empty_returns_initial() {
    assert_eq!(
        run_int(
            r#"
            function main() -> int {
                let xs: int[] = [];
                xs.reduce((acc: int, x: int) -> int { acc + x }, 42)
            }
            "#
        )
        .await,
        42
    );
}

#[tokio::test]
async fn array_reduce_visits_every_element() {
    // 1 * 2 * 3 * 4 = 24
    assert_eq!(
        run_int(
            r#"
            function main() -> int {
                [1, 2, 3, 4].reduce((acc: int, x: int) -> int { acc * x }, 1)
            }
            "#
        )
        .await,
        24
    );
}

#[tokio::test]
async fn array_reduce_concat_strings() {
    let output = baml_test!(
        r#"
        function main() -> string {
            ["a", "b", "c"].reduce((acc: string, x: string) -> string { acc + x }, "")
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("abc".to_string()))
    );
}

// ─── flat_map ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn array_flat_map_doubles_each_element() {
    assert_eq!(
        run_int_array(
            r#"
            function main() -> int[] {
                [1, 2, 3].flat_map((x: int) -> int[] { [x, x * 10] })
            }
            "#
        )
        .await,
        vec![1, 10, 2, 20, 3, 30]
    );
}

#[tokio::test]
async fn array_flat_map_with_empty_inner_arrays() {
    assert_eq!(
        run_int_array(
            r#"
            function main() -> int[] {
                [1, 2, 3].flat_map((x: int) -> int[] { [] })
            }
            "#
        )
        .await,
        Vec::<i64>::new()
    );
}

#[tokio::test]
async fn array_flat_map_empty_input() {
    assert_eq!(
        run_int_array(
            r#"
            function main() -> int[] {
                let xs: int[] = [];
                xs.flat_map((x: int) -> int[] { [x] })
            }
            "#
        )
        .await,
        Vec::<i64>::new()
    );
}

#[tokio::test]
async fn array_flat_map_uneven_inner_lengths() {
    assert_eq!(
        run_int_array(
            r#"
            function main() -> int[] {
                [1, 2, 3].flat_map((x: int) -> int[] {
                    if (x == 2) { [20, 21, 22] } else { [x] }
                })
            }
            "#
        )
        .await,
        vec![1, 20, 21, 22, 3]
    );
}
