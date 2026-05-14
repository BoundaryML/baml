//! Phase 5 sanity test: `reflect.Package.new()` allocates a runtime-compiled
//! package handle without crashing.
//!
//! At this point the only observable side of a runtime package is that it
//! exists — `add_compile`, `get`, and `eval` arrive in later commits. The
//! test just confirms the wiring from BAML syntax through the codegen-
//! generated trait into the gen0 `Object::Package` allocation.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn package_new_returns_without_crash() {
    let source = r#"
        function main() -> bool {
            let _pkg = reflect.Package.new();
            true
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
