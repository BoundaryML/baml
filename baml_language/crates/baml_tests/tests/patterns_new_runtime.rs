//! Runtime tests for new pattern features (chains, unions, etc.)

use baml_tests::{baml_test, engine::compile_source};
use bex_engine::BexExternalValue;

// ============================================================================
// Chains of bare bindings — every link binds the same value.
// ============================================================================

// `let x | let x = 1` — Or-pattern in a let-stmt with the same name on both
// branches. Both branches bind `x`; one alternative wins at runtime.
#[tokio::test]
async fn let_or_of_same_binding() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x | let x = 1;
            x
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// Six alternatives, all binding `x` — should still bind `x` to `1`.
#[tokio::test]
async fn let_or_of_six_same_bindings() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x | let x | let x | let x | let x | let x = 1;
            x
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// Match arm: `let x | let x | let x => x + 1` — Or of bare bindings in arm
// position; body uses the bound value.
#[tokio::test]
async fn match_or_of_bare_bindings_uses_value() {
    let output = baml_test!(
        r#"
        function main() -> int {
            match (5) {
                let x | let x | let x => x + 1
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

// Match arm with chain narrows on each Or branch — body sums the binding.
#[tokio::test]
async fn match_or_of_chain_narrows_uses_value() {
    let output = baml_test!(
        r#"
        function main() -> int {
            match (10) {
                (let x: int) | (let x: int) => x * 2
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(20)));
}

// Chain with multiple Bind links inside a match arm — every name aliases the
// scrutinee value.
#[tokio::test]
async fn match_chain_of_bindings_aliases_scrutinee() {
    let output = baml_test!(
        r#"
        function main() -> int {
            match (7) {
                let a: let b: let c => a + b + c
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(21)));
}

#[tokio::test]
async fn match_chain_trailing_binding_after_array_type_aliases_value() {
    let output = baml_test!(
        r#"
        function main() -> int {
            match ([[9]]) {
                let rows: [[let x]]: int[][] => rows[0][0] * 100 + x * 10,
                _ => 0
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(990)));
}

// Match arm with a guard that references a chain-bound name. Guard fires with
// the bound value at runtime.
#[tokio::test]
async fn match_chain_binding_used_in_guard() {
    let output = baml_test!(
        r#"
        function main() -> int {
            match (4) {
                let n: int if n > 2 => n * 10,
                _ => 0
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(40)));
}

// for-let with a chain of bare bindings — every iteration binds all names.
#[tokio::test]
async fn for_let_chain_of_bindings() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let total = 0;
            for (let a: let b in [1, 2, 3]) {
                total += a + b
            };
            total
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(12)));
}

#[tokio::test]
async fn for_let_chain_trailing_binding_after_type_aliases_iteration_value() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let total = 0;
            for (let row: int[] in [[4, 5], [6, 7, 8]]) {
                total += row.length() * 100 + row[0] * 10 + row.length()
            };
            total
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(605)));
}

// for-let chain aliases need the same per-iteration fresh cell as the first
// binding; otherwise closures that escape the loop can share the alias cell.
#[tokio::test]
async fn for_let_chain_alias_capture_per_iteration() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let funcs: (() -> int)[] = [];
            for (let a: let b in [1, 2, 3]) {
                funcs.push(() -> int { b })
            };
            funcs[0]() * 100 + funcs[1]() * 10 + funcs[2]()
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(123)));
}

// for-let with Or-of-same-binding — both branches bind `x`, body uses it.
#[tokio::test]
async fn for_let_or_of_same_binding() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let total = 0;
            for (let x | let x in [1, 2, 3, 4]) {
                total += x
            };
            total
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(10)));
}

// for-let with six Or alternatives all binding the same name.
#[tokio::test]
async fn for_let_or_of_six_same_bindings() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let total = 0;
            for (let x | let x | let x | let x | let x | let x in [1, 2, 3]) {
                total += x
            };
            total
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

#[tokio::test]
async fn for_array_destructure_rest_sums_rows() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let rows: int[][] = [[1, 2, 3], [], [4]];
            let total = 0;
            for (let row in rows) {
                total += row.length()
            };
            total
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(4)));
}

#[tokio::test]
async fn for_array_destructure_rest_capture_gets_fresh_cell_per_iteration() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let rows = [[1], [2, 3], [4, 5, 6]];
            let funcs: (() -> int)[] = [];
            for (let row in rows) {
                funcs.push(() -> int { row.length() })
            };
            funcs[0]() * 100 + funcs[1]() * 10 + funcs[2]()
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(123)));
}

#[tokio::test]
async fn for_array_destructure_inside_class_destructure_sums_fields() {
    let output = baml_test!(
        r#"
        class Team {
            scores int[]
        }

        function main() -> int {
            let teams = [
                Team { scores: [1, 2] },
                Team { scores: [] },
                Team { scores: [3] }
            ];
            let total = 0;
            for (let Team { scores: let s } in teams) {
                total += s.length()
            };
            total
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

// Catch arm with Or-of-typed-bindings — both branches bind the same name and
// type, so the body can use it.
#[tokio::test]
async fn catch_or_of_same_typed_binding() {
    let output = baml_test!(
        r#"
        function risky() -> string throws string {
            throw "boom"
        }

        function main() -> string {
            risky() catch (e) {
                (let s: string) | (let s: string) => s
            }
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("boom".to_string()))
    );
}

// `let _: int = 5` is no longer accepted (`_:` ascription removed); use a
// bare wildcard discard instead.
#[tokio::test]
async fn let_wildcard_discards_value() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let _ = 5;
            42
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

// Single-ascription bind in a match arm — covers all `int`, so no wildcard
// arm is needed.
#[tokio::test]
async fn match_bind_with_type_ascription() {
    let output = baml_test!(
        r#"
        function main() -> int {
            match (1) {
                let n: int => n + 100
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(101)));
}

// Function-type dispatch in a match: pass a real lambda to a generic
// function whose match arms narrow on signature, and verify the right arm
// fires and returns the expected value.
#[tokio::test]
async fn match_dispatches_int_callback() {
    let output = baml_test!(
        r#"
        function dispatch<T>(cb: ((int) -> T) | ((string) -> T)) -> T {
            match (cb) {
                let f: (int) -> T => f(5),
                let f: (string) -> T => f("x")
            }
        }

        function main() -> int {
            dispatch((n: int) -> int { n * 10 })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(50)));
}

// Without parens, `(int) -> int | (string) -> int` parses with `|` binding
// inside the return type — that's a known parser-precedence quirk worth
// fixing separately. Use explicit parens to express a union of function
// types.
#[tokio::test]
async fn match_dispatches_string_callback() {
    let output = baml_test!(
        r#"
        function dispatch(cb: ((int) -> int) | ((string) -> int)) -> int {
            match (cb) {
                let f: (int) -> int => f(5),
                let f: (string) -> int => f("hello")
            }
        }

        function main() -> int {
            dispatch((s: string) -> int { 42 })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

// Or-pattern where both branches bind the same names but in different order.
// HIR sees the same name set; MIR takes the first branch's order; both names
// alias the same value.
#[tokio::test]
async fn match_or_of_chain_bindings_swapped() {
    let output = baml_test!(
        r#"
        function main() -> int {
            match (7) {
                (let x: let y) | (let y: let x) => x + y
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(14)));
}

// Regression: each name introduced by a multi-bind chain needs its own binding
// identity for closure capture. If `let x: let y` collapses both names to the
// same BindingId, the lambda below captures `x` instead of the reassigned `y`.
#[tokio::test]
async fn chain_second_binding_capture_after_reassign() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x: let y = 1;
            y = 2;
            let f = () -> int {
                y
            };
            f()
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn let_class_destructure_binds_shorthand_and_ignores_omitted_fields() {
    let output = baml_test!(
        r#"
        class Person {
            name string
            age int
            score int
        }

        function main() -> int {
            let Person { age } = Person { name: "Ada", age: 41, score: 99 };
            age
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(41)));
}

#[tokio::test]
async fn let_class_destructure_renames_field_binding() {
    let output = baml_test!(
        r#"
        class Person {
            name string
            age int
        }

        function main() -> int {
            let Person { age: let years } = Person { name: "Ada", age: 42 };
            years
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn let_class_destructure_nested_class() {
    let output = baml_test!(
        r#"
        class Address {
            zip int
            city string
        }

        class Person {
            age int
            address Address
        }

        function main() -> int {
            let Person { address: Address { zip } } =
                Person { age: 1, address: Address { zip: 94107, city: "SF" } };
            zip
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(94107)));
}

#[tokio::test]
async fn let_nested_same_field_names_are_ok_with_distinct_bindings() {
    let output = baml_test!(
        r#"
        class Inner {
            field int
        }

        class Outer {
            field Inner
            other int
        }

        function main() -> int {
            let Outer {
                field: let whole_field: Inner { field: let inner_field }
            } = Outer {
                field: Inner { field: 21 },
                other: 1
            };
            whole_field.field + inner_field
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn let_generic_class_destructure_substitutes_field_types() {
    let output = baml_test!(
        r#"
        class Box<T> {
            value T
        }

        function main() -> int {
            let boxed: Box<int> = Box { value: 42 };
            let Box<int> { value } = boxed;
            value + 1
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(43)));
}

#[tokio::test]
async fn empty_generic_class_destructure_uses_contextual_type() {
    let output = baml_test!(
        r#"
        class Box<T> {
            value T
        }

        function classify_int(boxed: Box<int>) -> int {
            match (boxed) {
                Box<int> {} => 1
            }
        }

        function classify_string(boxed: Box<string>) -> int {
            match (boxed) {
                Box<string> {} => 20
            }
        }

        function main() -> int {
            let int_box: Box<int> = Box { value: 7 };
            let string_box: Box<string> = Box { value: "ready" };
            let Box<int> {} = int_box;
            let Box<string> {} = string_box;
            classify_int(int_box) + classify_string(string_box)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(21)));
}

#[tokio::test]
async fn match_generic_class_type_patterns_distinguish_type_args() {
    let output = baml_test!(
        r#"
        class Box<T> {
            value T
        }

        function classify(box: Box<int> | Box<string>) -> int {
            match (box) {
                Box<int> => 1,
                Box<string> => 2
            }
        }

        function main() -> int {
            let string_box: Box<string> = Box { value: "ready" };
            let int_box: Box<int> = Box { value: 7 };
            classify(string_box) * 10 + classify(int_box)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(21)));
}

#[tokio::test]
async fn match_generic_class_destructure_union_tests_substituted_field_type() {
    let output = baml_test!(
        r#"
        class Box<T> {
            value T
        }

        function classify(box: Box<int> | Box<string>) -> int {
            match (box) {
                Box<int> { value: let value: int } => value,
                Box<string> { value: let value: string } => value.length()
            }
        }

        function main() -> int {
            let string_box: Box<string> = Box { value: "ready" };
            let int_box: Box<int> = Box { value: 9 };
            classify(string_box) * 10 + classify(int_box)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(59)));
}

#[test]
fn generic_class_destructure_without_type_args_is_rejected() {
    let err = std::panic::catch_unwind(|| {
        compile_source(
            r#"
            class Box<T> {
                value T
            }

            function main() -> int {
                let boxed: Box<int> = Box { value: 42 };
                let Box { value } = boxed;
                value
            }
        "#,
        );
    })
    .expect_err("expected generic class destructure without type args to fail");

    let message = err
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| err.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(
        message.contains("generic class destructure `Box { ... }` must specify type arguments"),
        "unexpected panic message: {message}"
    );
}

#[tokio::test]
async fn generic_class_destructure_backfills_through_wrapped_flow_types() {
    let output = baml_test!(
        r#"
        class Box<T> {
            value T
        }

        function from_optional(box: Box<int>?) -> int {
            match (box) {
                Box<int> { value: let value: int } => value,
                null => 0
            }
        }

        function from_union(box: Box<int> | null) -> int {
            match (box) {
                Box<int> { value: let value: int } => value,
                null => 0
            }
        }

        function from_array(boxes: Box<int>[]) -> int {
            let total = 0;
            for (let Box<int> { value: let value: int } in boxes) {
                total += value;
            }
            total
        }

        function main() -> int {
            let boxed: Box<int> = Box { value: 40 };
            from_optional(boxed) + from_union(boxed) + from_array([boxed])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(120)));
}

#[tokio::test]
async fn empty_generic_class_destructure_covers_union_of_same_class_instantiations() {
    let output = baml_test!(
        r#"
        class Box<T> {
            value T
        }

        function classify(box: Box<int> | Box<string>) -> int {
            match (box) {
                Box<int> {} | Box<string> {} => 7
            }
        }

        function main() -> int {
            let string_box: Box<string> = Box { value: "ready" };
            let int_box: Box<int> = Box { value: 3 };
            classify(string_box) * 10 + classify(int_box)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(77)));
}

#[tokio::test]
async fn deep_generic_class_destructure_substitutes_through_nested_fields() {
    let output = baml_test!(
        r#"
        class Box<T> {
            value T
        }

        class Outer<T> {
            inner Box<T>
        }

        function main() -> int {
            let outer: Outer<int> = Outer {
                inner: Box<int> { value: 123 }
            };
            let Outer<int> {
                inner: Box<int> { value }
            } = outer;
            value
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(123)));
}

#[tokio::test]
async fn match_class_destructure_tests_field_literal_and_falls_through() {
    let output = baml_test!(
        r#"
        class Person {
            age int
            score int
        }

        function main() -> int {
            match (Person { age: 5, score: 99 }) {
                Person { age: 7 } => 70,
                Person { age: let actual } => actual
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

#[tokio::test]
async fn match_class_destructure_tests_union_field_type() {
    let output = baml_test!(
        r#"
        class Box {
            value int | string
        }

        function main() -> int {
            match (Box { value: "ready" }) {
                Box { value: int } => 1,
                Box { value: string } => 2
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn match_top_level_class_destructure_tests_union_scrutinee() {
    let output = baml_test!(
        r#"
        class Person {
            age int
        }

        function score(v: Person | int) -> int {
            match (v) {
                Person { age } => age,
                int => 0
            }
        }

        function main() -> int {
            score(Person { age: 42 }) * 10 + score(7)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(420)));
}

#[tokio::test]
async fn match_or_class_destructure_same_binding_name_compatible_types() {
    let output = baml_test!(
        r#"
        class Person {
            id int
        }

        class Admin {
            id int
        }

        function score(v: Person | Admin) -> int {
            match (v) {
                Person { id } | Admin { id } => id
            }
        }

        function main() -> int {
            score(Admin { id: 77 })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(77)));
}

#[tokio::test]
async fn match_or_class_destructure_same_binding_name_different_depths() {
    let output = baml_test!(
        r#"
        class Class {
            field int
        }

        class Class3 {
            field int
        }

        class Class2 {
            c Class3
        }

        function score(v: Class | Class2) -> int {
            match (v) {
                Class { field } |
                Class2 { c: Class3 { field } } => field
            }
        }

        function main() -> int {
            score(Class { field: 4 }) * 10 +
            score(Class2 { c: Class3 { field: 7 } })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(47)));
}

#[tokio::test]
async fn match_array_destructure_empty_and_prefix_rest() {
    let output = baml_test!(
        r#"
        function score(xs: int[]) -> int {
            match (xs) {
                [] => 0,
                [let first, ..] => first
            }
        }

        function main() -> int {
            score([]) * 10 + score([7, 8, 9])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

#[tokio::test]
async fn match_array_destructure_exact_length() {
    let output = baml_test!(
        r#"
        function main() -> int {
            match ([4, 5]) {
                [let one] => one,
                [let first, let second] => first * 10 + second,
                _ => 0
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(45)));
}

#[tokio::test]
async fn match_array_destructure_binds_rest_copy() {
    let output = baml_test!(
        r#"
        function score(xs: int[]) -> int {
            match (xs) {
                [let first, ..] => first * 10 + (xs.length() - 1),
                [] => 0
            }
        }

        function main() -> int {
            score([1, 2, 3])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(12)));
}

#[tokio::test]
async fn match_array_destructure_suffix_rest() {
    let output = baml_test!(
        r#"
        function score(xs: int[]) -> int {
            match (xs) {
                [.., let last] => (xs.length() - 1) * 10 + last,
                _ => 0
            }
        }

        function main() -> int {
            score([1, 2, 9])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(29)));
}

#[tokio::test]
async fn let_array_destructure_binds_whole_rest() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let [..] = [3, 4, 5];
            3
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn let_array_destructure_nested_rest_only_is_irrefutable() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let [..] = [1, 2, 3];
            1
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn match_array_destructure_entire_alphabet_of_bindings() {
    let output = baml_test!(
        r#"
        function main() -> int {
            match ([1, 2, 3, 4, 5, 6, 7, 8, 9, 10,
                    11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
                    21, 22, 23, 24, 25, 26]) {
                [let a, let b, let c, let d, let e, let f, let g, let h,
                 let i, let j, let k, let l, let m, let n, let o, let p,
                 let q, let r, let s, let t, let u, let v, let w, let x,
                 let y, let z] =>
                    a + b + c + d + e + f + g + h + i + j + k + l + m +
                    n + o + p + q + r + s + t + u + v + w + x + y + z,
                _ => 0
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(351)));
}

#[tokio::test]
async fn match_or_binding_from_class_field_or_nested_array_position() {
    let output = baml_test!(
        r#"
        class A {
            x int
        }

        class B {
            y int[]
        }

        function score(input: A | B) -> int {
            match (input) {
                A { x: let value } | B { y: [_, let value] } => value,
                _ => 0
            }
        }

        function main() -> int {
            score(A { x: 4 }) * 10 + score(B { y: [9, 7] })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(47)));
}

#[tokio::test]
async fn match_wildcard_inside_chained_binding_type_array_pattern() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let xs: [..]: int[] = [4, 5, 6];
            xs.length()
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn match_array_destructure_shadows_outer_local_only_inside_arm() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = 1;
            let inner = match ([4, 5]) {
                [let x, ..] => x,
                _ => 0
            };
            x * 10 + inner
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(14)));
}

#[tokio::test]
async fn for_array_destructure_nested_rest_only_is_irrefutable() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let total = 0;
            for (let [..] in [[1], [2, 3], []]) {
                total += 1;
            }
            total
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn match_array_destructure_literal_and_type_element_patterns() {
    let output = baml_test!(
        r#"
        function ints(xs: int[]) -> int {
            match (xs) {
                [1, 2, ..] => 12,
                [1, ..] => 10,
                _ => 0
            }
        }

        function strings(xs: string[]) -> int {
            match (xs) {
                [string, ..] => xs.length(),
                [] => 0
            }
        }

        function main() -> int {
            ints([1, 2, 3]) * 100 + ints([1, 9]) * 10 + strings(["a", "b"])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1302)));
}

#[tokio::test]
async fn match_array_destructure_or_patterns_share_bindings() {
    let output = baml_test!(
        r#"
        function score(xs: int[]) -> int {
            match (xs) {
                [let x] | [let x, ..] => x,
                _ => 0
            }
        }

        function main() -> int {
            score([4]) * 10 + score([5, 6])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(45)));
}

#[tokio::test]
async fn match_array_destructure_or_patterns_nested_arrays_share_bindings() {
    let output = baml_test!(
        r#"
        function score(rows: int[][]) -> int {
            match (rows) {
                [[let x, ..]] | [[let x, ..], ..] => x,
                _ => 0
            }
        }

        function main() -> int {
            score([[4, 5]]) * 10 + score([[7, 9], [2]])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(47)));
}

#[tokio::test]
async fn match_array_destructure_nested_array_binding_projects_inner_element() {
    let output = baml_test!(
        r#"
        function main() -> int {
            match ([[4]]) {
                [[let x]] => x,
                [..] => 0
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(4)));
}

#[tokio::test]
async fn match_array_destructure_or_patterns_nested_class_array_fields() {
    let output = baml_test!(
        r#"
        class Team {
            scores int[]
        }

        function score(teams: Team[]) -> int {
            match (teams) {
                [Team { scores: [let x, ..] }] |
                [Team { scores: [_, let x, ..] }, ..] => x,
                _ => 0
            }
        }

        function main() -> int {
            score([Team { scores: [3, 4] }]) * 10 +
            score([
                Team { scores: [8, 9] },
                Team { scores: [1] }
            ])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(39)));
}

#[tokio::test]
async fn match_array_destructure_outer_chain_applies_to_whole_array() {
    let output = baml_test!(
        r#"
        function score(xs: int[][]) -> int {
            match (xs) {
                [.., let second_last, let last]: int[][] =>
                    (xs.length() - 2) * 100 + second_last[0] * 10 + last[0],
                _ => 0
            }
        }

        function main() -> int {
            score([[1], [2], [3], [4]])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(234)));
}

// Five-level alternating class/array/class/array/class with a typed
// bind near the floor, joined across an Or that reaches the same name
// via a flipped traversal (prefix-then-suffix vs suffix-then-prefix on
// every slice level). All four combinations of the input must route to
// exactly one of the two arms, and `let x: int` must come out as `int`
// at every depth.
#[tokio::test]
async fn match_alternating_class_array_five_levels_or_flipped_traversal() {
    let output = baml_test!(
        r#"
        class Atom {
            value int
        }

        class Slot {
            atoms Atom[]
        }

        class Row {
            slots Slot[]
        }

        class Sheet {
            rows Row[]
        }

        function pick(s: Sheet) -> int {
            match (s) {
                Sheet { rows: [_, Row { slots: [_, Slot { atoms: [_, Atom { value: let x: int }, ..] }, ..] }, ..] }
                | Sheet { rows: [.., Row { slots: [.., Slot { atoms: [.., Atom { value: let x: int }, _] }] }] } => x,
                _ => 0
            }
        }

        function main() -> int {
            // Hits arm 1: rows[1].slots[1].atoms[1] = 42
            let target = Atom { value: 42 };
            let stuffer_atom = Atom { value: 0 };
            let stuffer_slot = Slot { atoms: [stuffer_atom, stuffer_atom, stuffer_atom] };
            let stuffer_row = Row { slots: [stuffer_slot, stuffer_slot, stuffer_slot] };
            let target_slot = Slot { atoms: [stuffer_atom, target, stuffer_atom] };
            let target_row = Row { slots: [stuffer_slot, target_slot, stuffer_slot] };
            pick(Sheet { rows: [stuffer_row, target_row, stuffer_row] })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

// Or-pattern with FOUR alternates, each reaching `let v: int` via a
// completely different traversal across mixed-depth class and array
// shapes. Same-name same-type bind across every alt forces the matrix
// to unify the binding type via join.
#[tokio::test]
async fn match_or_four_alternates_same_name_different_paths() {
    let output = baml_test!(
        r#"
        class Wrap {
            inner int[]
        }

        class Pair {
            left Wrap
            right int[]
        }

        function pick(p: Pair | Wrap | int[][] | int[]) -> int {
            match (p) {
                Pair { left: Wrap { inner: [_, let v: int, ..] }, right: _ }
                | Pair { left: _, right: [_, _, let v: int, ..] }
                | Wrap { inner: [.., let v: int, _, _] }
                | [_, [.., let v: int], ..] => v,
                _ => 0
            }
        }

        function main() -> int {
            // Pair branch 1: left Wrap inner [_, 7, ..]
            let a = pick(Pair { left: Wrap { inner: [99, 7, 100] }, right: [0] });
            // Pair branch 2: right [_, _, 8, ..]
            let b = pick(Pair { left: Wrap { inner: [99] }, right: [99, 99, 8, 99] });
            // Wrap branch: inner [.., 9, _, _]
            let c = pick(Wrap { inner: [99, 9, 99, 99] });
            // 2D array branch: [_, [.., 6], ..]
            let d = pick([[99], [4, 5, 6], [99]]);
            a * 1000 + b * 100 + c * 10 + d
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(7896)));
}

// Class-of-arrays-of-classes-of-arrays. Match drills through three
// levels of structural destructure with positional and rest-flanked
// bindings, then names a deep cell across an Or with an alternate that
// reaches the same cell via a different path.
#[tokio::test]
async fn match_deep_class_array_or_picks_deep_cell() {
    let output = baml_test!(
        r#"
        class Cell {
            row int[]
        }

        class Grid {
            cells Cell[]
        }

        function score(g: Grid) -> int {
            match (g) {
                Grid { cells: [_, Cell { row: [_, let x, ..] }, ..] }
                | Grid { cells: [.., Cell { row: [.., let x, _] }] } => x,
                _ => 0
            }
        }

        function main() -> int {
            score(Grid {
                cells: [
                    Cell { row: [1, 2, 3] },
                    Cell { row: [10, 42, 30] },
                    Cell { row: [100, 200, 300] }
                ]
            })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

// Or-pattern across a 2D array vs a class with a 2D array field, both
// reaching a deep `int` via different slice/class shapes. Verifies that
// the matrix lines up the binding's type as `int` despite the alternates
// taking very different paths.
#[tokio::test]
async fn match_or_2d_array_vs_class_with_2d_field() {
    let output = baml_test!(
        r#"
        class Wrap {
            grid int[][]
        }

        function score(v: int[][] | Wrap) -> int {
            match (v) {
                [[let x, ..], ..]
                | Wrap { grid: [.., [_, let x, ..]] } => x,
                _ => 0
            }
        }

        function main() -> int {
            let bare = score([[7, 1, 2], [8, 9]]);
            let wrapped = score(Wrap { grid: [[1, 2], [3, 11, 5]] });
            bare * 100 + wrapped
        }
    "#
    );
    // bare: [[7, 1, 2], [8, 9]] → first arm, x = 7 → 7
    // wrapped: Wrap { grid: [[1, 2], [3, 11, 5]] } → second arm,
    //          last cell row = [3, 11, 5], skip first, take next → x = 11
    assert_eq!(output.result, Ok(BexExternalValue::Int(7 * 100 + 11)));
}

// Multiple prefix bindings + wildcard rest + suffix binding:
// `[let x, let y, .., let z]` should bind `x`, `y` to the first two
// elements and `z` to the last.
#[tokio::test]
async fn match_array_destructure_two_prefix_rest_suffix_binding() {
    let output = baml_test!(
        r#"
        function score(xs: int[]) -> int {
            match (xs) {
                [let x, let y, .., let z] => x * 100 + y * 10 + z,
                _ => 0
            }
        }

        function main() -> int {
            score([7, 8, 99, 99, 9])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(789)));
}

// Prefix binding + wildcard rest + multiple suffix bindings:
// `[let x, .., let y, let z]` should bind `x` to the first element and
// `y`, `z` to the last two.
#[tokio::test]
async fn match_array_destructure_prefix_rest_two_suffix_bindings() {
    let output = baml_test!(
        r#"
        function score(xs: int[]) -> int {
            match (xs) {
                [let x, .., let y, let z] => x * 100 + y * 10 + z,
                _ => 0
            }
        }

        function main() -> int {
            score([7, 99, 99, 8, 9])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(789)));
}

#[tokio::test]
async fn match_array_destructure_inside_class_destructure_empty_field() {
    let output = baml_test!(
        r#"
        class Team {
            scores int[]
        }

        function score(team: Team) -> int {
            match (team) {
                Team { scores: [] } => 10,
                Team { scores: [let first, ..] } => first
            }
        }

        function main() -> int {
            score(Team { scores: [] }) + score(Team { scores: [7, 8] })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(17)));
}

#[tokio::test]
async fn match_array_destructure_with_class_elements() {
    let output = baml_test!(
        r#"
        class User {
            name string
            score int
        }

        function score(users: User[]) -> int {
            match (users) {
                [] => 0,
                [User { score: let first }, ..] =>
                    first * 10 + (users.length() - 1)
            }
        }

        function main() -> int {
            score([
                User { name: "a", score: 4 },
                User { name: "b", score: 9 },
                User { name: "c", score: 8 }
            ])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn match_array_destructure_wildcard_elements_and_rest() {
    let output = baml_test!(
        r#"
        function score(xs: int[]) -> int {
            match (xs) {
                [_, let second, ..] => second,
                _ => 0
            }
        }

        function main() -> int {
            score([5, 8, 13, 21]) * 10 + score([1])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(80)));
}

#[tokio::test]
async fn match_array_destructure_wildcard_length_shapes() {
    let output = baml_test!(
        r#"
        function exact(xs: int[]) -> int {
            match (xs) {
                [] => 0,
                [_] => 1,
                [_, _] => 2,
                [_, ..] => 3
            }
        }

        function prefix(xs: int[]) -> int {
            match (xs) {
                [_, ..] => 1,
                [] => 0
            }
        }

        function rest(xs: int[]) -> int {
            match (xs) {
                [..] => 7
            }
        }

        function main() -> int {
            exact([]) * 1000000 +
            prefix([]) * 10000000 +
            exact([9]) * 100000 +
            exact([9, 8]) * 10000 +
            exact([9, 8, 7]) * 1000 +
            prefix([5]) * 100 +
            rest([]) * 10 +
            rest([1])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(123177)));
}

#[tokio::test]
async fn match_array_destructure_wildcards_nested_in_class_fields() {
    let output = baml_test!(
        r#"
        class Team {
            scores int[]
        }

        function score(team: Team) -> int {
            match (team) {
                Team { scores: [_, let second, ..] } => second,
                _ => 0
            }
        }

        function main() -> int {
            score(Team { scores: [4, 9, 16] })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(9)));
}

#[tokio::test]
async fn match_array_destructure_wildcard_class_fields_and_rest() {
    let output = baml_test!(
        r#"
        class User {
            name string
            score int
        }

        function score(users: User[]) -> int {
            match (users) {
                [User { name: _, score: let first }, ..] => first,
                _ => 0
            }
        }

        function main() -> int {
            score([
                User { name: "hidden", score: 6 },
                User { name: "ignored", score: 7 }
            ])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

#[tokio::test]
async fn for_array_destructure_rest_wildcard_is_irrefutable() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let rows: int[][] = [[1], [], [2, 3]];
            let count = 0;
            for (let [..] in rows) {
                count += 1
            };
            count
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn class_destructure_shorthand_field_same_name_as_class_binds_field() {
    let output = baml_test!(
        r#"
        class User {
            User int
            other int
        }

        function main() -> int {
            let User { User } = User { User: 99, other: 1 };
            User
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

#[tokio::test]
async fn match_class_destructure_binds_nested_field() {
    let output = baml_test!(
        r#"
        class Address {
            zip int
        }

        class Person {
            address Address
        }

        function main() -> int {
            match (Person { address: Address { zip: 12345 } }) {
                Person { address: Address { zip } } => zip
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(12345)));
}

// Chain ascriptions on Class destructure patterns (e.g. `Class { ... }: T`)
// are no longer accepted — ascription is only valid on `let x` and `[…]`.
// The deep destructure still works on its own.
#[tokio::test]
async fn let_deep_class_destructure_binds_inner_field() {
    let output = baml_test!(
        r#"
        class Coordinate {
            zip int
            plus4 int
        }

        class Address {
            coordinate Coordinate
            city string
        }

        class Profile {
            address Address
            score int
        }

        class User {
            profile Profile
            active bool
        }

        function main() -> int {
            let User {
                profile: Profile {
                    address: Address {
                        coordinate: Coordinate {
                            zip: let zip
                        }
                    }
                }
            } = User {
                active: true,
                profile: Profile {
                    score: 9,
                    address: Address {
                        city: "SF",
                        coordinate: Coordinate { zip: 94107, plus4: 1200 }
                    }
                }
            };
            zip
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(94107)));
}

#[tokio::test]
async fn match_deep_class_destructure_falls_through_to_binding_arm() {
    let output = baml_test!(
        r#"
        class Coordinate {
            zip int
            plus4 int
        }

        class Address {
            coordinate Coordinate
        }

        class Profile {
            address Address
        }

        class User {
            profile Profile
        }

        function main() -> int {
            let user = User {
                profile: Profile {
                    address: Address {
                        coordinate: Coordinate { zip: 22222 }
                    }
                }
            };

            match (user) {
                User {
                    profile: Profile {
                        address: Address {
                            coordinate: Coordinate { zip: 11111 }
                        }
                    }
                } => 1,
                User {
                    profile: Profile {
                        address: Address {
                            coordinate: Coordinate { zip: let zip }
                        }
                    }
                } => zip
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(22222)));
}

#[tokio::test]
async fn match_or_deep_class_destructure_binds_when_second_alt_matches() {
    let output = baml_test!(
        r#"
        class Coordinate {
            zip int
            plus4 int
        }

        class Address {
            coordinate Coordinate
        }

        class User {
            address Address
        }

        function main() -> int {
            match (User { address: Address { coordinate: Coordinate { zip: 7, plus4: 2 } } }) {
                (
                    User { address: Address { coordinate: Coordinate { zip: let zip: int, plus4: 1 } } }
                ) | (
                    User { address: Address { coordinate: Coordinate { zip: let zip: int, plus4: 2 } } }
                ) => zip,
                _ => 0
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

#[tokio::test]
async fn for_deep_class_destructure_sums_fields() {
    let output = baml_test!(
        r#"
        class Coordinate {
            zip int
        }

        class Address {
            coordinate Coordinate
        }

        class User {
            address Address
        }

        function main() -> int {
            let users = [
                User { address: Address { coordinate: Coordinate { zip: 1 } } },
                User { address: Address { coordinate: Coordinate { zip: 20 } } },
                User { address: Address { coordinate: Coordinate { zip: 300 } } }
            ];
            let total = 0;
            for (let User {
                address: Address {
                    coordinate: Coordinate { zip: let zip }
                }
            } in users) {
                total += zip
            };
            total
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(321)));
}

#[tokio::test]
async fn for_deep_class_destructure_capture_gets_fresh_cell_per_iteration() {
    let output = baml_test!(
        r#"
        class Coordinate {
            zip int
        }

        class Address {
            coordinate Coordinate
        }

        class User {
            address Address
        }

        function main() -> int {
            let users = [
                User { address: Address { coordinate: Coordinate { zip: 1 } } },
                User { address: Address { coordinate: Coordinate { zip: 2 } } },
                User { address: Address { coordinate: Coordinate { zip: 3 } } }
            ];
            let funcs: (() -> int)[] = [];
            for (let User {
                address: Address {
                    coordinate: Coordinate { zip: let zip }
                }
            } in users) {
                funcs.push(() -> int { zip })
            };
            funcs[0]() * 100 + funcs[1]() * 10 + funcs[2]()
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(123)));
}

#[tokio::test]
async fn let_class_destructure_binds_inner_zip() {
    let output = baml_test!(
        r#"
        class Coordinate {
            zip int
        }

        class Address {
            coordinate Coordinate
        }

        class User {
            address Address
        }

        function main() -> int {
            let User {
                address: Address {
                    coordinate: Coordinate { zip: let zip }
                }
            } = User {
                address: Address {
                    coordinate: Coordinate { zip: 80808 }
                }
            };
            zip
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(80808)));
}

#[tokio::test]
async fn match_class_destructure_binds_inner_zip() {
    let output = baml_test!(
        r#"
        class Coordinate {
            zip int
        }

        class Address {
            coordinate Coordinate
        }

        class User {
            address Address
        }

        function main() -> int {
            match (User { address: Address { coordinate: Coordinate { zip: 9 } } }) {
                User {
                    address: Address {
                        coordinate: Coordinate { zip: 1 }
                    }
                } => 1,
                User {
                    address: Address {
                        coordinate: Coordinate { zip: let zip }
                    }
                } => zip
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(9)));
}

#[tokio::test]
async fn for_class_destructure_binds_inner_zip() {
    let output = baml_test!(
        r#"
        class Coordinate {
            zip int
        }

        class Address {
            coordinate Coordinate
        }

        class User {
            address Address
        }

        function main() -> int {
            let users = [
                User { address: Address { coordinate: Coordinate { zip: 4 } } },
                User { address: Address { coordinate: Coordinate { zip: 5 } } },
                User { address: Address { coordinate: Coordinate { zip: 6 } } }
            ];
            let product = 1;
            for (let User {
                address: Address {
                    coordinate: Coordinate { zip: let zip }
                }
            } in users) {
                product *= zip
            };
            product
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(120)));
}

#[tokio::test]
async fn match_class_field_pattern_should_refine_parent_union_scrutinee() {
    let output = baml_test!(
        r#"
        class A {
            field int
        }

        class B {
            field string
        }

        function score(v: A | B) -> int {
            match (v) {
                A { field: int } => v.field
                _ => 0
            }
        }

        function main() -> int {
            score(A { field: 42 })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

// Class destructure with a union-typed field, where each arm narrows on the
// field's runtime type. The outer class is fixed (`Foo`); the matrix should
// recurse into the field column and treat the three arms as exhaustive over
// `int | float | bool` without a wildcard.
#[tokio::test]
async fn match_class_destructure_union_field_int_arm() {
    let output = baml_test!(
        r#"
        class Foo {
            value int | float | bool
        }

        function score(f: Foo) -> int {
            match (f) {
                Foo { value: int } => 1,
                Foo { value: float } => 2,
                Foo { value: bool } => 3
            }
        }

        function main() -> int {
            score(Foo { value: 7 })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn match_class_destructure_union_field_float_arm() {
    let output = baml_test!(
        r#"
        class Foo {
            value int | float | bool
        }

        function score(f: Foo) -> int {
            match (f) {
                Foo { value: int } => 1,
                Foo { value: float } => 2,
                Foo { value: bool } => 3
            }
        }

        function main() -> int {
            score(Foo { value: 1.5 })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn match_class_destructure_union_field_bool_arm() {
    let output = baml_test!(
        r#"
        class Foo {
            value int | float | bool
        }

        function score(f: Foo) -> int {
            match (f) {
                Foo { value: int } => 1,
                Foo { value: float } => 2,
                Foo { value: bool } => 3
            }
        }

        function main() -> int {
            score(Foo { value: true })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn match_array_element_pattern_should_refine_parent_union_scrutinee() {
    let output = baml_test!(
        r#"
        function score(v: int[] | string[]) -> int {
            match (v) {
                [let x: int] => v[0] + x
                _ => 0
            }
        }

        function main() -> int {
            score([21])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn match_or_class_patterns_should_refine_parent_union_scrutinee() {
    let output = baml_test!(
        r#"
        class A {
            field int
        }

        class B {
            field int
        }

        class C {
            field int
        }

        class D {
            field int
        }

        class E {
            field string
        }

        function score(v: A | B | C | D | E) -> int {
            match (v) {
                A { field: int } | B { field: int } | C { field: int } | D { field: int } => v.field
                _ => 0
            }
        }

        function main() -> int {
            score(D { field: 42 })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn match_or_array_patterns_should_refine_parent_union_scrutinee() {
    let output = baml_test!(
        r#"
        function score(v: int[] | string[]) -> int {
            match (v) {
                [let x: int] | [let x: int, ..] => v[0] + x
                _ => 0
            }
        }

        function main() -> int {
            score([21])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

// ============================================================================
// Ported from rustc tests/ui/pattern/usefulness — runtime dispatch
// (skipping cases that need `..rest` named-rest bindings)
// ============================================================================

// rustc: `nested-exhaustive-match.rs` analog. Class with bool + optional;
// every (bool × null/non-null) combination is covered explicitly.
#[tokio::test]
async fn rustc_port_nested_class_optional_full_coverage_true_present() {
    let output = baml_test!(
        r#"
        class Foo3 {
            first bool
            second int[]?
        }

        function score(f: Foo3) -> int {
            match (f) {
                Foo3 { first: true, second: null } => 1,
                Foo3 { first: true, second: int[] } => 2,
                Foo3 { first: false, second: null } => 3,
                Foo3 { first: false, second: int[] } => 4
            }
        }

        function main() -> int {
            score(Foo3 { first: true, second: [1, 2, 3] })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn rustc_port_nested_class_optional_full_coverage_false_null() {
    let output = baml_test!(
        r#"
        class Foo3 {
            first bool
            second int[]?
        }

        function score(f: Foo3) -> int {
            match (f) {
                Foo3 { first: true, second: null } => 1,
                Foo3 { first: true, second: int[] } => 2,
                Foo3 { first: false, second: null } => 3,
                Foo3 { first: false, second: int[] } => 4
            }
        }

        function main() -> int {
            score(Foo3 { first: false, second: null })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

// rustc: `slice-patterns-reachability.rs` reachability dispatch — make sure
// `[true, ..]` actually fires for any non-empty true-leading bool array.
#[tokio::test]
async fn rustc_port_slice_prefix_dispatch() {
    let output = baml_test!(
        r#"
        function score(xs: bool[]) -> int {
            match (xs) {
                [true, ..] => 1,
                [.., false] => 2,
                [] => 3,
                _ => 4
            }
        }

        function main() -> int {
            score([true, false, false])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// `[.., false]` covers a non-empty false-trailing array even when the leading
// element isn't `true`.
#[tokio::test]
async fn rustc_port_slice_suffix_dispatch() {
    let output = baml_test!(
        r#"
        function score(xs: bool[]) -> int {
            match (xs) {
                [true, ..] => 1,
                [.., false] => 2,
                [] => 3,
                _ => 4
            }
        }

        function main() -> int {
            score([false, true, false])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

// rustc: enum dispatch over a closed variant set — each variant routes to
// its own arm.
#[tokio::test]
async fn rustc_port_enum_full_variant_dispatch_north() {
    let output = baml_test!(
        r#"
        enum Direction {
            North
            East
            South
            West
        }

        function describe(d: Direction) -> string {
            match (d) {
                Direction.North => "up",
                Direction.East => "right",
                Direction.South => "down",
                Direction.West => "left"
            }
        }

        function main() -> string {
            describe(Direction.North)
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("up".to_string()))
    );
}

#[tokio::test]
async fn rustc_port_enum_full_variant_dispatch_west() {
    let output = baml_test!(
        r#"
        enum Direction {
            North
            East
            South
            West
        }

        function describe(d: Direction) -> string {
            match (d) {
                Direction.North => "up",
                Direction.East => "right",
                Direction.South => "down",
                Direction.West => "left"
            }
        }

        function main() -> string {
            describe(Direction.West)
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("left".to_string()))
    );
}

// rustc: `or-patterns/exhaustiveness-pass.rs` shape — Or-pattern in a class
// field plus a wildcard catch-all. Each side of the Or must dispatch to the
// same arm.
#[tokio::test]
async fn rustc_port_or_pattern_in_class_field_first_alt() {
    let output = baml_test!(
        r#"
        class Pair {
            a int
            b int
        }

        function score(p: Pair) -> int {
            match (p) {
                Pair { a: 0 | 1, b: 2 | 3 } => 1,
                _ => 0
            }
        }

        function main() -> int {
            score(Pair { a: 0, b: 3 })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn rustc_port_or_pattern_in_class_field_falls_to_wildcard() {
    let output = baml_test!(
        r#"
        class Pair {
            a int
            b int
        }

        function score(p: Pair) -> int {
            match (p) {
                Pair { a: 0 | 1, b: 2 | 3 } => 1,
                _ => 0
            }
        }

        function main() -> int {
            score(Pair { a: 5, b: 5 })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

// rustc: optional with literal Or — `null | 0 | 1` covers the listed cases;
// any other int falls to the wildcard.
#[tokio::test]
async fn rustc_port_optional_literal_or_arm_zero() {
    let output = baml_test!(
        r#"
        function score(x: int?) -> int {
            match (x) {
                null | 0 | 1 => 1,
                _ => 0
            }
        }

        function main() -> int {
            score(0)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn rustc_port_optional_literal_or_arm_null() {
    let output = baml_test!(
        r#"
        function score(x: int?) -> int {
            match (x) {
                null | 0 | 1 => 1,
                _ => 0
            }
        }

        function main() -> int {
            score(null)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn rustc_port_optional_literal_or_arm_other() {
    let output = baml_test!(
        r#"
        function score(x: int?) -> int {
            match (x) {
                null | 0 | 1 => 1,
                _ => 0
            }
        }

        function main() -> int {
            score(99)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}
