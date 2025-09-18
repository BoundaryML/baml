//! VM tests for map operations.

use baml_vm::{ObjectIndex, RuntimeError, Value, VmExecState};

mod common;
use common::{
    assert_vm_executes, assert_vm_executes_with_inspection, assert_vm_fails, FailingProgram,
    Program,
};

#[test]
fn create_and_access() -> anyhow::Result<()> {
    let str_index = ObjectIndex::from_raw(0);
    assert_vm_executes_with_inspection(
        Program {
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
            expected: VmExecState::Complete(Value::Object(str_index)),
        },
        |vm| {
            assert_eq!(vm.objects[str_index].as_string().unwrap(), "world");
            Ok(())
        },
    )
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
    let str_index = ObjectIndex::from_raw(0);
    assert_vm_executes_with_inspection(
        Program {
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
            expected: VmExecState::Complete(Value::Object(str_index)),
        },
        |vm| {
            assert_eq!(vm.objects[str_index].as_string().unwrap(), "world");
            Ok(())
        },
    )
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
        expected: VmExecState::Complete(Value::Int(42)),
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
        expected: VmExecState::Complete(Value::Int(2)),
    })
}
