//! Compiler tests for function calls, parameters, and returns.

use baml_vm::{GlobalIndex, Instruction};

mod common;
use common::{assert_compiles, Program};

#[test]
fn return_function_call() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function one() -> int {
                1
            }

            function main() -> int {
                one()
            }
        ",
        expected: vec![
            ("one", vec![Instruction::LoadConst(0), Instruction::Return]),
            (
                "main",
                vec![
                    Instruction::LoadGlobal(GlobalIndex::from_raw(0)),
                    Instruction::Call(0),
                    Instruction::Return,
                ],
            ),
        ],
    })
}

#[test]
fn call_function() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function two() -> int {
                2
            }

            function main() -> int {
                let a = two();
                a
            }
        ",
        expected: vec![
            ("two", vec![Instruction::LoadConst(0), Instruction::Return]),
            (
                "main",
                vec![
                    Instruction::LoadGlobal(GlobalIndex::from_raw(0)),
                    Instruction::Call(0),
                    Instruction::LoadVar(1),
                    Instruction::Return,
                ],
            ),
        ],
    })
}

#[test]
fn function_returning_string() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function main() -> string {
                "hello"
            }
        "#,
        expected: vec![("main", vec![Instruction::LoadConst(0), Instruction::Return])],
    })
}

#[test]
fn mutable_variables() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function DeclareMutableInFunction(x: int) -> int {

                let y = 3;

                y = 5;

                y
            }

            function MutableInArg(x: int) -> int {
                x = 3;
                x
            }
        "#,
        expected: vec![
            (
                "DeclareMutableInFunction",
                vec![
                    Instruction::LoadConst(0),
                    Instruction::LoadConst(1),
                    Instruction::StoreVar(2),
                    Instruction::LoadVar(2),
                    Instruction::Return,
                ],
            ),
            (
                "MutableInArg",
                vec![
                    Instruction::LoadConst(0),
                    Instruction::StoreVar(1),
                    Instruction::LoadVar(1),
                    Instruction::Return,
                ],
            ),
        ],
    })
}
