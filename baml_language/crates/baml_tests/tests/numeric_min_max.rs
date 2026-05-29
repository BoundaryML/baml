//! Tests for root numeric min/max builtins.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn baml_max_int() {
    let output = baml_test!(
        r#"
            function main() -> int {
                baml.max(-7, 3)
            }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn baml_min_int() {
    let output = baml_test!(
        r#"
            function main() -> int {
                baml.min(-7, 3)
            }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(-7)));
}

#[tokio::test]
async fn baml_max_float() {
    let output = baml_test!(
        r#"
            function main() -> float {
                baml.max(-7.5, 3.25)
            }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Float(3.25)));
}

#[tokio::test]
async fn baml_min_float() {
    let output = baml_test!(
        r#"
            function main() -> float {
                baml.min(-7.5, 3.25)
            }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Float(-7.5)));
}

#[tokio::test]
#[should_panic(expected = "expects both arguments to be the same numeric type")]
async fn baml_min_rejects_mixed_numeric_types() {
    let _output = baml_test!(
        r#"
            function main() -> float {
                baml.min(1, 2.0)
            }
        "#
    );
}

#[tokio::test]
#[should_panic(expected = "expects both arguments to be the same numeric type")]
async fn baml_max_rejects_strings() {
    let _output = baml_test!(
        r#"
            function main() -> string {
                baml.max("a", "b")
            }
        "#
    );
}
