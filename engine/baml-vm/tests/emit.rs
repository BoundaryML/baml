//! VM tests for emit functionality.

mod common;
use common::{assert_vm_emits, EmitProgram, Node};

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
