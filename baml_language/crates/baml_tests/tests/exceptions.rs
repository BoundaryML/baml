//! Unified tests for catch/throw exception semantics.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn handled_runtime_error_continues_execution() {
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
async fn handled_throw_from_callee_returns_fallback_value() {
    let output = baml_test!(
        r#"
        function throws_now() -> int {
            throw 7;
        }

        function main() -> int {
            throws_now() catch (e) {
                _ => 99
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        call user.throws_now
        jump L0
        load_const 99

      L0:
        return
    }

    function throws_now() -> int {
        load_const 7
        throw
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

#[tokio::test]
async fn catch_binds_to_throw_expression_not_throw_payload() {
    let output = baml_test!(
        "
        function main() -> int {
            return throw 1 catch (e) {
                _ => 2
            };
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 2
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn match_arm_block_with_throw_is_not_typed_as_void() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let a = 1;
            return match (a) {
                1 => "1",
                _ => {
                    throw 1
                },
            };
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const 1
        copy 0
        load_const 1
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L1

      L0:
        pop 1
        load_const 1
        throw

      L1:
        load_const "1"
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::String("1".to_string())));
}

#[tokio::test]
#[ignore = "compiler2: inline throw-catch inside match arm triggers false unreachable arm"]
async fn throw_catch_inside_match_arm_returns_catch_value() {
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

    insta::assert_snapshot!(output.bytecode, @"");

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("..".to_string()))
    );
}

#[tokio::test]
async fn throw_in_match_arm_diverges() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let a = 1;
            return match (a) {
                1 => "one",
                _ => {
                    throw "error"
                },
            };
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const 1
        copy 0
        load_const 1
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L1

      L0:
        pop 1
        load_const "error"
        throw

      L1:
        load_const "one"
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("one".to_string()))
    );
}

#[tokio::test]
async fn return_diverges() {
    let output = baml_test!(
        r#"
        function main() -> string {
            return "hello";
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "hello"
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hello".to_string()))
    );
}

#[tokio::test]
async fn if_else_both_throw_diverges() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let a = 1;
            return match (a) {
                1 => "one",
                _ => {
                    if (true) {
                        throw "a"
                    } else {
                        throw "b"
                    }
                },
            };
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const 1
        copy 0
        load_const 1
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L3

      L0:
        pop 1
        load_const true
        pop_jump_if_false L1
        jump L2

      L1:
        load_const "b"
        throw

      L2:
        load_const "a"
        throw

      L3:
        load_const "one"
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("one".to_string()))
    );
}

#[tokio::test]
async fn unhandled_throw_fails_predictably() {
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

#[tokio::test]
async fn unhandled_throw_string_shows_value() {
    let output = baml_test!(
        r#"
        function main() -> string {
            throw "something went wrong";
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "something went wrong"
        throw
    }
    "#);

    assert_eq!(
        output.result,
        Err(bex_engine::EngineError::UnhandledThrow {
            value: Box::new(BexExternalValue::String("something went wrong".to_string())),
        })
    );
}

#[tokio::test]
async fn unhandled_throw_string_in_match_shows_value() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let a = 1;
            match (a) {
                _ => {
                    throw "oops"
                }
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "oops"
        throw
    }
    "#);

    assert_eq!(
        output.result,
        Err(bex_engine::EngineError::UnhandledThrow {
            value: Box::new(BexExternalValue::String("oops".to_string())),
        })
    );
}

#[tokio::test]
async fn throw_in_non_matching_match_arm_propagates() {
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

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const 2
        copy 0
        load_const 1
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L1

      L0:
        pop 1
        load_const "boom"
        throw

      L1:
        load_const "one"
        return
    }
    "#);

    assert_eq!(
        output.result,
        Err(bex_engine::EngineError::UnhandledThrow {
            value: Box::new(BexExternalValue::String("boom".to_string())),
        })
    );
}

// ============================================================================
// Panic narrowing: _ does NOT catch panics, explicit patterns do
// ============================================================================

// --- Wildcard behavior ---

#[tokio::test]
async fn wildcard_catches_user_throws() {
    let output = baml_test!(
        r#"
        function throws_error() -> int {
            throw "user error";
        }

        function main() -> int {
            throws_error() catch (e) {
                _ => -1
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn wildcard_does_not_catch_panic() {
    // divides() has a genuine throw path (throw "too big") so the throw
    // set is non-empty and `_` is reachable.  At runtime x=0 hits the
    // panic path, and `_` does NOT catch panics.
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
        "expected panic to propagate, got {:?}",
        output.result
    );
}

// --- Explicit panic patterns ---

#[tokio::test]
async fn explicit_panic_catches_division_by_zero() {
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
async fn explicit_panic_catches_index_out_of_bounds() {
    let output = baml_test!(
        r#"
        function bad_index() -> int {
            let arr = [10, 20, 30];
            arr[5]
        }

        function main() -> int {
            bad_index() catch (e) {
                IndexOutOfBounds => -1
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn explicit_panic_catches_key_not_found() {
    let output = baml_test!(
        r#"
        function bad_key() -> int {
            let m = {"a": 1};
            m["missing"]
        }

        function main() -> int {
            bad_key() catch (e) {
                KeyNotFound => -1
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn explicit_panic_catches_negative_index() {
    let output = baml_test!(
        r#"
        function bad_neg() -> int {
            let arr = [10, 20];
            arr[-1]
        }

        function main() -> int {
            bad_neg() catch (e) {
                NegativeIndex => -1
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

// --- Panic type alias ---

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

// --- Mix: user errors + panics in the same catch ---
//
// NOTE: Bare type sugar patterns (e.g. `DivisionByZero =>`) are
// `Pattern::Binding` at the AST level.  The TIR recognizes them as type
// patterns for diagnostics, but the MIR's `lower_pattern_test` treats
// them as unconditional bindings (`goto success`) without generating an
// `IsType` check.  Tests marked `ignore` below expose this pre-existing
// bug — they will pass once the MIR emits proper type discrimination
// for bare type sugar patterns in catch arms.

#[tokio::test]
async fn explicit_panic_and_wildcard_together() {
    // divides() has a throw path so `_` is reachable for user errors.
    // At runtime the panic fires and the explicit pattern catches it.
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
async fn wildcard_catches_user_error_when_panic_arm_present() {
    // At runtime the user error fires and `_` should catch it, but
    // the DivisionByZero arm matches unconditionally (missing type test).
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
async fn panic_alias_and_wildcard_together() {
    // Panic catches all panics, `_` catches user errors.
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
#[ignore = "bare type sugar in catch arms lacks IsType check — first arm matches unconditionally"]
async fn panic_alias_user_error_fallback() {
    // When user error fires, Panic shouldn't match, `_` should catch it.
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

// --- Multiple explicit panic patterns ---

#[tokio::test]
async fn multiple_panic_patterns_first_matches() {
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
async fn multiple_panic_patterns_second_matches() {
    // IndexOutOfBounds should match, not DivisionByZero.
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

// --- Nested catch: inner catches panic, outer catches user error ---

#[tokio::test]
async fn nested_inner_catches_panic_outer_catches_user_error() {
    let output = baml_test!(
        r#"
        function divides() -> int {
            1 / 0
        }

        function middle() -> int {
            let result = divides() catch (e) {
                DivisionByZero => -1
            };
            // After recovering from the panic, throw a user error
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

// --- Unmatched panic propagates past non-matching explicit pattern ---

#[tokio::test]
#[ignore = "bare type sugar in catch arms lacks IsType check — first arm matches unconditionally"]
async fn wrong_panic_pattern_does_not_catch() {
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
// Nested catch — inner handler catches, outer doesn't fire
// ============================================================================

#[tokio::test]
async fn nested_catch_inner_handles() {
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

    // Inner catch handles it, middle returns 42, outer catch never fires.
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

// ============================================================================
// Sequential errors — first caught, execution continues, second also caught
// ============================================================================

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
// Uncaught panics propagate as unhandled errors
// ============================================================================

#[tokio::test]
async fn uncaught_division_by_zero_propagates() {
    let output = baml_test!(
        r#"
        function div(a: int, b: int) -> int {
            a / b
        }

        function main() -> int {
            div(1, 0)
        }
    "#
    );

    assert!(
        output.result.is_err(),
        "expected error, got {:?}",
        output.result
    );
}

#[tokio::test]
async fn uncaught_index_out_of_bounds_propagates() {
    let output = baml_test!(
        r#"
        function oob() -> int {
            let arr = [1, 2, 3];
            arr[99]
        }

        function main() -> int {
            oob()
        }
    "#
    );

    assert!(
        output.result.is_err(),
        "expected error, got {:?}",
        output.result
    );
}

// ============================================================================
// Re-throw from catch arm propagates to outer handler
// ============================================================================

#[tokio::test]
async fn rethrow_propagates_to_outer_catch() {
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

// ============================================================================
// Typed panic instance fields — verify the caught error has accessible fields
// ============================================================================

#[tokio::test]
#[ignore = "needs catch arm type narrowing to access fields on specific panic type"]
async fn caught_index_oob_has_index_field() {
    let output = baml_test!(
        r#"
        function oob() -> int {
            let arr = [10, 20, 30];
            arr[7]
        }

        function main() -> int {
            oob() catch (e) {
                _ => e.index
            }
        }
    "#
    );

    // IndexOutOfBounds { index: 7, length: 3 } — e.index should be 7
    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

#[tokio::test]
#[ignore = "needs catch arm type narrowing to access fields on specific panic type"]
async fn caught_index_oob_has_length_field() {
    let output = baml_test!(
        r#"
        function oob() -> int {
            let arr = [10, 20, 30];
            arr[7]
        }

        function main() -> int {
            oob() catch (e) {
                _ => e.length
            }
        }
    "#
    );

    // IndexOutOfBounds { index: 7, length: 3 } — e.length should be 3
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
#[ignore = "needs catch arm type narrowing to access fields on specific panic type"]
async fn caught_division_by_zero_has_dividend_field() {
    let output = baml_test!(
        r#"
        function div(a: int, b: int) -> int {
            a / b
        }

        function main() -> int {
            div(42, 0) catch (e) {
                _ => e.dividend
            }
        }
    "#
    );

    // DivisionByZero { dividend: 42 } — e.dividend should be 42
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}
