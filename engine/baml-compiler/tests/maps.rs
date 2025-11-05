//! Compiler tests for map operations.

use baml_vm::{
    test::{Instruction, Value},
    BinOp,
};

mod common;
use common::{assert_compiles, Program};

#[test]
fn create_and_access() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function CreateMap() -> map<string, string> {
                { hello "world" }
            }

            function UseMap() -> string {
                let map = CreateMap();
                map["hello"]
            }
        "#,
        expected: vec![
            (
                "CreateMap",
                vec![
                    Instruction::LoadConst(Value::string("world")),
                    Instruction::LoadConst(Value::string("hello")),
                    Instruction::AllocMap(1),
                    Instruction::Return,
                ],
            ),
            (
                "UseMap",
                vec![
                    Instruction::LoadGlobal(Value::function("CreateMap")),
                    Instruction::Call(0),
                    Instruction::LoadVar("map".to_string()),
                    Instruction::LoadConst(Value::string("hello")),
                    Instruction::LoadMapElement,
                    Instruction::Return,
                ],
            ),
        ],
    })
}

#[test]
fn access_no_key() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function CreateMap() -> map<string, string> {
                { hello "world" }
            }

            function UseMapNoKey() -> string {
                let map = CreateMap();
                map["world"]
            }
        "#,
        expected: vec![
            (
                "CreateMap",
                vec![
                    Instruction::LoadConst(Value::string("world")),
                    Instruction::LoadConst(Value::string("hello")),
                    Instruction::AllocMap(1),
                    Instruction::Return,
                ],
            ),
            (
                "UseMapNoKey",
                vec![
                    Instruction::LoadGlobal(Value::function("CreateMap")),
                    Instruction::Call(0),
                    Instruction::LoadVar("map".to_string()),
                    Instruction::LoadConst(Value::string("world")),
                    Instruction::LoadMapElement,
                    Instruction::Return,
                ],
            ),
        ],
    })
}

#[test]
fn contains() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function CreateMapJSON() -> map<string, string> {
                {"hello": "world"}
            }
            function UseMapContains() -> string {
                let map = CreateMapJSON();
                if (map.has("hello")) {
                    map["hello"]
                } else {
                    "hi"
                }
            }
        "#,
        expected: vec![
            (
                "CreateMapJSON",
                vec![
                    Instruction::LoadConst(Value::string("world")),
                    Instruction::LoadConst(Value::string("hello")),
                    Instruction::AllocMap(1),
                    Instruction::Return,
                ],
            ),
            (
                "UseMapContains",
                vec![
                    Instruction::LoadGlobal(Value::function("CreateMapJSON")),
                    Instruction::Call(0),
                    Instruction::LoadGlobal(Value::function("baml.Map.has")),
                    Instruction::LoadVar("map".to_string()),
                    Instruction::LoadConst(Value::string("hello")),
                    Instruction::Call(2),
                    Instruction::JumpIfFalse(6),
                    Instruction::Pop(1),
                    Instruction::LoadVar("map".to_string()),
                    Instruction::LoadConst(Value::string("hello")),
                    Instruction::LoadMapElement,
                    Instruction::Jump(3),
                    Instruction::Pop(1),
                    Instruction::LoadConst(Value::string("hi")),
                    Instruction::Return,
                ],
            ),
        ],
    })
}

#[test]
fn modify() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function EditMapKey() -> int {
                let map = { hi 123 };

                map["hi"] = 42 - 4;
                map["hi"] += 4;

                map["hi"]

            }
        "#,
        expected: vec![(
            "EditMapKey",
            vec![
                Instruction::LoadConst(Value::Int(123)),
                Instruction::LoadConst(Value::string("hi")),
                Instruction::AllocMap(1),
                Instruction::LoadVar("map".to_string()),
                Instruction::LoadConst(Value::string("hi")),
                Instruction::LoadConst(Value::Int(42)),
                Instruction::LoadConst(Value::Int(4)),
                Instruction::BinOp(BinOp::Sub),
                Instruction::StoreMapElement,
                Instruction::LoadVar("map".to_string()),
                Instruction::LoadConst(Value::string("hi")),
                Instruction::Copy(1),
                Instruction::Copy(1),
                Instruction::LoadMapElement,
                Instruction::LoadConst(Value::Int(4)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreMapElement,
                Instruction::LoadVar("map".to_string()),
                Instruction::LoadConst(Value::string("hi")),
                Instruction::LoadMapElement,
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn len() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function Len() -> int {
                let map = {
                    hi 123
                    it_works 456
                };
                map.length()
            }
        "#,
        expected: vec![(
            "Len",
            vec![
                Instruction::LoadConst(Value::Int(123)),
                Instruction::LoadConst(Value::Int(456)),
                Instruction::LoadConst(Value::string("hi")),
                Instruction::LoadConst(Value::string("it_works")),
                Instruction::AllocMap(2),
                Instruction::LoadGlobal(Value::function("baml.Map.length")),
                Instruction::LoadVar("map".to_string()),
                Instruction::Call(1),
                Instruction::Return,
            ],
        )],
    })
}
