//! VM tests for strings.

use baml_vm::{ObjectIndex, Value, VmExecState};

mod common;
use common::{assert_vm_executes, assert_vm_executes_with_inspection, Program};

#[test]
fn concat_strings() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: r#"
                fn main() -> string {
                    let a = "Hello";
                    let b = " World";

                    a + b
                }
            "#,
            function: "main",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(39))),
        },
        |vm| {
            let baml_vm::Object::String(s) = &vm.objects[ObjectIndex::from_raw(39)] else {
                panic!(
                    "expected string, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(39)]
                );
            };

            assert_eq!(s, "Hello World");

            Ok(())
        },
    )
}

#[test]
fn string_equality_true() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
                fn main() -> bool {
                    "Hello" == "Hello"
                }
            "#,
        function: "main",
        expected: VmExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn string_equality_false() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> bool {
                "Hello" == "World"
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Bool(false)),
    })
}

#[test]
fn string_not_equal_true() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
                fn main() -> bool {
                    "Hello" != "World"
                }
            "#,
        function: "main",
        expected: VmExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn string_less_than() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
                fn main() -> bool {
                    "a" < "b"
                }
            "#,
        function: "main",
        expected: VmExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn string_less_than_or_equal() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
                fn main() -> bool {
                    "a" <= "b"
                }
            "#,
        function: "main",
        expected: VmExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn string_greater_than() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
                fn main() -> bool {
                    "b" > "a"
                }
            "#,
        function: "main",
        expected: VmExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn string_greater_than_or_equal() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
                fn main() -> bool {
                    "b" >= "a"
                }
            "#,
        function: "main",
        expected: VmExecState::Complete(Value::Bool(true)),
    })
}
