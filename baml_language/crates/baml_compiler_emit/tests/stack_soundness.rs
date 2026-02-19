//! Stack-discipline soundness regression tests.

use baml_tests::bytecode::{ExecState, Program, Value, assert_vm_executes};

#[test]
fn call_result_immediate_right_operand_subtraction() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function id(x: int) -> int { x }

            function main() -> int {
                1 - id(2)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(-1)),
    })
}

#[test]
fn phi_like_right_operand_subtraction() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                100 - if (2 > 1) { 7 } else { 3 }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(93)),
    })
}

#[test]
fn cross_block_virtual_misses_statement0_side_effect() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Box {
                v int
            }

            function main() -> int {
                let b = Box { v: 1 };
                let t = b.v;
                if (1 == 1) {
                }
                b.v = 2;
                t
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(1)),
    })
}
