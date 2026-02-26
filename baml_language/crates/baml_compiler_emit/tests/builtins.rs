//! Compiler tests for built-in method calls.

use baml_tests::{
    codegen::{Program, assert_compiles},
    vm::{Instruction, Value},
};

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
            // arr is Virtual (single-use), so array stays on stack as argument.
            // Method call arr.length() desugars to baml.Array.length(arr).
            // ReturnPhi: call result stays on stack for return.
            vec![
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::AllocArray(3),
                Instruction::Call("baml.Array.length".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
#[ignore = "baml.fetch_as not yet in HIR"]
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
                Instruction::LoadConst(Value::string("https://dummyjson.com/todos/1")),
                Instruction::LoadConst(Value::class("DummyJsonTodo")),
                Instruction::DispatchFuture("baml.fetch_as".to_string()),
                Instruction::Await,
                Instruction::Return,
            ],
        )],
    })
}

#[test]
#[ignore = "baml.fetch_as and enums not yet in HIR"]
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
                Instruction::DispatchFuture("baml.fetch_as".to_string()),
                Instruction::Await,
                Instruction::Return,
            ],
        )],
    })
}
