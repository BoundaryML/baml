//! VM tests for control flow statements (if/else, while, for loops, break, continue).

mod common;
use common::{assert_vm_executes, ExecState, Program, Value};

#[test]
fn exec_if_branch() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
            function run_if(b: bool) -> int {
                if (b) { 1 } else { 2 }
            }

            function main() -> int {
                let a = run_if(true);
                a
            }
        ",
        function: "main",
        expected: ExecState::Complete(Value::Int(1)),
    })
}

#[test]
fn exec_else_branch() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
            function run_if(b: bool) -> int {
                if (b) { 1 } else { 2 }
            }

            function main() -> int {
                let a = run_if(false);
                a
            }
        ",
        function: "main",
        expected: ExecState::Complete(Value::Int(2)),
    })
}

#[test]
fn exec_else_if_branch() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
            function run_if(a: bool, b: bool) -> int {
                if (a) {
                    1
                } else if (b) {
                    2
                } else {
                    3
                }
            }

            function main() -> int {
                let a = run_if(false, true);
                a
            }
        ",
        function: "main",
        expected: ExecState::Complete(Value::Int(2)),
    })
}

#[test]
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

#[test]
fn for_loop_sum() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function Sum(xs: int[]) -> int {
                let result = 0;

                for (let x in xs) {
                    result += x;
                }

                result
            }

            function main() -> int {
                Sum([1, 2, 3, 4])
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(10)),
    })
}

#[test]
fn for_loop_with_break() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function ForWithBreak(xs: int[]) -> int {
                let result = 0;

                for (let x in xs) {
                    if (x > 10) {
                        break;
                    }
                    result += x;
                }

                result
            }

            function main() -> int {
                ForWithBreak([3, 4, 11, 100])
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(7)),
    })
}

#[test]
fn for_loop_with_continue() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function ForWithContinue(xs: int[]) -> int {
                let result = 0;

                for (let x in xs) {
                    if (x > 10) {
                        continue;
                    }
                    result += x;
                }

                result
            }

            function main() -> int {
                ForWithContinue([5, 20, 6])
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(11)),
    })
}

#[test]
fn for_loop_nested() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function NestedFor(arr_a: int[], arr_b: int[]) -> int {

                let result =  0;

                for (let a in arr_a) {
                    for (let b in arr_b) {
                        result += a * b;
                    }
                }

                result
            }

            function main() -> int {
                NestedFor([1, 2], [3, 4])
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(21)),
    })
}

// C-style for loops
#[test]
fn c_for_sum_to_ten() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function SumToTen() -> int {
                let s = 0;

                for (let i = 1; i <= 10; i += 1) {
                    s += i;
                }

                s
            }"#,
        function: "SumToTen",
        expected: ExecState::Complete(Value::Int(55)),
    })
}

#[test]
fn c_for_after_with_break_continue() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function SumToTen() -> int {
                let s = 0;

                for (let i = 0; ; s += i) {
                    i += 1;
                    if (i > 10) {
                        let x = 0; // this tests that popping is correct.
                        break;
                    }
                    if (i == 5) {
                        // since `s += i` is in the for loop's after, this 'continue' is
                        // actually irrelevant and the function does the same as SumToTen.
                        // That's the behavior we're looking for.
                        continue;
                    }
                }

                s
            }"#,
        function: "SumToTen",
        expected: ExecState::Complete(Value::Int(55)),
    })
}

#[test]
fn c_for_only_cond() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function OnlyCond() -> int {
                let s = 0;

                for (; false;) {
                }

                s
            }"#,
        function: "OnlyCond",
        expected: ExecState::Complete(Value::Int(0)),
    })
}

#[test]
fn c_for_endless() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function Nothing() -> int {
                let s = 0;

                for (;;) {
                    break;
                }

                s
            }"#,
        function: "Nothing",
        expected: ExecState::Complete(Value::Int(0)),
    })
}

// Block expressions
#[test]
fn block_expr() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
            function main() -> int {
                let a = {
                    let b = 1;
                    b
                };

                a
            }
        ",
        function: "main",
        expected: ExecState::Complete(Value::Int(1)),
    })
}
