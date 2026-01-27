//! VM tests for match expressions with pattern matching and instanceof.

use baml_tests::bytecode::{ExecState, Program, Value, assert_vm_executes};

// ============================================================================
// Basic Match Tests
// ============================================================================

#[test]
fn match_simple_catch_all() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                match (42) {
                    _ => 100
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(100)),
    })
}

#[test]
fn match_catch_all_with_binding() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 42;
                match (x) {
                    y => y + 1
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(43)),
    })
}

// ============================================================================
// Literal Pattern Tests
// ============================================================================

#[test]
fn match_literal_int_first_arm() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 1;
                match (x) {
                    1 => 100,
                    2 => 200,
                    _ => 0
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(100)),
    })
}

#[test]
fn match_literal_int_second_arm() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 2;
                match (x) {
                    1 => 100,
                    2 => 200,
                    _ => 0
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(200)),
    })
}

#[test]
fn match_literal_int_fallback() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 999;
                match (x) {
                    1 => 100,
                    2 => 200,
                    _ => 0
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(0)),
    })
}

#[test]
fn match_literal_null() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                let x = null;
                match (x) {
                    null => "was null",
                    _ => "not null"
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("was null")),
    })
}

// ============================================================================
// Typed Pattern Tests (instanceof)
// ============================================================================

#[test]
fn match_typed_pattern_first_arm() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Success {
                data string
            }

            class Failure {
                reason string
            }

            function main() -> string {
                let result = Success { data: "hello" };
                match (result) {
                    s: Success => "success: " + s.data,
                    _: Failure => "failure",
                    _ => "unknown"
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("success: hello")),
    })
}

#[test]
fn match_typed_pattern_second_arm() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Success {
                data string
            }

            class Failure {
                reason string
            }

            function main() -> string {
                let result = Failure { reason: "error" };
                match (result) {
                    s: Success => "success: " + s.data,
                    f: Failure => "failure: " + f.reason,
                    _ => "unknown"
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("failure: error")),
    })
}

#[test]
fn match_typed_pattern_with_field_access() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Point {
                x int,
                y int
            }

            class Circle {
                radius int
            }

            function main() -> int {
                let shape = Point { x: 10, y: 20 };
                match (shape) {
                    p: Point => p.x + p.y,
                    c: Circle => c.radius,
                    _ => 0
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(30)),
    })
}

// ============================================================================
// Guard Tests
// ============================================================================

#[test]
fn match_guard_true() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Score {
                value int
            }

            function main() -> string {
                let s = Score { value: 95 };
                match (s) {
                    x: Score if x.value >= 90 => "excellent",
                    x: Score if x.value >= 70 => "good",
                    _: Score => "needs work",
                    _ => "unknown"
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("excellent")),
    })
}

#[test]
fn match_guard_fallthrough() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Score {
                value int
            }

            function main() -> string {
                let s = Score { value: 75 };
                match (s) {
                    x: Score if x.value >= 90 => "excellent",
                    x: Score if x.value >= 70 => "good",
                    _: Score => "needs work",
                    _ => "unknown"
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("good")),
    })
}

#[test]
fn match_guard_all_fail() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Score {
                value int
            }

            function main() -> string {
                let s = Score { value: 50 };
                match (s) {
                    x: Score if x.value >= 90 => "excellent",
                    x: Score if x.value >= 70 => "good",
                    _: Score => "needs work",
                    _ => "unknown"
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("needs work")),
    })
}

// ============================================================================
// Union Pattern Tests
// ============================================================================

#[test]
fn match_union_literal_first() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                let code = 200;
                match (code) {
                    200 | 201 => "success",
                    400 | 404 => "client error",
                    _ => "other"
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("success")),
    })
}

#[test]
fn match_union_literal_second() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                let code = 201;
                match (code) {
                    200 | 201 => "success",
                    400 | 404 => "client error",
                    _ => "other"
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("success")),
    })
}

// ============================================================================
// Expression Context Tests
// ============================================================================

#[test]
fn match_as_expression() -> anyhow::Result<()> {
    // Simpler version - just use match in arithmetic
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 2;
                match (x) {
                    1 => 10,
                    2 => 20,
                    _ => 0
                } + 1
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(21)),
    })
}

#[test]
fn match_nested() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let outer = 1;
                let inner = 2;
                match (outer) {
                    1 => match (inner) {
                        1 => 11,
                        2 => 12,
                        _ => 10
                    },
                    2 => 20,
                    _ => 0
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(12)),
    })
}

// ============================================================================
// Jump Table Tests (4+ dense arms trigger jump table optimization)
// ============================================================================

#[test]
fn match_jump_table_first_arm() -> anyhow::Result<()> {
    // 4 consecutive values trigger jump table: 0, 1, 2, 3
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 0;
                match (x) {
                    0 => 100,
                    1 => 101,
                    2 => 102,
                    3 => 103,
                    _ => 999
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(100)),
    })
}

#[test]
fn match_jump_table_middle_arm() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 2;
                match (x) {
                    0 => 100,
                    1 => 101,
                    2 => 102,
                    3 => 103,
                    _ => 999
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(102)),
    })
}

#[test]
fn match_jump_table_last_arm() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 3;
                match (x) {
                    0 => 100,
                    1 => 101,
                    2 => 102,
                    3 => 103,
                    _ => 999
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(103)),
    })
}

#[test]
fn match_jump_table_fallback() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 10;
                match (x) {
                    0 => 100,
                    1 => 101,
                    2 => 102,
                    3 => 103,
                    _ => 999
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(999)),
    })
}

#[test]
fn match_jump_table_negative_fallback() -> anyhow::Result<()> {
    // Value below table range should fall through to default
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = -1;
                match (x) {
                    0 => 100,
                    1 => 101,
                    2 => 102,
                    3 => 103,
                    _ => 999
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(999)),
    })
}

#[test]
fn match_jump_table_with_holes() -> anyhow::Result<()> {
    // 4 values in range of 7: density ~57% (above 50% threshold)
    // Hole at value 1, 3, 5 should fall through to default
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 1;
                match (x) {
                    0 => 100,
                    2 => 102,
                    4 => 104,
                    6 => 106,
                    _ => 999
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(999)),
    })
}

#[test]
fn match_jump_table_with_holes_hit() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 4;
                match (x) {
                    0 => 100,
                    2 => 102,
                    4 => 104,
                    6 => 106,
                    _ => 999
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(104)),
    })
}

#[test]
fn match_jump_table_offset_values() -> anyhow::Result<()> {
    // Jump table with non-zero base offset
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 11;
                match (x) {
                    10 => 110,
                    11 => 111,
                    12 => 112,
                    13 => 113,
                    _ => 999
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(111)),
    })
}

#[test]
fn match_jump_table_large() -> anyhow::Result<()> {
    // 8 consecutive values - should still use jump table
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 5;
                match (x) {
                    0 => 1000,
                    1 => 1001,
                    2 => 1002,
                    3 => 1003,
                    4 => 1004,
                    5 => 1005,
                    6 => 1006,
                    7 => 1007,
                    _ => 9999
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(1005)),
    })
}

// ============================================================================
// Binary Search Tests (4+ sparse arms trigger binary search optimization)
// ============================================================================

#[test]
fn match_binary_search_first_arm() -> anyhow::Result<()> {
    // 4 values spread over range of 100: density 4% (below 50%)
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 0;
                match (x) {
                    0 => 100,
                    30 => 130,
                    60 => 160,
                    99 => 199,
                    _ => 999
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(100)),
    })
}

#[test]
fn match_binary_search_middle_arm() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 60;
                match (x) {
                    0 => 100,
                    30 => 130,
                    60 => 160,
                    99 => 199,
                    _ => 999
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(160)),
    })
}

#[test]
fn match_binary_search_last_arm() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 99;
                match (x) {
                    0 => 100,
                    30 => 130,
                    60 => 160,
                    99 => 199,
                    _ => 999
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(199)),
    })
}

#[test]
fn match_binary_search_fallback() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 50;
                match (x) {
                    0 => 100,
                    30 => 130,
                    60 => 160,
                    99 => 199,
                    _ => 999
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(999)),
    })
}

#[test]
fn match_binary_search_very_sparse() -> anyhow::Result<()> {
    // Values spread over very large range
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 500;
                match (x) {
                    0 => 1000,
                    100 => 1100,
                    200 => 1200,
                    300 => 1300,
                    400 => 1400,
                    500 => 1500,
                    _ => 9999
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(1500)),
    })
}

#[test]
fn match_binary_search_large_values() -> anyhow::Result<()> {
    // Binary search with large spread values
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 750;
                match (x) {
                    250 => 1250,
                    500 => 1500,
                    750 => 1750,
                    1000 => 2000,
                    1250 => 2250,
                    _ => 9999
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(1750)),
    })
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn match_computed_discriminant() -> anyhow::Result<()> {
    // Discriminant is a computed value, not a variable
    assert_vm_executes(Program {
        source: r#"
            function get_value() -> int {
                2
            }

            function main() -> int {
                match (get_value()) {
                    0 => 100,
                    1 => 101,
                    2 => 102,
                    3 => 103,
                    _ => 999
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(102)),
    })
}

#[test]
fn match_in_loop() -> anyhow::Result<()> {
    // Match inside a loop - tests repeated execution
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let sum = 0;
                let i = 0;
                while (i < 5) {
                    sum = sum + match (i) {
                        0 => 10,
                        1 => 20,
                        2 => 30,
                        3 => 40,
                        _ => 50
                    };
                    i = i + 1;
                }
                sum
            }
        "#,
        function: "main",
        // 10 + 20 + 30 + 40 + 50 = 150
        expected: ExecState::Complete(Value::Int(150)),
    })
}

#[test]
fn match_exhaustive_no_fallback() -> anyhow::Result<()> {
    // Exhaustive match without catch-all (4 arms, dense)
    assert_vm_executes(Program {
        source: r#"
            function classify(n int) -> int {
                match (n) {
                    0 => 0,
                    1 => 1,
                    2 => 2,
                    3 => 3
                }
            }

            function main() -> int {
                classify(2)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(2)),
    })
}

// ============================================================================
// String Literal Tests (should NOT use jump table - use if-else chain)
// ============================================================================

#[test]
fn match_string_literal_first_arm() -> anyhow::Result<()> {
    // Use function parameter to prevent compile-time optimization
    assert_vm_executes(Program {
        source: r#"
            function classify(s string) -> int {
                match (s) {
                    "hello" => 100,
                    "world" => 200,
                    _ => 0
                }
            }
            function main() -> int {
                classify("hello")
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(100)),
    })
}

#[test]
fn match_string_literal_second_arm() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function classify(s string) -> int {
                match (s) {
                    "hello" => 100,
                    "world" => 200,
                    _ => 0
                }
            }
            function main() -> int {
                classify("world")
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(200)),
    })
}

#[test]
fn match_string_literal_fallback() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function classify(s string) -> int {
                match (s) {
                    "hello" => 100,
                    "world" => 200,
                    _ => 0
                }
            }
            function main() -> int {
                classify("other")
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(0)),
    })
}

#[test]
fn match_string_four_arms() -> anyhow::Result<()> {
    // 4+ string arms should NOT use jump table (strings can't be hashed efficiently)
    assert_vm_executes(Program {
        source: r#"
            function classify(s string) -> int {
                match (s) {
                    "a" => 1,
                    "b" => 2,
                    "c" => 3,
                    "d" => 4,
                    _ => 0
                }
            }
            function main() -> int {
                classify("c")
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(3)),
    })
}

// ============================================================================
// Bool Literal Tests
// ============================================================================

#[test]
fn match_bool_true() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                let b = true;
                match (b) {
                    true => "yes",
                    false => "no"
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("yes")),
    })
}

#[test]
fn match_bool_false() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                let b = false;
                match (b) {
                    true => "yes",
                    false => "no"
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("no")),
    })
}

// ============================================================================
// Guards with Integer Literals (should prevent switch optimization)
// NOTE: Tests with guards combined with integer literals have an optimization
// issue that needs investigation. The existing guard tests in the "Guard Tests"
// section above work correctly with typed patterns.
// ============================================================================

#[test]
fn match_guarded_int_literal_guard_true() -> anyhow::Result<()> {
    // Guards on integer literals should force if-else fallback
    assert_vm_executes(Program {
        source: r#"
            function classify(x int, flag bool) -> string {
                match (x) {
                    1 if flag => "one with flag",
                    1 => "one without flag",
                    2 => "two",
                    _ => "other"
                }
            }
            function main() -> string {
                classify(1, true)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("one with flag")),
    })
}

#[test]
fn match_guarded_int_literal_guard_false() -> anyhow::Result<()> {
    // Guard fails, falls through to unguarded arm
    assert_vm_executes(Program {
        source: r#"
            function classify(x int, flag bool) -> string {
                match (x) {
                    1 if flag => "one with flag",
                    1 => "one without flag",
                    2 => "two",
                    _ => "other"
                }
            }
            function main() -> string {
                classify(1, false)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("one without flag")),
    })
}

#[test]
fn match_all_arms_guarded() -> anyhow::Result<()> {
    // All arms have guards - all guards fail, fall to catch-all
    assert_vm_executes(Program {
        source: r#"
            function classify(x int, flag bool) -> string {
                match (x) {
                    0 if flag => "zero",
                    1 if flag => "one",
                    2 if flag => "two",
                    _ => "fallback"
                }
            }
            function main() -> string {
                classify(1, false)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("fallback")),
    })
}

// ============================================================================
// Mixed Patterns (instanceof + values - should fallback)
// ============================================================================

#[test]
fn match_mixed_instanceof_and_literal() -> anyhow::Result<()> {
    // Type patterns should use if-else chain
    assert_vm_executes(Program {
        source: r#"
            class Result {
                code int
            }

            function main() -> int {
                let x = Result { code: 200 };
                match (x) {
                    r: Result => r.code,
                    _ => 0
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(200)),
    })
}

// ============================================================================
// Density Threshold Tests (boundary at 50%)
// ============================================================================

#[test]
fn match_density_exactly_50_percent() -> anyhow::Result<()> {
    // 4 arms in range of 8: 0, 2, 4, 6 = 50% density (should use jump table)
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 4;
                match (x) {
                    0 => 100,
                    2 => 102,
                    4 => 104,
                    6 => 106,
                    _ => 999
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(104)),
    })
}

#[test]
fn match_density_below_50_percent() -> anyhow::Result<()> {
    // 4 arms in range of 10: 0, 3, 6, 9 = 40% density (should use binary search)
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 6;
                match (x) {
                    0 => 100,
                    3 => 103,
                    6 => 106,
                    9 => 109,
                    _ => 999
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(106)),
    })
}

#[test]
fn match_density_above_50_percent() -> anyhow::Result<()> {
    // 5 arms in range of 6: 0, 1, 2, 4, 5 = 83% density (should use jump table)
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 4;
                match (x) {
                    0 => 100,
                    1 => 101,
                    2 => 102,
                    4 => 104,
                    5 => 105,
                    _ => 999
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(104)),
    })
}

// ============================================================================
// Large Range Integer Values
// ============================================================================

#[test]
fn match_large_range_dense() -> anyhow::Result<()> {
    // Dense values starting from offset (100-103)
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 102;
                match (x) {
                    100 => 1000,
                    101 => 1001,
                    102 => 1002,
                    103 => 1003,
                    _ => 9999
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(1002)),
    })
}

#[test]
fn match_large_range_sparse() -> anyhow::Result<()> {
    // Sparse values with large gaps
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 1000;
                match (x) {
                    0 => 1,
                    500 => 2,
                    1000 => 3,
                    1500 => 4,
                    _ => 9999
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(3)),
    })
}

#[test]
fn match_zero_in_range() -> anyhow::Result<()> {
    // Match with 0 in the middle of the range
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 0;
                match (x) {
                    0 => 100,
                    1 => 101,
                    2 => 102,
                    3 => 103,
                    _ => 999
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(100)),
    })
}

// ============================================================================
// Union Patterns Edge Cases
// ============================================================================

#[test]
fn match_union_large() -> anyhow::Result<()> {
    // Large union pattern
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                let code = 204;
                match (code) {
                    200 | 201 | 202 | 204 => "success",
                    400 | 401 | 403 | 404 => "client error",
                    500 | 501 | 502 | 503 => "server error",
                    _ => "other"
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("success")),
    })
}

#[test]
fn match_union_client_error() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                let code = 404;
                match (code) {
                    200 | 201 | 202 | 204 => "success",
                    400 | 401 | 403 | 404 => "client error",
                    500 | 501 | 502 | 503 => "server error",
                    _ => "other"
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("client error")),
    })
}

#[test]
fn match_union_with_duplicates() -> anyhow::Result<()> {
    // Duplicate values in union patterns should be handled correctly
    // (deduplicated, not cause undefined behavior)
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                let x = 1;
                match (x) {
                    1 | 1 | 2 => "one or two",
                    3 | 3 => "three",
                    _ => "other"
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("one or two")),
    })
}

// ============================================================================
// Catch-All Binding with Integer Patterns
// ============================================================================

#[test]
fn match_catch_all_binding_with_int_patterns() -> anyhow::Result<()> {
    // Catch-all binding should still work with integer patterns
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 99;
                match (x) {
                    0 => 0,
                    1 => 1,
                    2 => 2,
                    3 => 3,
                    other => other * 10
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(990)),
    })
}

// ============================================================================
// Float Literal Tests (should NOT use jump table)
// ============================================================================

#[test]
fn match_float_literal() -> anyhow::Result<()> {
    // Float patterns should use if-else chain (not jump table)
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                let x = 1.5;
                match (x) {
                    1.0 => "one",
                    1.5 => "one point five",
                    2.0 => "two",
                    _ => "other"
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("one point five")),
    })
}

// ============================================================================
// Negative Literal Pattern Tests
// ============================================================================

#[test]
fn match_negative_int_first_arm() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                match (-1) {
                    -1 => "negative one",
                    0 => "zero",
                    1 => "one",
                    _ => "other"
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("negative one")),
    })
}

#[test]
fn match_negative_int_not_matched() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                match (1) {
                    -1 => "negative one",
                    1 => "one",
                    _ => "other"
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("one")),
    })
}

#[test]
fn match_negative_int_fallback() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                match (5) {
                    -1 => "negative one",
                    0 => "zero",
                    1 => "one",
                    _ => "other"
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("other")),
    })
}

#[test]
fn match_negative_int_with_variable() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                let x = -1;
                match (x) {
                    -1 => "negative one",
                    0 => "zero",
                    _ => "other"
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("negative one")),
    })
}

#[test]
fn match_negative_float_pattern() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                match (-1.5) {
                    -1.5 => "negative one point five",
                    0.0 => "zero",
                    1.5 => "one point five",
                    _ => "other"
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("negative one point five")),
    })
}

#[test]
fn match_multiple_negative_patterns() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                match (-2) {
                    -3 => "negative three",
                    -2 => "negative two",
                    -1 => "negative one",
                    _ => "other"
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("negative two")),
    })
}

#[test]
fn match_negative_in_union_pattern() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                match (-1) {
                    -1 | 0 | 1 => "small",
                    _ => "large"
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("small")),
    })
}

// ============================================================================
// Negative Integer Jump Table Tests
// ============================================================================

#[test]
fn match_negative_jump_table() -> anyhow::Result<()> {
    // Dense negative range should use jump table: -3, -2, -1, 0
    assert_vm_executes(Program {
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
            function main() -> string {
                classify(-2)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("neg two")),
    })
}

#[test]
fn match_negative_jump_table_fallback() -> anyhow::Result<()> {
    assert_vm_executes(Program {
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
            function main() -> string {
                classify(5)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("other")),
    })
}

#[test]
fn match_spanning_zero_jump_table() -> anyhow::Result<()> {
    // Dense range crossing zero: -2, -1, 0, 1, 2
    assert_vm_executes(Program {
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
            function main() -> string {
                classify(1)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("one")),
    })
}

#[test]
fn match_spanning_zero_negative_hit() -> anyhow::Result<()> {
    assert_vm_executes(Program {
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
            function main() -> string {
                classify(-1)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("neg one")),
    })
}

// ============================================================================
// Binary Search with Negative Values
// ============================================================================

#[test]
fn match_binary_search_negative_sparse() -> anyhow::Result<()> {
    // Sparse negative values
    assert_vm_executes(Program {
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
            function main() -> string {
                classify(-50)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("b")),
    })
}

#[test]
fn match_binary_search_spanning_zero_sparse() -> anyhow::Result<()> {
    // Sparse values spanning zero
    assert_vm_executes(Program {
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
            function main() -> string {
                classify(1)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("one")),
    })
}

// ============================================================================
// Mixed Patterns: Literals + Types + Guards
// ============================================================================

#[test]
fn match_mixed_literal_typed_guard() -> anyhow::Result<()> {
    // Literal + guarded literal + typed pattern
    assert_vm_executes(Program {
        source: r#"
            function classify(x int, flag bool) -> string {
                match (x) {
                    0 => "zero",
                    1 if flag => "one with flag",
                    n: int => "other int"
                }
            }
            function main() -> string {
                classify(1, true)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("one with flag")),
    })
}

#[test]
fn match_mixed_literal_typed_guard_fallthrough() -> anyhow::Result<()> {
    // Guard fails, falls through to typed pattern
    assert_vm_executes(Program {
        source: r#"
            function classify(x int, flag bool) -> string {
                match (x) {
                    0 => "zero",
                    1 if flag => "one with flag",
                    n: int => "other int"
                }
            }
            function main() -> string {
                classify(1, false)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("other int")),
    })
}

#[test]
fn match_guard_on_typed_pattern_field_access() -> anyhow::Result<()> {
    // Guard accesses bound variable's field
    assert_vm_executes(Program {
        source: r#"
            class Success { data: string }
            class Failure { reason: string }

            function classify(result Success | Failure) -> string {
                match (result) {
                    s: Success if s.data != "" => "success with data",
                    s: Success => "empty success",
                    f: Failure => "failure"
                }
            }
            function main() -> string {
                let r = Success { data: "hello" };
                classify(r)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("success with data")),
    })
}

#[test]
fn match_guard_on_typed_pattern_field_access_fails() -> anyhow::Result<()> {
    // Guard fails, falls to next arm
    assert_vm_executes(Program {
        source: r#"
            class Success { data: string }
            class Failure { reason: string }

            function classify(result Success | Failure) -> string {
                match (result) {
                    s: Success if s.data != "" => "success with data",
                    s: Success => "empty success",
                    f: Failure => "failure"
                }
            }
            function main() -> string {
                let r = Success { data: "" };
                classify(r)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("empty success")),
    })
}

#[test]
fn match_string_literal_with_typed_fallback() -> anyhow::Result<()> {
    // String literals + typed catch-all
    assert_vm_executes(Program {
        source: r#"
            function classify(s string) -> int {
                match (s) {
                    "ok" => 200,
                    "error" => 500,
                    _: string => 0
                }
            }
            function main() -> int {
                classify("unknown")
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(0)),
    })
}

// ============================================================================
// Three-Level Nested Match
// ============================================================================

#[test]
fn match_three_levels_nested() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function classify(x int, y int, z int) -> string {
                match (x) {
                    0 => match (y) {
                        0 => match (z) {
                            0 => "all zero",
                            _ => "z nonzero"
                        },
                        _ => "y nonzero"
                    },
                    _ => "x nonzero"
                }
            }
            function main() -> string {
                classify(0, 0, 0)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("all zero")),
    })
}

#[test]
fn match_three_levels_nested_middle() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function classify(x int, y int, z int) -> string {
                match (x) {
                    0 => match (y) {
                        0 => match (z) {
                            0 => "all zero",
                            _ => "z nonzero"
                        },
                        _ => "y nonzero"
                    },
                    _ => "x nonzero"
                }
            }
            function main() -> string {
                classify(0, 1, 0)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("y nonzero")),
    })
}

// ============================================================================
// Optional with Null Pattern
// ============================================================================

#[test]
fn match_optional_null_pattern() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function process(x int?) -> string {
                match (x) {
                    null => "none",
                    n: int => "some"
                }
            }
            function main() -> string {
                process(null)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("none")),
    })
}

#[test]
fn match_optional_value_pattern() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function process(x int?) -> string {
                match (x) {
                    null => "none",
                    n: int => "some"
                }
            }
            function main() -> string {
                process(42)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("some")),
    })
}

#[test]
fn match_optional_with_literal_and_typed() -> anyhow::Result<()> {
    // null + literal + typed pattern
    assert_vm_executes(Program {
        source: r#"
            function process(x int?) -> string {
                match (x) {
                    null => "none",
                    0 => "zero",
                    n: int => "other"
                }
            }
            function main() -> string {
                process(0)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("zero")),
    })
}

// ============================================================================
// Enum Variant Patterns
// ============================================================================

#[test]
fn match_enum_variant_first() -> anyhow::Result<()> {
    assert_vm_executes(Program {
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
            function main() -> string {
                classify(Status.Active)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("active")),
    })
}

#[test]
fn match_enum_variant_last() -> anyhow::Result<()> {
    assert_vm_executes(Program {
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
            function main() -> string {
                classify(Status.Pending)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("pending")),
    })
}

// ============================================================================
// Non-Exhaustive Enum Tests (with wildcard)
// ============================================================================

#[test]
fn match_enum_variant_with_wildcard() -> anyhow::Result<()> {
    // Non-exhaustive: wildcard catches unmatched variants
    assert_vm_executes(Program {
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
            function main() -> string {
                classify(Status.Pending)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("other")),
    })
}

#[test]
fn match_enum_variant_with_wildcard_matched() -> anyhow::Result<()> {
    // Non-exhaustive: explicit variant matched
    assert_vm_executes(Program {
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
            function main() -> string {
                classify(Status.Active)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("active")),
    })
}

// ============================================================================
// Exhaustive and Non-Exhaustive Class Type Tests
// ============================================================================

#[test]
fn match_class_types_exhaustive_first() -> anyhow::Result<()> {
    // Exhaustive: all classes covered, matches first
    assert_vm_executes(Program {
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
            function main() -> string {
                classify(Cat { name: "Whiskers" })
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("cat: Whiskers")),
    })
}

#[test]
fn match_class_types_exhaustive_last() -> anyhow::Result<()> {
    // Exhaustive: all classes covered, matches last
    assert_vm_executes(Program {
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
            function main() -> string {
                classify(Bird { name: "Tweety" })
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("bird: Tweety")),
    })
}

#[test]
fn match_class_types_non_exhaustive_wildcard() -> anyhow::Result<()> {
    // Non-exhaustive: wildcard catches unmatched class
    assert_vm_executes(Program {
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
            function main() -> string {
                classify(Bird { name: "Tweety" })
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("other")),
    })
}

#[test]
fn match_class_types_non_exhaustive_matched() -> anyhow::Result<()> {
    // Non-exhaustive: explicit class matched
    assert_vm_executes(Program {
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
            function main() -> string {
                classify(Dog { name: "Rex" })
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("dog: Rex")),
    })
}

// ============================================================================
// Complex Scrutinee Expressions
// ============================================================================

#[test]
fn match_arithmetic_scrutinee() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function classify(a int, b int) -> string {
                match (a + b) {
                    0 => "zero",
                    1 => "one",
                    _ => "other"
                }
            }
            function main() -> string {
                classify(2, -1)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("one")),
    })
}

#[test]
fn match_function_call_scrutinee() -> anyhow::Result<()> {
    assert_vm_executes(Program {
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
            function main() -> string {
                classify()
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("answer")),
    })
}
