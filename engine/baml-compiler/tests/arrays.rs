//! Compiler tests for array construction.

use baml_vm::Instruction;

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
                Instruction::LoadConst(0),
                Instruction::LoadConst(1),
                Instruction::LoadConst(2),
                Instruction::AllocArray(3),
                Instruction::LoadVar(1),
                Instruction::Return,
            ],
        )],
    })
}
