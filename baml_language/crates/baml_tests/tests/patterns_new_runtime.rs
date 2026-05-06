//! Runtime tests for new pattern features (chains, unions, etc.)

use baml_tests::baml_test;
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
            let Box { value } = boxed;
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
                Box {} => 1
            }
        }

        function classify_string(boxed: Box<string>) -> int {
            match (boxed) {
                Box {} => 20
            }
        }

        function main() -> int {
            let int_box: Box<int> = Box { value: 7 };
            let string_box: Box<string> = Box { value: "ready" };
            let Box {} = int_box;
            let Box {} = string_box;
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
                (Box { value: let value: int }: Box<int>) => value,
                (Box { value: let value: string }: Box<string>) => value.length()
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

#[tokio::test]
async fn empty_generic_class_destructure_covers_union_of_same_class_instantiations() {
    let output = baml_test!(
        r#"
        class Box<T> {
            value T
        }

        function classify(box: Box<int> | Box<string>) -> int {
            match (box) {
                Box {} => 7
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
            let Outer {
                inner: Box { value }
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
                Box { value: string } => 2,
                _ => 3
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
        }

        class Address {
            coordinate Coordinate
        }

        class User {
            address Address
        }

        function main() -> int {
            match (User { address: Address { coordinate: Coordinate { zip: 7 } } }) {
                (
                    User { address: Address { coordinate: Coordinate { zip: let zip: 0 }: Coordinate }: Address }: User
                ) | (
                    User { address: Address { coordinate: Coordinate { zip: let zip: int }: Coordinate }: Address }: User
                ) => zip
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
