mod common;

use common::{assert_vm_executes, ExecState, Object, Program, Value};

#[test]
fn test_instanceof_type_narrowing_simple() -> anyhow::Result<()> {
    // This test verifies that the basic instanceof narrowing works
    // when the variable is statically known to be a specific type
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
                    // After instanceof check, we can access Foo's fields
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
fn test_instanceof_false_case() -> anyhow::Result<()> {
    // This test verifies that instanceof returns false when types don't match
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
                    // x is Bar, not Foo
                    return "not foo";
                }
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::string("not foo")),
    })
}

#[test]
fn test_nested_instanceof_checks() -> anyhow::Result<()> {
    // This test verifies nested instanceof checks work
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
                    // Should reach here and access B's field
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
