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
        expected: ExecState::Complete(Value::string("Hello World")),
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

#[test]
fn string_length() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let s = "hello";
                s.length()
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(5)),
    })
}

#[test]
fn string_to_lower_case() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                let s = "HELLO World";
                s.toLowerCase()
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("hello world")),
    })
}

#[test]
fn string_to_upper_case() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                let s = "hello WORLD";
                s.toUpperCase()
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("HELLO WORLD")),
    })
}

#[test]
fn string_trim() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                let s = "  hello world  ";
                s.trim()
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("hello world")),
    })
}

#[test]
fn string_includes() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> bool {
                let s = "hello world";
                s.includes("world")
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn string_starts_with() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> bool {
                let s = "hello world";
                s.startsWith("hello")
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn string_ends_with() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> bool {
                let s = "hello world";
                s.endsWith("world")
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn string_split() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string[] {
                let s = "hello,world,test";
                s.split(",")
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Object(Object::Array(vec![
            Value::string("hello"),
            Value::string("world"),
            Value::string("test"),
        ]))),
    })
}

#[test]
fn string_substring() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                let s = "hello world";
                s.substring(0, 5)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("hello")),
    })
}

#[test]
fn string_substring_bounds() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                let s = "hello";
                s.substring(2, 10)  // Should clamp to string length
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("llo")),
    })
}

#[test]
fn string_replace() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                let s = "hello world world";
                s.replace("world", "BAML")
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("hello BAML world")),
    })
}
