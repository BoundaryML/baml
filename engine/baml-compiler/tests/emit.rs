//! Compiler tests for emit functionality.

use baml_vm::Instruction;

mod common;
use common::{assert_compiles, Program};

#[test]
fn emit_primitive() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function primitive() -> int {
                let value = 0 @emit;

                value = 1;

                value
            }
        ",
        expected: vec![(
            "primitive",
            vec![
                Instruction::LoadConst(0),
                Instruction::TrackEmittable,
                Instruction::LoadConst(1),
                Instruction::StoreVar(1),
                Instruction::LoadVar(1),
                Instruction::Return,
            ],
        )],
    })
}
