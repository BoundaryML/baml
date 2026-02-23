//! VM tests for catch/throw exception semantics.

use baml_tests::bytecode::{
    ExecState, FailingProgram, Program, Value, assert_vm_executes, assert_vm_fails,
};
use bex_vm::RuntimeError;
use bex_vm_types::Value as VmValue;

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
            value: VmValue::Int(42),
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
