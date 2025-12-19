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
            // MIR codegen with stackification:
            // - Temporary variables eliminated
            // - Values stay on stack
            // - Pre-allocated null slots eliminated
            vec![
                // Array elements pushed directly to stack
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::AllocArray(3),
                // Return
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
            // MIR codegen with stackification:
            // - All temporary variables eliminated
            // - Values stay on stack, flow directly to ALLOC_ARRAY
            vec![
                // Array elements pushed directly to stack
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::AllocArray(3),
                // Return - array is on stack
                Instruction::Return,
            ],
        )],
    })
}
