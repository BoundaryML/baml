//! Compiler tests for built-in method calls.

use baml_vm::test::{Instruction, Value};

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
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::AllocArray(3),
                Instruction::LoadGlobal(Value::function("baml.Array.length")),
                Instruction::LoadVar("arr".to_string()),
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
                Instruction::LoadGlobal(Value::function("baml.fetch_as")),
                Instruction::LoadConst(Value::string("https://dummyjson.com/todos/1")),
                Instruction::LoadConst(Value::class("DummyJsonTodo")),
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
                Instruction::LoadGlobal(Value::function("baml.fetch_as")),
                Instruction::AllocInstance(Value::class("baml.HttpRequest")),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(1)), // Enum variant index for Post
                Instruction::AllocVariant(Value::enm("baml.HttpMethod")),
                Instruction::StoreField(1),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::string("https://dummyjson.com/todos/add")),
                Instruction::StoreField(0),
                Instruction::Copy(0),
                // Map values first, then keys
                Instruction::LoadConst(Value::string("Buy milk")),
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::LoadConst(Value::Int(5)),
                Instruction::LoadConst(Value::string("todo")),
                Instruction::LoadConst(Value::string("completed")),
                Instruction::LoadConst(Value::string("userId")),
                Instruction::AllocMap(3),
                Instruction::StoreField(4),
                Instruction::LoadConst(Value::class("DummyJsonTodo")),
                Instruction::DispatchFuture(2),
                Instruction::Await,
                Instruction::Return,
            ],
        )],
    })
}
