//! VM tests for arrays.

use baml_vm::{errors::VmError, RuntimeError, Value};

mod common;

use crate::common::{assert_vm_fails_with_inspection, FailingProgram};

// Array tests
#[test]
fn error_stack_trace() -> anyhow::Result<()> {
    assert_vm_fails_with_inspection(
        FailingProgram {
            source: "
                function three() -> int {
                    return 3 / 0;
                }

                function two() -> int {
                    return three();
                }

                function one() -> int {
                    return two();
                }

                function main() -> int {
                    let t = one();

                    t
                }
            ",
            function: "main",
            expected: VmError::RuntimeError(RuntimeError::DivisionByZero {
                left: Value::Int(3),
                right: Value::Int(0),
            }),
        },
        |vm| {
            let stack_trace = vm.stack_trace(VmError::RuntimeError(RuntimeError::DivisionByZero {
                left: Value::Int(3),
                right: Value::Int(0),
            }));

            assert_eq!(stack_trace.trace.len(), 4);
            assert_eq!(stack_trace.trace[0].function_name, "main");
            assert_eq!(stack_trace.trace[1].function_name, "one");
            assert_eq!(stack_trace.trace[2].function_name, "two");
            assert_eq!(stack_trace.trace[3].function_name, "three");
            assert_eq!(stack_trace.trace[0].error_line, 14);
            assert_eq!(stack_trace.trace[1].error_line, 10);
            assert_eq!(stack_trace.trace[2].error_line, 6);
            assert_eq!(stack_trace.trace[3].error_line, 2);

            Ok(())
        },
    )
}
