//! Function-context pins for BEP-066 minted equality on `type` values.
//!
//! PR 1 characterized a known inconsistency: `==` canonicalized `RealizedTy`
//! while `baml.deep_equals` compared it syntactically. BEP-066 slice-1 PR 4
//! deliberately flips the deep-equality pin. Every equality path now compares
//! the mint, and equivalent static spellings receive the same canonical digest.
//! Matching test-block pins live in
//! `baml_src/ns_type_reflection/type_reflection.baml`.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn permuted_union_double_equals_is_canonical() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let a = type.of<int | string>();
            let b = type.of<string | int>();
            a == b
        }
        "#
    );
    // Canonical equivalence: member order does not matter.
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn permuted_union_deep_equals_uses_the_canonical_mint() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let a = type.of<int | string>();
            let b = type.of<string | int>();
            baml.deep_equals(a, b)
        }
        "#
    );
    // Flipped in BEP-066 slice-1 PR 4: deep_equals agrees with `==` because
    // both compare the same canonical static mint.
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn static_declaration_identity_survives_re_evaluation_and_a_helper_boundary() {
    let output = baml_test!(
        r#"
        class Foo { value int }

        function foo_type() -> type {
            type.of<Foo>()
        }

        function main() -> bool {
            type.of<Foo>() == type.of<Foo>()
                && type.of<Foo>() == foo_type()
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
            type.of_value(foo) == type.of<Foo>()
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
            type.of<string?>() == type.of<string | null>()
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
