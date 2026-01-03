//! Compiler tests for watch functionality.

use baml_tests::{
    codegen::{Program, assert_compiles},
    vm::{Instruction, Value},
};

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
                // Initialize locals with null
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                // Initialize watched variable
                Instruction::LoadConst(Value::Int(0)),
                Instruction::StoreVar("value".to_string()),
                // Register watch (only once, at initialization)
                Instruction::LoadConst(Value::string("value")), // channel "value"
                Instruction::LoadConst(Value::Null),            // filter null
                Instruction::Watch(2),
                // Assignment: value = 1
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("value".to_string()),
                // Return value
                Instruction::LoadVar("value".to_string()),
                Instruction::StoreVar("_0".to_string()),
                // Unwatch on scope exit
                Instruction::Unwatch(2),
                Instruction::LoadVar("_0".to_string()),
                Instruction::Return,
            ],
        )],
    })
}
