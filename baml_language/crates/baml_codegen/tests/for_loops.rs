//! Compiler tests for for-in loops.

use baml_tests::{
    codegen::{Program, assert_compiles},
    vm::{Instruction, Value},
};
use baml_vm::{BinOp, CmpOp};

// ============================================================================
// For-in loops
// ============================================================================

#[test]
fn for_loop_sum() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function Sum(xs: int[]) -> int {
                let result = 0;

                for (let x in xs) {
                    result += x;
                }

                result
            }
            "#,
        expected: vec![(
            "Sum",
            vec![
                Instruction::LoadConst(Value::Int(0)),
                Instruction::LoadVar("xs".to_string()),
                Instruction::LoadGlobal(Value::function("baml.Array.length")),
                Instruction::LoadVar("_iter".to_string()),
                Instruction::Call(1),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::LoadVar("_i".to_string()),
                Instruction::LoadVar("_len".to_string()),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::JumpIfFalse(15),
                Instruction::Pop(1),
                Instruction::LoadVar("_iter".to_string()),
                Instruction::LoadVar("_i".to_string()),
                Instruction::LoadArrayElement,
                Instruction::LoadVar("_i".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("_i".to_string()),
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadVar("x".to_string()),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("result".to_string()),
                Instruction::Pop(1),
                Instruction::Jump(-17),
                Instruction::Pop(1),
                Instruction::Pop(3),
                Instruction::LoadVar("result".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn for_with_break() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function ForWithBreak(xs: int[]) -> int {
                let result = 0;

                for (let x in xs) {
                    if (x > 10) {
                        break;
                    }
                    result += x;
                }

                result
            }
            "#,
        expected: vec![(
            "ForWithBreak",
            vec![
                Instruction::LoadConst(Value::Int(0)),
                Instruction::LoadVar("xs".to_string()),
                Instruction::LoadGlobal(Value::function("baml.Array.length")),
                Instruction::LoadVar("_iter".to_string()),
                Instruction::Call(1),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::LoadVar("_i".to_string()),
                Instruction::LoadVar("_len".to_string()),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::JumpIfFalse(24),
                Instruction::Pop(1),
                Instruction::LoadVar("_iter".to_string()),
                Instruction::LoadVar("_i".to_string()),
                Instruction::LoadArrayElement,
                Instruction::LoadVar("_i".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("_i".to_string()),
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(10)),
                Instruction::CmpOp(CmpOp::Gt),
                Instruction::JumpIfFalse(5),
                Instruction::Pop(1),
                Instruction::Pop(1),
                Instruction::Jump(10),
                Instruction::Jump(2),
                Instruction::Pop(1),
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadVar("x".to_string()),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("result".to_string()),
                Instruction::Pop(1),
                Instruction::Jump(-26),
                Instruction::Pop(1),
                Instruction::Pop(3),
                Instruction::LoadVar("result".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn for_with_continue() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function ForWithContinue(xs: int[]) -> int {
                let result = 0;

                for (let x in xs) {
                    if (x > 10) {
                        continue;
                    }
                    result += x;
                }

                result
            }
            "#,
        expected: vec![(
            "ForWithContinue",
            vec![
                Instruction::LoadConst(Value::Int(0)),
                Instruction::LoadVar("xs".to_string()),
                Instruction::LoadGlobal(Value::function("baml.Array.length")),
                Instruction::LoadVar("_iter".to_string()),
                Instruction::Call(1),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::LoadVar("_i".to_string()),
                Instruction::LoadVar("_len".to_string()),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::JumpIfFalse(24),
                Instruction::Pop(1),
                Instruction::LoadVar("_iter".to_string()),
                Instruction::LoadVar("_i".to_string()),
                Instruction::LoadArrayElement,
                Instruction::LoadVar("_i".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("_i".to_string()),
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(10)),
                Instruction::CmpOp(CmpOp::Gt),
                Instruction::JumpIfFalse(5),
                Instruction::Pop(1),
                Instruction::Pop(1),
                Instruction::Jump(8),
                Instruction::Jump(2),
                Instruction::Pop(1),
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadVar("x".to_string()),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("result".to_string()),
                Instruction::Pop(1),
                Instruction::Jump(-26),
                Instruction::Pop(1),
                Instruction::Pop(3),
                Instruction::LoadVar("result".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn for_nested() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function NestedFor(as: int[], bs: int[]) -> int {

                let result = 0;

                for (let a in as) {
                    for (let b in bs) {
                        result += a * b;
                    }
                }

                result
            }
            "#,
        expected: vec![(
            "NestedFor",
            vec![
                Instruction::LoadConst(Value::Int(0)),
                Instruction::LoadVar("as".to_string()),
                Instruction::LoadGlobal(Value::function("baml.Array.length")),
                Instruction::LoadVar("_iter".to_string()),
                Instruction::Call(1),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::LoadVar("_i".to_string()),
                Instruction::LoadVar("_len".to_string()),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::JumpIfFalse(38),
                Instruction::Pop(1),
                Instruction::LoadVar("_iter".to_string()),
                Instruction::LoadVar("_i".to_string()),
                Instruction::LoadArrayElement,
                Instruction::LoadVar("_i".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("_i".to_string()),
                Instruction::LoadVar("bs".to_string()),
                Instruction::LoadGlobal(Value::function("baml.Array.length")),
                Instruction::LoadVar("_iter1".to_string()),
                Instruction::Call(1),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::LoadVar("_i1".to_string()),
                Instruction::LoadVar("_len1".to_string()),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::JumpIfFalse(17),
                Instruction::Pop(1),
                Instruction::LoadVar("_iter1".to_string()),
                Instruction::LoadVar("_i1".to_string()),
                Instruction::LoadArrayElement,
                Instruction::LoadVar("_i1".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("_i1".to_string()),
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadVar("b".to_string()),
                Instruction::BinOp(BinOp::Mul),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("result".to_string()),
                Instruction::Pop(1),
                Instruction::Jump(-19),
                Instruction::Pop(1),
                Instruction::Pop(3),
                Instruction::Pop(1),
                Instruction::Jump(-40),
                Instruction::Pop(1),
                Instruction::Pop(3),
                Instruction::LoadVar("result".to_string()),
                Instruction::Return,
            ],
        )],
    })
}
