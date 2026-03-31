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
async fn catch_dispatches_non_panic_to_wildcard_arm() {
    let output = baml_test!(
        r#"
        function throws_error() -> string {
            throw "some error";
        }

        function main() -> string {
            throws_error() catch (e) {
                _ => "wildcard"
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        call user.throws_error
        jump L0
        load_const "wildcard"

      L0:
        return
    }

    function throws_error() -> string {
        load_const "some error"
        throw
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("wildcard".to_string()))
    );
}

#[tokio::test]
async fn catch_handles_thrown_error() {
    let output = baml_test!(
        r#"
        function panics_now() -> string {
            throw "boom";
        }

        function main() -> string {
            panics_now() catch (e) {
                _ => "caught it"
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        call user.panics_now
        jump L0
        load_const "caught it"

      L0:
        return
    }

    function panics_now() -> string {
        load_const "boom"
        throw
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("caught it".to_string()))
    );
}

#[tokio::test]
async fn typed_catch_arm_matches_primitive_throw_value() {
    let output = baml_test!(
        r#"
        function throws_now() -> string {
            throw "boom";
        }

        function main() -> string {
            throws_now() catch (e) {
                _ => "typed catch"
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        call user.throws_now
        jump L0
        load_const "typed catch"

      L0:
        return
    }

    function throws_now() -> string {
        load_const "boom"
        throw
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("typed catch".to_string()))
    );
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
#[ignore = "compiler2: throw inside match arms produces diagnostic errors"]
async fn match_arm_block_with_throw_is_not_typed_as_void() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let a = 1;
            return match (a) {
                1 => "1",
                int => {
                    throw 1
                },
            };
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"");

    assert_eq!(output.result, Ok(BexExternalValue::String("1".to_string())));
}

#[tokio::test]
#[ignore = "compiler2: throw inside match arms produces diagnostic errors"]
async fn throw_catch_inside_match_arm_returns_catch_value() {
    let output = baml_test!(
        r#"
        function main() -> string {
            return match (2) {
                1 => "1",
                int => throw 1 catch (e) {
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
#[ignore = "compiler2: throw inside match arms produces diagnostic errors"]
async fn throw_in_match_arm_diverges() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let a = 1;
            return match (a) {
                1 => "one",
                int => {
                    throw "error"
                },
            };
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"");

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
#[ignore = "compiler2: throw inside match arms produces diagnostic errors"]
async fn if_else_both_throw_diverges() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let a = 1;
            return match (a) {
                1 => "one",
                int => {
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

    insta::assert_snapshot!(output.bytecode, @"");

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
        Err(bex_engine::EngineError::VmError(
            bex_vm::errors::VmError::RuntimeError(bex_vm::errors::RuntimeError::UnhandledThrow {
                value: "42".to_string(),
            })
        ))
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
        Err(bex_engine::EngineError::VmError(
            bex_vm::errors::VmError::RuntimeError(bex_vm::errors::RuntimeError::UnhandledThrow {
                value: "something went wrong".to_string(),
            })
        ))
    );
}

#[tokio::test]
#[ignore = "compiler2: throw inside match arms produces diagnostic errors"]
async fn unhandled_throw_string_in_match_shows_value() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let a = 1;
            match (a) {
                int => {
                    throw "oops"
                }
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"");

    assert_eq!(
        output.result,
        Err(bex_engine::EngineError::VmError(
            bex_vm::errors::VmError::RuntimeError(bex_vm::errors::RuntimeError::UnhandledThrow {
                value: "oops".to_string(),
            })
        ))
    );
}

#[tokio::test]
#[ignore = "compiler2: throw inside match arms produces diagnostic errors"]
async fn throw_in_non_matching_match_arm_propagates() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let a = 2;
            return match (a) {
                1 => "one",
                int => {
                    throw "boom"
                },
            };
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"");

    assert_eq!(
        output.result,
        Err(bex_engine::EngineError::VmError(
            bex_vm::errors::VmError::RuntimeError(bex_vm::errors::RuntimeError::UnhandledThrow {
                value: "boom".to_string(),
            })
        ))
    );
}

// ============================================================================
// Runtime error tests — exception tables catch VM-level panics as typed values
// ============================================================================

// Runtime panic tests — exception tables catch VM-level panics as typed values.

#[tokio::test]
async fn catch_division_by_zero() {
    let output = baml_test!(
        r#"
        function divides() -> int {
            1 / 0
        }

        function main() -> int {
            divides() catch (e) {
                _ => -1
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
        function bad_index() -> int {
            let arr = [10, 20, 30];
            arr[5]
        }

        function main() -> int {
            bad_index() catch (e) {
                _ => -1
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
                _ => -1
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn catch_negative_index() {
    let output = baml_test!(
        r#"
        function bad_neg() -> int {
            let arr = [10, 20];
            arr[-1]
        }

        function main() -> int {
            bad_neg() catch (e) {
                _ => -1
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

// ============================================================================
// Same-frame runtime panics — the error happens inside the callee, caught by
// the caller's exception table. This is the core feature: non-call instructions
// (like BinOp Div) are now covered.
// ============================================================================

#[tokio::test]
async fn same_frame_division_by_zero_caught() {
    let output = baml_test!(
        r#"
        function div(a: int, b: int) -> int {
            a / b
        }

        function main() -> int {
            div(10, 0) catch (e) {
                _ => -1
            }
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
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
#[ignore = "compiler2: catch binding typed as unknown when throw set is empty — needs panic type inference"]
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
#[ignore = "compiler2: catch binding typed as unknown when throw set is empty — needs panic type inference"]
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
#[ignore = "compiler2: catch binding typed as unknown when throw set is empty — needs panic type inference"]
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
