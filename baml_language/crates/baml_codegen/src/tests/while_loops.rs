//! Compiler tests for while loops, break, and continue.

use baml_vm::{
    BinOp, CmpOp,
    test::{Instruction, Value},
};

use super::common::{Program, assert_compiles};

// ============================================================================
// While loops (all require assignment statements, currently ignored)
// ============================================================================

#[test]
#[ignore = "assignment statements not yet in HIR"]
fn while_loop_gcd() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function GCD(a: int, b: int) -> int {
                while (a != b) {
                    if (a > b) {
                        a = a - b;
                    } else {
                        b = b - a;
                    }
                }

                a
            }
        "#,
        expected: vec![(
            "GCD",
            vec![
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadVar("b".to_string()),
                Instruction::CmpOp(CmpOp::NotEq),
                Instruction::JumpIfFalse(18),
                Instruction::Pop(1),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadVar("b".to_string()),
                Instruction::CmpOp(CmpOp::Gt),
                Instruction::JumpIfFalse(7),
                Instruction::Pop(1),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadVar("b".to_string()),
                Instruction::BinOp(BinOp::Sub),
                Instruction::StoreVar("a".to_string()),
                Instruction::Jump(6),
                Instruction::Pop(1),
                Instruction::LoadVar("b".to_string()),
                Instruction::LoadVar("a".to_string()),
                Instruction::BinOp(BinOp::Sub),
                Instruction::StoreVar("b".to_string()),
                Instruction::Jump(-20),
                Instruction::Pop(1),
                Instruction::LoadVar("a".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
#[ignore = "break statement and assignment not yet in HIR"]
fn while_loop_with_break() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                let a = 1;

                while (a < 5) {
                    a += 1;

                    if (a == 2) {
                        break;
                    }
                }

                a
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(5)),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::JumpIfFalse(15),
                Instruction::Pop(1),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("a".to_string()),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::Jump(5),
                Instruction::Jump(2),
                Instruction::Pop(1),
                Instruction::Jump(-17),
                Instruction::Pop(1),
                Instruction::LoadVar("a".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
#[ignore = "break statement and assignment not yet in HIR"]
fn break_factorial() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function Factorial(limit: int) -> int {
                let result = 1;

                while (true) {
                    if (limit == 0) {
                        break;
                    }
                    result = result * limit;
                    limit = limit - 1;
                }

                result
            }
        "#,
        expected: vec![(
            "Factorial",
            vec![
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::JumpIfFalse(19),
                Instruction::Pop(1),
                Instruction::LoadVar("limit".to_string()),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::Jump(13),
                Instruction::Jump(2),
                Instruction::Pop(1),
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadVar("limit".to_string()),
                Instruction::BinOp(BinOp::Mul),
                Instruction::StoreVar("result".to_string()),
                Instruction::LoadVar("limit".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Sub),
                Instruction::StoreVar("limit".to_string()),
                Instruction::Jump(-19),
                Instruction::Pop(1),
                Instruction::LoadVar("result".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
#[ignore = "continue statement and assignment not yet in HIR"]
fn continue_factorial() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function Factorial(limit: int) -> int {
                let result = 1;

                let should_continue = true;
                while (should_continue) {
                    result = result * limit;
                    limit = limit - 1;

                    if (limit != 0) {
                        continue;
                    } else {
                        should_continue = false;
                    }
                }

                result
            }
        "#,
        expected: vec![(
            "Factorial",
            vec![
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::LoadVar("should_continue".to_string()),
                Instruction::JumpIfFalse(21),
                Instruction::Pop(1),
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadVar("limit".to_string()),
                Instruction::BinOp(BinOp::Mul),
                Instruction::StoreVar("result".to_string()),
                Instruction::LoadVar("limit".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Sub),
                Instruction::StoreVar("limit".to_string()),
                Instruction::LoadVar("limit".to_string()),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::NotEq),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::Jump(5),
                Instruction::Jump(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::StoreVar("should_continue".to_string()),
                Instruction::Jump(-21),
                Instruction::Pop(1),
                Instruction::LoadVar("result".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
#[ignore = "continue statement not yet in HIR"]
fn continue_nested() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function Nested() -> int {
                while (true) {
                    while (false) {
                        continue;
                    }
                    if (false) {
                        continue;
                    }
                }
                5
            }
        "#,
        expected: vec![(
            "Nested",
            vec![
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::JumpIfFalse(15),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::Jump(1),
                Instruction::Jump(-4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::Jump(3),
                Instruction::Jump(2),
                Instruction::Pop(1),
                Instruction::Jump(-15),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(5)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
#[ignore = "break statement and assignment not yet in HIR"]
fn break_nested() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function Nested() -> int {
                let a = 5;
                while (true) {
                    while (true) {
                        a = a + 1;
                        break;
                    }
                    a = a + 1;
                    break;
                }
                a
            }
        "#,
        expected: vec![(
            "Nested",
            vec![
                Instruction::LoadConst(Value::Int(5)),
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::JumpIfFalse(18),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::JumpIfFalse(8),
                Instruction::Pop(1),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("a".to_string()),
                Instruction::Jump(3),
                Instruction::Jump(-8),
                Instruction::Pop(1),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("a".to_string()),
                Instruction::Jump(3),
                Instruction::Jump(-18),
                Instruction::Pop(1),
                Instruction::LoadVar("a".to_string()),
                Instruction::Return,
            ],
        )],
    })
}
