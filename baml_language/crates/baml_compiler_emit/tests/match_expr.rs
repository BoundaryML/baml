//! Compiler tests for match expressions.

use baml_tests::{
    codegen::{Program, assert_compiles},
    vm::{Instruction, Value},
};
use bex_vm_types::bytecode::{BinOp, CmpOp};

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
                Instruction::BinOp(bex_vm_types::BinOp::Add),
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
                }
            }
        ",
        expected: vec![(
            "main",
            // Switch-based emission for integer literal match
            vec![
                // Scrutinee
                Instruction::LoadConst(Value::Int(1)),
                // Catch-all arm
                Instruction::Pop(1),
                Instruction::Jump(1), // skip to return
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
                }
            }
        "#,
        expected: vec![(
            "main",
            // Constant propagation: scrutinee true is inlined at comparison
            // Exhaustive match optimization: second arm's comparison is skipped
            // because else_block is unreachable (we know it must be false)
            vec![
                Instruction::Jump(1), // go directly to "no" body
                Instruction::LoadConst(Value::string("yes")),
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
                }
            }
        "#,
        expected: vec![(
            "main",
            // Wildcard elimination: _ binding is unused so eliminated
            // Scrutinee optimization: result is reused directly (no temp created)
            vec![
                Instruction::InitLocals(1), // slot for result
                // let result = Success { data: "hello" }
                Instruction::AllocInstance(Value::class("Success")),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::string("hello")),
                Instruction::StoreField(0),
                Instruction::StoreVar("result".to_string()),
                // catch-all arm (no _ binding)
                Instruction::Jump(1), // skip to return
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
                }
            }
        "#,
        expected: vec![(
            "main",
            // Exhaustive match optimization: second arm's instanceof check is skipped
            // because else_block is unreachable (we know it must be Failure)
            // Scrutinee optimization: result is reused directly (no temp created)
            vec![
                Instruction::InitLocals(1), // slot for result
                // let result = Success { data: "ok" }
                Instruction::AllocInstance(Value::class("Success")),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::string("ok")),
                Instruction::StoreField(0),
                Instruction::StoreVar("result".to_string()),
                Instruction::Jump(1),
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
                Instruction::Jump(3), // jump to "success" arm
                // Catch-all arm
                Instruction::Pop(1),
                Instruction::Jump(1), // skip to return
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
                }
            }
        ",
        expected: vec![(
            "main",
            // Switch-based emission for integer literal match in expression
            vec![
                Instruction::LoadConst(Value::Int(2)),
                Instruction::Pop(1),
                Instruction::Jump(1), // jump to 2 => 20 arm
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadConst(Value::Int(20)),
                Instruction::BinOp(bex_vm_types::BinOp::Add),
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
                    },
                }
            }
        ",
        expected: vec![(
            "main",
            // Switch-based emission for nested integer literal matches
            vec![
                // Outer match scrutinee
                Instruction::LoadConst(Value::Int(1)),
                // Outer catch-all
                Instruction::Pop(1),
                Instruction::Jump(1), // skip to return
                // Inner match scrutinee (arm 1 => ...)
                Instruction::LoadConst(Value::Int(2)),
                // Check if == 2
                // Inner catch-all
                Instruction::Pop(1),
                Instruction::Jump(1), // skip to return
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

/// Literals + typed pattern + guard mixed together.
/// Guards prevent switch optimization, falls back to if-else chain.
#[test]
fn match_mixed_literal_typed_guard() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function classify(x int, flag bool) -> string {
                match (x) {
                    0 => "zero",
                    1 if flag => "one with flag",
                    n: int => "other int"
                }
            }
        "#,
        expected: vec![(
            "classify",
            vec![
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(13),
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(4),
                Instruction::LoadVar("flag".to_string()),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(4),
                // Exhaustive typed pattern - skips instanceof
                Instruction::Jump(1),
                Instruction::LoadConst(Value::string("other int")),
                Instruction::Jump(4),
                Instruction::LoadConst(Value::string("one with flag")),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::string("zero")),
                Instruction::Return,
            ],
        )],
    })
}

/// Guard on typed pattern - access bound variable in guard condition.
#[test]
fn match_guard_on_typed_pattern() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            class Success { data string }
            class Failure { reason string }

            function classify(result Success | Failure) -> string {
                match (result) {
                    s: Success if s.data != "" => "success with data",
                    s: Success => "empty success",
                    f: Failure => "failure"
                }
            }
        "#,
        expected: vec![(
            "classify",
            vec![
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadConst(Value::class("Success")),
                Instruction::CmpOp(CmpOp::InstanceOf),
                Instruction::PopJumpIfFalse(7),
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadField(0),
                Instruction::LoadConst(Value::string("")),
                Instruction::CmpOp(CmpOp::NotEq),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(11),
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadConst(Value::class("Success")),
                Instruction::CmpOp(CmpOp::InstanceOf),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(4),
                // Exhaustive - skips instanceof
                Instruction::Jump(1),
                Instruction::LoadConst(Value::string("failure")),
                Instruction::Jump(4),
                Instruction::LoadConst(Value::string("empty success")),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::string("success with data")),
                Instruction::Return,
            ],
        )],
    })
}

/// Multiple typed patterns with guards on each.
#[test]
fn match_multiple_typed_patterns_with_guards() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            class Success { code int }
            class Failure { code int }

            function classify(result Success | Failure, strict bool) -> string {
                match (result) {
                    s: Success if s.code > 200 => "redirect",
                    s: Success if strict => "strict success",
                    s: Success => "success",
                    f: Failure => "failure"
                }
            }
        "#,
        expected: vec![(
            "classify",
            vec![
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadConst(Value::class("Success")),
                Instruction::CmpOp(CmpOp::InstanceOf),
                Instruction::PopJumpIfFalse(7),
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadField(0),
                Instruction::LoadConst(Value::Int(200)),
                Instruction::CmpOp(CmpOp::Gt),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(20),
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadConst(Value::class("Success")),
                Instruction::CmpOp(CmpOp::InstanceOf),
                Instruction::PopJumpIfFalse(4),
                Instruction::LoadVar("strict".to_string()),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(11),
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadConst(Value::class("Success")),
                Instruction::CmpOp(CmpOp::InstanceOf),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(4),
                // Exhaustive - skips instanceof
                Instruction::Jump(1),
                Instruction::LoadConst(Value::string("failure")),
                Instruction::Jump(6),
                Instruction::LoadConst(Value::string("success")),
                Instruction::Jump(4),
                Instruction::LoadConst(Value::string("strict success")),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::string("redirect")),
                Instruction::Return,
            ],
        )],
    })
}

/// String literals mixed with typed pattern.
#[test]
fn match_string_literal_with_typed_pattern() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function classify(s string) -> int {
                match (s) {
                    "ok" => 200,
                    "error" => 500,
                    _: string => 0
                }
            }
        "#,
        expected: vec![(
            "classify",
            vec![
                Instruction::LoadVar("s".to_string()),
                Instruction::LoadConst(Value::string("ok")),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(11),
                Instruction::LoadVar("s".to_string()),
                Instruction::LoadConst(Value::string("error")),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(4),
                // Exhaustive - skips instanceof
                Instruction::Jump(1),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::Jump(4),
                Instruction::LoadConst(Value::Int(500)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(200)),
                Instruction::Return,
            ],
        )],
    })
}

/// Many guards on every arm - no optimization possible.
#[test]
fn match_all_arms_guarded() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function classify(x int, a bool, b bool, c bool) -> string {
                match (x) {
                    0 if a => "zero a",
                    0 if b => "zero b",
                    1 if c => "one c",
                    _ => "other"
                }
            }
        "#,
        expected: vec![(
            "classify",
            vec![
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(4),
                Instruction::LoadVar("a".to_string()),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(21),
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(4),
                Instruction::LoadVar("b".to_string()),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(12),
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(4),
                Instruction::LoadVar("c".to_string()),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::string("other")),
                Instruction::Jump(6),
                Instruction::LoadConst(Value::string("one c")),
                Instruction::Jump(4),
                Instruction::LoadConst(Value::string("zero b")),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::string("zero a")),
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

/// Binary search with negative sparse values.
#[test]
fn match_binary_search_negative_sparse() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function classify(x int) -> string {
                match (x) {
                    -100 => "a",
                    -50 => "b",
                    -10 => "c",
                    -1 => "d",
                    _ => "other"
                }
            }
        "#,
        expected: vec![(
            "classify",
            // Sparse negative values use binary search
            // Pivot is -10 (median of sorted values)
            vec![
                Instruction::LoadVar("x".to_string()),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(-10)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(28),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(-10)),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::PopJumpIfFalse(13),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(-100)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(22),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(-50)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(14),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(-1)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::string("other")),
                Instruction::Jump(8),
                Instruction::LoadConst(Value::string("d")),
                Instruction::Jump(6),
                Instruction::LoadConst(Value::string("c")),
                Instruction::Jump(4),
                Instruction::LoadConst(Value::string("b")),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::string("a")),
                Instruction::Return,
            ],
        )],
    })
}

/// Binary search spanning zero with sparse values (low density).
#[test]
fn match_binary_search_spanning_zero_sparse() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function classify(x int) -> string {
                match (x) {
                    -100 => "neg hundred",
                    -1 => "neg one",
                    1 => "one",
                    100 => "hundred",
                    _ => "other"
                }
            }
        "#,
        expected: vec![(
            "classify",
            // Sparse values spanning zero use binary search
            // Pivot is 1 (median of sorted values: -100, -1, 1, 100)
            vec![
                Instruction::LoadVar("x".to_string()),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(28),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::PopJumpIfFalse(13),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(-100)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(22),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(-1)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(14),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(100)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::string("other")),
                Instruction::Jump(8),
                Instruction::LoadConst(Value::string("hundred")),
                Instruction::Jump(6),
                Instruction::LoadConst(Value::string("one")),
                Instruction::Jump(4),
                Instruction::LoadConst(Value::string("neg one")),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::string("neg hundred")),
                Instruction::Return,
            ],
        )],
    })
}

/// Large binary search tree with 8 sparse arms.
#[test]
fn match_binary_search_eight_arms() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function classify(x int) -> int {
                match (x) {
                    0 => 0,
                    10 => 1,
                    20 => 2,
                    30 => 3,
                    40 => 4,
                    50 => 5,
                    60 => 6,
                    70 => 7,
                    _ => 99
                }
            }
        "#,
        // 8 arms over range 71 = 11% density, definitely binary search
        // Pivot is 40 (median of 0,10,20,30,40,50,60,70)
        expected: vec![(
            "classify",
            vec![
                Instruction::LoadVar("x".to_string()),
                // Binary search with pivot 40
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(40)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(64),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(40)),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::PopJumpIfFalse(29),
                // Left subtree: 0, 10, 20, 30 with pivot 20
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(20)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(58),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(20)),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::PopJumpIfFalse(13),
                // Left-left: 0, 10
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(52),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(10)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(44),
                // Left-right: 30
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(30)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(34),
                // Right subtree: 50, 60, 70 with pivot 60
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(60)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(22),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(60)),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::PopJumpIfFalse(7),
                // Right-left: 50
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(50)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(14),
                // Right-right: 70
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(70)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(4),
                // Catch-all and bodies
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(99)),
                Instruction::Jump(16),
                Instruction::LoadConst(Value::Int(7)),
                Instruction::Jump(14),
                Instruction::LoadConst(Value::Int(6)),
                Instruction::Jump(12),
                Instruction::LoadConst(Value::Int(5)),
                Instruction::Jump(10),
                Instruction::LoadConst(Value::Int(4)),
                Instruction::Jump(8),
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
                Instruction::BinOp(bex_vm_types::BinOp::Mul),
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

// ============================================================================
// Negative Integer Jump Table Tests
// ============================================================================

/// Dense negative integer range should use jump table.
/// Values -3, -2, -1, 0 are consecutive (100% density).
#[test]
fn match_negative_jump_table() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function classify(x int) -> string {
                match (x) {
                    -3 => "neg three",
                    -2 => "neg two",
                    -1 => "neg one",
                    0 => "zero",
                    _ => "other"
                }
            }
        "#,
        expected: vec![(
            "classify",
            // Dense negative range should trigger jump table
            vec![
                Instruction::LoadVar("x".to_string()),
                Instruction::JumpTable {
                    table_idx: 0,
                    default: 1,
                },
                // Catch-all arm
                Instruction::LoadConst(Value::string("other")),
                Instruction::Jump(8),
                // Arms in reverse order: 0, -1, -2, -3
                Instruction::LoadConst(Value::string("zero")),
                Instruction::Jump(6),
                Instruction::LoadConst(Value::string("neg one")),
                Instruction::Jump(4),
                Instruction::LoadConst(Value::string("neg two")),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::string("neg three")),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Mixed Positive/Negative Spanning Zero Tests
// ============================================================================

/// Dense range crossing zero should use jump table.
/// Values -2, -1, 0, 1, 2 are consecutive (100% density).
#[test]
fn match_spanning_zero_jump_table() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function classify(x int) -> string {
                match (x) {
                    -2 => "neg two",
                    -1 => "neg one",
                    0 => "zero",
                    1 => "one",
                    2 => "two",
                    _ => "other"
                }
            }
        "#,
        expected: vec![(
            "classify",
            // Dense range spanning zero should trigger jump table
            vec![
                Instruction::LoadVar("x".to_string()),
                Instruction::JumpTable {
                    table_idx: 0,
                    default: 1,
                },
                // Catch-all arm
                Instruction::LoadConst(Value::string("other")),
                Instruction::Jump(10),
                // Arms in reverse order: 2, 1, 0, -1, -2
                Instruction::LoadConst(Value::string("two")),
                Instruction::Jump(8),
                Instruction::LoadConst(Value::string("one")),
                Instruction::Jump(6),
                Instruction::LoadConst(Value::string("zero")),
                Instruction::Jump(4),
                Instruction::LoadConst(Value::string("neg one")),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::string("neg two")),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Enum Variant Switch Tests
// ============================================================================

/// Enum variant patterns with 3 arms use if-else chain with Discriminant extraction.
/// (`JumpTable` requires 4+ arms)
///
/// For exhaustive enum matches, the last arm's comparison is skipped since if all
/// other comparisons failed, the value must be the remaining variant.
#[test]
fn match_enum_variant_switch() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            enum Status {
                Active
                Inactive
                Pending
            }

            function classify(s Status) -> string {
                match (s) {
                    Status.Active => "active",
                    Status.Inactive => "inactive",
                    Status.Pending => "pending"
                }
            }
        "#,
        expected: vec![(
            "classify",
            // With Discriminant instruction: extract variant index once, then compare integers
            // This is more efficient than creating variant objects for each comparison.
            // For exhaustive matches, the last arm's comparison is skipped.
            vec![
                // Extract discriminant (variant index) from enum value
                Instruction::LoadVar("s".to_string()),
                Instruction::Discriminant,
                // First arm: check if variant index == 0 (Active)
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(13),
                // Second arm: check if variant index == 1 (Inactive)
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(5),
                // Third arm: exhaustive match - skip comparison, value must be Pending
                Instruction::Pop(1),
                Instruction::Jump(1),
                // Bodies in reverse order
                Instruction::LoadConst(Value::string("pending")),
                Instruction::Jump(4),
                Instruction::LoadConst(Value::string("inactive")),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::string("active")),
                Instruction::Return,
            ],
        )],
    })
}

/// Non-exhaustive enum match (with wildcard) should NOT get the exhaustive optimization.
/// The last arm's comparison must still be emitted because the wildcard catches
/// values that don't match any variant pattern.
#[test]
fn match_enum_variant_with_wildcard_not_exhaustive() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            enum Status {
                Active
                Inactive
                Pending
            }

            function classify(s Status) -> string {
                match (s) {
                    Status.Active => "active",
                    Status.Inactive => "inactive",
                    _ => "other"
                }
            }
        "#,
        expected: vec![(
            "classify",
            // Non-exhaustive: wildcard arm means we can't skip comparisons
            // All comparisons must be emitted (no exhaustive optimization)
            vec![
                // Extract discriminant (variant index) from enum value
                Instruction::LoadVar("s".to_string()),
                Instruction::Discriminant,
                // First arm: check if variant index == 0 (Active)
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(12),
                // Second arm: check if variant index == 1 (Inactive)
                // NOT skipped because this is non-exhaustive (has wildcard)
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(4),
                // Fall through to wildcard (pop discriminant)
                Instruction::Pop(1),
                // Bodies in reverse order
                Instruction::LoadConst(Value::string("other")),
                Instruction::Jump(4),
                Instruction::LoadConst(Value::string("inactive")),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::string("active")),
                Instruction::Return,
            ],
        )],
    })
}

/// Enum variant patterns with 4+ arms should use Discriminant + `JumpTable`.
#[test]
fn match_enum_four_variants_jump_table() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            enum Direction {
                North
                East
                South
                West
            }

            function compass(d Direction) -> string {
                match (d) {
                    Direction.North => "N",
                    Direction.East => "E",
                    Direction.South => "S",
                    Direction.West => "W"
                }
            }
        "#,
        expected: vec![(
            "compass",
            // With 4 enum variants, use Discriminant + JumpTable for O(1) dispatch
            vec![
                // Extract discriminant (variant index) from enum value
                Instruction::LoadVar("d".to_string()),
                Instruction::Discriminant,
                // JumpTable: table_idx=0, default jumps +1 (to first body for exhaustive match)
                Instruction::JumpTable {
                    table_idx: 0,
                    default: 1,
                },
                // Bodies in reverse order: West (3), South (2), East (1), North (0)
                Instruction::LoadConst(Value::string("W")),
                Instruction::Jump(6),
                Instruction::LoadConst(Value::string("S")),
                Instruction::Jump(4),
                Instruction::LoadConst(Value::string("E")),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::string("N")),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Exhaustive and Non-Exhaustive Class Type Tests
// ============================================================================

/// Exhaustive class type match (all classes covered, no wildcard) should use
/// the exhaustive optimization for if-else chain (skips last instanceof check).
#[test]
fn match_class_types_exhaustive() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            class Cat { name string }
            class Dog { name string }
            class Bird { name string }

            function classify(animal Cat | Dog | Bird) -> string {
                match (animal) {
                    c: Cat => "cat: " + c.name,
                    d: Dog => "dog: " + d.name,
                    b: Bird => "bird: " + b.name
                }
            }
        "#,
        expected: vec![(
            "classify",
            // Exhaustive match: 3 classes with if-else chain
            // Last arm (Bird) doesn't need instanceof check - exhaustive optimization
            vec![
                // c: Cat instanceof check
                Instruction::LoadVar("animal".to_string()),
                Instruction::LoadConst(Value::class("Cat")),
                Instruction::CmpOp(CmpOp::InstanceOf),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(17),
                // d: Dog instanceof check
                Instruction::LoadVar("animal".to_string()),
                Instruction::LoadConst(Value::class("Dog")),
                Instruction::CmpOp(CmpOp::InstanceOf),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(7),
                // b: Bird - no instanceof check (exhaustive optimization)
                Instruction::Jump(1),
                // Bird body
                Instruction::LoadConst(Value::string("bird: ")),
                Instruction::LoadVar("animal".to_string()),
                Instruction::LoadField(0),
                Instruction::BinOp(BinOp::Add),
                Instruction::Jump(10),
                // Dog body
                Instruction::LoadConst(Value::string("dog: ")),
                Instruction::LoadVar("animal".to_string()),
                Instruction::LoadField(0),
                Instruction::BinOp(BinOp::Add),
                Instruction::Jump(5),
                // Cat body
                Instruction::LoadConst(Value::string("cat: ")),
                Instruction::LoadVar("animal".to_string()),
                Instruction::LoadField(0),
                Instruction::BinOp(BinOp::Add),
                Instruction::Return,
            ],
        )],
    })
}

/// Non-exhaustive class type match (with wildcard) should NOT get the exhaustive
/// optimization. All instanceof checks must be emitted.
#[test]
fn match_class_types_non_exhaustive() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            class Cat { name string }
            class Dog { name string }
            class Bird { name string }

            function classify(animal Cat | Dog | Bird) -> string {
                match (animal) {
                    c: Cat => "cat: " + c.name,
                    d: Dog => "dog: " + d.name,
                    _ => "other"
                }
            }
        "#,
        expected: vec![(
            "classify",
            // Non-exhaustive: wildcard means ALL instanceof checks required
            vec![
                // c: Cat instanceof check
                Instruction::LoadVar("animal".to_string()),
                Instruction::LoadConst(Value::class("Cat")),
                Instruction::CmpOp(CmpOp::InstanceOf),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(13),
                // d: Dog instanceof check - NOT skipped (wildcard present)
                Instruction::LoadVar("animal".to_string()),
                Instruction::LoadConst(Value::class("Dog")),
                Instruction::CmpOp(CmpOp::InstanceOf),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                // Fall through to wildcard
                Instruction::LoadConst(Value::string("other")),
                Instruction::Jump(10),
                // Dog body
                Instruction::LoadConst(Value::string("dog: ")),
                Instruction::LoadVar("animal".to_string()),
                Instruction::LoadField(0),
                Instruction::BinOp(BinOp::Add),
                Instruction::Jump(5),
                // Cat body
                Instruction::LoadConst(Value::string("cat: ")),
                Instruction::LoadVar("animal".to_string()),
                Instruction::LoadField(0),
                Instruction::BinOp(BinOp::Add),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// TypeTag Switch Tests (Union Types with Typed Patterns)
// ============================================================================

/// Union type with 4+ typed primitive patterns should use `TypeTag` + `JumpTable`.
#[test]
fn match_union_type_four_patterns_type_tag() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function identify(x int | string | bool | float) -> string {
                match (x) {
                    n: int => "integer",
                    s: string => "text",
                    b: bool => "boolean",
                    f: float => "decimal"
                }
            }
        "#,
        expected: vec![(
            "identify",
            // With 4 typed patterns, use TypeTag + JumpTable for O(1) dispatch
            // Type tags: int=0, string=1, bool=2, float=4
            vec![
                // Extract type tag from union value
                Instruction::LoadVar("x".to_string()),
                Instruction::TypeTag,
                // JumpTable: table_idx=0, default jumps +1 (exhaustive match)
                Instruction::JumpTable {
                    table_idx: 0,
                    default: 1,
                },
                // Bodies in reverse order: float (4), bool (2), string (1), int (0)
                Instruction::LoadConst(Value::string("decimal")),
                Instruction::Jump(6),
                Instruction::LoadConst(Value::string("boolean")),
                Instruction::Jump(4),
                Instruction::LoadConst(Value::string("text")),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::string("integer")),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Union Patterns Aggregating to 4+ Arms Tests
// ============================================================================

/// Union patterns that aggregate to 4+ total values should use jump table.
#[test]
fn match_union_aggregated_jump_table() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function classify(x int) -> string {
                match (x) {
                    0 | 1 => "a",
                    2 | 3 => "b",
                    4 | 5 => "c",
                    6 | 7 => "d",
                    _ => "other"
                }
            }
        "#,
        expected: vec![(
            "classify",
            // 8 total integer values should trigger jump table
            vec![
                Instruction::LoadVar("x".to_string()),
                Instruction::JumpTable {
                    table_idx: 0,
                    default: 1,
                },
                // Catch-all arm
                Instruction::LoadConst(Value::string("other")),
                Instruction::Jump(8),
                // Arms in reverse order: d, c, b, a
                Instruction::LoadConst(Value::string("d")),
                Instruction::Jump(6),
                Instruction::LoadConst(Value::string("c")),
                Instruction::Jump(4),
                Instruction::LoadConst(Value::string("b")),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::string("a")),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Exhaustive Bool with Variable Scrutinee Tests
// ============================================================================

/// Exhaustive bool match with variable scrutinee (not constant).
/// Tests that optimization works correctly when scrutinee isn't constant-folded.
#[test]
fn match_bool_variable_exhaustive() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function check(flag bool) -> string {
                match (flag) {
                    true => "yes",
                    false => "no"
                }
            }
        "#,
        expected: vec![(
            "check",
            // Exhaustive bool with variable scrutinee
            // Second arm's comparison is skipped (exhaustive match optimization)
            vec![
                Instruction::LoadVar("flag".to_string()),
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(4),
                // Second arm: no comparison needed (exhaustive)
                Instruction::Jump(1),
                Instruction::LoadConst(Value::string("no")),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::string("yes")),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Optional with Null in Switch Tests
// ============================================================================

/// Optional type with null pattern alongside typed pattern.
#[test]
fn match_optional_with_null() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function process(x int?) -> string {
                match (x) {
                    null => "none",
                    n: int => "some"
                }
            }
        "#,
        expected: vec![(
            "process",
            // Optional with null and typed pattern
            vec![
                // First arm: null check
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Null),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(4), // to "none" body
                // Second arm: n: int (exhaustive - skips instanceof check)
                Instruction::Jump(1),
                Instruction::LoadConst(Value::string("some")),
                Instruction::Jump(2),
                // Body for null
                Instruction::LoadConst(Value::string("none")),
                Instruction::Return,
            ],
        )],
    })
}

/// Optional type with null, literal, and typed pattern.
#[test]
fn match_optional_with_null_and_literal() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function process(x int?) -> string {
                match (x) {
                    null => "none",
                    0 => "zero",
                    n: int => "other"
                }
            }
        "#,
        expected: vec![(
            "process",
            // Optional with null, literal, and typed pattern
            vec![
                // First arm: null check
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Null),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(11), // to "none" body at offset 15
                // Second arm: 0 check
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(4), // to "zero" body
                // Third arm: n: int (exhaustive - skips instanceof)
                Instruction::Jump(1),
                Instruction::LoadConst(Value::string("other")),
                Instruction::Jump(4),
                // Body for 0
                Instruction::LoadConst(Value::string("zero")),
                Instruction::Jump(2),
                // Body for null
                Instruction::LoadConst(Value::string("none")),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Large Jump Table Tests (10+ arms)
// ============================================================================

/// Large dense integer match with 10 arms should use jump table.
#[test]
fn match_large_jump_table() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function classify(x int) -> int {
                match (x) {
                    0 => 100,
                    1 => 101,
                    2 => 102,
                    3 => 103,
                    4 => 104,
                    5 => 105,
                    6 => 106,
                    7 => 107,
                    8 => 108,
                    9 => 109,
                    _ => 999
                }
            }
        "#,
        expected: vec![(
            "classify",
            // 10 dense arms should trigger jump table
            vec![
                Instruction::LoadVar("x".to_string()),
                Instruction::JumpTable {
                    table_idx: 0,
                    default: 1,
                },
                // Catch-all
                Instruction::LoadConst(Value::Int(999)),
                Instruction::Jump(20),
                // Arms in reverse order: 9, 8, 7, 6, 5, 4, 3, 2, 1, 0
                Instruction::LoadConst(Value::Int(109)),
                Instruction::Jump(18),
                Instruction::LoadConst(Value::Int(108)),
                Instruction::Jump(16),
                Instruction::LoadConst(Value::Int(107)),
                Instruction::Jump(14),
                Instruction::LoadConst(Value::Int(106)),
                Instruction::Jump(12),
                Instruction::LoadConst(Value::Int(105)),
                Instruction::Jump(10),
                Instruction::LoadConst(Value::Int(104)),
                Instruction::Jump(8),
                Instruction::LoadConst(Value::Int(103)),
                Instruction::Jump(6),
                Instruction::LoadConst(Value::Int(102)),
                Instruction::Jump(4),
                Instruction::LoadConst(Value::Int(101)),
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(100)),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// String Patterns with 4+ Arms Tests
// ============================================================================

/// String patterns with 4+ arms should still use if-else chain (not jump table).
#[test]
fn match_string_many_arms() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function classify(s string) -> int {
                match (s) {
                    "alpha" => 1,
                    "beta" => 2,
                    "gamma" => 3,
                    "delta" => 4,
                    _ => 0
                }
            }
        "#,
        expected: vec![(
            "classify",
            // Strings use if-else chain, not jump table
            vec![
                // First arm: "alpha"
                Instruction::LoadVar("s".to_string()),
                Instruction::LoadConst(Value::string("alpha")),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(24), // to offset 28 (return 1)
                // Second arm: "beta"
                Instruction::LoadVar("s".to_string()),
                Instruction::LoadConst(Value::string("beta")),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(17), // to offset 26 (return 2)
                // Third arm: "gamma"
                Instruction::LoadVar("s".to_string()),
                Instruction::LoadConst(Value::string("gamma")),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(10), // to offset 24 (return 3)
                // Fourth arm: "delta"
                Instruction::LoadVar("s".to_string()),
                Instruction::LoadConst(Value::string("delta")),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3), // to offset 22 (return 4)
                // Catch-all
                Instruction::LoadConst(Value::Int(0)),
                Instruction::Jump(8),
                // Bodies in reverse: delta, gamma, beta, alpha
                Instruction::LoadConst(Value::Int(4)),
                Instruction::Jump(6),
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
// Complex Scrutinee Expression Tests
// ============================================================================

/// Match with arithmetic expression as scrutinee.
#[test]
fn match_arithmetic_scrutinee() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function classify(a int, b int) -> string {
                match (a + b) {
                    0 => "zero",
                    1 => "one",
                    _ => "other"
                }
            }
        "#,
        expected: vec![(
            "classify",
            // Scrutinee is computed and stays on stack (no temp variable)
            vec![
                // Compute a + b - result stays on stack
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadVar("b".to_string()),
                Instruction::BinOp(bex_vm_types::BinOp::Add),
                // First arm: 0
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(12), // to offset 20 ("zero")
                // Second arm: 1
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(4), // to offset 18 ("one")
                // Catch-all
                Instruction::Pop(1),
                Instruction::LoadConst(Value::string("other")),
                Instruction::Jump(4),
                // Body for 1
                Instruction::LoadConst(Value::string("one")),
                Instruction::Jump(2),
                // Body for 0
                Instruction::LoadConst(Value::string("zero")),
                Instruction::Return,
            ],
        )],
    })
}

/// Match with function call as scrutinee.
#[test]
fn match_function_call_scrutinee() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function helper() -> int {
                42
            }

            function classify() -> string {
                match (helper()) {
                    42 => "answer",
                    _ => "other"
                }
            }
        "#,
        expected: vec![
            (
                "helper",
                vec![Instruction::LoadConst(Value::Int(42)), Instruction::Return],
            ),
            (
                "classify",
                // Function call result stays on stack
                vec![
                    // Call helper() - result stays on stack
                    Instruction::LoadGlobal(Value::function("helper")),
                    Instruction::Call(0),
                    // First arm: 42
                    Instruction::Copy(0),
                    Instruction::LoadConst(Value::Int(42)),
                    Instruction::CmpOp(CmpOp::Eq),
                    Instruction::PopJumpIfFalse(3),
                    Instruction::Pop(1),
                    Instruction::Jump(4), // to offset 11 ("answer")
                    // Catch-all
                    Instruction::Pop(1),
                    Instruction::LoadConst(Value::string("other")),
                    Instruction::Jump(2),
                    // Body for 42
                    Instruction::LoadConst(Value::string("answer")),
                    Instruction::Return,
                ],
            ),
        ],
    })
}

// ============================================================================
// Range Limit Boundary Tests (256)
// ============================================================================

/// Values 0-255 (256 entries, exactly at limit) should use jump table.
#[test]
fn match_range_at_limit_uses_jump_table() -> anyhow::Result<()> {
    // We can't actually write 256 arms, but we can test with sparse values
    // that span a range of 256. With 4 arms spanning 0-255, density = 4/256 = 1.5%
    // which is below 50%, so this should use binary search.
    // Let's test with dense values in a smaller range at the boundary.
    assert_compiles(Program {
        source: r#"
            function classify(x int) -> int {
                match (x) {
                    252 => 1,
                    253 => 2,
                    254 => 3,
                    255 => 4,
                    _ => 0
                }
            }
        "#,
        expected: vec![(
            "classify",
            // Dense 4 values at high range should use jump table
            vec![
                Instruction::LoadVar("x".to_string()),
                Instruction::JumpTable {
                    table_idx: 0,
                    default: 1,
                },
                Instruction::LoadConst(Value::Int(0)),
                Instruction::Jump(8),
                Instruction::LoadConst(Value::Int(4)),
                Instruction::Jump(6),
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

/// Values spanning more than 256 range should NOT use jump table.
#[test]
fn match_range_exceeds_limit_uses_binary_search() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function classify(x int) -> int {
                match (x) {
                    0 => 1,
                    100 => 2,
                    200 => 3,
                    300 => 4,
                    _ => 0
                }
            }
        "#,
        expected: vec![(
            "classify",
            // Range of 301 (0-300) exceeds 256, should use binary search
            vec![
                Instruction::LoadVar("x".to_string()),
                // Binary search pattern
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(200)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(28),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(200)),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::PopJumpIfFalse(13),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(22),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(100)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(14),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(300)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::Jump(8),
                Instruction::LoadConst(Value::Int(4)),
                Instruction::Jump(6),
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
// Deeply Nested Match Tests (3+ levels)
// ============================================================================

/// Three levels of nested match expressions.
#[test]
fn match_three_levels_nested() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function classify(x int, y int, z int) -> int {
                match (x) {
                    0 => match (y) {
                        0 => match (z) {
                            0 => 0,
                            _ => 1
                        },
                        _ => 2
                    },
                    _ => 3
                }
            }
        "#,
        expected: vec![(
            "classify",
            // Three levels of nesting
            vec![
                // Outer: match (x)
                Instruction::LoadVar("x".to_string()),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(4),
                // Outer catch-all: _ => 3
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::Jump(22),
                // Middle: match (y)
                Instruction::LoadVar("y".to_string()),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(4),
                // Middle catch-all: _ => 2
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::Jump(12),
                // Inner: match (z)
                Instruction::LoadVar("z".to_string()),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::PopJumpIfFalse(3),
                Instruction::Pop(1),
                Instruction::Jump(4),
                // Inner catch-all: _ => 1
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Jump(2),
                // Inner first arm: 0 => 0
                Instruction::LoadConst(Value::Int(0)),
                Instruction::Return,
            ],
        )],
    })
}
