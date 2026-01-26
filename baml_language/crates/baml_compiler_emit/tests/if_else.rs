//! Compiler tests for if/else expressions and statements.

use baml_tests::{
    codegen::{Program, assert_compiles},
    vm::{Instruction, Value},
};
use bex_vm_types::bytecode::{BinOp, CmpOp};

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
            // ReturnPhi optimization: _0 stays on stack, no Store/Load needed
            vec![
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                // Else branch: load 2, jump to return
                Instruction::LoadConst(Value::Int(2)),
                Instruction::Jump(2),
                // Then branch: load 1
                Instruction::LoadConst(Value::Int(1)),
                // Return (value already on stack)
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
            // ReturnPhi optimization: _0 stays on stack, no Store/Load needed
            vec![
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(1)),
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
            // ReturnPhi optimization: _0 stays on stack, no Store/Load needed
            vec![
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(20)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(10)),
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
            // ReturnPhi optimization: _0 stays on stack, no Store/Load needed
            vec![
                Instruction::LoadConst(Value::Int(5)),
                Instruction::LoadConst(Value::Int(5)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(200)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(100)),
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
            // Phi-like optimization: x is assigned in both branches and used once,
            // so it can stay on the stack without Store/Load.
            vec![
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(42)),
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
            // 'a' and 'b' are Virtual (single-use, inlined). '_0' is ReturnPhi:
            vec![
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                // Else branch: b inlined as 2
                Instruction::LoadConst(Value::Int(2)),
                Instruction::Jump(2),
                // Then branch: a inlined as 1
                Instruction::LoadConst(Value::Int(1)),
                // Return (value on stack)
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
            // ReturnPhi optimization: _0 stays on stack, no Store/Load needed
            vec![
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::Jump(7),
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(1)),
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
            // ReturnPhi optimization: _0 stays on stack, no Store/Load needed
            vec![
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(8),
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::Jump(4),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(1)),
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
            // ReturnPhi optimization: _0 stays on stack, no Store/Load needed
            // Constant propagation: x = 5 is single-definition constant, inlined at each use
            vec![
                Instruction::LoadConst(Value::Int(5)), // x inlined
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(10),
                Instruction::LoadConst(Value::Int(5)), // x inlined
                Instruction::LoadConst(Value::Int(10)),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::Jump(4),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(0)),
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
            // THIR codegen (efficient):
            // vec![
            //     Instruction::LoadConst(Value::Bool(true)),
            //     Instruction::PopJumpIfFalse(5),
            //     Instruction::Pop(1),
            //     Instruction::LoadGlobal(Value::function("get_value")),
            //     Instruction::Call(0),
            //     Instruction::Jump(3),
            //     Instruction::Pop(1),
            //     Instruction::LoadConst(Value::Int(0)),
            //     Instruction::Return,
            // ],
            // ReturnPhi optimization: _0 stays on stack, no Store/Load needed
            vec![
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::Jump(3),
                Instruction::LoadGlobal(Value::function("get_value")),
                Instruction::Call(0),
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
            // ReturnPhi optimization: _0 stays on stack, no Store/Load needed
            vec![
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(100)),
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
            // ReturnPhi + Phi-like: both _0 and short-circuit result stay on stack
            vec![
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(1)),
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
            // ReturnPhi + Phi-like: both _0 and short-circuit result stay on stack
            vec![
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(1)),
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
            // Phi-like optimization: if-else result stays on stack, no Store/Load needed.
            vec![
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::LoadConst(Value::Int(1)),
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
            // Phi-like optimization: if-else result stays on stack for the call.
            // ReturnPhi optimization: Call result goes directly to stack, no Store/Load for _0.
            vec![
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(20)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(10)),
                Instruction::LoadGlobal(Value::function("identity")),
                Instruction::Call(1),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn if_else_assigned_then_passed_to_call() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function identity(x: int) -> int { x }

            function main() -> int {
                let tmp = if (false) { 10 } else { 20 };
                identity(tmp)
            }
        ",
        expected: vec![(
            "main",
            // tmp is PhiLike (assigned in both branches, used once).
            // ReturnPhi optimization: Call result goes directly to stack.
            vec![
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(20)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(10)),
                Instruction::LoadGlobal(Value::function("identity")),
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
            // Phi-like optimization: second if-else result stays on stack.
            // First if-else (_1) needs Store/Load because second if-else is computed before use.
            vec![
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(4),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::StoreVar("_1".to_string()),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("_1".to_string()),
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(4)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::LoadVar("_1".to_string()),
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
            // Phi-like optimization: second if-else result stays on stack.
            // First if-else (_1) needs Store/Load because second if-else is computed before use.
            vec![
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(4),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::StoreVar("_1".to_string()),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("_1".to_string()),
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(4)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::LoadVar("_1".to_string()),
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

#[test]
fn if_without_else_statement() -> anyhow::Result<()> {
    // If-without-else does NOT produce a value, so Stmt::Expr doesn't pop.
    // Both paths pop the condition, then join at the end.
    assert_compiles(Program {
        source: "
            function main() -> int {
                let x = 0;
                if (true) {
                    x = 5;
                }
                x
            }
        ",
        expected: vec![(
            "main",
            // THIR codegen (efficient):
            // vec![
            //     Instruction::LoadConst(Value::Int(0)), // let x = 0
            //     // if (true)
            //     Instruction::LoadConst(Value::Bool(true)),
            //     Instruction::PopJumpIfFalse(5), // jump to false path pop
            //     Instruction::Pop(1),         // pop condition (true path)
            //     // then block: x = 5
            //     Instruction::LoadConst(Value::Int(5)),
            //     Instruction::StoreVar("x".to_string()),
            //     Instruction::Jump(2), // skip false path pop
            //     // false path
            //     Instruction::Pop(1), // pop condition (false path)
            //     // No Stmt::Expr pop - if-without-else doesn't produce a value
            //     // return x
            //     Instruction::LoadVar("x".to_string()),
            //     Instruction::Return,
            // ],
            // Stackification with fall-through elimination:
            // if-without-else is void - no temporary needed
            vec![
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::StoreVar("x".to_string()),
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(5)),
                Instruction::StoreVar("x".to_string()),
                Instruction::LoadVar("x".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn if_without_else_with_local_var() -> anyhow::Result<()> {
    // If-without-else with a local variable in the then block
    // The block's exit_scope pops the local, no Stmt::Expr pop needed
    assert_compiles(Program {
        source: "
            function main() -> int {
                let result = 0;
                if (true) {
                    let temp = 10;
                }
                result
            }
        ",
        expected: vec![(
            "main",
            // 'result' is Virtual (single-use, inlined as 0). 'temp' is Real (assigned but unused):
            vec![
                Instruction::LoadConst(Value::Null), // Pre-allocate for 'temp'
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(10)),
                Instruction::StoreVar("temp".to_string()),
                Instruction::LoadConst(Value::Int(0)), // result inlined
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn consecutive_if_without_else() -> anyhow::Result<()> {
    // Two consecutive if-without-else statements
    // Neither produces a value, so no Stmt::Expr pops
    assert_compiles(Program {
        source: "
            function main() -> int {
                let x = 0;
                if (true) { x = 1; }
                if (false) { x = 2; }
                x
            }
        ",
        expected: vec![(
            "main",
            // THIR codegen (efficient):
            // vec![
            //     Instruction::LoadConst(Value::Int(0)), // let x = 0
            //     // first if (true)
            //     Instruction::LoadConst(Value::Bool(true)),
            //     Instruction::PopJumpIfFalse(5),
            //     Instruction::Pop(1),
            //     Instruction::LoadConst(Value::Int(1)),
            //     Instruction::StoreVar("x".to_string()),
            //     Instruction::Jump(2),
            //     Instruction::Pop(1),
            //     // No Stmt::Expr pop for first if
            //     // second if (false)
            //     Instruction::LoadConst(Value::Bool(false)),
            //     Instruction::PopJumpIfFalse(5),
            //     Instruction::Pop(1),
            //     Instruction::LoadConst(Value::Int(2)),
            //     Instruction::StoreVar("x".to_string()),
            //     Instruction::Jump(2),
            //     Instruction::Pop(1),
            //     // No Stmt::Expr pop for second if
            //     // return x
            //     Instruction::LoadVar("x".to_string()),
            //     Instruction::Return,
            // ],
            // Stackification with fall-through elimination:
            // Both if-without-else are void - no temporaries needed
            vec![
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::StoreVar("x".to_string()),
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("x".to_string()),
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::StoreVar("x".to_string()),
                Instruction::LoadVar("x".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

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
            // 'a' and 'b' are Virtual (single-use), fully inlined:
            vec![Instruction::LoadConst(Value::Int(1)), Instruction::Return],
        )],
    })
}

// ============================================================================
// Tests requiring function parameters (ignored until HIR supports them)
// ============================================================================

#[test]
fn if_else_assignment_with_locals() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main(b: bool) -> int {
                let i = if (b) {
                    let a = 1;
                    a
                } else {
                    let a = 2;
                    a
                };

                i
            }
        ",
        expected: vec![(
            "main",
            // Phi-like optimization: i is assigned in both branches and used once.
            // Inner `let a` variables are virtual (inlined).
            vec![
                Instruction::LoadVar("b".to_string()),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn if_else_normal_statement() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function identity(i: int) -> int {
                i
            }

            function main(b: bool) -> int {
                let a = 1;

                if (b) {
                    let x = 1;
                    let y = 2;
                    identity(x);
                } else {
                    let x = 3;
                    let y = 4;
                    identity(y);
                }

                a
            }
        ",
        expected: vec![(
            "main",
            // Constant propagation: single-def constants x, y are inlined at use sites
            // Only unused user-named variables (x in else, y in then) need slots
            vec![
                // Pre-allocate 2 slots for unused user-named variables
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                // Check condition b
                Instruction::LoadVar("b".to_string()),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(8),
                // Else branch: x = 3 (unused, stored), identity(4) (y inlined)
                Instruction::LoadConst(Value::Int(3)),
                Instruction::StoreVar("x".to_string()),
                Instruction::LoadGlobal(Value::function("identity")),
                Instruction::LoadConst(Value::Int(4)), // y inlined
                Instruction::Call(1),
                Instruction::Pop(1), // discard unused call result
                Instruction::Jump(7),
                // Then branch: y = 2 (unused, stored), identity(1) (x inlined)
                Instruction::LoadConst(Value::Int(2)),
                Instruction::StoreVar("y".to_string()),
                Instruction::LoadGlobal(Value::function("identity")),
                Instruction::LoadConst(Value::Int(1)), // x inlined
                Instruction::Call(1),
                Instruction::Pop(1), // discard unused call result
                // Return a (inlined as 1)
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn else_if_return_expr_with_locals() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main(a: bool, b: bool) -> int {
                if (a) {
                    let x = 1;
                    x
                } else if (b) {
                    let y = 2;
                    y
                } else {
                    let z = 3;
                    z
                }
            }
        ",
        expected: vec![(
            "main",
            // ReturnPhi optimization: _0 stays on stack, no Store/Load needed
            // Local variables x, y, z are all virtualized away by copy propagation
            vec![
                Instruction::LoadVar("a".to_string()),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(8),
                Instruction::LoadVar("b".to_string()),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::Jump(4),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn else_if_assignment() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main(a: bool, b: bool) -> int {
                let result = if (a) {
                    1
                } else if (b) {
                    2
                } else {
                    3
                };

                result
            }
        ",
        expected: vec![(
            "main",
            // MIR-based codegen with local pre-allocation
            vec![
                // Pre-allocate result
                Instruction::LoadConst(Value::Null),
                // Check condition a
                Instruction::LoadVar("a".to_string()),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(10),
                // Else: check condition b
                Instruction::LoadVar("b".to_string()),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(4),
                // Else else: result = 3
                Instruction::LoadConst(Value::Int(3)),
                Instruction::StoreVar("result".to_string()),
                // Jump threading: direct to return (was Jump(3))
                Instruction::Jump(6),
                // Else if true: result = 2
                Instruction::LoadConst(Value::Int(2)),
                Instruction::StoreVar("result".to_string()),
                Instruction::Jump(3),
                // If true: result = 1
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("result".to_string()),
                // Return result
                Instruction::LoadVar("result".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn else_if_assignment_with_locals() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main(a: bool, b: bool) -> int {
                let result = if (a) {
                    let x = 1;
                    x
                } else if (b) {
                    let y = 2;
                    y
                } else {
                    let z = 3;
                    z
                };

                result
            }
        ",
        expected: vec![(
            "main",
            // MIR-based codegen with local pre-allocation
            vec![
                // Pre-allocate result
                Instruction::LoadConst(Value::Null),
                // Check condition a
                Instruction::LoadVar("a".to_string()),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(10),
                // Else: check condition b
                Instruction::LoadVar("b".to_string()),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(4),
                // Else else: result = 3
                Instruction::LoadConst(Value::Int(3)),
                Instruction::StoreVar("result".to_string()),
                // Jump threading: direct to return (was Jump(3))
                Instruction::Jump(6),
                // Else if true: result = 2
                Instruction::LoadConst(Value::Int(2)),
                Instruction::StoreVar("result".to_string()),
                Instruction::Jump(3),
                // If true: result = 1
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("result".to_string()),
                // Return result
                Instruction::LoadVar("result".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn nested_block_expr_with_ending_normal_if() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                let a = 1;

                {
                    let b = 2;
                    let c = 3;
                    a = b + c;

                    if (a == 5) {
                        a = 10;
                    }
                }

                a
            }
        ",
        expected: vec![(
            "main",
            // MIR-based codegen with local pre-allocation
            vec![
                // Pre-allocate a
                Instruction::LoadConst(Value::Null),
                // a = 1
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("a".to_string()),
                // a = 2 + 3 (b and c are inlined)
                Instruction::LoadConst(Value::Int(2)),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("a".to_string()),
                // if (a == 5)
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(5)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(2),
                // If false: skip to return
                Instruction::Jump(3),
                // If true: a = 10
                Instruction::LoadConst(Value::Int(10)),
                Instruction::StoreVar("a".to_string()),
                // Return a
                Instruction::LoadVar("a".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn return_with_stack() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function WithStack(x: int) -> int {
              let a = 1;

              // NOTE: currently there's no empty returns.

              if (a == 0) { return 0; }

              {
                 let b = 1;
                 if (a != b) {
                    return 0;
                 }
              }

              {
                 let c = 2;
                 let b = 3;
                 while (b != c) {
                    if (true) {
                       return 0;
                    }
                 }
              }

               7
            }
        ",
        expected: vec![(
            "WithStack",
            // Constant propagation: a, b, c are all single-def constants inlined at use sites
            // No local slots needed (all inlined), _0 is ReturnPhi
            vec![
                // if (a == 0) where a=1 is inlined
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(21),
                // if (a != b) where a=1 and b=1 are both inlined
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::CmpOp(CmpOp::NotEq),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(14),
                // while (b != c) where b=3 and c=2 are both inlined
                Instruction::LoadConst(Value::Int(3)),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::CmpOp(CmpOp::NotEq),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                // Loop exit: return 7
                Instruction::LoadConst(Value::Int(7)),
                Instruction::Return,
                // Loop body: if (true)
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(2),
                // Jump back to while condition
                Instruction::Jump(-10),
                // return 0 (from if true in loop)
                Instruction::LoadConst(Value::Int(0)),
                Instruction::Return,
                // return 0 (from a != b check)
                Instruction::LoadConst(Value::Int(0)),
                Instruction::Return,
                // return 0 (from a == 0 check)
                Instruction::LoadConst(Value::Int(0)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn if_else_return_expr() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main(b: bool) -> int {
                if (b) { 1 } else { 2 }
            }
        ",
        expected: vec![(
            "main",
            // ReturnPhi optimization: _0 stays on stack, no Store/Load needed
            vec![
                Instruction::LoadVar("b".to_string()),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn if_else_return_expr_with_locals() -> anyhow::Result<()> {
    // Note: The MIR optimizer performs copy propagation, so `let a = 1; a`
    // is optimized to just `1`. The local variable `a` is virtualized away.
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
            // ReturnPhi optimization: _0 stays on stack, no Store/Load needed
            vec![
                Instruction::LoadVar("b".to_string()),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
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
            // Phi-like optimization: i is assigned in both branches and used once,
            // so it stays on the stack without Store/Load.
            vec![
                Instruction::LoadVar("b".to_string()),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
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
            // ReturnPhi optimization: _0 stays on stack, no Store/Load needed
            vec![
                Instruction::LoadVar("a".to_string()),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(8),
                Instruction::LoadVar("b".to_string()),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::Jump(4),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Return statements (early returns)
// ============================================================================

#[test]
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
            // ReturnPhi optimization: _0 stays on stack, no Store/Load needed
            vec![
                // if (x == 42)
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(42)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(5),
                // Default return: x + 5 (value on stack, Return directly)
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(5)),
                Instruction::BinOp(BinOp::Add),
                Instruction::Return,
                // Early return: return 1 (value on stack, Return directly)
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Return,
            ],
        )],
    })
}
