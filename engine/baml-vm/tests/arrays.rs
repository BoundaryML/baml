//! VM tests for arrays.

mod common;
use common::{assert_vm_executes, ExecState, Object, Program, Value};

// Array tests
#[test]
fn array_constructor() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
            function main() -> int[] {
                let a = [1, 2, 3];
                a
            }
        ",
        function: "main",
        expected: ExecState::Complete(Value::Object(Object::Array(vec![
            Value::Int(1),
            Value::Int(2),
            Value::Int(3),
        ]))),
    })
}

#[test]
fn array_push() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r"
            function main() -> int[] {
                let a = [1, 2, 3];
                a.push(4);
                a
            }
        ",
        function: "main",
        expected: ExecState::Complete(Value::Object(Object::Array(vec![
            Value::Int(1),
            Value::Int(2),
            Value::Int(3),
            Value::Int(4),
        ]))),
    })
}
