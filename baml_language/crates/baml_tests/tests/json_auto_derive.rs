//! Auto-derived `to_json` / `from_json` on user classes.
//!
//! First-iteration thin slice: the synthesized methods are wrappers around
//! the existing `baml.json.to_string<Self>` / `baml.json.from_string<Self>`
//! intrinsics shipped by Phase 5. This proves the framework end-to-end:
//!
//! - methods are appended at AST lowering time,
//! - they are dispatched as regular class methods (`u.to_json()` and
//!   `User.from_json(j)`),
//! - they round-trip,
//! - they are filtered from the default bytecode display so user-source
//!   snapshots stay focused.
//!
//! Override-honouring on nested classes is **not** in scope for this
//! iteration; the runtime walker behind `to_string<T>` does not yet dispatch
//! through user `to_json` overrides on nested fields. That's the next
//! follow-up (per BEP-038 §"to_json / from_json Protocol").

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn simple_class_to_json_roundtrip() {
    // `to_json()` followed by `from_json(...)` should yield a value with the
    // original field values.
    let source = r#"
        class User { name string  age int }
        function main() -> string {
            let u: User = User { name: "Ada", age: 30 };
            let j: baml.json.json = u.to_json();
            let v: User = User.from_json(j);
            v.name
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Ada".to_string()))
    );
}

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

#[tokio::test]
async fn user_to_json_override_suppresses_auto_derive() {
    // If the user defines `to_json`, the synthesizer must skip BOTH methods.
    // Calling the user's method should win.
    let source = r#"
        class Secret {
            value string

            function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
                "[redacted]"
            }
        }
        function main() -> baml.json.json throws baml.json.JsonSerializationError {
            let s: Secret = Secret { value: "hunter2" };
            s.to_json()
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("[redacted]".to_string()))
    );
}
