//! Compiler tests for array construction.

use baml_vm::test::{Instruction, Value};

mod common;
use common::{assert_compiles, Program};

#[test]
fn array_constructor() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int[] {
                let a = [1, 2, 3];
                a
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::AllocArray(3),
                Instruction::LoadVar("a".to_string()),
                Instruction::Return,
            ],
        )],
    })
}
