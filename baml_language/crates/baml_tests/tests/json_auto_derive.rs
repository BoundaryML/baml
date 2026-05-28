//! Auto-derived `to_json` / `from_json` on user classes.
//!
//! Only tests that require Rust-harness-only features remain here:
//!   - `auto_derive_filtered_from_bytecode_by_default`: inspects `output.bytecode`
//!   - `auto_derive_visible_with_show_auto_derive_flag`: uses `show_auto_derive: true`

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn auto_derive_filtered_from_bytecode_by_default() {
    // The synthesized `to_json` / `from_json` methods must not appear in the
    // default bytecode snapshot — they would bloat every user-class test.
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
        "default bytecode should not show auto-derived to_json:\n{}",
        output.bytecode
    );
    assert!(
        !output.bytecode.contains("from_json"),
        "default bytecode should not show auto-derived from_json:\n{}",
        output.bytecode
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(30)));
}

#[tokio::test]
async fn auto_derive_visible_with_show_auto_derive_flag() {
    // `show_auto_derive: true` opts in to seeing the synthesized methods so
    // the synthesizer itself can be tested.
    let source = r#"
        class User { name string  age int }
        function main() -> int { 1 }
    "#;
    let output = baml_test!(baml: source, show_auto_derive: true);
    assert!(
        output.bytecode.contains("to_json"),
        "show_auto_derive: true should expose to_json in bytecode:\n{}",
        output.bytecode
    );
    assert!(
        output.bytecode.contains("from_json"),
        "show_auto_derive: true should expose from_json in bytecode:\n{}",
        output.bytecode
    );
}
