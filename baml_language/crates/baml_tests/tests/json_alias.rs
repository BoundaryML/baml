//! Phase 1 runtime integration tests for the `json` type alias.
//!
//! These tests verify:
//! - A class field of type `json` compiles and the alias resolves to
//!   `Ty::TypeAlias("baml.json.json")`.
//! - `let j: json = <union-arm>` type-checks for each arm: null, bool, int,
//!   float, string, array, and map.
//! - A fully exhaustive `match (j: json) { ... }` arm chain compiles without
//!   a `NonExhaustiveMatch` diagnostic.
//! - Inline bytecode snapshots prove the alias is resolved end-to-end with no
//!   diagnostics.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ─── 1.1 Class field of type json ────────────────────────────────────────────

#[tokio::test]
async fn field_of_type_json_lowers_cleanly() {
    let source = r#"
        class Foo { data json }
        function main() -> Foo {
            Foo { data: null }
        }
    "#;
    let output = baml_test!(source);
    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> Foo {
        alloc_instance user.Foo
        load_const null
        init_field .data
        return
    }
    ");
    // Should return a Foo instance with data = null.
    // The class_name carries the qualified name "user.Foo" at runtime.
    assert!(
        matches!(
            &output.result,
            Ok(BexExternalValue::Instance { class_name, .. }) if class_name == "user.Foo"
        ),
        "expected user.Foo instance, got {:?}",
        output.result
    );
}

// ─── 1.2 Union arm: null ─────────────────────────────────────────────────────

#[tokio::test]
async fn let_json_null() {
    let source = r#"
        function main() -> json {
            null
        }
    "#;
    let output = baml_test!(source);
    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> baml.json.json {
        load_const null
        return
    }
    ");
    assert_eq!(output.result, Ok(BexExternalValue::Null));
}

// ─── 1.3 Union arm: int ──────────────────────────────────────────────────────

#[tokio::test]
async fn let_json_int() {
    let source = r#"
        function main() -> json {
            42
        }
    "#;
    let output = baml_test!(source);
    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> baml.json.json {
        load_const 42
        return
    }
    ");
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

// ─── 1.4 Union arm: string ───────────────────────────────────────────────────

#[tokio::test]
async fn let_json_string() {
    let source = r#"
        function main() -> json {
            "hello"
        }
    "#;
    let output = baml_test!(source);
    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> baml.json.json {
        load_const "hello"
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hello".to_string()))
    );
}

// ─── 1.5 Union arm: array ────────────────────────────────────────────────────

#[tokio::test]
async fn let_json_array() {
    let source = r#"
        function main() -> json {
            [1, 2, 3]
        }
    "#;
    let output = baml_test!(source);
    // The bytecode snapshot demonstrates the alias is resolved and the array
    // literal type-checks against the json[] arm.
    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> baml.json.json {
        load_const 1
        load_const 2
        load_const 3
        alloc_array 3
        return
    }
    ");
    assert!(
        matches!(&output.result, Ok(BexExternalValue::Array { items, .. }) if items.len() == 3),
        "expected array of 3, got {:?}",
        output.result
    );
}

// ─── 1.6 Union arm: map literal ──────────────────────────────────────────────

#[tokio::test]
async fn let_json_map() {
    let source = r#"
        function main() -> json {
            {"key": "value"}
        }
    "#;
    let output = baml_test!(source);
    // The map literal must type-check against the map<string, json> arm.
    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> baml.json.json {
        load_const "value"
        load_const "key"
        alloc_map 1
        return
    }
    "#);
    assert!(
        matches!(
            &output.result,
            Ok(BexExternalValue::Map { entries, .. }) if entries.contains_key("key")
        ),
        "expected map with 'key', got {:?}",
        output.result
    );
}

// ─── 1.7 Exhaustive match against json ───────────────────────────────────────

#[tokio::test]
async fn match_json_arms_exhaustive() {
    // All seven arms of the json union are covered. The function takes `json` as a
    // parameter (not a narrowed `let`), ensuring all arms are reachable. This must
    // compile without a NonExhaustiveMatch diagnostic.
    let source = r#"
        function main(j: json) -> string {
            match (j) {
                null => "null",
                let b: bool => "bool",
                let i: int => "int",
                let f: float => "float",
                let s: string => "string",
                let arr: json[] => "array",
                let obj: map<string, json> => "map"
            }
        }
    "#;
    let output = baml_test! {
        baml: source,
        entry: "main",
        args: { "j" => BexExternalValue::Null },
    };
    insta::assert_snapshot!(output.bytecode, @r#"
    function main(j: baml.json.json) -> string {
        load_var j
        load_const null
        cmp_op ==
        pop_jump_if_false L0
        jump L11

      L0:
        load_var j
        is_type bool
        pop_jump_if_false L1
        jump L10

      L1:
        load_var j
        is_type int
        pop_jump_if_false L2
        jump L9

      L2:
        load_var j
        is_type float
        pop_jump_if_false L3
        jump L8

      L3:
        load_var j
        is_type string
        pop_jump_if_false L4
        jump L7

      L4:
        load_var j
        is_type baml.json.json[]
        pop_jump_if_false L5
        jump L6

      L5:
        load_const "map"
        jump L12

      L6:
        load_const "array"
        jump L12

      L7:
        load_const "string"
        jump L12

      L8:
        load_const "float"
        jump L12

      L9:
        load_const "int"
        jump L12

      L10:
        load_const "bool"
        jump L12

      L11:
        load_const "null"

      L12:
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("null".to_string()))
    );
}
