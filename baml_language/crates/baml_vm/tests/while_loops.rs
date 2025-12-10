//! VM tests for while loops, break, and continue.

use baml_tests::bytecode::{ExecState, Program, Value, assert_vm_executes};

#[test]
#[ignore = "loop codegen causes infinite loop"]
fn while_loop() -> anyhow::Result<()> {
    const SOURCE: &str = r#"
        function GCD(a: int, b: int) -> int {

            while (a != b) {

               if (a > b) {
                   a = a - b;
               } else {
                   b = b - a;
               }

            }

            a
        }

        function main() -> int {
            GCD(21, 15)
        }
    "#;

    assert_vm_executes(Program {
        source: SOURCE,
        function: "main",
        expected: ExecState::Complete(Value::Int(3)),
    })
}

#[test]
#[ignore = "loop codegen causes infinite loop"]
fn while_with_scope() -> anyhow::Result<()> {
    const SOURCE: &str = r#"
        function Fib(n: int) -> int {

            let a = 0;
            let b = 1;

            while (n > 0) {
                n -= 1;
                let t = a + b;
                b = a;
                a = t;
            }

            a
        }

        function main() -> int {
            Fib(5)
        }
    "#;

    assert_vm_executes(Program {
        source: SOURCE,
        function: "main",
        expected: ExecState::Complete(Value::Int(5)),
    })
}

#[test]
#[ignore = "loop codegen causes infinite loop"]
fn break_factorial() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function Factorial(limit: int) -> int {
                let result = 1;

                while (true) {
                    if (limit == 0) {
                        break;
                    }
                    result = result * limit;
                    limit = limit - 1;
                }

                result
            }

            function main() -> int {
                Factorial(5)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(120)),
    })
}

#[test]
fn break_nested_loops() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function Nested() -> int {
                let a = 5;
                while (true) {
                    while (true) {
                        a = a + 1;
                        break;
                    }
                    a = a + 1;
                    break;
                }
                a
            }

            function main() -> int {
                Nested()
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(7)),
    })
}

#[test]
#[ignore = "loop codegen causes infinite loop"]
fn continue_factorial() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function Factorial(limit: int) -> int {
                let result = 1;

                // used to make the loop break without relying on `break` implementation.
                let should_continue = true;
                while (should_continue) {
                    result = result * limit;
                    limit = limit - 1;

                    if (limit != 0) {
                        continue;
                    } else {
                        should_continue = false;
                    }
                }

                result
            }

            function main() -> int {
                Factorial(5)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(120)),
    })
}

#[test]
#[ignore = "loop codegen causes infinite loop"]
fn continue_nested() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function ContinueNested() -> int {
                let execute = true;
                while (execute) {
                    while (false) {
                        continue;
                    }
                    if (false) {
                        continue;
                    }
                    execute = false;
                }
                5
            }

            function main() -> int {
                ContinueNested()
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(5)),
    })
}
