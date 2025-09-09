//! VM tests for built-in methods and operations.

use baml_vm::{Value, VmExecState};

mod common;
use common::{assert_vm_executes, Program};

#[test]
fn builtin_method_call() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                let arr = [1, 2, 3];
                arr.len()
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(3)),
    })
}

#[test]
fn bind_method_call() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                let arr = [1, 2, 3];
                let v = arr.len();

                v
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(3)),
    })
}
