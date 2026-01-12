//! Compiler tests for operators (arithmetic, logical, assignment).

use baml_tests::{
    codegen::{Program, assert_compiles},
    vm::{Instruction, Value},
};
use baml_vm_types::bytecode::BinOp;

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
            // ReturnPhi + Phi-like optimizations: result stays on stack
            vec![
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::Jump(3),
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
            // ReturnPhi + Phi-like optimizations: result stays on stack
            vec![
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(4),
                Instruction::LoadGlobal(Value::function("ret_bool")),
                Instruction::Call(0),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Bool(true)),
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
            // x is Real (used 3 times: init, compound assign read, return)
            // Compound assignment x += 2 expands to x = x + 2
            vec![
                Instruction::LoadConst(Value::Null), // slot for x
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("x".to_string()), // let x = 1
                Instruction::LoadVar("x".to_string()),  // read x
                Instruction::LoadConst(Value::Int(2)),
                Instruction::BinOp(BinOp::Add),         // x + 2
                Instruction::StoreVar("x".to_string()), // x = (x + 2)
                Instruction::LoadVar("x".to_string()),  // return x
                Instruction::Return,
            ],
        )],
    })
}
