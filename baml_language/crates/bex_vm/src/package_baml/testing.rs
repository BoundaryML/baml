//! Integration-test access to the runtime resolver.
//!
//! The resolver is an internal of virtual dispatch (`ImplResolver` is
//! crate-private), but its semantics are pinned by `tests/member_resolution.rs` —
//! a hand-derived-verdict suite that must drive real compiled programs, and the
//! in-crate `test_vm` has an empty package index, so the suite is an integration
//! test in a separate crate. These thin wrappers are its only entry; they are not
//! a public API and add no behavior of their own.

use baml_type::{Name, RealizedTy, TypeName};
use bex_vm_types::types::RuntimeImplRule;

use super::resolve::ImplResolver;
use crate::BexVm;

/// See `ImplResolver::type_implements`.
pub fn type_implements(
    vm: &BexVm,
    subject: &RealizedTy,
    iface: &TypeName,
    args: &[RealizedTy],
    assoc: &[(Name, RealizedTy)],
) -> bool {
    ImplResolver::new(vm).type_implements(subject, iface, args, assoc)
}

/// See `ImplResolver::resolve_implements_rule`.
pub fn resolve_implements_rule<'vm>(
    vm: &'vm BexVm,
    subject: &RealizedTy,
    iface: &TypeName,
    args: &[RealizedTy],
) -> Option<(&'vm RuntimeImplRule, Vec<RealizedTy>)> {
    ImplResolver::new(vm).resolve_implements_rule(subject, iface, args)
}
