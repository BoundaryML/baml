//! VM tests for function calls, parameters, and return statements.

mod common;
use common::{assert_vm_executes, ExecState, Program, Value};

use crate::common::Object;

#[test]
fn return_function_call() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
            function one() -> int {
                1
            }

            function main() -> int {
                one()
            }
        ",
        function: "main",
        expected: ExecState::Complete(Value::Int(1)),
    })
}

#[test]
fn function_call_without_parameters() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
            function two() -> int {
                let v = 2;
                v
            }

            function main() -> int {
                let v = two();
                v
            }
        ",
        function: "main",
        expected: ExecState::Complete(Value::Int(2)),
    })
}

#[test]
fn function_call_with_parameters() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
            function one_of(a: int, b: int) -> int {
                a
            }

            function main() -> int {
                let v = one_of(1, 2);
                v
            }
        ",
        function: "main",
        expected: ExecState::Complete(Value::Int(1)),
    })
}

#[test]
fn function_returning_string() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> string {
                "hello"
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("hello")),
    })
}

#[test]
fn multiple_strings() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function get_greeting() -> string {
                "Hello"
            }

            function main() -> string {
                let greeting = get_greeting();
                let name = "World";
                greeting
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("Hello")),
    })
}

#[test]
fn early_return() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function EarlyReturn(x: int) -> int {
               if (x == 42) { return 1; }

               x + 5
            }

            function main() -> int {
                EarlyReturn(42)
            }"#,
        function: "main",
        expected: ExecState::Complete(Value::Int(1)),
    })
}

#[test]
fn return_with_stack() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function WithStack() -> int {
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
        expected: ExecState::Complete(Value::Int(0)),
    })
}

#[test]
fn recursive() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function fib(n: int) -> int {
                if (n <= 1) {
                    n
                } else {
                    fib(n - 1) + fib(n - 2)
                }
            }

            function main() -> int {
                fib(3)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(2)),
    })
}
