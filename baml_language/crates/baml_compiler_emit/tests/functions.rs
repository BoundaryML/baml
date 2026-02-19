//! Compiler tests for function calls, parameters, and returns.

use baml_tests::{
    codegen::{Program, assert_compiles},
    vm::{Instruction, Value},
};

#[test]
fn return_literal_int() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                42
            }
        ",
        expected: vec![(
            "main",
            // Stackification with Virtual _0 and fall-through elimination:
            vec![Instruction::LoadConst(Value::Int(42)), Instruction::Return],
        )],
    })
}

#[test]
fn return_literal_bool() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> bool {
                true
            }
        ",
        expected: vec![(
            "main",
            // Stackification with Virtual _0 and fall-through elimination:
            vec![
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn return_literal_string() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function main() -> string {
                "hello"
            }
        "#,
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::string("hello")),
                Instruction::Return,
            ],
        )],
    })
}

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
            (
                "one",
                // Stackification with Virtual _0:
                vec![Instruction::LoadConst(Value::Int(1)), Instruction::Return],
            ),
            (
                "main",
                // ReturnPhi optimization: Call result goes directly to stack, no Store/Load
                vec![Instruction::Call("one".to_string()), Instruction::Return],
            ),
        ],
    })
}

#[test]
fn call_function_assign_to_variable() -> anyhow::Result<()> {
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
            (
                "two",
                // Stackification with Virtual _0 and fall-through elimination:
                vec![Instruction::LoadConst(Value::Int(2)), Instruction::Return],
            ),
            (
                "main",
                // CallResultImmediate optimization: Call result stays on stack,
                // used immediately as return value (no Store/Load)
                vec![Instruction::Call("two".to_string()), Instruction::Return],
            ),
        ],
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
                // Multi-def locals are kept real (not virtualized)
                vec![
                    Instruction::LoadConst(Value::Int(3)),
                    Instruction::StoreVar("y".to_string()),
                    Instruction::LoadConst(Value::Int(5)),
                    Instruction::StoreVar("y".to_string()),
                    Instruction::LoadVar("y".to_string()),
                    Instruction::Return,
                ],
            ),
            (
                "MutableInArg",
                // x is a Parameter, so it needs a real slot for the reassignment
                vec![
                    Instruction::LoadConst(Value::Int(3)),
                    Instruction::StoreVar("x".to_string()),
                    Instruction::LoadVar("x".to_string()),
                    Instruction::Return,
                ],
            ),
        ],
    })
}
