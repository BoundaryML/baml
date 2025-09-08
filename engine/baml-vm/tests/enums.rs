//! VM tests for enum variants.

use baml_vm::{ObjectIndex, Value, VmExecState};

mod common;
use common::{assert_vm_executes_with_inspection, Program};

#[test]
fn return_enum_variant() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: r#"
                enum Shape {
                    Square
                    Rectangle
                    Circle
                }

                fn main() -> Shape {
                    Shape.Rectangle
                }
            "#,
            function: "main",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(7))),
        },
        |vm| {
            let baml_vm::Object::Variant(variant) = &vm.objects[ObjectIndex::from_raw(7)] else {
                panic!(
                    "expected Variant, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(7)]
                );
            };

            assert_eq!(variant.index, 1);

            Ok(())
        },
    )
}

#[test]
fn assign_enum_variant() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: r#"
                enum Shape {
                    Square
                    Rectangle
                    Circle
                }

                fn main() -> Shape {
                    let s = Shape.Rectangle;
                    s
                }
            "#,
            function: "main",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(7))),
        },
        |vm| {
            let baml_vm::Object::Variant(variant) = &vm.objects[ObjectIndex::from_raw(7)] else {
                panic!(
                    "expected Variant, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(7)]
                );
            };

            assert_eq!(variant.index, 1);

            Ok(())
        },
    )
}

#[test]
fn take_and_return_enum_variant() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: r#"
                enum Shape {
                    Square
                    Rectangle
                    Circle
                }

                function return_shape(shape: Shape) -> Shape {
                    shape
                }

                fn main() -> Shape {
                    return_shape(Shape.Rectangle)
                }
            "#,
            function: "main",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(8))),
        },
        |vm| {
            let baml_vm::Object::Variant(variant) = &vm.objects[ObjectIndex::from_raw(8)] else {
                panic!(
                    "expected Variant, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(8)]
                );
            };

            assert_eq!(variant.index, 1);

            Ok(())
        },
    )
}
