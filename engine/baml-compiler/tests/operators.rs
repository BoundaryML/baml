//! Compiler tests for operators (arithmetic, logical, assignment).

use baml_vm::{BinOp, GlobalIndex, Instruction};

mod common;
use common::{assert_compiles, Program};

#[test]
fn basic_and() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function ret_bool() -> bool {
                true
            }

            function main() -> bool {
                true && ret_bool()
            }
        "#,
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(0),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadGlobal(GlobalIndex::from_raw(0)),
                Instruction::Call(0),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn basic_or() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function ret_bool() -> bool {
                true
            }

            function main() -> bool {
                true || ret_bool()
            }
        "#,
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(0),
                Instruction::JumpIfFalse(2),
                Instruction::Jump(4),
                Instruction::Pop(1),
                Instruction::LoadGlobal(GlobalIndex::from_raw(0)),
                Instruction::Call(0),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn basic_add() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function main() -> int {
                let a = 1 + 2;
                a
            }
        "#,
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(0),
                Instruction::LoadConst(1),
                Instruction::BinOp(BinOp::Add),
                Instruction::LoadVar(1),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn basic_assign_add() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function main() -> int {
                let x = 1;
                x += 2;
                x
            }
        "#,
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(0),
                Instruction::LoadVar(1),
                Instruction::LoadConst(1),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar(1),
                Instruction::LoadVar(1),
                Instruction::Return,
            ],
        )],
    })
}
