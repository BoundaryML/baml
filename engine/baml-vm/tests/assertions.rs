//! VM tests for assert statements.

use baml_vm::{RuntimeError, Value, VmExecState};

mod common;
use common::{assert_vm_executes, assert_vm_fails, FailingProgram, Program};

#[test]
fn assert_ok() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn assertOk() -> int {

                assert 2 + 2 == 4;

                3
            }"#,
        function: "assertOk",
        expected: VmExecState::Complete(Value::Int(3)),
    })
}

#[test]
fn assert_not_ok() -> anyhow::Result<()> {
    assert_vm_fails(FailingProgram {
        source: r#"
            fn assertNotOk() -> int {
                assert 3 == 1;

                2
            }"#,
        function: "assertNotOk",
        expected: RuntimeError::AssertionError.into(),
    })
}
