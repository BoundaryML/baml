//! VM tests for instanceof operator & narrowing.

use baml_tests::bytecode::{ExecState, Program, Value, assert_vm_executes};

#[test]
fn instance_of_returns_true() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class StopTool {
                action "stop"
            }

            function main() -> bool {
                let t = StopTool { action: "stop" };

                t instanceof StopTool
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn instance_of_returns_false() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class StopTool {
                action "stop"
            }

            class StartTool {
                action "start"
            }

            function main() -> bool {
                let t = StopTool { action: "stop" };

                t instanceof StartTool
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(false)),
    })
}

#[test]
fn instanceof_narrowing_true_branch() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Foo {
                field string
            }

            class Bar {
                other int
            }

            function main() -> string {
                let x = Foo { field: "test value" };

                if (x instanceof Foo) {
                    return x.field;
                } else {
                    return "not foo";
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("test value")),
    })
}

#[test]
fn instanceof_narrowing_false_branch() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Foo {
                field string
            }

            class Bar {
                other int
            }

            function main() -> string {
                let x = Bar { other: 42 };

                if (x instanceof Foo) {
                    return "is foo";
                } else {
                    return "not foo";
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("not foo")),
    })
}

#[test]
fn instanceof_chained_checks() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class A {
                a_field string
            }

            class B {
                b_field string
            }

            class C {
                c_field string
            }

            function main() -> string {
                let x = B { b_field: "b value" };

                if (x instanceof A) {
                    return "is A";
                } else if (x instanceof B) {
                    return x.b_field;
                } else if (x instanceof C) {
                    return "is C";
                } else {
                    return "unknown";
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("b value")),
    })
}
