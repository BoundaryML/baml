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
            // THIR codegen (efficient):
            // vec![
            //     Instruction::LoadConst(Value::Int(1)),
            //     Instruction::LoadConst(Value::Int(2)),
            //     Instruction::LoadConst(Value::Int(3)),
            //     Instruction::AllocArray(3),
            //     Instruction::LoadVar("a".to_string()),
            //     Instruction::Return,
            // ],
            // MIR codegen (naive) - same semantics, more instructions:
            vec![
                // Pre-allocate locals with null
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                // Evaluate array elements to temps
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("_2".to_string()),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::StoreVar("_3".to_string()),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::StoreVar("_4".to_string()),
                // Load temps and create array
                Instruction::LoadVar("_2".to_string()),
                Instruction::LoadVar("_3".to_string()),
                Instruction::LoadVar("_4".to_string()),
                Instruction::AllocArray(3),
                Instruction::StoreVar("a".to_string()),
                // Return value
                Instruction::LoadVar("a".to_string()),
                Instruction::StoreVar("_0".to_string()),
                Instruction::Jump(1),
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
            // THIR codegen (efficient):
            // vec![
            //     Instruction::LoadConst(Value::Int(1)),
            //     Instruction::LoadConst(Value::Int(2)),
            //     Instruction::LoadConst(Value::Int(3)),
            //     Instruction::AllocArray(3),
            //     Instruction::Return,
            // ],
            // MIR codegen (naive) - same semantics, more instructions:
            vec![
                // Pre-allocate locals with null
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                // Evaluate array elements to temps
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("_1".to_string()),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::StoreVar("_2".to_string()),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::StoreVar("_3".to_string()),
                // Load temps and create array
                Instruction::LoadVar("_1".to_string()),
                Instruction::LoadVar("_2".to_string()),
                Instruction::LoadVar("_3".to_string()),
                Instruction::AllocArray(3),
                Instruction::StoreVar("_0".to_string()),
                // Return
                Instruction::Jump(1),
                Instruction::LoadVar("_0".to_string()),
                Instruction::Return,
            ],
        )],
    })
}
