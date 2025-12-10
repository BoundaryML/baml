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
            vec![
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::AllocArray(3),
                Instruction::LoadVar("a".to_string()),
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
            vec![
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::AllocArray(3),
                Instruction::Return,
            ],
        )],
    })
}
