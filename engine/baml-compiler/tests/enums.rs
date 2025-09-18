//! Compiler tests for enum variants.

use baml_vm::{Instruction, ObjectIndex};

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
                Instruction::LoadConst(0),
                Instruction::AllocVariant(ObjectIndex::from_raw(3)),
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
                Instruction::LoadConst(0),
                Instruction::AllocVariant(ObjectIndex::from_raw(3)),
                Instruction::LoadVar(1),
                Instruction::Return,
            ],
        )],
    })
}
