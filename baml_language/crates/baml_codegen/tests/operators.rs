//! Compiler tests for operators (arithmetic, logical, assignment).

use baml_tests::{
    codegen::{Program, assert_compiles},
    vm::{Instruction, Value},
};
use baml_vm::BinOp;

#[test]
fn basic_and() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function ret_bool() -> bool {
                true
            }

            function main() -> bool {
                true && ret_bool()
            }
        "#,
        expected: vec![(
            "main",
            // THIR codegen (efficient):
            // vec![
            //     Instruction::LoadConst(Value::Bool(true)),
            //     Instruction::JumpIfFalse(4),
            //     Instruction::Pop(1),
            //     Instruction::LoadGlobal(Value::function("ret_bool")),
            //     Instruction::Call(0),
            //     Instruction::Return,
            // ],
            // MIR codegen (naive) - same semantics, more instructions:
            vec![
                // Pre-allocate locals
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                // Evaluate LHS of &&
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::StoreVar("_1".to_string()),
                Instruction::LoadVar("_1".to_string()),
                Instruction::JumpIfFalse(10),
                // Return block
                Instruction::Jump(3),
                Instruction::LoadVar("_0".to_string()),
                Instruction::Return,
                // Evaluate RHS (short-circuit path)
                Instruction::LoadGlobal(Value::function("ret_bool")),
                Instruction::StoreVar("_2".to_string()),
                Instruction::LoadVar("_2".to_string()),
                Instruction::Call(0),
                Instruction::StoreVar("_0".to_string()),
                Instruction::Jump(5),
                // False branch (short-circuit)
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::StoreVar("_0".to_string()),
                Instruction::Jump(1),
                // Control flow
                Instruction::Jump(-11),
                Instruction::Jump(-1),
            ],
        )],
    })
}

#[test]
fn basic_or() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function ret_bool() -> bool {
                true
            }

            function main() -> bool {
                true || ret_bool()
            }
        "#,
        expected: vec![(
            "main",
            // THIR codegen (efficient):
            // vec![
            //     Instruction::LoadConst(Value::Bool(true)),
            //     Instruction::JumpIfFalse(2),
            //     Instruction::Jump(4),
            //     Instruction::Pop(1),
            //     Instruction::LoadGlobal(Value::function("ret_bool")),
            //     Instruction::Call(0),
            //     Instruction::Return,
            // ],
            // MIR codegen (naive) - same semantics, more instructions:
            vec![
                // Pre-allocate locals
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                // Evaluate LHS of ||
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::StoreVar("_1".to_string()),
                Instruction::LoadVar("_1".to_string()),
                Instruction::JumpIfFalse(7),
                // Return block
                Instruction::Jump(3),
                Instruction::LoadVar("_0".to_string()),
                Instruction::Return,
                // True branch (short-circuit)
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::StoreVar("_0".to_string()),
                Instruction::Jump(7),
                // Evaluate RHS (false path)
                Instruction::LoadGlobal(Value::function("ret_bool")),
                Instruction::StoreVar("_2".to_string()),
                Instruction::LoadVar("_2".to_string()),
                Instruction::Call(0),
                Instruction::StoreVar("_0".to_string()),
                Instruction::Jump(2),
                // Control flow
                Instruction::Jump(-11),
                Instruction::Jump(-1),
            ],
        )],
    })
}

#[test]
fn basic_add() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function main() -> int {
                let a = 1 + 2;
                a
            }
        "#,
        expected: vec![(
            "main",
            // THIR codegen (efficient):
            // vec![
            //     Instruction::LoadConst(Value::Int(1)),
            //     Instruction::LoadConst(Value::Int(2)),
            //     Instruction::BinOp(BinOp::Add),
            //     Instruction::LoadVar("a".to_string()),
            //     Instruction::Return,
            // ],
            // MIR codegen (naive) - same semantics, more instructions:
            vec![
                // Pre-allocate locals
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                // Evaluate 1 + 2
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("_2".to_string()),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::StoreVar("_3".to_string()),
                Instruction::LoadVar("_2".to_string()),
                Instruction::LoadVar("_3".to_string()),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("a".to_string()),
                // Return a
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
#[ignore = "assignment statements not yet in HIR"]
fn basic_assign_add() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function main() -> int {
                let x = 1;
                x += 2;
                x
            }
        "#,
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("x".to_string()),
                Instruction::LoadVar("x".to_string()),
                Instruction::Return,
            ],
        )],
    })
}
