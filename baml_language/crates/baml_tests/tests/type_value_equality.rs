//! Function-context pins for equality on `type` values.
//!
//! A `type` value denotes a type and nothing more, so `==` is equivalence:
//! spellings that denote the same type are equal.
//! Matching test-block pins live in
//! `baml_src/ns_type_reflection/type_reflection.baml`.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn permuted_union_double_equals_is_canonical() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let a = reflect.Type.of<int | string>();
            let b = reflect.Type.of<string | int>();
            a == b
        }
        "#
    );
    // Canonical equivalence: member order does not matter.
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn permuted_union_equality_operators_agree() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let a = reflect.Type.of<int | string>();
            let b = reflect.Type.of<string | int>();
            a == b && !(a != b)
        }
        "#
    );
    // Both equality operators decide the same equivalence.
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn static_declaration_identity_survives_re_evaluation_and_a_helper_boundary() {
    let output = baml_test!(
        r#"
        class Foo { value int }

        function foo_type() -> reflect.Type {
            reflect.Type.of<Foo>()
        }

        function main() -> bool {
            reflect.Type.of<Foo>() == reflect.Type.of<Foo>()
                && reflect.Type.of<Foo>() == foo_type()
                && foo_type() == foo_type()
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn of_value_reuses_the_static_class_identity() {
    let output = baml_test!(
        r#"
        class Foo { value int }

        function main() -> bool {
            let foo = Foo { value: 1 };
            reflect.Type.of_value(foo) == reflect.Type.of<Foo>()
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn optional_and_explicit_null_union_share_a_static_identity() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            reflect.Type.of<string?>() == reflect.Type.of<string | null>()
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
