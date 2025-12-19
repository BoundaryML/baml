//! Compiler tests for array construction.

use baml_tests::{
    codegen::{Program, assert_compiles},
    vm::{Instruction, Value},
};

#[test]
fn array_constructor() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int[] {
                let a = [1, 2, 3];
                a
            }
        ",
        expected: vec![(
            "main",
            // Stackification codegen - virtual locals inlined:
            vec![
                // Pre-allocate only real locals (_0 return value)
                Instruction::LoadConst(Value::Null),
                // Array elements pushed directly (temps are virtual)
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::AllocArray(3),
                // Store to _0 (return value)
                Instruction::StoreVar("_0".to_string()),
                // Goto exit block
                Instruction::Jump(1),
                // Return
                Instruction::LoadVar("_0".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn return_array_literal() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int[] {
                [1, 2, 3]
            }
        ",
        expected: vec![(
            "main",
            // Stackification codegen - virtual locals inlined:
            vec![
                // Pre-allocate only real locals (_0 return value)
                Instruction::LoadConst(Value::Null),
                // Array elements pushed directly (temps are virtual)
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::AllocArray(3),
                // Store to _0 (return value)
                Instruction::StoreVar("_0".to_string()),
                // Goto exit block
                Instruction::Jump(1),
                // Return
                Instruction::LoadVar("_0".to_string()),
                Instruction::Return,
            ],
        )],
    })
}
