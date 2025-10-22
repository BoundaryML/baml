//! Compiler tests for watch functionality.

use baml_vm::Instruction;

mod common;
use common::{assert_compiles, Program};

#[test]
fn watch_primitive() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function primitive() -> int {
                watch let value = 0;

                value = 1;

                value
            }
        ",
        expected: vec![(
            "primitive",
            vec![
                Instruction::LoadConst(0),
                Instruction::LoadConst(1),
                Instruction::Watch,
                Instruction::LoadConst(2),
                Instruction::StoreVar(1),
                Instruction::LoadVar(1),
                Instruction::Return,
            ],
        )],
    })
}
