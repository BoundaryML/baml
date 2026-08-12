//! BEP-034: cancellation race tests.
//!
//! Unobserved spawn errors are reported through the host default instead of
//! being attached to an unrelated `await` or function result.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// B-405: an unobserved spawn error does not replace an unrelated await.
#[tokio::test]
async fn fire_and_forget_error_does_not_replace_unrelated_await() {
    let output = baml_test!(
        r#"
        function boom() -> int throws baml.errors.Io {
            throw baml.errors.Io { message: "boom" }
        }
        function main() -> int {
            let _ = spawn { boom() };
            await spawn { 99 }
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}
