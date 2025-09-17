//! VM tests for built-in methods and operations.

use baml_vm::{ObjectIndex, Value, VmExecState};

mod common;
use common::{assert_vm_executes, Program};

use crate::common::assert_vm_executes_with_inspection;

#[test]
fn builtin_method_call() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                let arr = [1, 2, 3];
                arr.length()
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(3)),
    })
}

#[test]
fn bind_method_call() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                let arr = [1, 2, 3];
                let v = arr.length();

                v
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(3)),
    })
}

#[test]
fn deep_copy_object() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: r#"
            class Tree {
                value string
                children Tree[]
            }

            fn main() -> Tree {
                let t = Tree { value: "1", children: [
                    Tree { value: "2", children: [] },
                    Tree { value: "3", children: [] },
                ] };

                let copy = baml.deep_copy(t);

                copy
            }
        "#,
            function: "main",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(47))),
        },
        |vm| {
            let baml_vm::Object::Instance(tree) = &vm.objects[ObjectIndex::from_raw(47)] else {
                panic!(
                    "expected Instance, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(47)]
                );
            };

            let baml_vm::Object::Instance(copy) = &vm.objects[ObjectIndex::from_raw(44)] else {
                panic!(
                    "expected Instance, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(44)]
                );
            };

            assert_eq!(tree.class, copy.class);

            Ok(())
        },
    )
}

#[test]
fn any_value_to_string() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
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

            fn main() -> string {
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
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(48))),
        },
        |vm| {
            let baml_vm::Object::String(result) = &vm.objects[ObjectIndex::from_raw(48)] else {
                panic!(
                    "expected String, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(48)]
                );
            };

            // Expected format with proper indentation
            let expected = r#"Person {
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
}"#;

            assert_eq!(result, expected);
            Ok(())
        },
    )
}
