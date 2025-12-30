//! Compiler tests for match expressions.

use baml_tests::{
    codegen::{Program, assert_compiles},
    vm::{Instruction, Value},
};
use baml_vm::CmpOp;

// ============================================================================
// Basic Catch-All Tests
// ============================================================================

#[test]
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
            // Scrutinee stored to _ (wasteful for wildcard, but correct)
            vec![
                Instruction::LoadConst(Value::Null), // slot for _
                Instruction::LoadConst(Value::Int(42)),
                Instruction::StoreVar("_".to_string()),
                Instruction::LoadConst(Value::Int(100)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
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
            // x binds to 42, used once in x+1 -> optimizer inlines to 42+1
            vec![
                Instruction::LoadConst(Value::Int(42)),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(baml_vm::BinOp::Add),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Literal Pattern Tests
// ============================================================================

#[test]
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
                Instruction::LoadConst(Value::Null),   // slot for _
                Instruction::LoadConst(Value::Null),   // slot for _1 (scrutinee)
                Instruction::LoadConst(Value::Int(1)), // scrutinee value
                Instruction::StoreVar("_1".to_string()),
                Instruction::LoadVar("_1".to_string()), // load for comparison
                Instruction::LoadConst(Value::Int(1)),  // literal 1
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2), // if false, skip to catch-all
                Instruction::Jump(5),           // if true, skip to arm body (100)
                Instruction::LoadVar("_1".to_string()), // catch-all: load scrutinee
                Instruction::StoreVar("_".to_string()), // bind to _
                Instruction::LoadConst(Value::Int(0)), // catch-all result
                Instruction::Jump(2),           // skip to return
                Instruction::LoadConst(Value::Int(100)), // first arm result
                Instruction::Return,
            ],
        )],
    })
}

#[test]
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
                Instruction::LoadConst(Value::Null), // slot for _1 (scrutinee)
                Instruction::LoadConst(Value::Bool(true)), // scrutinee value
                Instruction::StoreVar("_1".to_string()),
                Instruction::LoadVar("_1".to_string()), // load for first comparison
                Instruction::LoadConst(Value::Bool(true)), // literal true
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2), // if false, try next arm
                Instruction::Jump(9),           // if true, skip to "yes"
                Instruction::LoadVar("_1".to_string()), // load for second comparison
                Instruction::LoadConst(Value::Bool(false)), // literal false
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(6), // if false (shouldn't happen)
                Instruction::Jump(2),           // if true, skip to "no"
                Instruction::Jump(4),           // unreachable fallthrough
                Instruction::LoadConst(Value::string("no")),
                Instruction::Jump(2), // skip to return
                Instruction::LoadConst(Value::string("yes")),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
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
                Instruction::LoadConst(Value::Null), // slot for _
                Instruction::LoadConst(Value::Null), // slot for _1 (scrutinee)
                Instruction::LoadConst(Value::Null), // scrutinee value
                Instruction::StoreVar("_1".to_string()),
                Instruction::LoadVar("_1".to_string()), // load for comparison
                Instruction::LoadConst(Value::Null),    // literal null
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2), // if false, skip to catch-all
                Instruction::Jump(5),           // if true, skip to "nothing"
                Instruction::LoadVar("_1".to_string()), // catch-all: load scrutinee
                Instruction::StoreVar("_".to_string()), // bind to _
                Instruction::LoadConst(Value::string("something")), // catch-all result
                Instruction::Jump(2),           // skip to return
                Instruction::LoadConst(Value::string("nothing")), // first arm result
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Typed Pattern Tests (instanceof)
// ============================================================================

#[test]
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
                Instruction::LoadConst(Value::Null), // slot for _
                Instruction::LoadConst(Value::Null), // slot for _3 (scrutinee)
                // let result = Success { data: "hello" }
                Instruction::AllocInstance(Value::class("Success")),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::string("hello")),
                Instruction::StoreField(0),
                Instruction::StoreVar("_3".to_string()),
                // instanceof check
                Instruction::LoadVar("_3".to_string()),
                Instruction::LoadConst(Value::class("Success")),
                Instruction::CmpOp(CmpOp::InstanceOf),
                Instruction::PopJumpIfFalse(2), // if false, skip to catch-all
                Instruction::Jump(5),           // if true, skip to s.data
                // catch-all arm
                Instruction::LoadVar("_3".to_string()),
                Instruction::StoreVar("_".to_string()),
                Instruction::LoadConst(Value::string("unknown")),
                Instruction::Jump(3), // skip to return
                // s: Success arm - access s.data (s is virtual, so use _3 directly)
                Instruction::LoadVar("_3".to_string()),
                Instruction::LoadField(0),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
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
                Instruction::LoadConst(Value::Null), // slot for _3 (scrutinee)
                // let result = Success { data: "ok" }
                Instruction::AllocInstance(Value::class("Success")),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::string("ok")),
                Instruction::StoreField(0),
                Instruction::StoreVar("_3".to_string()),
                // s: Success instanceof check
                Instruction::LoadVar("_3".to_string()),
                Instruction::LoadConst(Value::class("Success")),
                Instruction::CmpOp(CmpOp::InstanceOf),
                Instruction::PopJumpIfFalse(2), // if false, try Failure
                Instruction::Jump(10),          // if true, skip to s.data
                // f: Failure instanceof check
                Instruction::LoadVar("_3".to_string()),
                Instruction::LoadConst(Value::class("Failure")),
                Instruction::CmpOp(CmpOp::InstanceOf),
                Instruction::PopJumpIfFalse(8), // if false (shouldn't happen)
                Instruction::Jump(2),           // if true, skip to f.reason
                Instruction::Jump(6),           // unreachable fallthrough
                // f: Failure arm - access f.reason
                Instruction::LoadVar("_3".to_string()),
                Instruction::LoadField(0),
                Instruction::Jump(3), // skip to return
                // s: Success arm - access s.data
                Instruction::LoadVar("_3".to_string()),
                Instruction::LoadField(0),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Union Literal Pattern Tests
// ============================================================================

#[test]
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
                Instruction::LoadConst(Value::Null),     // slot for _
                Instruction::LoadConst(Value::Null),     // slot for _1 (scrutinee)
                Instruction::LoadConst(Value::Int(200)), // scrutinee value
                Instruction::StoreVar("_1".to_string()),
                // First part of union: 200
                Instruction::LoadVar("_1".to_string()),
                Instruction::LoadConst(Value::Int(200)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2), // if false, try 201
                Instruction::Jump(10),          // if true, skip to "success"
                // Second part of union: 201
                Instruction::LoadVar("_1".to_string()),
                Instruction::LoadConst(Value::Int(201)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2), // if false, try catch-all
                Instruction::Jump(5),           // if true, skip to "success"
                // Catch-all arm
                Instruction::LoadVar("_1".to_string()),
                Instruction::StoreVar("_".to_string()),
                Instruction::LoadConst(Value::string("other")),
                Instruction::Jump(2), // skip to return
                // Union arm result
                Instruction::LoadConst(Value::string("success")),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Match as Expression
// ============================================================================

#[test]
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
                Instruction::LoadConst(Value::Null),   // slot for _
                Instruction::LoadConst(Value::Null),   // slot for _2 (match result)
                Instruction::LoadConst(Value::Null),   // slot for _3 (scrutinee)
                Instruction::LoadConst(Value::Int(2)), // scrutinee value
                Instruction::StoreVar("_3".to_string()),
                Instruction::LoadVar("_3".to_string()), // load for comparison
                Instruction::LoadConst(Value::Int(2)),  // literal 2
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2), // if false, skip to catch-all
                Instruction::Jump(6),           // if true, skip to 20
                // Catch-all arm
                Instruction::LoadVar("_3".to_string()),
                Instruction::StoreVar("_".to_string()),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::StoreVar("_2".to_string()), // store match result
                Instruction::Jump(3),                    // skip to addition
                // First arm
                Instruction::LoadConst(Value::Int(20)),
                Instruction::StoreVar("_2".to_string()), // store match result
                // Addition: 1 + match result
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadVar("_2".to_string()),
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
                Instruction::LoadConst(Value::Null), // slot for outer _
                Instruction::LoadConst(Value::Null), // slot for _1 (outer scrutinee)
                Instruction::LoadConst(Value::Null), // slot for inner _
                Instruction::LoadConst(Value::Null), // slot for _3 (inner scrutinee)
                // Outer scrutinee
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("_1".to_string()),
                Instruction::LoadVar("_1".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2), // if outer != 1, skip to outer catch-all
                Instruction::Jump(5),           // if outer == 1, skip to inner match
                // Outer catch-all arm
                Instruction::LoadVar("_1".to_string()),
                Instruction::StoreVar("_".to_string()),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::Jump(13), // skip to return
                // Inner match scrutinee
                Instruction::LoadConst(Value::Int(2)),
                Instruction::StoreVar("_3".to_string()),
                Instruction::LoadVar("_3".to_string()),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2), // if inner != 2, skip to inner catch-all
                Instruction::Jump(5),           // if inner == 2, skip to 12
                // Inner catch-all arm
                Instruction::LoadVar("_3".to_string()),
                Instruction::StoreVar("_".to_string()),
                Instruction::LoadConst(Value::Int(10)),
                Instruction::Jump(2), // skip to return
                // Inner first arm
                Instruction::LoadConst(Value::Int(12)),
                Instruction::Return,
            ],
        )],
    })
}
