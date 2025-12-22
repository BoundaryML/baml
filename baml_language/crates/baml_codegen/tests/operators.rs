//! Compiler tests for operators (arithmetic, logical, assignment).

use baml_tests::{
    codegen::{Program, assert_compiles},
    vm::{Instruction, Value},
};
use baml_vm::BinOp;

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
            // THIR codegen (efficient):
            // vec![
            //     Instruction::LoadConst(Value::Bool(true)),
            //     Instruction::JumpIfFalse(4),
            //     Instruction::Pop(1),
            //     Instruction::LoadGlobal(Value::function("ret_bool")),
            //     Instruction::Call(0),
            //     Instruction::Return,
            // ],
            // Stackification with fall-through elimination:
            vec![
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::JumpIfFalse(2),
                Instruction::Jump(4),
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::StoreVar("_0".to_string()),
                Instruction::Jump(4),
                Instruction::LoadGlobal(Value::function("ret_bool")),
                Instruction::Call(0),
                Instruction::StoreVar("_0".to_string()),
                Instruction::LoadVar("_0".to_string()),
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
            // THIR codegen (efficient):
            // vec![
            //     Instruction::LoadConst(Value::Bool(true)),
            //     Instruction::JumpIfFalse(2),
            //     Instruction::Jump(4),
            //     Instruction::Pop(1),
            //     Instruction::LoadGlobal(Value::function("ret_bool")),
            //     Instruction::Call(0),
            //     Instruction::Return,
            // ],
            // Stackification with fall-through elimination:
            vec![
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::JumpIfFalse(2),
                Instruction::Jump(5),
                Instruction::LoadGlobal(Value::function("ret_bool")),
                Instruction::Call(0),
                Instruction::StoreVar("_0".to_string()),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::StoreVar("_0".to_string()),
                Instruction::LoadVar("_0".to_string()),
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
            // 'a' is Virtual (single-use), inlined:
            vec![
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::BinOp(BinOp::Add),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
#[ignore = "assignment statements not yet in HIR"]
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
