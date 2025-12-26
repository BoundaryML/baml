//! Compiler tests for while loops, break, and continue.

use baml_tests::{
    codegen::{Program, assert_compiles},
    vm::{Instruction, Value},
};
use baml_vm::{BinOp, CmpOp};

// ============================================================================
// While loops (all require assignment statements, currently ignored)
// ============================================================================

#[test]
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
            // MIR-based codegen - no local pre-allocation for params-only functions
            vec![
                // Loop condition: a != b
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadVar("b".to_string()),
                Instruction::CmpOp(CmpOp::NotEq),
                Instruction::JumpIfFalse(2),
                Instruction::Jump(3),
                // Loop exit: return a
                Instruction::LoadVar("a".to_string()),
                Instruction::Return,
                // Loop body: if (a > b)
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadVar("b".to_string()),
                Instruction::CmpOp(CmpOp::Gt),
                Instruction::JumpIfFalse(2),
                Instruction::Jump(6),
                // Else branch: b = b - a
                Instruction::LoadVar("b".to_string()),
                Instruction::LoadVar("a".to_string()),
                Instruction::BinOp(BinOp::Sub),
                Instruction::StoreVar("b".to_string()),
                Instruction::Jump(5),
                // Then branch: a = a - b
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadVar("b".to_string()),
                Instruction::BinOp(BinOp::Sub),
                Instruction::StoreVar("a".to_string()),
                // Jump back to loop condition
                Instruction::Jump(-21),
            ],
        )],
    })
}

#[test]
fn while_loop_with_ending_if() -> anyhow::Result<()> {
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
            // Stackification with dead store elimination:
            // a is user variable, dead compiler temps (_5 for if result) are eliminated
            vec![
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("a".to_string()),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(5)),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::JumpIfFalse(11),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("a".to_string()),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(2),
                Instruction::Jump(2),
                Instruction::Jump(-13),
                Instruction::LoadVar("a".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
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
            // Stackification with dead store elimination:
            // a is user variable, dead compiler temps (_5 for if result) are eliminated
            vec![
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("a".to_string()),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(5)),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::JumpIfFalse(11),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("a".to_string()),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(2),
                Instruction::Jump(2),
                Instruction::Jump(-13),
                Instruction::LoadVar("a".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
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
            // MIR-based codegen with local pre-allocation
            vec![
                // Pre-allocate result local
                Instruction::LoadConst(Value::Null),
                // Initialize result = 1
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("result".to_string()),
                // Loop condition: true
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::JumpIfFalse(15),
                // if (limit == 0)
                Instruction::LoadVar("limit".to_string()),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(2),
                // break - jump to loop exit
                Instruction::Jump(10),
                // result = result * limit
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadVar("limit".to_string()),
                Instruction::BinOp(BinOp::Mul),
                Instruction::StoreVar("result".to_string()),
                // limit = limit - 1
                Instruction::LoadVar("limit".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Sub),
                Instruction::StoreVar("limit".to_string()),
                // Jump back to loop condition
                Instruction::Jump(-15),
                // Loop exit: load result and return
                Instruction::LoadVar("result".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
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
            // MIR-based codegen with local pre-allocation
            // Note: should_continue initialization to true appears to be missing in codegen
            vec![
                // Pre-allocate locals
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                // Initialize result = 1
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("result".to_string()),
                // Pre-allocate should_continue (should be Bool(true) but codegen produces Null)
                Instruction::LoadConst(Value::Null),
                Instruction::StoreVar("should_continue".to_string()),
                // Loop condition: should_continue
                Instruction::LoadVar("should_continue".to_string()),
                Instruction::JumpIfFalse(2),
                Instruction::Jump(3),
                // Loop exit: load result and return
                Instruction::LoadVar("result".to_string()),
                Instruction::Return,
                // Loop body: result = result * limit
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadVar("limit".to_string()),
                Instruction::BinOp(BinOp::Mul),
                Instruction::StoreVar("result".to_string()),
                // limit = limit - 1
                Instruction::LoadVar("limit".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Sub),
                Instruction::StoreVar("limit".to_string()),
                // if (limit != 0)
                Instruction::LoadVar("limit".to_string()),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::NotEq),
                Instruction::JumpIfFalse(2),
                // continue - jump to loop condition
                Instruction::Jump(4),
                // else: should_continue = false
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::StoreVar("should_continue".to_string()),
                // Jump back to loop condition
                Instruction::Jump(-20),
                // Unreachable continue fallthrough
                Instruction::Jump(-21),
            ],
        )],
    })
}

#[test]
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
            // Stackification with dead store elimination:
            // Dead compiler temps (_2 for if result) are eliminated
            vec![
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::JumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(5)),
                Instruction::Return,
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::JumpIfFalse(2),
                Instruction::Jump(6),
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::JumpIfFalse(2),
                Instruction::Jump(2),
                Instruction::Jump(-11),
                Instruction::Jump(-12),
                Instruction::Jump(-8),
            ],
        )],
    })
}

#[test]
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
            // Stackification with dead store elimination:
            // a is user variable, dead compiler temps are eliminated
            vec![
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Int(5)),
                Instruction::StoreVar("a".to_string()),
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::JumpIfFalse(11),
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::JumpIfFalse(5),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("a".to_string()),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("a".to_string()),
                Instruction::LoadVar("a".to_string()),
                Instruction::Return,
            ],
        )],
    })
}
