//! Compiler tests for enum variants.

use baml_vm::test::{Instruction, Value};

mod common;
use common::{assert_compiles, Program};

#[test]
fn return_enum_variant() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            enum Shape {
                Square
                Rectangle
                Circle
            }

            function main() -> Shape {
                Shape.Rectangle
            }
        "#,
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Int(1)), // Rectangle is variant index 1
                Instruction::AllocVariant(Value::enm("Shape")),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn assign_enum_variant() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            enum Shape {
                Square
                Rectangle
                Circle
            }

            function main() -> Shape {
                let s = Shape.Rectangle;
                s
            }
        "#,
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Int(1)), // Rectangle is variant index 1
                Instruction::AllocVariant(Value::enm("Shape")),
                Instruction::LoadVar("s".to_string()),
                Instruction::Return,
            ],
        )],
    })
}
