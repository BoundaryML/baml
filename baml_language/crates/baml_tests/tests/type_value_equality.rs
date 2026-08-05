//! Function-context pins for equality on `type` values.
//!
//! CHARACTERIZATION of a known inconsistency (do not read these as the
//! intended semantics): BAML currently has divergent equality
//! implementations for `type` values —
//!
//! * `==` lowers through the `baml.ops.equals_equals` driver and compares
//!   type values with `vm.equivalent(..)` (canonical equivalence — union
//!   member order is irrelevant): `bex_vm/src/package_baml/ops.rs`.
//! * `baml.deep_equals` uses the derived `PartialEq` on `RealizedTy`
//!   (syntactic equality — union member order matters):
//!   `bex_vm/src/package_baml/root.rs`.
//!
//! The follow-up s1-mint-identity PR replaces both with a single
//! mint-identity comparison; update these pins together with it.
//! Diagnosis: thoughts/antonio/s1-vm-bug-diagnosis.md. The matching
//! test-block-context pins live in
//! `baml_src/ns_type_reflection/type_reflection.baml` — both contexts must
//! agree (the historical context divergence was the #3782 lowering bug).

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn permuted_union_double_equals_is_canonical() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let a = reflect.type_of<int | string>();
            let b = reflect.type_of<string | int>();
            a == b
        }
        "#
    );
    // Canonical equivalence: member order does not matter.
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn permuted_union_deep_equals_is_syntactic() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let a = reflect.type_of<int | string>();
            let b = reflect.type_of<string | int>();
            baml.deep_equals(a, b)
        }
        "#
    );
    // Syntactic comparison: member order matters, disagreeing with `==`
    // above. Known bug — resolved by s1-mint-identity.
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}
