//! Compiler tests for match expressions.
//!
//! NOTE: These tests were written for the old direct THIR->bytecode compiler.
//! The new MIR-based pipeline produces functionally equivalent but structurally
//! different bytecode (using explicit locals instead of stack manipulation).
//! These tests are ignored until they can be rewritten for the MIR format.
//! Match functionality is tested through snapshot tests in `baml_tests`.

use baml_tests::{
    codegen::{Program, assert_compiles},
    vm::{Instruction, Value},
};
use baml_vm::CmpOp;

// ============================================================================
// Basic Catch-All Tests
// ============================================================================

#[test]
#[ignore = "needs rewrite for MIR-based bytecode format"]
fn match_catch_all_underscore() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                match (42) {
                    _ => 100
                }
            }
        ",
        expected: vec![(
            "main",
            vec![
                // Scrutinee stored (but catch-all doesn't need to check it)
                Instruction::LoadConst(Value::Int(42)),
                // Catch-all arm body
                Instruction::LoadConst(Value::Int(100)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
#[ignore = "needs rewrite for MIR-based bytecode format"]
fn match_catch_all_named_binding() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                match (42) {
                    x => x + 1
                }
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Int(42)),
                // x is bound to scrutinee, then x + 1
                Instruction::LoadVar("@match_scrut_0".to_string()),
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(baml_vm::BinOp::Add),
                Instruction::PopReplace(1),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Literal Pattern Tests
// ============================================================================

#[test]
#[ignore = "needs rewrite for MIR-based bytecode format"]
fn match_literal_int_with_fallback() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                match (1) {
                    1 => 100,
                    _ => 0
                }
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Int(1)), // scrutinee
                Instruction::LoadVar("@match_scrut_0".to_string()),
                Instruction::LoadConst(Value::Int(1)), // literal 1
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(4), // skip to next arm
                Instruction::Pop(1),         // pop comparison result
                Instruction::LoadConst(Value::Int(100)), // arm body
                Instruction::Jump(3),        // jump to end
                Instruction::Pop(1),         // pop comparison result (false path)
                Instruction::LoadConst(Value::Int(0)), // catch-all body
                Instruction::Return,
            ],
        )],
    })
}

#[test]
#[ignore = "needs rewrite for MIR-based bytecode format"]
fn match_literal_bool_exhaustive() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function main() -> string {
                match (true) {
                    true => "yes",
                    false => "no"
                }
            }
        "#,
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Bool(true)), // scrutinee
                Instruction::LoadVar("@match_scrut_0".to_string()),
                Instruction::LoadConst(Value::Bool(true)), // literal true
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(4), // skip to next arm
                Instruction::Pop(1),
                Instruction::LoadConst(Value::string("yes")),
                Instruction::Jump(7), // jump to end
                Instruction::Pop(1),
                Instruction::LoadVar("@match_scrut_0".to_string()),
                Instruction::LoadConst(Value::Bool(false)), // literal false
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::Pop(1), // last arm, no jump needed
                Instruction::LoadConst(Value::string("no")),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
#[ignore = "needs rewrite for MIR-based bytecode format"]
fn match_literal_null() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function main() -> string {
                match (null) {
                    null => "nothing",
                    _ => "something"
                }
            }
        "#,
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Null), // scrutinee
                Instruction::LoadVar("@match_scrut_0".to_string()),
                Instruction::LoadConst(Value::Null), // literal null
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::string("nothing")),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::string("something")),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Typed Pattern Tests (instanceof)
// ============================================================================

#[test]
#[ignore = "needs rewrite for MIR-based bytecode format"]
fn match_typed_pattern_single_class() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            class Success {
                data string
            }

            function main() -> string {
                let result = Success { data: "hello" };
                match (result) {
                    s: Success => s.data,
                    _ => "unknown"
                }
            }
        "#,
        expected: vec![(
            "main",
            vec![
                // let result = Success { data: "hello" }
                Instruction::AllocInstance(Value::class("Success")),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::string("hello")),
                Instruction::StoreField(0),
                // match (result)
                Instruction::LoadVar("result".to_string()),
                // s: Success pattern - instanceof check
                Instruction::LoadVar("@match_scrut_0".to_string()),
                Instruction::LoadConst(Value::class("Success")),
                Instruction::CmpOp(CmpOp::InstanceOf),
                Instruction::JumpIfFalse(7), // skip to catch-all
                Instruction::Pop(1),
                // bind s and access s.data
                Instruction::LoadVar("@match_scrut_0".to_string()),
                Instruction::LoadVar("s".to_string()),
                Instruction::LoadField(0), // field 0 is 'data'
                Instruction::PopReplace(1),
                Instruction::Jump(3), // jump past catch-all
                Instruction::Pop(1),
                Instruction::LoadConst(Value::string("unknown")),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
#[ignore = "needs rewrite for MIR-based bytecode format"]
fn match_typed_pattern_two_classes() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            class Success {
                data string
            }

            class Failure {
                reason string
            }

            function main() -> string {
                let result = Success { data: "ok" };
                match (result) {
                    s: Success => s.data,
                    f: Failure => f.reason
                }
            }
        "#,
        expected: vec![(
            "main",
            vec![
                // let result = Success { data: "ok" }
                Instruction::AllocInstance(Value::class("Success")),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::string("ok")),
                Instruction::StoreField(0),
                // match (result)
                Instruction::LoadVar("result".to_string()),
                // s: Success
                Instruction::LoadVar("@match_scrut_0".to_string()),
                Instruction::LoadConst(Value::class("Success")),
                Instruction::CmpOp(CmpOp::InstanceOf),
                Instruction::JumpIfFalse(7),
                Instruction::Pop(1),
                Instruction::LoadVar("@match_scrut_0".to_string()),
                Instruction::LoadVar("s".to_string()),
                Instruction::LoadField(0),
                Instruction::PopReplace(1),
                Instruction::Jump(10), // jump past f: Failure arm
                // f: Failure
                Instruction::Pop(1),
                Instruction::LoadVar("@match_scrut_0".to_string()),
                Instruction::LoadConst(Value::class("Failure")),
                Instruction::CmpOp(CmpOp::InstanceOf),
                Instruction::Pop(1), // last arm
                Instruction::LoadVar("@match_scrut_0".to_string()),
                Instruction::LoadVar("f".to_string()),
                Instruction::LoadField(0),
                Instruction::PopReplace(1),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Union Literal Pattern Tests
// ============================================================================

#[test]
#[ignore = "needs rewrite for MIR-based bytecode format"]
fn match_union_literal_two_values() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function main() -> string {
                match (200) {
                    200 | 201 => "success",
                    _ => "other"
                }
            }
        "#,
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Int(200)), // scrutinee
                // Union pattern: 200 | 201
                Instruction::LoadVar("@match_scrut_0".to_string()),
                Instruction::LoadConst(Value::Int(200)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(3), // try 201
                Instruction::Pop(1),
                Instruction::Jump(7), // matched! jump to arm body
                Instruction::Pop(1),
                Instruction::LoadVar("@match_scrut_0".to_string()),
                Instruction::LoadConst(Value::Int(201)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(4), // neither matched, try next arm
                Instruction::Pop(1),
                Instruction::LoadConst(Value::string("success")),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::string("other")),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Match as Expression
// ============================================================================

#[test]
#[ignore = "needs rewrite for MIR-based bytecode format"]
fn match_in_arithmetic() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                1 + match (2) {
                    2 => 20,
                    _ => 0
                }
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Int(1)),
                // match expression
                Instruction::LoadConst(Value::Int(2)), // scrutinee
                Instruction::LoadVar("@match_scrut_0".to_string()),
                Instruction::LoadConst(Value::Int(2)), // literal 2
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(20)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(0)),
                // addition
                Instruction::BinOp(baml_vm::BinOp::Add),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Nested Match
// ============================================================================

#[test]
#[ignore = "needs rewrite for MIR-based bytecode format"]
fn match_nested() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                match (1) {
                    1 => match (2) {
                        2 => 12,
                        _ => 10
                    },
                    _ => 0
                }
            }
        ",
        expected: vec![(
            "main",
            vec![
                // Outer match scrutinee
                Instruction::LoadConst(Value::Int(1)),
                // outer: 1 pattern
                Instruction::LoadVar("@match_scrut_0".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(14), // skip to outer catch-all
                Instruction::Pop(1),
                // Inner match scrutinee
                Instruction::LoadConst(Value::Int(2)),
                // inner: 2 pattern
                Instruction::LoadVar("@match_scrut_1".to_string()),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(12)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(10)),
                // End of inner match - replace scrutinee on stack
                Instruction::PopReplace(1),
                Instruction::Jump(3), // skip outer catch-all
                // Outer catch-all
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::Return,
            ],
        )],
    })
}
