//! Regression: inferring one type var from two distinct literal arguments must
//! union them (T = `3 | 4`), not strict-check arg 2 against arg 1's literal
//! binding. Found by the BEP-058 brutal-test workflow (independent of mocks).

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// `add<T>(3, 4)` — T inferred from two distinct int literals.
#[tokio::test]
async fn infer_typevar_from_two_distinct_int_literals() {
    let output = baml_test!(
        r#"
        function add<T>(x: T, y: T) -> int { 7 }
        function main() -> int { add(3, 4) }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

/// Same for distinct string literals.
#[tokio::test]
async fn infer_typevar_from_two_distinct_string_literals() {
    let output = baml_test!(
        r#"
        function pick<T>(a: T, b: T) -> int { 1 }
        function main() -> int { pick("a", "b") }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

/// The builtin `deep_equals` exercises the same inference path.
#[tokio::test]
async fn deep_equals_two_distinct_literals_compiles() {
    let output = baml_test!(
        r#"
        function main() -> bool { baml.deep_equals(3, 4) }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

/// Three distinct literals across three positions still unify.
#[tokio::test]
async fn infer_typevar_from_three_distinct_literals() {
    let output = baml_test!(
        r#"
        function tri<T>(a: T, b: T, c: T) -> int { 9 }
        function main() -> int { tri(1, 2, 3) }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(9)));
}
