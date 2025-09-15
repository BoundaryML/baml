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
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(45))),
        },
        |vm| {
            let baml_vm::Object::Instance(tree) = &vm.objects[ObjectIndex::from_raw(45)] else {
                panic!(
                    "expected Instance, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(45)]
                );
            };

            let baml_vm::Object::Instance(copy) = &vm.objects[ObjectIndex::from_raw(42)] else {
                panic!(
                    "expected Instance, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(42)]
                );
            };

            assert_eq!(tree.class, copy.class);

            Ok(())
        },
    )
}
