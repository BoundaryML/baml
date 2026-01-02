//! Compiler tests for map operations.

use baml_tests::{
    codegen::{Program, assert_compiles},
    vm::{Instruction, Value},
};
use baml_vm::BinOp;

#[test]
fn create_and_access() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function CreateMap() -> map<string, string> {
                { "hello": "world" }
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
                // CallResultImmediate optimization: Call result stays on stack,
                // used immediately for map access (no Store/Load)
                vec![
                    Instruction::LoadGlobal(Value::function("CreateMap")),
                    Instruction::Call(0),
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
                { "hello": "world" }
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
                // CallResultImmediate optimization: Call result stays on stack,
                // used immediately for map access (no Store/Load)
                vec![
                    Instruction::LoadGlobal(Value::function("CreateMap")),
                    Instruction::Call(0),
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
                    // Init local for map (return value is PhiLike, _3 is CallResultImmediate)
                    Instruction::LoadConst(Value::Null),
                    // let map = CreateMapJSON();
                    Instruction::LoadGlobal(Value::function("CreateMapJSON")),
                    Instruction::Call(0),
                    Instruction::StoreVar("map".to_string()),
                    // map.has("hello") - method call, result stays on stack (CallResultImmediate)
                    Instruction::LoadGlobal(Value::function("baml.Map.has")),
                    Instruction::LoadVar("map".to_string()),
                    Instruction::LoadConst(Value::string("hello")),
                    Instruction::Call(2),
                    // if condition - Call result used directly from stack
                    Instruction::PopJumpIfFalse(2),
                    Instruction::Jump(3),
                    // else branch: "hi"
                    Instruction::LoadConst(Value::string("hi")),
                    Instruction::Jump(4),
                    // then branch: map["hello"]
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
fn modify() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function EditMapKey() -> int {
                let map = { "hi": 123 };

                map["hi"] = 42 - 4;
                map["hi"] += 4;

                map["hi"]

            }
        "#,
        expected: vec![(
            "EditMapKey",
            vec![
                // Init local for map (return value is ReturnPhi, no slot needed)
                Instruction::LoadConst(Value::Null),
                // let map = { "hi": 123 }; (values first, then keys)
                Instruction::LoadConst(Value::Int(123)),
                Instruction::LoadConst(Value::string("hi")),
                Instruction::AllocMap(1),
                Instruction::StoreVar("map".to_string()),
                // map["hi"] = 42 - 4;
                Instruction::LoadVar("map".to_string()),
                Instruction::LoadConst(Value::string("hi")),
                Instruction::LoadConst(Value::Int(42)),
                Instruction::LoadConst(Value::Int(4)),
                Instruction::BinOp(BinOp::Sub),
                Instruction::StoreMapElement,
                // map["hi"] += 4; (constant propagation: key inlined at each use)
                Instruction::LoadVar("map".to_string()),
                Instruction::LoadConst(Value::string("hi")),
                Instruction::LoadVar("map".to_string()),
                Instruction::LoadConst(Value::string("hi")),
                Instruction::LoadMapElement,
                Instruction::LoadConst(Value::Int(4)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreMapElement,
                // map["hi"] - final value
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
                    "hi": 123,
                    "it_works": 456
                };
                map.length()
            }
        "#,
        expected: vec![(
            "Len",
            vec![
                // Method call on map literal
                Instruction::LoadGlobal(Value::function("baml.Map.length")),
                // Map literal: values first, then keys
                // { "hi": 123, "it_works": 456 }
                Instruction::LoadConst(Value::Int(123)),
                Instruction::LoadConst(Value::Int(456)),
                Instruction::LoadConst(Value::string("hi")),
                Instruction::LoadConst(Value::string("it_works")),
                Instruction::AllocMap(2),
                Instruction::Call(1),
                Instruction::Return,
            ],
        )],
    })
}
