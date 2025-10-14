//! VM tests for emit functionality.

mod common;
use common::{assert_vm_emits, EmitProgram, Node, Object, Value};
use indexmap::indexmap;

// removed incorrect import path to common module

#[test]
fn emit_primitive_on_change() -> anyhow::Result<()> {
    assert_vm_emits(EmitProgram {
        source: r#"
            function primitive() -> int {
                let value = 0 @emit;

                value = 1;

                value
            }
        "#,
        function: "primitive",
        expected: vec![vec![Node::variable("value")]],
    })
}

#[test]
fn emit_primitive_on_nested_scope() -> anyhow::Result<()> {
    assert_vm_emits(EmitProgram {
        source: r#"
            function primitive() -> int {
                let value = 0 @emit;

                if (true) {
                    value = 1;
                }

                value
            }
        "#,
        function: "primitive",
        expected: vec![vec![Node::variable("value")]],
    })
}

#[test]
fn emit_object_when_binding_goes_out_of_scope() -> anyhow::Result<()> {
    assert_vm_emits(EmitProgram {
        source: r#"
            class Point {
               x int
               y int
            }

            function object() -> Point {
                let p = {
                    let scoped = Point { x: 0, y: 0 } @emit;
                    scoped
                };

                p.x = 1;

                p
            }
        "#,
        function: "object",
        expected: vec![vec![Node::Object(Object::instance(
            "Point",
            indexmap! {
                "x" => Value::Int(0),
                "y" => Value::Int(0),
            },
        ))]],
    })
}
