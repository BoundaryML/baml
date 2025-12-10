//! VM tests for for loops (for-in and C-style for).

use baml_tests::bytecode::{ExecState, Program, Value, assert_vm_executes};

#[test]
#[ignore = "loop codegen causes infinite loop"]
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
#[ignore = "loop codegen causes infinite loop"]
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
#[ignore = "loop codegen causes infinite loop"]
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
#[ignore = "loop codegen causes infinite loop"]
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
#[ignore = "loop codegen causes infinite loop"]
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
#[ignore = "loop codegen causes infinite loop"]
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
#[ignore = "loop codegen causes infinite loop"]
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
#[ignore = "loop codegen causes infinite loop"]
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
