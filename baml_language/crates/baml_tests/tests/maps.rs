//! Unified tests for map construction, access, and methods.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;
#[tokio::test]
#[ignore = "compiler2: map element access returns 'unknown | T' making the result non-indexable (TypeError: expected String, got Any)"]
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
        call user.create_map
        load_var _3
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
#[ignore = "compiler2: map element access returns 'unknown | T' making the result non-indexable (TypeError: expected String, got Any)"]
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
        call user.create_map
        load_var _3
        load_map_element
        return
    }
    "#);

    assert!(matches!(
        output.result,
        Err(bex_engine::EngineError::UnhandledThrow { .. })
    ));
}

#[tokio::test]
#[ignore = "compiler2: map element access returns 'unknown | T' making the result non-indexable (TypeError: expected String, got Any)"]
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
        call user.create_map
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
        load_var _5
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
#[ignore = "compiler2: map element access returns 'unknown | T' making the result non-indexable (TypeError: expected Int, got Any)"]
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

// ─── Helpers for the BEP-043 method tests ─────────────────────────────────────

async fn run_int(src: &str) -> i64 {
    let output = baml_test!(baml: src, entry: "main");
    match output.result {
        Ok(BexExternalValue::Int(n)) => n,
        other => panic!("expected int, got {other:?}"),
    }
}

async fn run_int_or_null(src: &str) -> Option<i64> {
    let output = baml_test!(baml: src, entry: "main");
    match output.result {
        Ok(BexExternalValue::Int(n)) => Some(n),
        Ok(BexExternalValue::Null) => None,
        other => panic!("expected int or null, got {other:?}"),
    }
}

// ─── set returns displaced value ──────────────────────────────────────────────

#[tokio::test]
async fn map_set_first_insert_returns_null() {
    let output = baml_test!(
        r#"
        function main() -> int? {
            let m: map<string, int> = {};
            m.set("a", 1)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Null));
}

#[tokio::test]
async fn map_set_replace_returns_previous() {
    let output = baml_test!(
        r#"
        function main() -> int? {
            let m: map<string, int> = {};
            let _ = m.set("a", 1);
            m.set("a", 2)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn map_set_then_get_reads_latest() {
    assert_eq!(
        run_int(
            r#"
            function main() -> int {
                let m: map<string, int> = {};
                let _ = m.set("a", 1);
                let _ = m.set("a", 2);
                m.get("a") ?? -1
            }
            "#
        )
        .await,
        2
    );
}

#[tokio::test]
async fn map_set_grows_length() {
    assert_eq!(
        run_int(
            r#"
            function main() -> int {
                let m: map<string, int> = {};
                let _ = m.set("a", 1);
                let _ = m.set("b", 2);
                let _ = m.set("c", 3);
                m.length()
            }
            "#
        )
        .await,
        3
    );
}

#[tokio::test]
async fn map_set_replace_preserves_length() {
    assert_eq!(
        run_int(
            r#"
            function main() -> int {
                let m: map<string, int> = {};
                let _ = m.set("a", 1);
                let _ = m.set("a", 2);
                let _ = m.set("a", 3);
                m.length()
            }
            "#
        )
        .await,
        1
    );
}

// ─── delete ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn map_delete_returns_previous() {
    let output = baml_test!(
        r#"
        function main() -> int? {
            let m: map<string, int> = {};
            let _ = m.set("a", 42);
            m.delete("a")
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn map_delete_absent_returns_null() {
    assert_eq!(
        run_int_or_null(
            r#"
            function main() -> int? {
                let m: map<string, int> = {};
                m.delete("missing")
            }
            "#
        )
        .await,
        None
    );
}

#[tokio::test]
async fn map_delete_already_deleted_returns_null() {
    assert_eq!(
        run_int_or_null(
            r#"
            function main() -> int? {
                let m: map<string, int> = {};
                let _ = m.set("a", 1);
                let _ = m.delete("a");
                m.delete("a")
            }
            "#
        )
        .await,
        None
    );
}

#[tokio::test]
async fn map_delete_shrinks_length() {
    assert_eq!(
        run_int(
            r#"
            function main() -> int {
                let m: map<string, int> = {};
                let _ = m.set("a", 1);
                let _ = m.set("b", 2);
                let _ = m.delete("a");
                m.length()
            }
            "#
        )
        .await,
        1
    );
}

#[tokio::test]
async fn map_delete_does_not_affect_other_keys() {
    assert_eq!(
        run_int(
            r#"
            function main() -> int {
                let m: map<string, int> = {};
                let _ = m.set("a", 1);
                let _ = m.set("b", 2);
                let _ = m.delete("a");
                m.get("b") ?? -1
            }
            "#
        )
        .await,
        2
    );
}

// ─── get_or_insert ────────────────────────────────────────────────────────────

#[tokio::test]
async fn map_get_or_insert_inserts_when_absent() {
    assert_eq!(
        run_int(
            r#"
            function main() -> int {
                let m: map<string, int> = {};
                m.get_or_insert("k", 10)
            }
            "#
        )
        .await,
        10
    );
}

#[tokio::test]
async fn map_get_or_insert_returns_existing_when_present() {
    assert_eq!(
        run_int(
            r#"
            function main() -> int {
                let m: map<string, int> = {};
                let _ = m.set("k", 1);
                m.get_or_insert("k", 99)
            }
            "#
        )
        .await,
        1
    );
}

#[tokio::test]
async fn map_get_or_insert_does_not_overwrite() {
    // The default `99` is ignored when the key is present.
    assert_eq!(
        run_int(
            r#"
            function main() -> int {
                let m: map<string, int> = {};
                let _ = m.set("k", 1);
                let _ = m.get_or_insert("k", 99);
                m.get("k") ?? -1
            }
            "#
        )
        .await,
        1
    );
}

#[tokio::test]
async fn map_get_or_insert_persists_inserted_value() {
    // After get_or_insert on an absent key, the value is in the map.
    assert_eq!(
        run_int(
            r#"
            function main() -> int {
                let m: map<string, int> = {};
                let _ = m.get_or_insert("k", 7);
                m.get("k") ?? -1
            }
            "#
        )
        .await,
        7
    );
}

#[tokio::test]
async fn map_get_or_insert_grows_length_only_when_absent() {
    assert_eq!(
        run_int(
            r#"
            function main() -> int {
                let m: map<string, int> = {};
                let _ = m.get_or_insert("a", 1);  // inserts
                let _ = m.get_or_insert("a", 2);  // existing
                m.length()
            }
            "#
        )
        .await,
        1
    );
}

// ─── clear ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn map_clear_removes_all() {
    assert_eq!(
        run_int(
            r#"
            function main() -> int {
                let m: map<string, int> = {};
                let _ = m.set("a", 1);
                let _ = m.set("b", 2);
                m.clear();
                m.length()
            }
            "#
        )
        .await,
        0
    );
}

#[tokio::test]
async fn map_clear_already_empty_is_noop() {
    assert_eq!(
        run_int(
            r#"
            function main() -> int {
                let m: map<string, int> = {};
                m.clear();
                m.length()
            }
            "#
        )
        .await,
        0
    );
}

#[tokio::test]
async fn map_clear_then_reuse() {
    assert_eq!(
        run_int(
            r#"
            function main() -> int {
                let m: map<string, int> = {};
                let _ = m.set("a", 1);
                m.clear();
                let _ = m.set("b", 99);
                m.get("b") ?? -1
            }
            "#
        )
        .await,
        99
    );
}

#[tokio::test]
async fn map_clear_makes_get_return_null() {
    assert_eq!(
        run_int_or_null(
            r#"
            function main() -> int? {
                let m: map<string, int> = {};
                let _ = m.set("a", 1);
                m.clear();
                m.get("a")
            }
            "#
        )
        .await,
        None
    );
}

// ─── has integrates with new methods ──────────────────────────────────────────

#[tokio::test]
async fn map_has_after_delete_is_false() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let m: map<string, int> = {};
            let _ = m.set("a", 1);
            let _ = m.delete("a");
            m.has("a")
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn map_has_after_get_or_insert_is_true() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let m: map<string, int> = {};
            let _ = m.get_or_insert("k", 7);
            m.has("k")
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
