//! VM tests for built-in methods and operations.

mod common;
use common::{assert_vm_executes, ExecState, Program, Value};

#[test]
fn builtin_method_call() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let arr = [1, 2, 3];
                arr.length()
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(3)),
    })
}

#[test]
fn bind_method_call() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let arr = [1, 2, 3];
                let v = arr.length();

                v
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(3)),
    })
}

#[test]
fn any_value_to_string() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Point {
                x int
                y int
            }

            class Person {
                name string
                age int
                location Point
                hobbies string[]
                scores map<string, int>
            }

            function main() -> string {
                let p = Point { x: 10, y: 20 };
                let person = Person {
                    name: "Alice",
                    age: 25,
                    location: p,
                    hobbies: ["reading", "coding"],
                    scores: {"math": 95, "english": 88}
                };

                baml.unstable.string(person)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string(
            r#"Person {
    name: "Alice"
    age: 25
    location: Point {
        x: 10
        y: 20
    }
    hobbies: ["reading", "coding"]
    scores: {
        "math": 95
        "english": 88
    }
}"#,
        )),
    })
}
