//! Runtime tests for new pattern features (chains, unions, etc.)

use baml_tests::{baml_test, engine::compile_source};
use bex_engine::BexExternalValue;

// ============================================================================
// Chains of bare bindings — every link binds the same value.
// ============================================================================

// `let x: let y: let z = 1` — three bindings, all bound to 1.
#[tokio::test]
async fn let_chain_of_bare_bindings() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x: let y: let z = 1;
            x + y + z
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn let_chain_trailing_binding_after_type_aliases_narrowed_value() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x: int: let y = 1;
            x * 10 + y
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(11)));
}

#[tokio::test]
async fn let_chain_binding_array_pattern_and_type_alias_all_flow() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let xs: [..let rest]: int[] = [4, 5, 6];
            xs.length() * 100 + rest[1] * 10 + rest[2]
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(356)));
}

#[tokio::test]
async fn let_chain_class_field_alias_array_pattern_and_type() {
    let output = baml_test!(
        r#"
        class Team {
            scores int[]
        }

        function main() -> int {
            let Team {
                scores: let scores: [..let rest]: int[]
            } = Team { scores: [7, 8, 9] };
            scores.length() * 100 + rest[0] * 10 + rest[2]
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(379)));
}

#[tokio::test]
async fn let_chain_class_field_trailing_binding_after_type_aliases_field() {
    let output = baml_test!(
        r#"
        class Team {
            scores int[]
        }

        function main() -> int {
            let Team {
                scores: [..let rest]: int[]: let scores_again
            } = Team { scores: [7, 8, 9] };
            rest[0] * 100 + rest[1] * 10 + scores_again[2]
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(789)));
}

#[tokio::test]
async fn let_chain_rest_alias_array_pattern_and_type() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let [..let rest: [..let rows]: int[][]] = [[4], [5]];
            rest.length() * 100 + rows[1][0]
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(205)));
}

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
                let rows: [[let x]]: int[][]: let again => rows[0][0] * 100 + x * 10 + again[0][0],
                _ => 0
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(999)));
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
async fn for_let_chain_binding_array_pattern_and_type() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let total = 0;
            for (let row: [..let rest]: int[] in [[4, 5], [6, 7, 8]]) {
                total += row.length() * 100 + rest[0] * 10 + rest.length()
            };
            total
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(605)));
}

#[tokio::test]
async fn for_let_chain_trailing_binding_after_type_aliases_iteration_value() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let total = 0;
            for (let row: int[]: let alias in [[4, 5], [6, 7, 8]]) {
                total += row.length() * 100 + alias[0] * 10 + alias.length()
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
            for (let [..let row] in rows) {
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
            for (let [..let row] in rows) {
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
            for (let Team { scores: [..let scores] } in teams) {
                total += scores.length()
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

// Wildcard chain binding: `let _: int: int? = 5` declares no name but the
// chain narrow flows through. Body doesn't reference the binding.
#[tokio::test]
async fn let_wildcard_chain() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let _: int: int? = 5;
            42
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

// Chain with a literal narrow widened to a primitive — the binding takes the
// rightmost concrete type, so we can use ints. The chain covers all of `int`,
// making any wildcard arm unreachable, so we omit it.
#[tokio::test]
async fn match_literal_widens_chain() {
    let output = baml_test!(
        r#"
        function main() -> int {
            match (1) {
                let n: 1: int => n + 100
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
fn generic_class_destructure_rejects_trailing_type_chain_substitution() {
    let err = std::panic::catch_unwind(|| {
        compile_source(
            r#"
            class Box<T> {
                value T
            }

            function main() -> int {
                let boxed: Box<int> = Box { value: 42 };
                let Box { value }: Box<int> = boxed;
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
        function main() -> int {
            match ([1, 2, 3]) {
                [let first, ..let rest] => first * 10 + rest.length(),
                [] => 0
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(12)));
}

#[tokio::test]
async fn match_array_destructure_suffix_rest() {
    let output = baml_test!(
        r#"
        function main() -> int {
            match ([1, 2, 9]) {
                [..let rest, let last] => rest.length() * 10 + last,
                _ => 0
            }
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
            let [..let xs] = [3, 4, 5];
            xs.length()
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn match_array_destructure_typed_rest_narrows_element_type() {
    let output = baml_test!(
        r#"
        function score(xs: int[]) -> int {
            match (xs) {
                [..let rest: int[]] => rest[0] + rest.length()
            }
        }

        function main() -> int {
            score([4, 5])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

#[tokio::test]
async fn match_array_destructure_rest_chain_applies_to_rest_slice() {
    let output = baml_test!(
        r#"
        function score(xs: int[][]) -> int {
            match (xs) {
                [..let rest: int[][], let second_last: int[], let last: int[]] =>
                    rest.length() * 100 + second_last[0] * 10 + last[0],
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

#[tokio::test]
async fn match_array_destructure_rest_subpattern_array_shapes() {
    let output = baml_test!(
        r#"
        function score(xs: int[]) -> int {
            match (xs) {
                [_, ..[], _] => 20,
                [_, ..[_], _] => 30,
                [_, ..[_, _], _] => 40,
                _ => 0
            }
        }

        function main() -> int {
            score([1, 2]) * 1000 +
            score([1, 2, 3]) * 100 +
            score([1, 2, 3, 4]) * 10 +
            score([1, 2, 3, 4, 5])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(23400)));
}

#[tokio::test]
async fn match_array_destructure_rest_subpattern_array_with_class_destructure() {
    let output = baml_test!(
        r#"
        class Box {
            value int
        }

        function score(xs: Box[]) -> int {
            match (xs) {
                [..[Box { value: let first }, Box { value: let second }]] =>
                    first * 10 + second,
                _ => 0
            }
        }

        function main() -> int {
            score([Box { value: 3 }, Box { value: 4 }])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(34)));
}

#[tokio::test]
async fn match_array_destructure_rest_subpattern_deep_class_array_nesting() {
    let output = baml_test!(
        r#"
        class Leaf {
            value int
        }

        class Node {
            rows Leaf[][]
        }

        function score(xs: Node[]) -> int {
            match (xs) {
                [..[Node {
                    rows: [
                        [Leaf { value: let first }, ..[Leaf { value: let second }]],
                        ..[[Leaf { value: let third }]]
                    ]
                }]] => first * 100 + second * 10 + third,
                _ => 0
            }
        }

        function main() -> int {
            score([
                Node {
                    rows: [
                        [Leaf { value: 1 }, Leaf { value: 2 }],
                        [Leaf { value: 3 }]
                    ]
                }
            ])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(123)));
}

#[tokio::test]
async fn let_array_destructure_nested_rest_only_is_irrefutable() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let [..[..[..[..]]]] = [1, 2, 3];
            1
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn match_array_destructure_empty_rest_slice_vs_irrefutable_rest_slice() {
    let output = baml_test!(
        r#"
        function empty_only(xs: int[]) -> int {
            match (xs) {
                [..[]] => 1,
                _ => 2
            }
        }

        function rest_only(xs: int[]) -> int {
            match (xs) {
                [..[..]] => 3
            }
        }

        function main() -> int {
            empty_only([]) * 100 + empty_only([1]) * 10 + rest_only([1, 2])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(123)));
}

#[tokio::test]
async fn match_array_destructure_nested_rest_with_fixed_edges() {
    let output = baml_test!(
        r#"
        function main() -> int {
            match ([9, 1, 2, 3, 8]) {
                [let head, ..[let a, ..let middle, let b], let tail] =>
                    head * 10000 + a * 1000 + middle.length() * 100 + b * 10 + tail,
                _ => 0
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(91138)));
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
async fn match_or_binding_from_nested_rest_or_prefix_array() {
    let output = baml_test!(
        r#"
        function score(xs: int[]) -> int {
            match (xs) {
                [..[let value]] | [let value, ..] => value,
                _ => 0
            }
        }

        function main() -> int {
            score([4]) * 10 + score([7, 8])
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
            let xs: [.._]: int[] = [4, 5, 6];
            xs.length()
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn match_or_binding_from_class_array_field_or_array_rest_subpattern() {
    let output = baml_test!(
        r#"
        class User {
            scores int[]
        }

        function score(input: User | int[]) -> int {
            match (input) {
                User { scores: [let value, ..] } | [..[let value]] => value,
                _ => 0
            }
        }

        function main() -> int {
            score(User { scores: [4, 5] }) * 10 + score([7])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(47)));
}

#[tokio::test]
async fn match_class_nested_array_literals_and_bindings() {
    let output = baml_test!(
        r#"
        class User {
            rows int[][]
        }

        function main() -> int {
            match (User { rows: [[1, 2, 7], [9]] }) {
                User { rows: [[1, ..[2, let value]], ..] } => value,
                _ => 0
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

#[tokio::test]
async fn match_rest_with_typed_empty_slice_pattern() {
    let output = baml_test!(
        r#"
        function score(xs: int[]) -> int {
            match (xs) {
                [..[]: int[]] => 1,
                _ => 2
            }
        }

        function main() -> int {
            score([]) * 10 + score([1])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(12)));
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
            for (let [..[..[..]]] in [[1], [2, 3], []]) {
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
                [..let rest, let second_last, let last]: int[][] =>
                    rest.length() * 100 + second_last[0] * 10 + last[0],
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
                [User { score: let first }, ..let rest] =>
                    first * 10 + rest.length()
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
                [_, let second, .._] => second,
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
                [.._] => 7
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
                Team { scores: [_, let second, .._] } => second,
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
                [User { name: _, score: let first }, .._] => first,
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
            for (let [.._] in rows) {
                count += 1
            };
            count
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn match_array_destructure_deep_class_array_class_array_class() {
    let output = baml_test!(
        r#"
        class Leaf {
            value int
        }

        class Node {
            leaves Leaf[]
        }

        class Box {
            nodes Node[]
        }

        class Root {
            boxes Box[]
        }

        function score(root: Root) -> int {
            match (root) {
                Root {
                    boxes: [
                        Box {
                            nodes: [
                                Node {
                                    leaves: [
                                        Leaf { value: let first },
                                        ..let middle,
                                        Leaf { value: let last }
                                    ]
                                },
                                ..
                            ]
                        },
                        ..let extra_boxes
                    ]
                } => first * 1000 + middle.length() * 100 + last * 10 + extra_boxes.length(),
                _ => 0
            }
        }

        function main() -> int {
            score(Root {
                boxes: [
                    Box {
                        nodes: [
                            Node {
                                leaves: [
                                    Leaf { value: 1 },
                                    Leaf { value: 2 },
                                    Leaf { value: 3 }
                                ]
                            },
                            Node { leaves: [Leaf { value: 9 }] }
                        ]
                    },
                    Box { nodes: [Node { leaves: [Leaf { value: 8 }] }] }
                ]
            })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1131)));
}

#[tokio::test]
async fn match_array_destructure_quad_nested_array_class_suffixes() {
    let output = baml_test!(
        r#"
        class Item {
            value int
        }

        class Bucket {
            items Item[]
        }

        class Shelf {
            buckets Bucket[]
        }

        function score(shelves: Shelf[]) -> int {
            match (shelves) {
                [
                    Shelf {
                        buckets: [
                            ..let skipped_buckets,
                            Bucket {
                                items: [
                                    Item { value: let penultimate },
                                    Item { value: let last }
                                ]
                            }
                        ]
                    },
                    ..,
                    Shelf {
                        buckets: [
                            Bucket {
                                items: [Item { value: let tail }, ..]
                            },
                            ..
                        ]
                    }
                ] => skipped_buckets.length() * 1000 + penultimate * 100 + last * 10 + tail,
                _ => 0
            }
        }

        function main() -> int {
            score([
                Shelf {
                    buckets: [
                        Bucket { items: [Item { value: 1 }] },
                        Bucket {
                            items: [
                                Item { value: 4 },
                                Item { value: 5 }
                            ]
                        }
                    ]
                },
                Shelf {
                    buckets: [
                        Bucket { items: [Item { value: 9 }] }
                    ]
                },
                Shelf {
                    buckets: [
                        Bucket {
                            items: [
                                Item { value: 7 },
                                Item { value: 8 }
                            ]
                        }
                    ]
                }
            ])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1457)));
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

#[tokio::test]
async fn let_deep_class_destructure_with_chain_annotations() {
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
                        }: Coordinate
                    }: Address
                }: Profile
            }: User = User {
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
async fn match_deep_class_destructure_chain_falls_through_to_binding_arm() {
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
                            coordinate: Coordinate { zip: 11111 }: Coordinate
                        }: Address
                    }: Profile
                }: User => 1,
                User {
                    profile: Profile {
                        address: Address {
                            coordinate: Coordinate { zip: let zip }: Coordinate
                        }: Address
                    }: Profile
                }: User => zip
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(22222)));
}

#[tokio::test]
async fn match_or_deep_class_destructure_chain_binds_when_second_alt_matches() {
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
                    User { address: Address { coordinate: Coordinate { zip: let zip: int, plus4: 1 }: Coordinate }: Address }: User
                ) | (
                    User { address: Address { coordinate: Coordinate { zip: let zip: int, plus4: 2 }: Coordinate }: Address }: User
                ) => zip,
                _ => 0
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

#[tokio::test]
async fn for_deep_class_destructure_with_chain_annotations_sums_fields() {
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
                    coordinate: Coordinate { zip: let zip }: Coordinate
                }: Address
            }: User in users) {
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
                    coordinate: Coordinate { zip: let zip }: Coordinate
                }: Address
            }: User in users) {
                funcs.push(() -> int { zip })
            };
            funcs[0]() * 100 + funcs[1]() * 10 + funcs[2]()
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(123)));
}

#[tokio::test]
async fn let_class_destructure_repeated_class_chain_annotations() {
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
                    coordinate: Coordinate { zip: let zip }: Coordinate: Coordinate: Coordinate
                }: Address: Address: Address
            }: User: User: User: User = User {
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
async fn match_class_destructure_repeated_class_chain_annotations() {
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
                        coordinate: Coordinate { zip: 1 }: Coordinate: Coordinate
                    }: Address: Address
                }: User: User: User => 1,
                User {
                    address: Address {
                        coordinate: Coordinate { zip: let zip }: Coordinate: Coordinate: Coordinate
                    }: Address: Address: Address
                }: User: User: User: User => zip
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(9)));
}

#[tokio::test]
async fn for_class_destructure_repeated_class_chain_annotations() {
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
                    coordinate: Coordinate { zip: let zip }: Coordinate: Coordinate
                }: Address: Address: Address
            }: User: User: User: User in users) {
                product *= zip
            };
            product
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(120)));
}

#[tokio::test]
async fn match_array_of_class_destructure_trailing_let_aliases_at_many_levels() {
    let output = baml_test!(
        r#"
        class User {
            name string
            scores int[]
        }

        function main() -> int {
            let users = [
                User { name: "Ada", scores: [4, 5, 6] },
                User { name: "Ben", scores: [7, 8] },
                User { name: "Cy", scores: [9] }
            ];

            match (users) {
                [
                    User {
                        name: let first_name,
                        scores: [let head, ..let tail]: int[]: let scores_again
                    }: User: let first_user,
                    ..let rest_users: User[]: let rest_alias
                ]: User[]: let all_users => {
                    first_name.length() * 1000 +
                    head * 100 +
                    tail[0] * 10 +
                    scores_again[2] +
                    first_user.scores[0] +
                    rest_users.length() +
                    rest_alias.length() +
                    all_users.length()
                },
                _ => 0
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3467)));
}

#[tokio::test]
async fn match_or_nested_array_destructure_with_trailing_let_aliases() {
    let output = baml_test!(
        r#"
        function main() -> int {
            match ([[1], [2, 3, 4]]) {
                (
                    [[let first, ..let rest]: int[]: let row, _, _]: int[][]: let rows
                ) | (
                    [_, [let first, ..let rest]: int[]: let row]: int[][]: let rows
                ) => {
                    first * 1000 + rows.length() * 100 + row.length() * 10 + rest[1]
                },
                _ => 0
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2234)));
}

#[tokio::test]
async fn for_class_field_rest_destructure_trailing_let_aliases_at_many_levels() {
    let output = baml_test!(
        r#"
        class Bucket {
            values int[]
        }

        function main() -> int {
            let buckets = [
                Bucket { values: [1, 2] },
                Bucket { values: [3, 4, 5] }
            ];
            let total = 0;

            for (let Bucket {
                values: [..let values]: int[]: let values_again
            }: Bucket: let bucket_again in buckets) {
                total += values.length() * 100 +
                    values_again[0] * 10 +
                    bucket_again.values[bucket_again.values.length() - 1]
            };

            total
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(547)));
}

#[tokio::test]
async fn match_or_mixed_array_class_binding_class_field_or_array_rest() {
    let output = baml_test!(
        r#"
        class Class {
            field int[]
        }

        function main() -> int {
            let from_class: Class | int[][] = Class { field: [4, 5, 6] };
            let from_array: Class | int[][] = [[7, 8, 9]];

            let a = match (from_class) {
                Class { field } | [[..let field]: int[]] => field[0] * 100 + field[1] * 10 + field[2],
                _ => 0
            };

            let b = match (from_array) {
                Class { field } | [[..let field]: int[]] => field[0] * 100 + field[1] * 10 + field[2],
                _ => 0
            };

            a + b
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1245)));
}

#[tokio::test]
async fn let_or_mixed_class_field_or_whole_array_rest_is_irrefutable() {
    let output = baml_test!(
        r#"
        class Class {
            field int[][]
        }

        function pick(v: Class | int[][]) -> int {
            let Class { field } | [..let field] = v;
            field.length() * 100 + field[0][0] * 10 + field[field.length() - 1][0]
        }

        function main() -> int {
            pick(Class { field: [[1], [2]] }) +
            pick([[3], [4], [5]])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(547)));
}

#[tokio::test]
async fn match_or_mixed_written_class_and_structural_array_binding() {
    let output = baml_test!(
        r#"
        class Class {
            field int[]
        }

        function pick(v: Class | int[][]) -> int {
            match (v) {
                Class { field } | [[..let field]] => field[0] * 100 + field[1] * 10 + field[2],
                _ => 0
            }
        }

        function main() -> int {
            pick(Class { field: [1, 2, 3] }) + pick([[4, 5, 6]])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(579)));
}

#[tokio::test]
async fn match_or_mixed_structural_array_first_and_written_class_second() {
    let output = baml_test!(
        r#"
        class Class {
            field int[]
        }

        function pick(v: Class | int[][]) -> int {
            match (v) {
                [[..let field]] | Class { field } => field[0] * 100 + field[1] * 10 + field[2],
                _ => 0
            }
        }

        function main() -> int {
            pick([[1, 2, 3]]) + pick(Class { field: [4, 5, 6] })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(579)));
}

#[tokio::test]
async fn match_or_mixed_multiple_written_and_structural_alternatives() {
    let output = baml_test!(
        r#"
        class ClassA {
            field int[]
        }

        class ClassB {
            field int[]
        }

        function pick(v: ClassA | ClassB | int[][]) -> int {
            match (v) {
                ClassA { field } | [[..let field]] | ClassB { field } =>
                    field[0] * 100 + field[1] * 10 + field[2],
                _ => 0
            }
        }

        function main() -> int {
            pick(ClassA { field: [1, 2, 3] }) +
            pick([[4, 5, 6]]) +
            pick(ClassB { field: [7, 8, 9] })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1368)));
}

#[tokio::test]
async fn match_or_mixed_array_class_binding_array_rest_inside_class_or_class_inside_array() {
    let output = baml_test!(
        r#"
        class Class {
            field int[]
        }

        class Wrapper {
            matrix int[][]
        }

        function main() -> int {
            let from_wrapper: Wrapper | Class[][] = Wrapper { matrix: [[1, 2, 3]] };
            let from_array: Wrapper | Class[][] = [[Class { field: [4, 5] }]];

            let a = match (from_wrapper) {
                Wrapper { matrix: [[..let x]: int[]] } | [[Class { field: let x }]] =>
                    x[0] * 100 + x[1] * 10 + x.length(),
                _ => 0
            };

            let b = match (from_array) {
                Wrapper { matrix: [[..let x]: int[]] } | [[Class { field: let x }]] =>
                    x[0] * 100 + x[1] * 10 + x.length(),
                _ => 0
            };

            a + b
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(575)));
}

#[tokio::test]
async fn match_or_mixed_array_class_binding_deep_triple_array_and_nested_class() {
    let output = baml_test!(
        r#"
        class Leaf {
            xs int[]
        }

        class Node {
            leaf Leaf
        }

        function main() -> int {
            let from_nodes: Node[][] | Leaf[][][] = [[Node { leaf: Leaf { xs: [1, 2, 3] } }]];
            let from_leaves: Node[][] | Leaf[][][] = [[[Leaf { xs: [4, 5, 6] }]]];

            let a = match (from_nodes) {
                [[Node { leaf: Leaf { xs: [..let x]: int[] } }]] | [[[Leaf { xs: let x }]]] =>
                    x[0] * 100 + x[1] * 10 + x[2],
                _ => 0
            };

            let b = match (from_leaves) {
                [[Node { leaf: Leaf { xs: [..let x]: int[] } }]] | [[[Leaf { xs: let x }]]] =>
                    x[0] * 100 + x[1] * 10 + x[2],
                _ => 0
            };

            a + b
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(579)));
}

#[tokio::test]
async fn match_or_mixed_suffix_class_binding_and_prefix_array_rest_binding() {
    let output = baml_test!(
        r#"
        class Bucket {
            field int[]
        }

        function main() -> int {
            let from_class: Bucket[] | int[][] = [
                Bucket { field: [1, 2] },
                Bucket { field: [3, 4] }
            ];
            let from_array: Bucket[] | int[][] = [[5, 6], [7, 8]];

            let a = match (from_class) {
                [.._, Bucket { field: let x }] | [[..let x]: int[], .._] =>
                    x[0] * 10 + x[1],
                _ => 0
            };

            let b = match (from_array) {
                [.._, Bucket { field: let x }] | [[..let x]: int[], .._] =>
                    x[0] * 10 + x[1],
                _ => 0
            };

            a * 100 + b
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3456)));
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
