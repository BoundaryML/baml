//! VM tests for enum variants.

mod common;
use common::{assert_vm_executes, ExecState, Program, Value};

use crate::common::{Object, Variant};

#[test]
fn return_enum_variant() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            enum Shape {
                Square
                Rectangle
                Circle
            }

            function main() -> Shape {
                Shape.Rectangle
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Object(Object::Variant(Variant {
            enm: String::from("Shape"),
            variant: String::from("Rectangle"),
        }))),
    })
}

#[test]
fn assign_enum_variant() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            enum Shape {
                Square
                Rectangle
                Circle
            }

            function main() -> Shape {
                let s = Shape.Rectangle;
                s
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Object(Object::Variant(Variant {
            enm: String::from("Shape"),
            variant: String::from("Rectangle"),
        }))),
    })
}

#[test]
fn take_and_return_enum_variant() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            enum Shape {
                Square
                Rectangle
                Circle
            }

            function return_shape(shape: Shape) -> Shape {
                shape
            }

            function main() -> Shape {
                return_shape(Shape.Rectangle)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Object(Object::Variant(Variant {
            enm: String::from("Shape"),
            variant: String::from("Rectangle"),
        }))),
    })
}
