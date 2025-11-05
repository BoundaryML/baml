//! Compiler tests for operators (arithmetic, logical, assignment).

use baml_vm::{
    test::{Instruction, Value},
    BinOp,
};

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
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadGlobal(Value::function("ret_bool")),
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
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::JumpIfFalse(2),
                Instruction::Jump(4),
                Instruction::Pop(1),
                Instruction::LoadGlobal(Value::function("ret_bool")),
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
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::BinOp(BinOp::Add),
                Instruction::LoadVar("a".to_string()),
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
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("x".to_string()),
                Instruction::LoadVar("x".to_string()),
                Instruction::Return,
            ],
        )],
    })
}
