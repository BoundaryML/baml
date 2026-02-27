//! VM tests for catch/throw exception semantics.

use baml_tests::bytecode::{
    ExecState, FailingProgram, Program, Value, assert_vm_executes, assert_vm_fails,
};
use bex_vm::RuntimeError;

#[test]
fn handled_runtime_error_continues_execution() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function fails() -> string {
                assert false;
                "ok"
            }

            function main() -> string {
                fails() catch (e) {
                    _ => "recovered"
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("recovered")),
    })
}

#[test]
fn handled_throw_from_callee_returns_fallback_value() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function throws_now() -> int {
                throw 7;
                0
            }

            function main() -> int {
                throws_now() catch (e) {
                    _ => 99
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(99)),
    })
}

#[test]
fn unhandled_throw_fails_predictably() -> anyhow::Result<()> {
    assert_vm_fails(FailingProgram {
        source: r#"
            function main() -> int {
                throw 42;
                0
            }
        "#,
        function: "main",
        expected: RuntimeError::UnhandledThrow {
            value: "42".to_string(),
        }
        .into(),
    })
}

#[test]
fn unhandled_throw_string_shows_value() -> anyhow::Result<()> {
    assert_vm_fails(FailingProgram {
        source: r#"
            function main() -> string {
                throw "something went wrong";
                ""
            }
        "#,
        function: "main",
        expected: RuntimeError::UnhandledThrow {
            value: "something went wrong".to_string(),
        }
        .into(),
    })
}

#[test]
fn unhandled_throw_string_in_match_shows_value() -> anyhow::Result<()> {
    assert_vm_fails(FailingProgram {
        source: r#"
            function main() -> string {
                let a = 1;
                match (a) {
                    int => {
                        throw "string"
                    }
                }
                return "..."
            }
        "#,
        function: "main",
        expected: RuntimeError::UnhandledThrow {
            value: "string".to_string(),
        }
        .into(),
    })
}

#[test]
fn panic_only_catch_does_not_swallow_non_panic_error() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function divide_by_zero() -> string {
                let _x = 1 / 0;
                "ok"
            }

            function main() -> string {
                divide_by_zero() catch (e) {
                    "panic: assertion failed" => "panic"
                } catch (e2) {
                    _ => "non-panic"
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("non-panic")),
    })
}

#[test]
fn panic_only_catch_handles_panic_error() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function panics_now() -> string {
                assert false;
                "ok"
            }

            function main() -> string {
                panics_now() catch (e) {
                    "panic: assertion failed" => "panic"
                } catch (e2) {
                    _ => "non-panic"
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("panic")),
    })
}

#[test]
fn typed_catch_arm_matches_primitive_throw_value() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function throws_now() -> string {
                throw "boom";
                "ok"
            }

            function main() -> string {
                throws_now() catch (e) {
                    string => "typed catch",
                    _ => "fallback"
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("typed catch")),
    })
}

#[test]
fn catch_binds_to_throw_expression_not_throw_payload() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                return throw 1 catch (e) {
                    _ => 2
                };
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(2)),
    })
}

#[test]
fn match_arm_block_with_throw_is_not_typed_as_void() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                let a = 1;
                return match (a) {
                    1 => "1",
                    int => {
                        throw 1
                    },
                };
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("1")),
    })
}

#[test]
fn throw_catch_inside_match_arm_returns_catch_value() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                return match (2) {
                    1 => "1",
                    int => throw 1 catch (e) {
                        _ => ".."
                    },
                };
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("..")),
    })
}

#[test]
fn throw_followed_by_dead_code_still_diverges_in_match_arm() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                let a = 1;
                return match (a) {
                    1 => "one",
                    int => {
                        throw "error";
                        let dead = 2;
                    },
                };
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("one")),
    })
}

#[test]
fn return_followed_by_dead_code_still_diverges_in_block() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                return "hello";
                let x = 1;
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("hello")),
    })
}

#[test]
fn throw_with_multiple_dead_stmts_still_diverges() -> anyhow::Result<()> {
    assert_vm_fails(FailingProgram {
        source: r#"
            function main() -> string {
                let a = 2;
                return match (a) {
                    1 => "one",
                    int => {
                        throw "boom";
                        let x = 1;
                        let y = 2;
                    },
                };
            }
        "#,
        function: "main",
        expected: RuntimeError::UnhandledThrow {
            value: "boom".to_string(),
        }
        .into(),
    })
}

#[test]
fn if_else_both_throw_followed_by_dead_code_diverges() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                let a = 1;
                return match (a) {
                    1 => "one",
                    int => {
                        if (true) {
                            throw "a"
                        } else {
                            throw "b"
                        };
                        let dead = 0;
                    },
                };
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("one")),
    })
}
