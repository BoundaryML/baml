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
            // Wildcard elimination: _ binding is unused so eliminated entirely
            // Scrutinee 42 is also unused, so no code for it
            vec![Instruction::LoadConst(Value::Int(100)), Instruction::Return],
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
            // Switch-based emission for integer literal match
            vec![
                // Scrutinee
                Instruction::LoadConst(Value::Int(1)),
                // Check if == 1
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(4), // jump to 1 => 100 arm
                // Catch-all arm
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::Jump(2), // skip to return
                // First arm: 1 => 100
                Instruction::LoadConst(Value::Int(100)),
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
            // Constant propagation: scrutinee true is inlined at comparison
            // Exhaustive match optimization: second arm's comparison is skipped
            // because else_block is unreachable (we know it must be false)
            vec![
                Instruction::LoadConst(Value::Bool(true)), // scrutinee (inlined)
                Instruction::LoadConst(Value::Bool(true)), // literal true
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2), // if false, skip to second arm body
                Instruction::Jump(4),           // if true, skip to "yes"
                // Second arm: no comparison needed (exhaustive match optimization)
                Instruction::Jump(1), // go directly to "no" body
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
            // Constant propagation: scrutinee null is inlined at each use
            // Wildcard elimination: _ binding is unused so eliminated
            vec![
                Instruction::LoadConst(Value::Null), // scrutinee (inlined for comparison)
                Instruction::LoadConst(Value::Null), // literal null
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2), // if false, skip to catch-all
                Instruction::Jump(3),           // if true, skip to "nothing"
                Instruction::LoadConst(Value::string("something")), // catch-all result (no _ binding)
                Instruction::Jump(2),                               // skip to return
                Instruction::LoadConst(Value::string("nothing")),   // first arm result
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
            // Wildcard elimination: _ binding is unused so eliminated
            // Scrutinee optimization: result is reused directly (no temp created)
            vec![
                Instruction::LoadConst(Value::Null), // slot for result
                // let result = Success { data: "hello" }
                Instruction::AllocInstance(Value::class("Success")),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::string("hello")),
                Instruction::StoreField(0),
                Instruction::StoreVar("result".to_string()),
                // instanceof check
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadConst(Value::class("Success")),
                Instruction::CmpOp(CmpOp::InstanceOf),
                Instruction::PopJumpIfFalse(2), // if false, skip to catch-all
                Instruction::Jump(3),           // if true, skip to s.data
                // catch-all arm (no _ binding)
                Instruction::LoadConst(Value::string("unknown")),
                Instruction::Jump(3), // skip to return
                // s: Success arm - access s.data (s is virtual, uses result directly)
                Instruction::LoadVar("result".to_string()),
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
            // Exhaustive match optimization: second arm's instanceof check is skipped
            // because else_block is unreachable (we know it must be Failure)
            // Scrutinee optimization: result is reused directly (no temp created)
            vec![
                Instruction::LoadConst(Value::Null), // slot for result
                // let result = Success { data: "ok" }
                Instruction::AllocInstance(Value::class("Success")),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::string("ok")),
                Instruction::StoreField(0),
                Instruction::StoreVar("result".to_string()),
                // s: Success instanceof check
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadConst(Value::class("Success")),
                Instruction::CmpOp(CmpOp::InstanceOf),
                Instruction::PopJumpIfFalse(2), // if false, skip to Failure arm body
                Instruction::Jump(5),           // if true, skip to s.data
                // f: Failure arm - no instanceof check needed (exhaustive match optimization)
                Instruction::Jump(1), // go directly to f.reason body
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadField(0),
                Instruction::Jump(3), // skip to return
                // s: Success arm - access s.data
                Instruction::LoadVar("result".to_string()),
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
            // Switch-based emission: union 200|201 creates two switch arms
            // pointing to the same target block
            vec![
                // Scrutinee
                Instruction::LoadConst(Value::Int(200)),
                // First part of union: check 200
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(200)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(10), // jump to "success" arm
                // Second part of union: check 201
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(201)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(4), // jump to "success" arm
                // Catch-all arm
                Instruction::Pop(1),
                Instruction::LoadConst(Value::string("other")),
                Instruction::Jump(2), // skip to return
                // Union arm result (200 | 201 => "success")
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
            // Switch-based emission for integer literal match in expression
            vec![
                // Scrutinee
                Instruction::LoadConst(Value::Int(2)),
                // Check if == 2
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(4), // jump to 2 => 20 arm
                // Catch-all arm
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::Jump(2), // skip to addition
                // First arm: 2 => 20
                Instruction::LoadConst(Value::Int(20)),
                // Addition: 1 + match result
                Instruction::LoadConst(Value::Int(1)),
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
            // Switch-based emission for nested integer literal matches
            vec![
                // Outer match scrutinee
                Instruction::LoadConst(Value::Int(1)),
                // Check if == 1
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(4), // jump to inner match
                // Outer catch-all
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::Jump(12), // skip to return
                // Inner match scrutinee (arm 1 => ...)
                Instruction::LoadConst(Value::Int(2)),
                // Check if == 2
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(4), // jump to 12 arm
                // Inner catch-all
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(10)),
                Instruction::Jump(2), // skip to return
                // Inner arm 2 => 12
                Instruction::LoadConst(Value::Int(12)),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Jump Table Tests (4+ dense arms)
// ============================================================================

/// Tests that a match with 4 dense consecutive integer arms uses a jump table.
/// With 4 arms covering values 0-3 (100% density), the codegen should emit
/// a `JumpTable` instruction instead of a linear if-else chain.
#[test]
fn match_jump_table_dense_four_arms() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function classify(x int) -> int {
                match (x) {
                    0 => 100,
                    1 => 101,
                    2 => 102,
                    3 => 103,
                    _ => 999
                }
            }
        ",
        expected: vec![(
            "classify",
            vec![
                // Load discriminant (argument x)
                Instruction::LoadVar("x".to_string()),
                // JumpTable with table_idx=0, default offset jumps to wildcard arm
                Instruction::JumpTable {
                    table_idx: 0,
                    default: 1, // jumps to next instruction (wildcard arm)
                },
                // Block for wildcard arm _ => 999 (default target)
                Instruction::LoadConst(Value::Int(999)),
                Instruction::Jump(8), // jump to return
                // Block for arm 3 => 103
                Instruction::LoadConst(Value::Int(103)),
                Instruction::Jump(6), // jump to return
                // Block for arm 2 => 102
                Instruction::LoadConst(Value::Int(102)),
                Instruction::Jump(4), // jump to return
                // Block for arm 1 => 101
                Instruction::LoadConst(Value::Int(101)),
                Instruction::Jump(2), // jump to return
                // Block for arm 0 => 100
                Instruction::LoadConst(Value::Int(100)),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Binary Search Tests (4+ sparse arms)
// ============================================================================

/// Tests that a match with 4 sparse integer arms uses binary search.
/// With 4 arms spread over a range of 100 (4% density), the codegen should
/// emit a binary search tree instead of a linear chain or jump table.
#[test]
fn match_binary_search_sparse_four_arms() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function classify(x int) -> int {
                match (x) {
                    0 => 100,
                    30 => 130,
                    60 => 160,
                    99 => 199,
                    _ => 999
                }
            }
        ",
        expected: vec![(
            "classify",
            // Binary search emits a tree of comparisons:
            // - Check pivot (middle value)
            // - If less, check left subtree
            // - If greater, check right subtree
            vec![
                // Load discriminant
                Instruction::LoadVar("x".to_string()),
                // Binary search tree: pivot = 60 (mid of sorted [0, 30, 60, 99])

                // Compare with pivot (60)
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(60)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(28), // jump to arm 60 => 160
                // Compare < pivot for left subtree
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(60)),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::PopJumpIfFalse(13), // if >= 60, check right subtree
                // Left subtree: check 0
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(22), // jump to arm 0 => 100
                // Check 30
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(30)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(14), // jump to arm 30 => 130
                // Right subtree: check 99
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(99)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(4), // jump to arm 99 => 199
                // Fall through to catch-all
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(999)), // catch-all arm
                Instruction::Jump(8),
                // Arm bodies (emitted in reverse order: 99, 60, 30, 0)
                Instruction::LoadConst(Value::Int(199)), // 99 => 199
                Instruction::Jump(6),
                Instruction::LoadConst(Value::Int(160)), // 60 => 160
                Instruction::Jump(4),
                Instruction::LoadConst(Value::Int(130)), // 30 => 130
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(100)), // 0 => 100
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// If-Else Chain Tests (< 4 arms)
// ============================================================================

/// Tests that a match with fewer than 4 arms uses if-else chain.
/// Note: Even small matches with integer literals now use the Switch terminator
/// which produces a different (but correct) bytecode pattern.
#[test]
fn match_if_else_chain_three_arms() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function classify(x int) -> int {
                match (x) {
                    0 => 100,
                    1 => 101,
                    _ => 999
                }
            }
        ",
        expected: vec![(
            "classify",
            // Switch-based emission with Copy/LoadConst/CmpOp pattern
            vec![
                // Load discriminant
                Instruction::LoadVar("x".to_string()),
                // Check first arm (0)
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(12), // jump to arm 0 => 100
                // Check second arm (1)
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(4), // jump to arm 1 => 101
                // Fall through to catch-all
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(999)), // catch-all
                Instruction::Jump(4),
                // Arm bodies (reverse order: 1 then 0)
                Instruction::LoadConst(Value::Int(101)), // 1 => 101
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(100)), // 0 => 100
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// String Literal Tests (should NOT use jump table)
// ============================================================================

/// String patterns should NOT use jump table (would need perfect hashing).
/// They should fall back to if-else chain.
#[test]
fn match_string_literal() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function classify(s string) -> int {
                match (s) {
                    "hello" => 100,
                    "world" => 200,
                    _ => 0
                }
            }
        "#,
        expected: vec![(
            "classify",
            // Should use if-else chain, not jump table
            vec![
                Instruction::LoadVar("s".to_string()),
                Instruction::LoadConst(Value::string("hello")),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(10), // jump to first arm (100)
                Instruction::LoadVar("s".to_string()),
                Instruction::LoadConst(Value::string("world")),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3), // jump to second arm (200)
                Instruction::LoadConst(Value::Int(0)), // catch-all arm
                Instruction::Jump(4),
                Instruction::LoadConst(Value::Int(200)), // second arm
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(100)), // first arm
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Guards with Integer Literals (should prevent switch optimization)
// ============================================================================

/// Guards on any arm prevent the Switch optimization entirely.
/// The whole match falls back to if-else chain with guard evaluation.
#[test]
fn match_guarded_int_literal() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function classify(x int, flag bool) -> string {
                match (x) {
                    1 if flag => "one with flag",
                    1 => "one",
                    _ => "other"
                }
            }
        "#,
        expected: vec![(
            "classify",
            // The presence of a guard prevents Switch optimization
            // Falls back to Branch-based if-else chain
            vec![
                // First arm: 1 if flag
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(4), // if x != 1, skip to next arm
                Instruction::LoadVar("flag".to_string()), // guard check
                Instruction::PopJumpIfFalse(2), // if guard false, skip to next arm
                Instruction::Jump(10),          // guard passed, jump to body "one with flag"
                // Second arm: unguarded 1
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2), // if x != 1, jump to catch-all
                Instruction::Jump(3),           // jump to body "one"
                // Catch-all body
                Instruction::LoadConst(Value::string("other")),
                Instruction::Jump(4),
                // Body for unguarded 1
                Instruction::LoadConst(Value::string("one")),
                Instruction::Jump(2),
                // Body for guarded 1
                Instruction::LoadConst(Value::string("one with flag")),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Density Threshold Tests
// ============================================================================

/// At exactly 50% density (4 arms in range of 8), should use jump table.
#[test]
fn match_density_50_percent_uses_jump_table() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function classify(x int) -> int {
                match (x) {
                    0 => 100,
                    2 => 102,
                    4 => 104,
                    6 => 106,
                    _ => 999
                }
            }
        ",
        expected: vec![(
            "classify",
            // 50% density triggers jump table
            vec![
                Instruction::LoadVar("x".to_string()),
                Instruction::JumpTable {
                    table_idx: 0,
                    default: 1,
                },
                Instruction::LoadConst(Value::Int(999)),
                Instruction::Jump(8),
                Instruction::LoadConst(Value::Int(106)),
                Instruction::Jump(6),
                Instruction::LoadConst(Value::Int(104)),
                Instruction::Jump(4),
                Instruction::LoadConst(Value::Int(102)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(100)),
                Instruction::Return,
            ],
        )],
    })
}

/// Below 50% density (4 arms in range of 10), should use binary search.
#[test]
fn match_density_40_percent_uses_binary_search() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function classify(x int) -> int {
                match (x) {
                    0 => 100,
                    3 => 103,
                    6 => 106,
                    9 => 109,
                    _ => 999
                }
            }
        ",
        expected: vec![(
            "classify",
            // 40% density triggers binary search (not jump table)
            // Just verify it starts with LoadVar and uses Copy (binary search pattern)
            vec![
                Instruction::LoadVar("x".to_string()),
                // Binary search uses Copy for comparisons
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(6)), // pivot
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(28),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(6)),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::PopJumpIfFalse(13),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(22),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(14),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(9)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(999)),
                Instruction::Jump(8),
                Instruction::LoadConst(Value::Int(109)),
                Instruction::Jump(6),
                Instruction::LoadConst(Value::Int(106)),
                Instruction::Jump(4),
                Instruction::LoadConst(Value::Int(103)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(100)),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Large Offset Values Tests
// ============================================================================

/// Dense values with large offset should use jump table.
#[test]
fn match_large_offset_values_dense() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function classify(x int) -> int {
                match (x) {
                    100 => 1000,
                    101 => 1001,
                    102 => 1002,
                    103 => 1003,
                    _ => 9999
                }
            }
        ",
        expected: vec![(
            "classify",
            // Dense values with offset use jump table
            vec![
                Instruction::LoadVar("x".to_string()),
                Instruction::JumpTable {
                    table_idx: 0,
                    default: 1,
                },
                Instruction::LoadConst(Value::Int(9999)),
                Instruction::Jump(8),
                Instruction::LoadConst(Value::Int(1003)),
                Instruction::Jump(6),
                Instruction::LoadConst(Value::Int(1002)),
                Instruction::Jump(4),
                Instruction::LoadConst(Value::Int(1001)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(1000)),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Catch-All Binding Tests
// ============================================================================

/// Named catch-all binding should work with integer patterns.
#[test]
fn match_catch_all_binding_with_int_patterns() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function classify(x int) -> int {
                match (x) {
                    0 => 0,
                    1 => 1,
                    2 => 2,
                    3 => 3,
                    other => other * 10
                }
            }
        ",
        expected: vec![(
            "classify",
            // Jump table with named binding in catch-all
            vec![
                Instruction::LoadVar("x".to_string()),
                Instruction::JumpTable {
                    table_idx: 0,
                    default: 1,
                },
                // Catch-all arm with binding: other => other * 10
                // 'other' binds to x, then other * 10
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(10)),
                Instruction::BinOp(baml_vm::BinOp::Mul),
                Instruction::Jump(8),
                // Arms in reverse order
                Instruction::LoadConst(Value::Int(3)),
                Instruction::Jump(6),
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

// ============================================================================
// Negative Literal Pattern Tests
// ============================================================================

/// Negative integer patterns are parsed correctly and generate proper bytecode.
/// Uses if-else chain because there are only 3 integer patterns (< 4 threshold).
#[test]
fn match_negative_int_pattern() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function classify(x int) -> string {
                match (x) {
                    -1 => "negative one",
                    0 => "zero",
                    1 => "one",
                    _ => "other"
                }
            }
        "#,
        expected: vec![(
            "classify",
            vec![
                // Scrutinee
                Instruction::LoadVar("x".to_string()),
                // First arm: -1
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(-1)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(20),
                // Second arm: 0
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(12),
                // Third arm: 1
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(4),
                // Catch-all
                Instruction::Pop(1),
                Instruction::LoadConst(Value::string("other")),
                Instruction::Jump(6),
                // Body for 1
                Instruction::LoadConst(Value::string("one")),
                Instruction::Jump(4),
                // Body for 0
                Instruction::LoadConst(Value::string("zero")),
                Instruction::Jump(2),
                // Body for -1
                Instruction::LoadConst(Value::string("negative one")),
                Instruction::Return,
            ],
        )],
    })
}

/// Multiple negative patterns in a match expression.
/// Uses if-else chain because there are only 2 integer patterns (< 4 threshold).
#[test]
fn match_multiple_negative_patterns() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function classify(x int) -> string {
                match (x) {
                    -2 => "negative two",
                    -1 => "negative one",
                    _ => "other"
                }
            }
        "#,
        expected: vec![(
            "classify",
            vec![
                // Scrutinee
                Instruction::LoadVar("x".to_string()),
                // First arm: -2
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(-2)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(12),
                // Second arm: -1
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(-1)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(4),
                // Catch-all
                Instruction::Pop(1),
                Instruction::LoadConst(Value::string("other")),
                Instruction::Jump(4),
                // Body for -1
                Instruction::LoadConst(Value::string("negative one")),
                Instruction::Jump(2),
                // Body for -2
                Instruction::LoadConst(Value::string("negative two")),
                Instruction::Return,
            ],
        )],
    })
}
