//! VM tests for strings.

mod common;
use common::{assert_vm_executes, ExecState, Program, Value};

use crate::common::Object;

#[test]
fn concat_strings() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                let a = "Hello";
                let b = " World";

                a + b
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Object(Object::String(String::from("Hello World")))),
    })
}

#[test]
fn string_equality_true() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> bool {
                "Hello" == "Hello"
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn string_equality_false() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> bool {
                "Hello" == "World"
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(false)),
    })
}

#[test]
fn string_not_equal_true() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> bool {
                "Hello" != "World"
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn string_less_than() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> bool {
                "a" < "b"
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn string_less_than_or_equal() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> bool {
                "a" <= "b"
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn string_greater_than() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> bool {
                "b" > "a"
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn string_greater_than_or_equal() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> bool {
                "b" >= "a"
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(true)),
    })
}
