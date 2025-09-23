//! Compiler tests for map operations.

use baml_vm::{BinOp, GlobalIndex, Instruction};

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
                    Instruction::LoadConst(0),
                    Instruction::LoadConst(1),
                    Instruction::AllocMap(1),
                    Instruction::Return,
                ],
            ),
            (
                "UseMap",
                vec![
                    Instruction::LoadGlobal(GlobalIndex::from_raw(0)),
                    Instruction::Call(0),
                    Instruction::LoadVar(1),
                    Instruction::LoadConst(0),
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
                    Instruction::LoadConst(0),
                    Instruction::LoadConst(1),
                    Instruction::AllocMap(1),
                    Instruction::Return,
                ],
            ),
            (
                "UseMapNoKey",
                vec![
                    Instruction::LoadGlobal(GlobalIndex::from_raw(0)),
                    Instruction::Call(0),
                    Instruction::LoadVar(1),
                    Instruction::LoadConst(0),
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
                    Instruction::LoadConst(0),
                    Instruction::LoadConst(1),
                    Instruction::AllocMap(1),
                    Instruction::Return,
                ],
            ),
            (
                "UseMapContains",
                vec![
                    Instruction::LoadGlobal(GlobalIndex::from_raw(0)),
                    Instruction::Call(0),
                    Instruction::LoadGlobal(GlobalIndex::from_raw(6)),
                    Instruction::LoadVar(1),
                    Instruction::LoadConst(0),
                    Instruction::Call(2),
                    Instruction::JumpIfFalse(6),
                    Instruction::Pop(1),
                    Instruction::LoadVar(1),
                    Instruction::LoadConst(1),
                    Instruction::LoadMapElement,
                    Instruction::Jump(3),
                    Instruction::Pop(1),
                    Instruction::LoadConst(2),
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
                Instruction::LoadConst(0),
                Instruction::LoadConst(1),
                Instruction::AllocMap(1),
                Instruction::LoadVar(1),
                Instruction::LoadConst(2),
                Instruction::LoadConst(3),
                Instruction::LoadConst(4),
                Instruction::BinOp(BinOp::Sub),
                Instruction::StoreMapElement,
                Instruction::LoadVar(1),
                Instruction::LoadConst(5),
                Instruction::Copy(1),
                Instruction::Copy(1),
                Instruction::LoadMapElement,
                Instruction::LoadConst(6),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreMapElement,
                Instruction::LoadVar(1),
                Instruction::LoadConst(7),
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
                Instruction::LoadConst(0),
                Instruction::LoadConst(1),
                Instruction::LoadConst(2),
                Instruction::LoadConst(3),
                Instruction::AllocMap(2),
                Instruction::LoadGlobal(GlobalIndex::from_raw(4)),
                Instruction::LoadVar(1),
                Instruction::Call(1),
                Instruction::Return,
            ],
        )],
    })
}
