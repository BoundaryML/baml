//! Unified tests for catch/throw exception semantics.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn handled_runtime_error_continues_execution() {
    let output = baml_test!(
        "
        function fails() -> string {
            assert false;
            \"ok\"
        }

        function main() -> string {
            fails() catch (e) {
                _ => \"recovered\"
            }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function fails() -> string {
        load_const false
        assert
        load_const "ok"
        return
    }

    function main() -> string {
        push_unwind L0, slot 1
        call fails
        pop_unwind
        jump L1

      L0:
        load_var _1
        store_var e
        load_const "recovered"

      L1:
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
        "
        function throws_now() -> int {
            throw 7;
            0
        }

        function main() -> int {
            throws_now() catch (e) {
                _ => 99
            }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        push_unwind L0, slot 1
        call throws_now
        pop_unwind
        jump L1

      L0:
        load_var _1
        store_var e
        load_const 99

      L1:
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
async fn panic_only_catch_does_not_swallow_non_panic_error() {
    let output = baml_test!(
        "
        function divide_by_zero() -> string {
            let _x = 1 / 0;
            \"ok\"
        }

        function main() -> string {
            divide_by_zero() catch (e) {
                \"panic: assertion failed\" => \"panic\"
            } catch (e2) {
                _ => \"non-panic\"
            }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function divide_by_zero() -> string {
        load_const 1
        load_const 0
        bin_op /
        store_var _x
        load_const "ok"
        return
    }

    function main() -> string {
        push_unwind L0, slot 1
        call divide_by_zero
        pop_unwind
        jump L3

      L0:
        load_var _1
        store_var e
        load_var _1
        load_const "panic: assertion failed"
        cmp_op ==
        pop_jump_if_false L1
        jump L2

      L1:
        load_var _1
        store_var _2
        load_var _2
        store_var e2
        load_const "non-panic"
        jump L3

      L2:
        load_const "panic"

      L3:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("non-panic".to_string()))
    );
}

#[tokio::test]
async fn panic_only_catch_handles_panic_error() {
    let output = baml_test!(
        "
        function panics_now() -> string {
            assert false;
            \"ok\"
        }

        function main() -> string {
            panics_now() catch (e) {
                \"panic: assertion failed\" => \"panic\"
            } catch (e2) {
                _ => \"non-panic\"
            }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        push_unwind L0, slot 1
        call panics_now
        pop_unwind
        jump L3

      L0:
        load_var _1
        store_var e
        load_var _1
        load_const "panic: assertion failed"
        cmp_op ==
        pop_jump_if_false L1
        jump L2

      L1:
        load_var _1
        store_var _2
        load_var _2
        store_var e2
        load_const "non-panic"
        jump L3

      L2:
        load_const "panic"

      L3:
        return
    }

    function panics_now() -> string {
        load_const false
        assert
        load_const "ok"
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("panic".to_string()))
    );
}

#[tokio::test]
async fn typed_catch_arm_matches_primitive_throw_value() {
    let output = baml_test!(
        "
        function throws_now() -> string {
            throw \"boom\";
            \"ok\"
        }

        function main() -> string {
            throws_now() catch (e) {
                string => \"typed catch\",
                _ => \"fallback\"
            }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        push_unwind L0, slot 1
        call throws_now
        pop_unwind
        jump L3

      L0:
        load_var _1
        store_var e
        load_var _1
        type_tag
        load_const 1
        cmp_op ==
        pop_jump_if_false L1
        jump L2

      L1:
        load_const "fallback"
        jump L3

      L2:
        load_const "typed catch"

      L3:
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
        load_const 1
        store_var _2
        load_var _2
        store_var e
        load_const 2
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn match_arm_block_with_throw_is_not_typed_as_void() {
    let output = baml_test!(
        "
        function main() -> string {
            let a = 1;
            return match (a) {
                1 => \"1\",
                int => {
                    throw 1
                },
            };
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const 1
        load_const 1
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
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
async fn throw_catch_inside_match_arm_returns_catch_value() {
    let output = baml_test!(
        "
        function main() -> string {
            return match (2) {
                1 => \"1\",
                int => throw 1 catch (e) {
                    _ => \"..\"
                },
            };
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const 2
        load_const 1
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 1
        store_var _5
        load_var _5
        store_var e
        load_const ".."
        jump L2

      L1:
        load_const "1"

      L2:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("..".to_string()))
    );
}

#[tokio::test]
async fn throw_followed_by_dead_code_still_diverges_in_match_arm() {
    let output = baml_test!(
        "
        function main() -> string {
            let a = 1;
            return match (a) {
                1 => \"one\",
                int => {
                    throw \"error\";
                    let dead = 2;
                },
            };
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const 1
        load_const 1
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
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
async fn return_followed_by_dead_code_still_diverges_in_block() {
    let output = baml_test!(
        "
        function main() -> string {
            return \"hello\";
            let x = 1;
        }
    "
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
async fn if_else_both_throw_followed_by_dead_code_diverges() {
    let output = baml_test!(
        "
        function main() -> string {
            let a = 1;
            return match (a) {
                1 => \"one\",
                int => {
                    if (true) {
                        throw \"a\"
                    } else {
                        throw \"b\"
                    };
                    let dead = 0;
                },
            };
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const 1
        load_const 1
        cmp_op ==
        pop_jump_if_false L0
        jump L3

      L0:
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
        "
        function main() -> int {
            throw 42;
            0
        }
    "
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
        "
        function main() -> string {
            throw \"something went wrong\";
            \"\"
        }
    "
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
async fn unhandled_throw_string_in_match_shows_value() {
    let output = baml_test!(
        "
        function main() -> string {
            let a = 1;
            match (a) {
                int => {
                    throw \"string\"
                }
            }
            return \"...\"
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "string"
        throw
    }
    "#);

    assert_eq!(
        output.result,
        Err(bex_engine::EngineError::VmError(
            bex_vm::errors::VmError::RuntimeError(bex_vm::errors::RuntimeError::UnhandledThrow {
                value: "string".to_string(),
            })
        ))
    );
}

#[tokio::test]
async fn throw_with_multiple_dead_stmts_still_diverges() {
    let output = baml_test!(
        "
        function main() -> string {
            let a = 2;
            return match (a) {
                1 => \"one\",
                int => {
                    throw \"boom\";
                    let x = 1;
                    let y = 2;
                },
            };
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const 2
        load_const 1
        cmp_op ==
        pop_jump_if_false L0
        jump L1

      L0:
        load_const "boom"
        throw

      L1:
        load_const "one"
        return
    }
    "#);

    assert_eq!(
        output.result,
        Err(bex_engine::EngineError::VmError(
            bex_vm::errors::VmError::RuntimeError(bex_vm::errors::RuntimeError::UnhandledThrow {
                value: "boom".to_string(),
            })
        ))
    );
}
