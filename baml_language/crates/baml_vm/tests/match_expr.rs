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
