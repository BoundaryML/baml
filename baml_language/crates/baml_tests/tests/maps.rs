//! Unified tests for map construction, access, and methods.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;
#[tokio::test]
async fn create_and_access() {
    let output = baml_test! {
        baml: r#"
            function create_map() -> map<string, string> {
                { "hello": "world" }
            }

            function use_map() -> string {
                let map = create_map();
                map["hello"]
            }
        "#,
        entry: "use_map",
    };

    insta::assert_snapshot!(output.bytecode, @r#"
    function create_map() -> map<string, string> {
        load_const "world"
        load_const "hello"
        alloc_map 1
        return
    }

    function use_map() -> string {
        call create_map
        load_const "hello"
        load_map_element
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("world".to_string()))
    );
}

#[tokio::test]
async fn access_no_key() {
    let output = baml_test! {
        baml: r#"
            function create_map() -> map<string, string> {
                { "hello": "world" }
            }

            function use_map_no_key() -> string {
                let map = create_map();
                map["world"]
            }
        "#,
        entry: "use_map_no_key",
    };

    insta::assert_snapshot!(output.bytecode, @r#"
    function create_map() -> map<string, string> {
        load_const "world"
        load_const "hello"
        alloc_map 1
        return
    }

    function use_map_no_key() -> string {
        call create_map
        load_const "world"
        load_map_element
        return
    }
    "#);

    assert_eq!(
        output.result,
        Err(bex_engine::EngineError::VmError(
            bex_vm::errors::VmError::RuntimeError(bex_vm::errors::RuntimeError::NoSuchKeyInMap)
        ))
    );
}

#[tokio::test]
async fn contains() {
    let output = baml_test! {
        baml: r#"
            function create_map() -> map<string, string> {
                {"hello": "world"}
            }

            function use_map_contains() -> string {
                let map = create_map();
                if (map.has("hello")) {
                    map["hello"]
                } else {
                    "hi"
                }
            }
        "#,
        entry: "use_map_contains",
    };

    insta::assert_snapshot!(output.bytecode, @r#"
    function create_map() -> map<string, string> {
        load_const "world"
        load_const "hello"
        alloc_map 1
        return
    }

    function use_map_contains() -> string {
        call create_map
        store_var map
        load_var map
        load_const "hello"
        call baml.Map.has
        pop_jump_if_false L0
        jump L1

      L0:
        load_const "hi"
        jump L2

      L1:
        load_var map
        load_const "hello"
        load_map_element

      L2:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("world".to_string()))
    );
}

#[tokio::test]
async fn modify() {
    let output = baml_test! {
        baml: r#"
            function edit_map_key() -> int {
                let map = { "hi": 123 };

                map["hi"] = 42 - 4;
                map["hi"] += 4;

                map["hi"]
            }
        "#,
        entry: "edit_map_key",
    };

    insta::assert_snapshot!(output.bytecode, @r#"
    function edit_map_key() -> int {
        load_const 123
        load_const "hi"
        alloc_map 1
        store_var map
        load_var map
        load_const "hi"
        load_const 42
        load_const 4
        bin_op -
        store_map_element
        load_var map
        load_const "hi"
        load_var map
        load_const "hi"
        load_map_element
        load_const 4
        bin_op +
        store_map_element
        load_var map
        load_const "hi"
        load_map_element
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn len() {
    let output = baml_test! {
        baml: r#"
            function map_len() -> int {
                let map = {
                    "hi": 123,
                    "it_works": 456
                };
                map.length()
            }
        "#,
        entry: "map_len",
    };

    insta::assert_snapshot!(output.bytecode, @r#"
    function map_len() -> int {
        load_const 123
        load_const 456
        load_const "hi"
        load_const "it_works"
        alloc_map 2
        call baml.Map.length
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}
