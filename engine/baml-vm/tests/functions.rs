//! VM tests for function calls, parameters, and return statements.

use baml_compiler::test::ast;
use baml_vm::{
    BamlVmProgram, EvalStack, Frame, GlobalPool, ObjectIndex, ObjectPool, StackIndex, Value, Vm,
    VmExecState,
};

mod common;
use common::{assert_vm_executes, assert_vm_executes_with_inspection, Program};

#[test]
fn return_function_call() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
            fn one() -> int {
                1
            }

            fn main() -> int {
                one()
            }
        ",
        function: "main",
        expected: VmExecState::Complete(Value::Int(1)),
    })
}

#[test]
fn function_call_without_parameters() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
            fn two() -> int {
                let v = 2;
                v
            }

            fn main() -> int {
                let v = two();
                v
            }
        ",
        function: "main",
        expected: VmExecState::Complete(Value::Int(2)),
    })
}

#[test]
fn function_call_with_parameters() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
            fn one_of(a: int, b: int) -> int {
                a
            }

            fn main() -> int {
                let v = one_of(1, 2);
                v
            }
        ",
        function: "main",
        expected: VmExecState::Complete(Value::Int(1)),
    })
}

#[test]
fn function_returning_string() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: r#"
                fn main() -> string {
                    "hello"
                }
            "#,
            function: "main",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(0))),
        },
        |vm| {
            let baml_vm::Object::String(string) = &vm.objects[ObjectIndex::from_raw(0)] else {
                panic!(
                    "expected String, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(0)]
                );
            };

            assert_eq!(string, "hello");

            Ok(())
        },
    )
}

#[test]
fn multiple_strings() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: r#"
                fn get_greeting() -> string {
                    "Hello"
                }

                fn main() -> string {
                    let greeting = get_greeting();
                    let name = "World";
                    greeting
                }
            "#,
            function: "main",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(0))), // "Hello" should be the first string object
        },
        |vm| {
            // Check that we have the expected strings in the objects pool
            let strings: Vec<&str> = vm
                .objects
                .iter()
                .filter_map(|obj| match obj {
                    baml_vm::Object::String(s) => Some(s.as_str()),
                    _ => None,
                })
                .collect();

            assert!(strings.contains(&"Hello"));
            assert!(strings.contains(&"World"));

            Ok(())
        },
    )
}

#[test]
fn early_return() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn EarlyReturn(x: int) -> int {
               if (x == 42) { return 1; }

               x + 5
            }

            fn main() -> int {
                EarlyReturn(42)
            }"#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(1)),
    })
}

#[test]
fn return_with_stack() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn WithStack() -> int {
               let a = 1;

               if (a == 0) { return 0; }

               {
                  let b = 1;
                  if (a != b) {
                     return 0;
                  }
               }

               {
                  let c = 2;
                  let b = 3;
                  while (b != c) {
                     if (true) {
                          return 0;
                     }
                  }
                }

                7
            }"#,
        function: "WithStack",
        expected: VmExecState::Complete(Value::Int(0)),
    })
}
