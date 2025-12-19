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
            // THIR codegen (efficient):
            // vec![Instruction::LoadConst(Value::Int(42)), Instruction::Return],
            // MIR codegen (naive) - same semantics, more instructions:
            vec![
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Int(42)),
                Instruction::StoreVar("_0".to_string()),
                Instruction::Jump(1),
                Instruction::LoadVar("_0".to_string()),
                Instruction::Return,
            ],
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
            // THIR codegen (efficient):
            // vec![
            //     Instruction::LoadConst(Value::Bool(true)),
            //     Instruction::Return,
            // ],
            // MIR codegen (naive) - same semantics, more instructions:
            vec![
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::StoreVar("_0".to_string()),
                Instruction::Jump(1),
                Instruction::LoadVar("_0".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
#[ignore = "string literals not yet supported in HIR"]
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
                // Stackification codegen:
                vec![
                    Instruction::LoadConst(Value::Null),
                    Instruction::LoadConst(Value::Int(1)),
                    Instruction::StoreVar("_0".to_string()),
                    Instruction::Jump(1),
                    Instruction::LoadVar("_0".to_string()),
                    Instruction::Return,
                ],
            ),
            (
                "main",
                // Stackification codegen - function reference is virtual:
                vec![
                    Instruction::LoadConst(Value::Null),
                    // Load function directly (inlined)
                    Instruction::LoadGlobal(Value::function("one")),
                    Instruction::Call(0),
                    Instruction::StoreVar("_0".to_string()),
                    // Exit block and return block share same jump target
                    Instruction::Jump(1),
                    Instruction::Jump(1),
                    Instruction::LoadVar("_0".to_string()),
                    Instruction::Return,
                ],
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
                // Stackification codegen:
                vec![
                    Instruction::LoadConst(Value::Null),
                    Instruction::LoadConst(Value::Int(2)),
                    Instruction::StoreVar("_0".to_string()),
                    Instruction::Jump(1),
                    Instruction::LoadVar("_0".to_string()),
                    Instruction::Return,
                ],
            ),
            (
                "main",
                // Stackification codegen - function reference is virtual, a is real:
                vec![
                    Instruction::LoadConst(Value::Null), // _0
                    Instruction::LoadConst(Value::Null), // a (user variable, used later)
                    // Load function directly (inlined)
                    Instruction::LoadGlobal(Value::function("two")),
                    Instruction::Call(0),
                    Instruction::StoreVar("a".to_string()),
                    Instruction::Jump(1),
                    // Return a
                    Instruction::LoadVar("a".to_string()),
                    Instruction::StoreVar("_0".to_string()),
                    Instruction::Jump(1),
                    Instruction::LoadVar("_0".to_string()),
                    Instruction::Return,
                ],
            ),
        ],
    })
}

#[test]
#[ignore = "assignment statements not yet in HIR"]
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
                    Instruction::LoadConst(Value::Int(3)),
                    Instruction::LoadConst(Value::Int(5)),
                    Instruction::StoreVar("y".to_string()),
                    Instruction::LoadVar("y".to_string()),
                    Instruction::Return,
                ],
            ),
            (
                "MutableInArg",
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
