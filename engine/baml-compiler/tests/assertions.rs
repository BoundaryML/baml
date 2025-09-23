//! Compiler tests for assert statements.

use baml_vm::{BinOp, CmpOp, Instruction};

mod common;
use common::{assert_compiles, Program};

#[test]
fn assert_statement_ok() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function assertOk() -> int {
                assert 2 + 2 == 4;
                3
            }
        ",
        expected: vec![(
            "assertOk",
            vec![
                Instruction::LoadConst(0), // 2
                Instruction::LoadConst(1), // 2
                Instruction::BinOp(BinOp::Add),
                Instruction::LoadConst(2), // 4
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::Assert,
                Instruction::LoadConst(3), // 3
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn assert_statement_not_ok() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function assertNotOk() -> int {
                assert 3 == 1;
                2
            }
        ",
        expected: vec![(
            "assertNotOk",
            vec![
                Instruction::LoadConst(0), // 3
                Instruction::LoadConst(1), // 1
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::Assert,
                Instruction::LoadConst(2), // 2
                Instruction::Return,
            ],
        )],
    })
}
