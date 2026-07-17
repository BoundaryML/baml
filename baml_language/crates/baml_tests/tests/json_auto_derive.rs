//! These tests use `output.bytecode` inspection and the Rust-only
//! `show_auto_derive: true` option, not available in BAML test blocks.
//!
//! No per-class JSON method is synthesized: `to_json` and `from_json`
//! are both sugars (`obj.to_json()` -> `baml.json.from(obj)`,
//! `Type.from_json(j)` -> `baml.json.to<Type>(j)`), owned by the `baml.ToJson` /
//! `baml.FromJson` interfaces. So no auto-derived method should appear in a
//! user class's bytecode even with `show_auto_derive: true`.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn no_synthesized_json_methods_in_bytecode() {
    // Neither `to_json` nor `from_json` is a synthesized method — they desugar
    // to `baml.json.from` / `baml.json.to`. A class that never calls them has
    // no such method in its bytecode.
    let source = r#"
        class User { name string  age int }
        function main() -> int {
            let u: User = User { name: "Ada", age: 30 };
            u.age
        }
    "#;
    let output = baml_test!(source);
    assert!(
        !output.bytecode.contains("to_json"),
        "no synthesized to_json should appear:\n{}",
        output.bytecode
    );
    assert!(
        !output.bytecode.contains("from_json"),
        "no synthesized from_json should appear:\n{}",
        output.bytecode
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(30)));
}

#[tokio::test]
async fn no_synthesized_json_methods_even_with_show_auto_derive() {
    // `show_auto_derive: true` exposes synthesized methods; there are none for
    // JSON now (both directions are sugars), so neither appears.
    let source = r#"
        class User { name string  age int }
        function main() -> int { 1 }
    "#;
    let output = baml_test!(baml: source, show_auto_derive: true);
    assert!(
        !output.bytecode.contains("from_json"),
        "from_json is a sugar, not auto-derived; none should appear:\n{}",
        output.bytecode
    );
    assert!(
        !output.bytecode.contains("to_json"),
        "to_json is a sugar, not auto-derived; none should appear:\n{}",
        output.bytecode
    );
}
