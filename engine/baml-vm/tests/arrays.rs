//! VM tests for arrays.

use baml_vm::{ObjectIndex, Value, VmExecState};

mod common;
use common::{assert_vm_executes, assert_vm_executes_with_inspection, Program};

// Array tests
#[test]
fn array_constructor() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: "
                fn main() -> int[] {
                    let a = [1, 2, 3];
                    a
                }
            ",
            function: "main",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(34))),
        },
        |vm| {
            let baml_vm::Object::Array(array) = &vm.objects[ObjectIndex::from_raw(34)] else {
                panic!(
                    "expected Array, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(34)]
                );
            };

            assert_eq!(array, &[Value::Int(1), Value::Int(2), Value::Int(3)]);

            Ok(())
        },
    )
}
