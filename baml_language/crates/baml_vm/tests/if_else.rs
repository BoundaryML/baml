//! VM tests for if/else and block expressions.

use baml_tests::bytecode::{ExecState, Program, Value, assert_vm_executes};

#[test]
#[ignore = "if/else codegen issue"]
fn exec_if_branch() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
            function run_if(b: bool) -> int {
                if (b) { 1 } else { 2 }
            }

            function main() -> int {
                let a = run_if(true);
                a
            }
        ",
        function: "main",
        expected: ExecState::Complete(Value::Int(1)),
    })
}

#[test]
#[ignore = "if/else codegen issue"]
fn exec_else_branch() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
            function run_if(b: bool) -> int {
                if (b) { 1 } else { 2 }
            }

            function main() -> int {
                let a = run_if(false);
                a
            }
        ",
        function: "main",
        expected: ExecState::Complete(Value::Int(2)),
    })
}

#[test]
#[ignore = "if/else codegen issue"]
fn exec_else_if_branch() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
            function run_if(a: bool, b: bool) -> int {
                if (a) {
                    1
                } else if (b) {
                    2
                } else {
                    3
                }
            }

            function main() -> int {
                let a = run_if(false, true);
                a
            }
        ",
        function: "main",
        expected: ExecState::Complete(Value::Int(2)),
    })
}
