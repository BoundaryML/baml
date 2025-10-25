//! Compiler tests for built-in method calls.

use baml_vm::{GlobalIndex, Instruction, ObjectIndex};

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
                Instruction::LoadGlobal(GlobalIndex::from_raw(5)),
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
                Instruction::LoadGlobal(GlobalIndex::from_raw(51)),
                Instruction::LoadConst(0),
                Instruction::LoadConst(1),
                Instruction::DispatchFuture(2),
                Instruction::Await,
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn fetch_as_with_request_param() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            class DummyJsonTodo {
                id int
                todo string
                completed bool
                userId int
            }

            function main() -> DummyJsonTodo {
                baml.fetch_as<DummyJsonTodo>(baml.HttpRequest {
                    method: baml.HttpMethod.Post,
                    url: "https://dummyjson.com/todos/add",
                    json: {
                        "todo": "Buy milk",
                        "completed": false,
                        "userId": 5
                    },
                })
            }
        "#,
        expected: vec![(
            "main",
            vec![
                Instruction::LoadGlobal(GlobalIndex::from_raw(51)),
                Instruction::AllocInstance(ObjectIndex::from_raw(7)),
                Instruction::Copy(0),
                Instruction::LoadConst(0),
                Instruction::AllocVariant(ObjectIndex::from_raw(10)),
                Instruction::StoreField(1),
                Instruction::Copy(0),
                Instruction::LoadConst(1),
                Instruction::StoreField(0),
                Instruction::Copy(0),
                Instruction::LoadConst(2),
                Instruction::LoadConst(3),
                Instruction::LoadConst(4),
                Instruction::LoadConst(5),
                Instruction::LoadConst(6),
                Instruction::LoadConst(7),
                Instruction::AllocMap(3),
                Instruction::StoreField(4),
                Instruction::LoadConst(8),
                Instruction::DispatchFuture(2),
                Instruction::Await,
                Instruction::Return,
            ],
        )],
    })
}
