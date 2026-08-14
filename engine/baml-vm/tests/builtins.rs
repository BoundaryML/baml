//! VM tests for built-in methods and operations.

mod common;
use common::{assert_vm_executes, ExecState, Program, Value};

#[test]
fn builtin_method_call() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let arr = [1, 2, 3];
                arr.length()
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(3)),
    })
}

#[test]
fn bind_method_call() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let arr = [1, 2, 3];
                let v = arr.length();

                v
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(3)),
    })
}
