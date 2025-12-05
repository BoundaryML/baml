//! Compiler tests for if/else expressions and statements.

use baml_vm::{
    BinOp, CmpOp,
    test::{Instruction, Value},
};

use super::common::{Program, assert_compiles};

// ============================================================================
// If/else with literal conditions (no function parameters needed)
// ============================================================================

#[test]
fn if_else_literal_true() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                if (true) { 1 } else { 2 }
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn if_else_literal_false() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                if (false) { 1 } else { 2 }
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn if_else_comparison_condition() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                if (1 < 2) { 10 } else { 20 }
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(10)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(20)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn if_else_equality_condition() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                if (5 == 5) { 100 } else { 200 }
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Int(5)),
                Instruction::LoadConst(Value::Int(5)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(100)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(200)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn if_else_assign_to_variable() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                let x = if (true) { 42 } else { 0 };
                x
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(42)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::LoadVar("x".to_string()),
                Instruction::PopReplace(1),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn if_else_with_local_in_branches() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                if (true) {
                    let a = 1;
                    a
                } else {
                    let b = 2;
                    b
                }
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::JumpIfFalse(6),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadVar("a".to_string()),
                Instruction::PopReplace(1),
                Instruction::Jump(5),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::LoadVar("b".to_string()),
                Instruction::PopReplace(1),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn if_else_nested() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                if (true) {
                    if (false) { 1 } else { 2 }
                } else {
                    3
                }
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::JumpIfFalse(10),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn else_if_chain() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                if (false) {
                    1
                } else if (false) {
                    2
                } else {
                    3
                }
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Jump(9),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn else_if_with_comparisons() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                let x = 5;
                if (x < 0) {
                    0
                } else if (x < 10) {
                    1
                } else {
                    2
                }
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Int(5)),
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::Jump(11),
                Instruction::Pop(1),
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(10)),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::PopReplace(1),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn if_else_with_function_call_in_branch() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function get_value() -> int {
                42
            }

            function main() -> int {
                if (true) { get_value() } else { 0 }
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::JumpIfFalse(5),
                Instruction::Pop(1),
                Instruction::LoadGlobal(Value::function("get_value")),
                Instruction::Call(0),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn if_else_with_arithmetic_in_condition() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                if (1 + 1 == 2) { 100 } else { 0 }
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(100)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn if_else_with_logical_and_in_condition() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                if (true && true) { 1 } else { 0 }
            }
        ",
        expected: vec![(
            "main",
            vec![
                // Short-circuit AND evaluation: if left is false, skip right and use left for if-else
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::JumpIfFalse(3), // If false, jump to the if's JumpIfFalse (keeps false on stack)
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Bool(true)),
                // If-else
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn if_else_with_logical_or_in_condition() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                if (false || true) { 1 } else { 0 }
            }
        ",
        expected: vec![(
            "main",
            vec![
                // Short-circuit OR evaluation: if left is true, skip right and use left for if-else
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::JumpIfFalse(2),
                Instruction::Jump(3), // If true, jump past Pop+LoadConst to the if's JumpIfFalse
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Bool(true)),
                // If-else
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// If-else in expression contexts
// ============================================================================

#[test]
fn if_else_in_arithmetic() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                1 + if (true) { 2 } else { 3 }
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Int(1)),
                // if-else expression
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(3)),
                // addition
                Instruction::BinOp(BinOp::Add),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn if_else_as_function_arg() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function identity(x: int) -> int { x }

            function main() -> int {
                identity(if (false) { 10 } else { 20 })
            }
        ",
        expected: vec![(
            "main",
            vec![
                // Load function
                Instruction::LoadGlobal(Value::function("identity")),
                // if-else as argument
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(10)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(20)),
                // Call with 1 arg
                Instruction::Call(1),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn parenthesized_if_else_in_arithmetic() -> anyhow::Result<()> {
    // Workaround: wrap if-else in parentheses when using as left operand
    assert_compiles(Program {
        source: "
            function main() -> int {
                (if (true) { 1 } else { 2 }) + (if (false) { 3 } else { 4 })
            }
        ",
        expected: vec![(
            "main",
            vec![
                // First if-else (in parens)
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(2)),
                // Second if-else (in parens)
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(4)),
                // Addition
                Instruction::BinOp(BinOp::Add),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn chained_if_else_in_arithmetic() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                if (true) { 1 } else { 2 } + if (false) { 3 } else { 4 }
            }
        ",
        expected: vec![(
            "main",
            vec![
                // First if-else
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(2)),
                // Second if-else
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(4)),
                // Addition
                Instruction::BinOp(BinOp::Add),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// If without else
// ============================================================================

// Note: If-without-else where the then-branch produces a value is a type error.
// The type checker ensures that if-without-else is only used when the then-branch
// block has no trailing expression (i.e., doesn't produce a value).
// Example: `if (cond) { x = 5; }` is valid, `if (cond) { 5 }` is a type error.

// ============================================================================
// Block expressions
// ============================================================================

#[test]
fn block_expr() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                let a = {
                    let b = 1;
                    b
                };

                a
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadVar("b".to_string()),
                Instruction::PopReplace(1),
                Instruction::LoadVar("a".to_string()),
                Instruction::PopReplace(1),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Tests requiring function parameters (ignored until HIR supports them)
// ============================================================================

#[test]
#[ignore = "function parameters not yet tracked in HIR"]
fn if_else_return_expr() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main(b: bool) -> int {
                if (b) { 1 } else { 2 }
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadVar("b".to_string()),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
#[ignore = "function parameters not yet tracked in HIR"]
fn if_else_return_expr_with_locals() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main(b: bool) -> int {
                if (b) {
                    let a = 1;
                    a
                } else {
                    let a = 2;
                    a
                }
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadVar("b".to_string()),
                Instruction::JumpIfFalse(6),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadVar("a".to_string()),
                Instruction::PopReplace(1),
                Instruction::Jump(5),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::LoadVar("a".to_string()),
                Instruction::PopReplace(1),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
#[ignore = "function parameters not yet tracked in HIR"]
fn if_else_assignment() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main(b: bool) -> int {
                let i = if (b) { 1 } else { 2 };
                i
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadVar("b".to_string()),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::LoadVar("i".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
#[ignore = "function parameters not yet tracked in HIR"]
fn else_if_return_expr() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main(a: bool, b: bool) -> int {
                if (a) {
                    1
                } else if (b) {
                    2
                } else {
                    3
                }
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadVar("a".to_string()),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Jump(9),
                Instruction::Pop(1),
                Instruction::LoadVar("b".to_string()),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Return statements (early returns)
// ============================================================================

#[test]
#[ignore = "function parameters not yet tracked in HIR"]
fn early_return() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function EarlyReturn(x: int) -> int {
              if (x == 42) { return 1; }

              x + 5
            }
        ",
        expected: vec![(
            "EarlyReturn",
            vec![
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(42)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(5),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Return,
                Instruction::Jump(2),
                Instruction::Pop(1),
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(5)),
                Instruction::BinOp(BinOp::Add),
                Instruction::Return,
            ],
        )],
    })
}
