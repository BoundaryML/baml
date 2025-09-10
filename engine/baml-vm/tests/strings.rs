//! VM tests for strings.

use baml_vm::{ObjectIndex, Value, VmExecState};

mod common;
use common::{assert_vm_executes_with_inspection, Program};

// Array tests
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
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(36))),
        },
        |vm| {
            let baml_vm::Object::String(s) = &vm.objects[ObjectIndex::from_raw(36)] else {
                panic!(
                    "expected string, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(36)]
                );
            };

            assert_eq!(s, "Hello World");

            Ok(())
        },
    )
}
