//! Phase 5 sanity tests for `reflect.Package.new()` and the
//! `Package.add_compile` skeleton.

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

/// `Package.add_compile` should throw `Unsupported` when called on an engine
/// constructed without `set_project_db`. The current `baml_test!` harness
/// builds engines without reflection support, so we expect the throw to
/// surface as an uncaught panic / engine error.
///
/// At this skeleton stage the test just verifies the throw path runs without
/// panicking the host process. Once `add_compile` is fully wired (Phase
/// 5.2c+) a separate fixture will exercise the happy path with a reflection-
/// bearing engine.
#[tokio::test]
async fn package_add_compile_throws_without_project_db() {
    let source = r#"
        function attempt() -> bool {
            let pkg = reflect.Package.new();
            let _result = pkg.add_compile({});
            false
        }
        function main() -> bool {
            attempt() catch (_e) {
                _ => true
            }
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
