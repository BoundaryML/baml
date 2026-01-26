//! Compiler tests for watch functionality.

use baml_vm::test::{Instruction, Value};

mod common;
use common::{assert_compiles, Program};

#[test]
fn watch_primitive() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function primitive() -> int {
                watch let value = 0;

                value = 1;

                value
            }
        ",
        expected: vec![(
            "primitive",
            vec![
                Instruction::LoadConst(Value::Int(0)),
                Instruction::LoadConst(Value::string("value")), // channel "value"
                Instruction::LoadConst(Value::Null),            // filter null
                Instruction::Watch(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("value".to_string()),
                Instruction::LoadVar("value".to_string()),
                Instruction::Return,
            ],
        )],
    })
}
