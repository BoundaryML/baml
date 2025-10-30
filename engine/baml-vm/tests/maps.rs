//! VM tests for map operations.

use baml_vm::RuntimeError;

mod common;
use common::{assert_vm_executes, assert_vm_fails, ExecState, FailingProgram, Program, Value};

use crate::common::Object;

#[test]
fn create_and_access() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
                function CreateMap() -> map<string, string> {
                    { hello "world" }
                }
                function UseMap() -> string {
                    let map = CreateMap();
                    map["hello"]
                }
            "#,
        function: "UseMap",
        expected: ExecState::Complete(Value::string("world")),
    })
}

#[test]
fn access_no_key() -> anyhow::Result<()> {
    assert_vm_fails(FailingProgram {
        source: r#"
            function CreateMap() -> map<string, string> {
                { hello "world" }
            }

            function UseMapNoKey() -> string {
                let map = CreateMap();
                map["world"]
            }
        "#,
        function: "UseMapNoKey",
        expected: RuntimeError::NoSuchKeyInMap.into(),
    })
}

#[test]
fn contains() -> anyhow::Result<()> {
    assert_vm_executes(Program {
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
        function: "UseMapContains",
        expected: ExecState::Complete(Value::string("world")),
    })
}

#[test]
fn modify() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function EditMapKey() -> int {
                let map = { hi 123 };

                map["hi"] = 42 - 4;
                map["hi"] += 4;

                map["hi"]

            }
        "#,
        function: "EditMapKey",
        expected: ExecState::Complete(Value::Int(42)),
    })
}

#[test]
fn len() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function Len() -> int {
                let map = {
                    hi 123
                    it_works 456
                };
                map.length()
            }
        "#,
        function: "Len",
        expected: ExecState::Complete(Value::Int(2)),
    })
}
