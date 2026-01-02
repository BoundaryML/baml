//! Compiler tests for enum variants.

use baml_tests::{
    codegen::{Program, assert_compiles},
    vm::{Instruction, Value},
};

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
    // When a variable is single-use, the compiler optimizes it away (stackification).
    // The enum value stays on the stack and is returned directly.
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
                Instruction::Return,
            ],
        )],
    })
}
