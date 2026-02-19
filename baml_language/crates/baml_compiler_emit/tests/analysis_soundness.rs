//! Bytecode-shape regressions for analysis soundness.

use baml_tests::{
    codegen::{Program, assert_compiles},
    vm::{Instruction, Value},
};

#[test]
fn virtual_cross_block_soundness_codegen() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function main(c: bool) -> int {
                let a = 1;
                let b = a;
                if (c) {
                    a = 2;
                }
                b
            }
        "#,
        expected: vec![(
            "main",
            vec![
                Instruction::InitLocals(2),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("a".to_string()),
                Instruction::LoadVar("a".to_string()),
                Instruction::StoreVar("b".to_string()),
                Instruction::LoadVar("c".to_string()),
                Instruction::PopJumpIfFalse(3),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::StoreVar("a".to_string()),
                Instruction::LoadVar("b".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn virtual_cross_block_param_mutation_soundness_codegen() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function main(c: bool, p: int) -> int {
                let x = p;
                if (c) {
                    p = 2;
                }
                x
            }
        "#,
        expected: vec![(
            "main",
            vec![
                Instruction::InitLocals(1),
                Instruction::LoadVar("p".to_string()),
                Instruction::StoreVar("x".to_string()),
                Instruction::LoadVar("c".to_string()),
                Instruction::PopJumpIfFalse(3),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::StoreVar("p".to_string()),
                Instruction::LoadVar("x".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn copy_of_mutable_param_soundness_codegen() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function main(x: int) -> int {
                let y = x;
                x = 2;
                y
            }
        "#,
        expected: vec![(
            "main",
            vec![
                Instruction::InitLocals(1),
                Instruction::LoadVar("x".to_string()),
                Instruction::StoreVar("y".to_string()),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::StoreVar("x".to_string()),
                Instruction::LoadVar("y".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn virtual_cross_block_transitive_param_mutation_soundness_codegen() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function main(c: bool, p: int) -> int {
                let t = p;
                let x = t;
                if (c) {
                    p = 2;
                }
                x
            }
        "#,
        expected: vec![(
            "main",
            vec![
                Instruction::InitLocals(1),
                Instruction::LoadVar("p".to_string()),
                Instruction::StoreVar("x".to_string()),
                Instruction::LoadVar("c".to_string()),
                Instruction::PopJumpIfFalse(3),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::StoreVar("p".to_string()),
                Instruction::LoadVar("x".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn virtual_multiple_defs_preserve_side_effects_codegen() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function fail() -> int {
                assert(false);
                1
            }

            function main() -> int {
                let x = fail();
                x = 2;
                x
            }
        "#,
        expected: vec![
            (
                "fail",
                vec![
                    Instruction::LoadConst(Value::Bool(false)),
                    Instruction::Assert,
                    Instruction::LoadConst(Value::Int(1)),
                    Instruction::Return,
                ],
            ),
            (
                "main",
                vec![
                    Instruction::InitLocals(1),
                    Instruction::LoadGlobal(Value::function("fail")),
                    Instruction::Call(0),
                    Instruction::StoreVar("x".to_string()),
                    Instruction::LoadConst(Value::Int(2)),
                    Instruction::StoreVar("x".to_string()),
                    Instruction::LoadVar("x".to_string()),
                    Instruction::Return,
                ],
            ),
        ],
    })
}
