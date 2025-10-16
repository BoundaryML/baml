//! Compiler tests for built-in method calls.

use baml_vm::{GlobalIndex, Instruction};

mod common;
use common::{assert_compiles, Program};

#[test]
fn builtin_method_call() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function main() -> int {
                let arr = [1, 2, 3];
                arr.length()
            }
        "#,
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(0),
                Instruction::LoadConst(1),
                Instruction::LoadConst(2),
                Instruction::AllocArray(3),
                Instruction::LoadGlobal(GlobalIndex::from_raw(3)),
                Instruction::LoadVar(1),
                // call with one argument (self)
                Instruction::Call(1),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn fetch_as() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            class DummyJsonTodo {
                id int
                todo string
                completed bool
                userId int
            }

            function main() -> DummyJsonTodo {
                baml.fetch_as<DummyJsonTodo>("https://dummyjson.com/todos/1")
            }
        "#,
        expected: vec![(
            "main",
            vec![
                Instruction::LoadGlobal(GlobalIndex::from_raw(38)),
                Instruction::LoadConst(0),
                Instruction::LoadConst(1),
                Instruction::DispatchFuture(2),
                Instruction::Await,
                Instruction::Return,
            ],
        )],
    })
}
