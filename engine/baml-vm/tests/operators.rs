//! VM tests for operators (arithmetic, logical, bitwise, comparison, assignment).

mod common;
use common::{assert_vm_executes, ExecState, Program, Value};

// Arithmetic operators
#[test]
fn basic_add() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                1 + 2
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(3)),
    })
}

#[test]
fn basic_sub() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                1 - 2
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(-1)),
    })
}

#[test]
fn basic_mul() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                1 * 2
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(2)),
    })
}

#[test]
fn basic_div() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                10 / 2
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(5)),
    })
}

#[test]
fn basic_mod() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                10 % 3
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(1)),
    })
}

// Bitwise operators
#[test]
fn basic_bit_and() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                10 & 3
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(2)),
    })
}

#[test]
fn basic_bit_or() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                10 | 3
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(11)),
    })
}

#[test]
fn basic_bit_xor() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                10 ^ 3
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(9)),
    })
}

#[test]
fn basic_bit_shift_left() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                10 << 3
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(80)),
    })
}

#[test]
fn basic_bit_shift_right() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                10 >> 3
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(1)),
    })
}

// Unary operators
#[test]
fn unary_neg() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                -1
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(-1)),
    })
}

#[test]
fn unary_not() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> bool {
                !true
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(false)),
    })
}

// Comparison operators
#[test]
fn basic_eq() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> bool {
                1 == 2
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(false)),
    })
}

#[test]
fn basic_not_eq() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> bool {
                1 != 2
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn basic_gt() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> bool {
                1 > 2
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(false)),
    })
}

#[test]
fn basic_gt_eq() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> bool {
                1 >= 2
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(false)),
    })
}

#[test]
fn basic_lt() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> bool {
                1 < 2
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn basic_lt_eq() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> bool {
                1 <= 2
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(true)),
    })
}

// Logical operators
#[test]
fn basic_and() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> bool {
                true && false
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(false)),
    })
}

#[test]
fn basic_or() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> bool {
                true || false
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(true)),
    })
}

// Assignment operators
#[test]
fn basic_assign_add() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 1;
                x += 2;
                x
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(3)),
    })
}

#[test]
fn basic_assign_sub() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 1;
                x -= 2;
                x
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(-1)),
    })
}

#[test]
fn basic_assign_mul() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 1;
                x *= 2;
                x
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(2)),
    })
}

#[test]
fn basic_assign_div() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 10;
                x /= 2;
                x
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(5)),
    })
}

#[test]
fn basic_assign_mod() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 10;
                x %= 3;
                x
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(1)),
    })
}

#[test]
fn basic_assign_bit_and() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 10;
                x &= 3;
                x
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(2)),
    })
}

#[test]
fn basic_assign_bit_or() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 10;
                x |= 3;
                x
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(11)),
    })
}

#[test]
fn basic_assign_bit_xor() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let x = 10;
                x ^= 3;
                x
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(9)),
    })
}

#[test]
fn instance_of_returns_true() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class StopTool {
                action "stop"
            }

            function main() -> bool {
                let t = StopTool { action: "stop" };

                t instanceof StopTool
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn instance_of_returns_false() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class StopTool {
                action "stop"
            }

            class StartTool {
                action "start"
            }

            function main() -> bool {
                let t = StopTool { action: "stop" };

                t instanceof StartTool
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(false)),
    })
}
