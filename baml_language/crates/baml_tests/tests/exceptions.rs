//! Unified tests for catch/throw exception semantics.
//!
//! Organized as a combinatorial matrix:
//!   error source × catch pattern → expected outcome
//!
//! Error sources: user `throw` (string/int), runtime panics (DivisionByZero,
//! IndexOutOfBounds, MapKeyNotFound).
//!
//! Catch patterns: `_` (wildcard — user errors only), explicit panic class,
//! `Panic` type alias (all panics), mixed.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ============================================================================
// §1 — Bytecode snapshots: representative codegen for throw/catch
// ============================================================================

#[tokio::test]
async fn bytecode_handled_throw() {
    let output = baml_test!(
        r#"
        function fails() -> string {
            throw "boom";
        }

        function main() -> string {
            fails() catch (e) {
                _ => "recovered"
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function fails() -> string {
        load_const "boom"
        throw
    }

    function main() -> string {
        call user.fails
        jump L0
        load_const "recovered"

      L0:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("recovered".to_string()))
    );
}

#[tokio::test]
async fn bytecode_unhandled_throw() {
    let output = baml_test!(
        r#"
        function main() -> int {
            throw 42;
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 42
        throw
    }
    ");

    assert_eq!(
        output.result,
        Err(bex_engine::EngineError::UnhandledThrow {
            value: Box::new(BexExternalValue::Int(42)),
        })
    );
}

// ============================================================================
// §2 — Unhandled errors propagate with correct values
// ============================================================================

#[tokio::test]
async fn unhandled_throw_string() {
    let output = baml_test!(
        r#"
        function main() -> string {
            throw "something went wrong";
        }
    "#
    );

    assert_eq!(
        output.result,
        Err(bex_engine::EngineError::UnhandledThrow {
            value: Box::new(BexExternalValue::String("something went wrong".to_string())),
        })
    );
}

#[tokio::test]
async fn unhandled_division_by_zero() {
    let output = baml_test!(
        r#"
        function main() -> int {
            1 / 0
        }
    "#
    );

    assert!(
        output.result.is_err(),
        "expected DivisionByZero panic, got {:?}",
        output.result
    );
}

#[tokio::test]
async fn unhandled_index_out_of_bounds() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let arr = [1, 2, 3];
            arr[99]
        }
    "#
    );

    assert!(
        output.result.is_err(),
        "expected IndexOutOfBounds panic, got {:?}",
        output.result
    );
}

// ============================================================================
// §3 — Wildcard: catches user throws, does NOT catch panics
// ============================================================================

#[tokio::test]
async fn wildcard_catches_user_throw() {
    let output = baml_test!(
        r#"
        function fails() -> int {
            throw "user error";
        }

        function main() -> int {
            fails() catch (e) {
                _ => -1
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn wildcard_does_not_catch_division_by_zero() {
    // divides() has a genuine throw path so _ is reachable (matches the
    // string throw type).  At runtime x=0 hits the panic path instead.
    let output = baml_test!(
        r#"
        function divides(x: int) -> int {
            if (x > 100) { throw "too big" }
            1 / x
        }

        function main() -> int {
            divides(0) catch (e) {
                _ => -1
            }
        }
    "#
    );

    assert!(
        output.result.is_err(),
        "expected panic to propagate past _, got {:?}",
        output.result
    );
}

#[tokio::test]
async fn wildcard_does_not_catch_index_out_of_bounds() {
    let output = baml_test!(
        r#"
        function oob(x: int) -> int {
            if (x > 100) { throw "too big" }
            let arr = [1, 2, 3];
            arr[x]
        }

        function main() -> int {
            oob(99) catch (e) {
                _ => -1
            }
        }
    "#
    );

    assert!(
        output.result.is_err(),
        "expected panic to propagate past _, got {:?}",
        output.result
    );
}

// ============================================================================
// §4 — Explicit panic patterns: each type catches its own panic
// ============================================================================

#[tokio::test]
async fn catch_division_by_zero() {
    let output = baml_test!(
        r#"
        function divides() -> int {
            1 / 0
        }

        function main() -> int {
            divides() catch (e) {
                DivisionByZero => -1
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn catch_index_out_of_bounds() {
    let output = baml_test!(
        r#"
        function oob() -> int {
            let arr = [10, 20, 30];
            arr[5]
        }

        function main() -> int {
            oob() catch (e) {
                IndexOutOfBounds => -1
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn catch_map_key_not_found() {
    let output = baml_test!(
        r#"
        function bad_key() -> int {
            let m = {"a": 1};
            m["missing"]
        }

        function main() -> int {
            bad_key() catch (e) {
                MapKeyNotFound => -1
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn catch_negative_index_as_out_of_bounds() {
    let output = baml_test!(
        r#"
        function bad_neg() -> int {
            let arr = [10, 20];
            arr[-1]
        }

        function main() -> int {
            bad_neg() catch (e) {
                IndexOutOfBounds => -1
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

// ============================================================================
// §5 — Wrong explicit pattern does not catch
// ============================================================================

#[tokio::test]
#[ignore = "bare type sugar in catch arms lacks IsType check — first arm matches unconditionally"]
async fn wrong_panic_pattern_propagates() {
    // DivisionByZero should propagate past the IndexOutOfBounds arm.
    let output = baml_test!(
        r#"
        function divides() -> int {
            1 / 0
        }

        function main() -> int {
            divides() catch (e) {
                IndexOutOfBounds => -1
            }
        }
    "#
    );

    assert!(
        output.result.is_err(),
        "expected DivisionByZero to propagate, got {:?}",
        output.result
    );
}

// ============================================================================
// §6 — Panic type alias catches all panics
// ============================================================================

#[tokio::test]
#[ignore = "IsType emit doesn't handle union types — Panic alias expands to a union"]
async fn panic_alias_catches_division_by_zero() {
    let output = baml_test!(
        r#"
        function divides() -> int {
            1 / 0
        }

        function main() -> int {
            divides() catch (e) {
                Panic => -1
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
#[ignore = "IsType emit doesn't handle union types — Panic alias expands to a union"]
async fn panic_alias_catches_index_out_of_bounds() {
    let output = baml_test!(
        r#"
        function oob() -> int {
            let arr = [1, 2];
            arr[99]
        }

        function main() -> int {
            oob() catch (e) {
                Panic => -1
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

// ============================================================================
// §7 — Mixed dispatch: user errors + panics in the same catch
// ============================================================================

#[tokio::test]
async fn mixed_panic_fires_explicit_arm_catches() {
    // x=0 → DivisionByZero panic → explicit arm catches it.
    let output = baml_test!(
        r#"
        function divides(x: int) -> int {
            if (x > 100) { throw "too big" }
            1 / x
        }

        function main() -> int {
            divides(0) catch (e) {
                DivisionByZero => 42,
                _ => -1
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
#[ignore = "bare type sugar in catch arms lacks IsType check — first arm matches unconditionally"]
async fn mixed_user_error_fires_wildcard_catches() {
    // x=999 → throw "too big" → wildcard catches it.
    let output = baml_test!(
        r#"
        function divides(x: int) -> int {
            if (x > 100) { throw "too big" }
            1 / x
        }

        function main() -> int {
            divides(999) catch (e) {
                DivisionByZero => 42,
                _ => -1
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
#[ignore = "IsType emit doesn't handle union types — Panic alias expands to a union"]
async fn mixed_panic_alias_catches_panic() {
    let output = baml_test!(
        r#"
        function divides(x: int) -> int {
            if (x > 100) { throw "too big" }
            1 / x
        }

        function main() -> int {
            divides(0) catch (e) {
                Panic => 42,
                _ => -1
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
#[ignore = "IsType emit doesn't handle union types — Panic alias expands to a union"]
async fn mixed_panic_alias_wildcard_catches_user_error() {
    let output = baml_test!(
        r#"
        function divides(x: int) -> int {
            if (x > 100) { throw "too big" }
            1 / x
        }

        function main() -> int {
            divides(999) catch (e) {
                Panic => 42,
                _ => -1
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

// ============================================================================
// §8 — Multiple explicit panic patterns
// ============================================================================

#[tokio::test]
async fn multiple_panics_first_matches() {
    let output = baml_test!(
        r#"
        function divides() -> int {
            1 / 0
        }

        function main() -> int {
            divides() catch (e) {
                DivisionByZero => 1,
                IndexOutOfBounds => 2
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
#[ignore = "bare type sugar in catch arms lacks IsType check — first arm matches unconditionally"]
async fn multiple_panics_second_matches() {
    let output = baml_test!(
        r#"
        function oob() -> int {
            let arr = [10];
            arr[5]
        }

        function main() -> int {
            oob() catch (e) {
                DivisionByZero => 1,
                IndexOutOfBounds => 2
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

// ============================================================================
// §9 — Nested catch, rethrow, sequential
// ============================================================================

#[tokio::test]
async fn nested_inner_catches_outer_does_not_fire() {
    let output = baml_test!(
        r#"
        function inner_throws() -> int {
            throw "inner";
        }

        function middle() -> int {
            inner_throws() catch (e) {
                _ => 42
            }
        }

        function main() -> int {
            middle() catch (e) {
                _ => -1
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn nested_inner_catches_panic_outer_catches_rethrow() {
    let output = baml_test!(
        r#"
        function divides() -> int {
            1 / 0
        }

        function middle() -> int {
            let result = divides() catch (e) {
                DivisionByZero => -1
            };
            throw "recovered but failing"
        }

        function main() -> int {
            middle() catch (e) {
                _ => 99
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

#[tokio::test]
async fn rethrow_propagates_to_outer() {
    let output = baml_test!(
        r#"
        function inner() -> int {
            throw "original";
        }

        function middle() -> int {
            inner() catch (e) {
                _ => throw "rethrown"
            }
        }

        function main() -> int {
            middle() catch (e) {
                _ => 99
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

#[tokio::test]
async fn sequential_catches_both_recover() {
    let output = baml_test!(
        r#"
        function fails1() -> int {
            throw "one";
        }

        function fails2() -> int {
            throw "two";
        }

        function main() -> int {
            let a = fails1() catch (e) { _ => 10 };
            let b = fails2() catch (e) { _ => 20 };
            a + b
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(30)));
}

// ============================================================================
// §10 — Divergence and special cases
// ============================================================================

#[tokio::test]
async fn throw_in_match_arm_propagates() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let a = 2;
            return match (a) {
                1 => "one",
                _ => {
                    throw "boom"
                },
            };
        }
    "#
    );

    assert_eq!(
        output.result,
        Err(bex_engine::EngineError::UnhandledThrow {
            value: Box::new(BexExternalValue::String("boom".to_string())),
        })
    );
}

#[tokio::test]
async fn inline_throw_catch_binds_correctly() {
    let output = baml_test!(
        "
        function main() -> int {
            return throw 1 catch (e) {
                _ => 2
            };
        }
    "
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
#[ignore = "compiler2: inline throw-catch inside match arm triggers false unreachable arm"]
async fn throw_catch_inside_match_arm() {
    let output = baml_test!(
        r#"
        function main() -> string {
            return match (2) {
                1 => "1",
                _ => throw 1 catch (e) {
                    _ => ".."
                },
            };
        }
    "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("..".to_string()))
    );
}

// ============================================================================
// §11 — Typed panic instance fields (future: catch arm type narrowing)
// ============================================================================

#[tokio::test]
#[ignore = "needs catch arm type narrowing to access fields on specific panic type"]
async fn caught_panic_has_accessible_fields() {
    let output = baml_test!(
        r#"
        function oob() -> int {
            let arr = [10, 20, 30];
            arr[7]
        }

        function main() -> int {
            oob() catch (e) {
                IndexOutOfBounds => e.index
            }
        }
    "#
    );

    // IndexOutOfBounds { index: 7, length: 3 }
    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}
