//! Compiler tests for assert statements.

use baml_vm::{
    test::{Instruction, Value},
    BinOp, CmpOp,
};

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
                Instruction::LoadConst(Value::Int(2)), // 2
                Instruction::LoadConst(Value::Int(2)), // 2
                Instruction::BinOp(BinOp::Add),
                Instruction::LoadConst(Value::Int(4)), // 4
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::Assert,
                Instruction::LoadConst(Value::Int(3)), // 3
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
                Instruction::LoadConst(Value::Int(3)), // 3
                Instruction::LoadConst(Value::Int(1)), // 1
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::Assert,
                Instruction::LoadConst(Value::Int(2)), // 2
                Instruction::Return,
            ],
        )],
    })
}
