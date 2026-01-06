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
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                // Loop exit: return a
                Instruction::LoadVar("a".to_string()),
                Instruction::Return,
                // Loop body: if (a > b)
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadVar("b".to_string()),
                Instruction::CmpOp(CmpOp::Gt),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(6),
                // Else branch: b = b - a
                Instruction::LoadVar("b".to_string()),
                Instruction::LoadVar("a".to_string()),
                Instruction::BinOp(BinOp::Sub),
                Instruction::StoreVar("b".to_string()),
                // Jump threading: direct jump back to loop condition (was Jump(5) -> Jump(-21))
                Instruction::Jump(-16),
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
                Instruction::PopJumpIfFalse(11),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("a".to_string()),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2),
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
                Instruction::PopJumpIfFalse(11),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("a".to_string()),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2),
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
                Instruction::PopJumpIfFalse(15),
                // if (limit == 0)
                Instruction::LoadVar("limit".to_string()),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2),
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
            vec![
                // Pre-allocate locals
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                // Initialize result = 1
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("result".to_string()),
                // Initialize should_continue = true
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::StoreVar("should_continue".to_string()),
                // Loop condition: should_continue
                Instruction::LoadVar("should_continue".to_string()),
                Instruction::PopJumpIfFalse(2),
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
                Instruction::PopJumpIfFalse(2),
                // continue - jump threading: direct to loop condition (was Jump(4) -> Jump(-21))
                Instruction::Jump(-17),
                // else: should_continue = false
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::StoreVar("should_continue".to_string()),
                // Jump back to loop condition
                Instruction::Jump(-20),
                // Note: unreachable continue fallthrough eliminated by jump threading
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
            // Jump threading eliminates intermediate jumps:
            // - Inner continue jumps directly to inner condition
            // - Outer continue jumps directly to outer condition
            vec![
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(5)),
                Instruction::Return,
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(-2), // inner continue: direct to inner condition
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(-10), // outer continue: direct to outer condition
                Instruction::Jump(-11), // outer loop back
                Instruction::Jump(-7),  // inner loop back
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
                Instruction::PopJumpIfFalse(11),
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::PopJumpIfFalse(5),
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

/// Test break with variable conditions to verify bytecode generation.
///
/// Key observation: The compiler detects that `break` is unconditional at the
/// end of each loop body, so it eliminates:
/// 1. Explicit jump instructions for `break` (uses fall-through instead)
/// 2. Loop-back jumps (dead code since break always executes)
///
/// This is NOT constant folding - it happens with variable conditions too.
#[test]
fn break_nested_with_variable_conditions() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function Nested(x: bool, y: bool) -> int {
                let a = 5;
                while (x) {
                    while (y) {
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
                // let a = 5
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Int(5)),
                Instruction::StoreVar("a".to_string()),
                // outer while (x) - condition check
                Instruction::LoadVar("x".to_string()),
                Instruction::PopJumpIfFalse(11), // if false, jump to return (idx 15)
                // inner while (y) - condition check
                Instruction::LoadVar("y".to_string()),
                Instruction::PopJumpIfFalse(5), // if false, jump to outer body (idx 11)
                // inner body: a = a + 1; break (no explicit break jump - falls through!)
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("a".to_string()),
                // outer body after inner: a = a + 1; break (no explicit break jump!)
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("a".to_string()),
                // after outer loop: return a (no loop-back jumps exist!)
                Instruction::LoadVar("a".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

/// Test a loop that should actually iterate (conditional break, not unconditional).
/// This verifies that loop-back jumps ARE generated when needed.
///
/// Key difference from unconditional break:
/// - Unconditional break at end of loop body → no loop-back jump (dead code)
/// - Conditional break inside if-statement → loop-back jump IS generated
#[test]
fn while_loop_with_conditional_break() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function CountDown(n: int) -> int {
                let result = 0;
                while (true) {
                    result = result + n;
                    n = n - 1;
                    if (n == 0) {
                        break;
                    }
                }
                result
            }
        "#,
        expected: vec![(
            "CountDown",
            vec![
                // let result = 0
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::StoreVar("result".to_string()),
                // while (true) - condition
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::PopJumpIfFalse(15), // if false, jump to return (idx 19)
                // loop body: result = result + n
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadVar("n".to_string()),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("result".to_string()),
                // n = n - 1
                Instruction::LoadVar("n".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Sub),
                Instruction::StoreVar("n".to_string()),
                // if (n == 0)
                Instruction::LoadVar("n".to_string()),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2), // if false (n != 0), jump to loop-back
                Instruction::Jump(2),           // if true (n == 0), jump to break/exit
                // else path: LOOP-BACK JUMP (this is the key difference!)
                Instruction::Jump(-15), // back to while condition (idx 3)
                // after loop: return result
                Instruction::LoadVar("result".to_string()),
                Instruction::Return,
            ],
        )],
    })
}
