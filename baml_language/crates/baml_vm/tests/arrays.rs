//! VM tests for arrays.

use baml_tests::bytecode::{ExecState, Object, Program, Value, assert_vm_executes};

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
#[ignore = "method calls on arrays not yet implemented"]
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
