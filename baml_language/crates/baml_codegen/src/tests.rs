//! Tests for bytecode generation.
//!
//! These tests verify that the compiler generates correct bytecode
//! for various BAML constructs by compiling BAML source code through
//! the full pipeline.

mod common;

use baml_vm::{
    BinOp,
    test::{Instruction, Value},
};
use common::{Program, assert_compiles};

// ============================================================================
// Literal Tests
// ============================================================================

#[test]
fn return_literal_int() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                42
            }
        ",
        expected: vec![(
            "main",
            vec![Instruction::LoadConst(Value::Int(42)), Instruction::Return],
        )],
    })
}

#[test]
fn return_literal_bool() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> bool {
                true
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::Return,
            ],
        )],
    })
}

// TODO: Enable when string literals are supported in HIR/THIR
#[test]
#[ignore = "string literals not yet supported in HIR"]
fn return_literal_string() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function main() -> string {
                "hello"
            }
        "#,
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::string("hello")),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Function Tests
// ============================================================================

#[test]
fn return_function_call() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function one() -> int {
                1
            }

            function main() -> int {
                one()
            }
        ",
        expected: vec![
            (
                "one",
                vec![Instruction::LoadConst(Value::Int(1)), Instruction::Return],
            ),
            (
                "main",
                vec![
                    Instruction::LoadGlobal(Value::function("one")),
                    Instruction::Call(0),
                    Instruction::Return,
                ],
            ),
        ],
    })
}

#[test]
fn assign_function_call_to_variable() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function two() -> int {
                2
            }

            function main() -> int {
                let a = two();
                a
            }
        ",
        expected: vec![
            (
                "two",
                vec![Instruction::LoadConst(Value::Int(2)), Instruction::Return],
            ),
            (
                "main",
                vec![
                    Instruction::LoadGlobal(Value::function("two")),
                    Instruction::Call(0),
                    Instruction::LoadVar("a".to_string()),
                    Instruction::PopReplace(1),
                    Instruction::Return,
                ],
            ),
        ],
    })
}

// ============================================================================
// Operator Tests
// ============================================================================

#[test]
fn basic_add() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                let a = 1 + 2;
                a
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::BinOp(BinOp::Add),
                Instruction::LoadVar("a".to_string()),
                Instruction::PopReplace(1),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn basic_and() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function ret_bool() -> bool {
                true
            }

            function main() -> bool {
                true && ret_bool()
            }
        ",
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
        source: "
            function ret_bool() -> bool {
                true
            }

            function main() -> bool {
                true || ret_bool()
            }
        ",
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

// ============================================================================
// Control Flow Tests
// ============================================================================

// TODO: Enable when function parameters are properly tracked in HIR
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

// TODO: Enable when function parameters are properly tracked in HIR
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

// ============================================================================
// Array Tests
// ============================================================================

#[test]
fn return_array_constructor() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int[] {
                [1, 2, 3]
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::AllocArray(3),
                Instruction::Return,
            ],
        )],
    })
}
