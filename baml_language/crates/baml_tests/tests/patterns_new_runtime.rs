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
