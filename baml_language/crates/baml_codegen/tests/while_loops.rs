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
#[ignore = "function parameters not yet tracked in HIR"]
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
            // THIR codegen (efficient):
            // vec![
            //     Instruction::LoadConst(Value::Int(1)),
            //     Instruction::LoadVar("a".to_string()),
            //     Instruction::LoadConst(Value::Int(5)),
            //     Instruction::CmpOp(CmpOp::Lt),
            //     Instruction::JumpIfFalse(15),
            //     Instruction::Pop(1),
            //     Instruction::LoadVar("a".to_string()),
            //     Instruction::LoadConst(Value::Int(1)),
            //     Instruction::BinOp(BinOp::Add),
            //     Instruction::StoreVar("a".to_string()),
            //     Instruction::LoadVar("a".to_string()),
            //     Instruction::LoadConst(Value::Int(2)),
            //     Instruction::CmpOp(CmpOp::Eq),
            //     Instruction::JumpIfFalse(4),
            //     Instruction::Pop(1),
            //     Instruction::Jump(5), // break jumps past if-without-else and loop
            //     Instruction::Jump(2), // skip false-path pop
            //     Instruction::Pop(1),  // pop condition (false path)
            //     Instruction::Jump(-17),
            //     Instruction::Pop(1),
            //     Instruction::LoadVar("a".to_string()),
            //     Instruction::Return,
            // ],
            // MIR codegen (naive) - same semantics, more instructions:
            vec![
                // Pre-allocate locals with null
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                // Initialize a = 1
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("a".to_string()),
                // Return block jump
                Instruction::Jump(3),
                Instruction::LoadVar("_0".to_string()),
                Instruction::Return,
                // While loop condition: a < 5
                Instruction::LoadVar("a".to_string()),
                Instruction::StoreVar("_3".to_string()),
                Instruction::LoadConst(Value::Int(5)),
                Instruction::StoreVar("_4".to_string()),
                Instruction::LoadVar("_3".to_string()),
                Instruction::LoadVar("_4".to_string()),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::StoreVar("_2".to_string()),
                Instruction::LoadVar("_2".to_string()),
                Instruction::JumpIfFalse(23),
                Instruction::Jump(1),
                // a += 1
                Instruction::LoadVar("a".to_string()),
                Instruction::StoreVar("_6".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("_7".to_string()),
                Instruction::LoadVar("_6".to_string()),
                Instruction::LoadVar("_7".to_string()),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("_8".to_string()),
                Instruction::LoadVar("_8".to_string()),
                Instruction::StoreVar("a".to_string()),
                // if (a == 2) condition
                Instruction::LoadVar("a".to_string()),
                Instruction::StoreVar("_10".to_string()),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::StoreVar("_11".to_string()),
                Instruction::LoadVar("_10".to_string()),
                Instruction::LoadVar("_11".to_string()),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::StoreVar("_9".to_string()),
                Instruction::LoadVar("_9".to_string()),
                Instruction::JumpIfFalse(6),
                // break: return a
                Instruction::Jump(4),
                Instruction::LoadVar("a".to_string()),
                Instruction::StoreVar("_0".to_string()),
                Instruction::Jump(-36),
                // Skip else
                Instruction::Jump(-3),
                // After if (null for unit type)
                Instruction::LoadConst(Value::Null),
                Instruction::StoreVar("_5".to_string()),
                Instruction::Jump(1),
                // Loop back
                Instruction::Jump(-39),
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
            // THIR codegen (efficient):
            // vec![
            //     Instruction::LoadConst(Value::Int(1)),
            //     Instruction::LoadVar("a".to_string()),
            //     Instruction::LoadConst(Value::Int(5)),
            //     Instruction::CmpOp(CmpOp::Lt),
            //     Instruction::JumpIfFalse(15),
            //     Instruction::Pop(1),
            //     Instruction::LoadVar("a".to_string()),
            //     Instruction::LoadConst(Value::Int(1)),
            //     Instruction::BinOp(BinOp::Add),
            //     Instruction::StoreVar("a".to_string()),
            //     Instruction::LoadVar("a".to_string()),
            //     Instruction::LoadConst(Value::Int(2)),
            //     Instruction::CmpOp(CmpOp::Eq),
            //     Instruction::JumpIfFalse(4),
            //     Instruction::Pop(1),
            //     Instruction::Jump(5), // break jumps past if-without-else and loop
            //     Instruction::Jump(2), // skip false-path pop
            //     Instruction::Pop(1),  // pop condition (false path)
            //     Instruction::Jump(-17),
            //     Instruction::Pop(1),
            //     Instruction::LoadVar("a".to_string()),
            //     Instruction::Return,
            // ],
            // MIR codegen (naive) - same semantics, more instructions:
            vec![
                // Pre-allocate locals with null
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                // Initialize a = 1
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("a".to_string()),
                // Return block jump
                Instruction::Jump(3),
                Instruction::LoadVar("_0".to_string()),
                Instruction::Return,
                // While loop condition: a < 5
                Instruction::LoadVar("a".to_string()),
                Instruction::StoreVar("_3".to_string()),
                Instruction::LoadConst(Value::Int(5)),
                Instruction::StoreVar("_4".to_string()),
                Instruction::LoadVar("_3".to_string()),
                Instruction::LoadVar("_4".to_string()),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::StoreVar("_2".to_string()),
                Instruction::LoadVar("_2".to_string()),
                Instruction::JumpIfFalse(23),
                Instruction::Jump(1),
                // a += 1
                Instruction::LoadVar("a".to_string()),
                Instruction::StoreVar("_6".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("_7".to_string()),
                Instruction::LoadVar("_6".to_string()),
                Instruction::LoadVar("_7".to_string()),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("_8".to_string()),
                Instruction::LoadVar("_8".to_string()),
                Instruction::StoreVar("a".to_string()),
                // if (a == 2) condition
                Instruction::LoadVar("a".to_string()),
                Instruction::StoreVar("_10".to_string()),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::StoreVar("_11".to_string()),
                Instruction::LoadVar("_10".to_string()),
                Instruction::LoadVar("_11".to_string()),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::StoreVar("_9".to_string()),
                Instruction::LoadVar("_9".to_string()),
                Instruction::JumpIfFalse(6),
                // break: return a
                Instruction::Jump(4),
                Instruction::LoadVar("a".to_string()),
                Instruction::StoreVar("_0".to_string()),
                Instruction::Jump(-36),
                // Skip else
                Instruction::Jump(-3),
                // After if (null for unit type)
                Instruction::LoadConst(Value::Null),
                Instruction::StoreVar("_5".to_string()),
                Instruction::Jump(1),
                // Loop back
                Instruction::Jump(-39),
            ],
        )],
    })
}

#[test]
#[ignore = "function parameters not yet tracked in HIR"]
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
#[ignore = "function parameters not yet tracked in HIR"]
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
            // THIR codegen (efficient):
            // vec![
            //     Instruction::LoadConst(Value::Bool(true)),
            //     Instruction::JumpIfFalse(15),
            //     Instruction::Pop(1),
            //     Instruction::LoadConst(Value::Bool(false)),
            //     Instruction::JumpIfFalse(4),
            //     Instruction::Pop(1),
            //     Instruction::Jump(1),
            //     Instruction::Jump(-4),
            //     Instruction::Pop(1),
            //     Instruction::LoadConst(Value::Bool(false)),
            //     Instruction::JumpIfFalse(4),
            //     Instruction::Pop(1),
            //     Instruction::Jump(3), // continue jumps to loop start
            //     Instruction::Jump(2), // skip false-path pop
            //     Instruction::Pop(1),  // pop condition (false path)
            //     Instruction::Jump(-15),
            //     Instruction::Pop(1),
            //     Instruction::LoadConst(Value::Int(5)),
            //     Instruction::Return,
            // ],
            // MIR codegen (naive) - same semantics, more instructions:
            vec![
                // Pre-allocate locals with null
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                // Return block jump
                Instruction::Jump(3),
                Instruction::LoadVar("_0".to_string()),
                Instruction::Return,
                // Outer while (true) condition
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::StoreVar("_1".to_string()),
                Instruction::LoadVar("_1".to_string()),
                Instruction::JumpIfFalse(3),
                Instruction::Jump(1),
                // After outer loop: return 5
                Instruction::Jump(4),
                Instruction::LoadConst(Value::Int(5)),
                Instruction::StoreVar("_0".to_string()),
                Instruction::Jump(-10),
                // Inner while (false) condition
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::StoreVar("_3".to_string()),
                Instruction::LoadVar("_3".to_string()),
                Instruction::JumpIfFalse(3),
                Instruction::Jump(1),
                // continue in inner loop (jump back)
                Instruction::Jump(-5),
                // if (false) condition
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::StoreVar("_5".to_string()),
                Instruction::LoadVar("_5".to_string()),
                Instruction::JumpIfFalse(3),
                Instruction::Jump(1),
                // continue in outer loop
                Instruction::Jump(-20),
                // After if (null for unit type)
                Instruction::LoadConst(Value::Null),
                Instruction::StoreVar("_2".to_string()),
                Instruction::Jump(1),
                // Loop back to outer condition
                Instruction::Jump(-24),
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
            // THIR codegen (efficient):
            // vec![
            //     Instruction::LoadConst(Value::Int(5)),
            //     Instruction::LoadConst(Value::Bool(true)),
            //     Instruction::JumpIfFalse(18),
            //     Instruction::Pop(1),
            //     Instruction::LoadConst(Value::Bool(true)),
            //     Instruction::JumpIfFalse(8),
            //     Instruction::Pop(1),
            //     Instruction::LoadVar("a".to_string()),
            //     Instruction::LoadConst(Value::Int(1)),
            //     Instruction::BinOp(BinOp::Add),
            //     Instruction::StoreVar("a".to_string()),
            //     Instruction::Jump(3),
            //     Instruction::Jump(-8),
            //     Instruction::Pop(1),
            //     Instruction::LoadVar("a".to_string()),
            //     Instruction::LoadConst(Value::Int(1)),
            //     Instruction::BinOp(BinOp::Add),
            //     Instruction::StoreVar("a".to_string()),
            //     Instruction::Jump(3),
            //     Instruction::Jump(-18),
            //     Instruction::Pop(1),
            //     Instruction::LoadVar("a".to_string()),
            //     Instruction::Return,
            // ],
            // MIR codegen (naive) - same semantics, more instructions:
            vec![
                // Pre-allocate locals with null
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                // Initialize a = 5
                Instruction::LoadConst(Value::Int(5)),
                Instruction::StoreVar("a".to_string()),
                // Return block jump
                Instruction::Jump(3),
                Instruction::LoadVar("_0".to_string()),
                Instruction::Return,
                // Outer while (true) condition
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::StoreVar("_2".to_string()),
                Instruction::LoadVar("_2".to_string()),
                Instruction::JumpIfFalse(3),
                Instruction::Jump(1),
                // After outer loop: return a
                Instruction::Jump(4),
                Instruction::LoadVar("a".to_string()),
                Instruction::StoreVar("_0".to_string()),
                Instruction::Jump(-10),
                // Inner while (true) condition
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::StoreVar("_4".to_string()),
                Instruction::LoadVar("_4".to_string()),
                Instruction::JumpIfFalse(13),
                Instruction::Jump(1),
                // Inner loop body: a = a + 1
                Instruction::LoadVar("a".to_string()),
                Instruction::StoreVar("_7".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("_8".to_string()),
                Instruction::LoadVar("_7".to_string()),
                Instruction::LoadVar("_8".to_string()),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("_6".to_string()),
                Instruction::LoadVar("_6".to_string()),
                Instruction::StoreVar("a".to_string()),
                // break from inner loop
                Instruction::Jump(1),
                // After inner loop: a = a + 1
                Instruction::LoadVar("a".to_string()),
                Instruction::StoreVar("_10".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("_11".to_string()),
                Instruction::LoadVar("_10".to_string()),
                Instruction::LoadVar("_11".to_string()),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("_9".to_string()),
                Instruction::LoadVar("_9".to_string()),
                Instruction::StoreVar("a".to_string()),
                // break from outer loop (loop back to outer condition then exit)
                Instruction::Jump(-29),
            ],
        )],
    })
}
