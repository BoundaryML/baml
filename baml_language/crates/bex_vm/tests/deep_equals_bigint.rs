//! Tests for `PackageBamlImpl::deep_equals` on `Object::Bigint`.
//!
//! `==` on bigint at the BAML surface goes through a specialized `CmpBigintOp`
//! opcode that compares numeric values, so the user never observes the
//! `deep_equals` path directly. But class-instance equality and any future
//! recursive consumer route through
//! `deep_equals_recursive`, which must compare `Arc<BigInt>`s by numeric value
//! rather than pointer identity.

use std::sync::{Arc, atomic::AtomicBool};

use baml_project::testing::compile_source;
use bex_vm::{BexVm, package_baml::PackageBamlImpl};
use bex_vm_types::{Value, types::Object};
use num_bigint::BigInt;

fn make_vm() -> BexVm {
    let program = compile_source("function noop() -> int { 0 }");
    BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program")
}

fn alloc_bigint(vm: &mut BexVm, value: BigInt) -> Value {
    let arc = Arc::new(value);
    let ptr = vm.tlab.alloc(Object::Bigint(arc));
    Value::object(ptr)
}

#[test]
fn deep_equals_bigint_distinct_arcs_same_value() {
    use bex_vm::package_baml::BamlPackageBaml;
    let mut vm = make_vm();
    let a = alloc_bigint(&mut vm, BigInt::from(42));
    let b = alloc_bigint(&mut vm, BigInt::from(42));
    // Distinct heap pointers (so they exercise the by-value comparison path),
    // same numeric value.
    let Some(pa) = a.as_object_ptr() else {
        unreachable!()
    };
    let Some(pb) = b.as_object_ptr() else {
        unreachable!()
    };
    assert_ne!(pa, pb, "test setup must allocate distinct objects");
    assert!(
        PackageBamlImpl::deep_equals(&vm, &a, &b),
        "two bigints with equal numeric value must deep-equal regardless of Arc identity"
    );
}

#[test]
fn deep_equals_bigint_different_values() {
    use bex_vm::package_baml::BamlPackageBaml;
    let mut vm = make_vm();
    let a = alloc_bigint(&mut vm, BigInt::from(42));
    let b = alloc_bigint(&mut vm, BigInt::from(43));
    assert!(
        !PackageBamlImpl::deep_equals(&vm, &a, &b),
        "bigints with different numeric values must not deep-equal"
    );
}

#[test]
fn deep_equals_bigint_large_values() {
    use bex_vm::package_baml::BamlPackageBaml;
    let mut vm = make_vm();
    let big = BigInt::parse_bytes(b"99999999999999999999", 10).unwrap();
    let a = alloc_bigint(&mut vm, big.clone());
    let b = alloc_bigint(&mut vm, big);
    assert!(
        PackageBamlImpl::deep_equals(&vm, &a, &b),
        "out-of-i64 bigints with equal value must deep-equal"
    );
}
