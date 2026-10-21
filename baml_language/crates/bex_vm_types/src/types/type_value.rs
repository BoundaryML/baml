//! The payload of a runtime `type` value: the type it denotes.
//!
//! A `type` value has no identity of its own. Two `type` values denote the
//! same type exactly when the types they carry are **equivalent** — mutual
//! subtypes, decided by structural equality of canonical forms (`TYPE_SYSTEM.md`,
//! "Equivalence and canonical forms"). Equivalence is a function of the
//! program's facts (alias definitions, interface `requires` edges), not of the
//! payload alone, so [`TypeValue`] deliberately implements neither `PartialEq`
//! nor `Hash`: every comparison threads a `TypeContext` (inside the VM, that is
//! the VM itself). Nominal types compare through their declaration identity, so
//! two runtime declarations are distinct types however alike their shapes.
//!
//! `ty` is stored **as spelled**, never canonicalized in place: unreflection
//! feeds SAP and the LLM boundary, where union member order is semantic.

/// What an `Object::Type` wraps: the type this value denotes.
///
/// Carried data only — see the module doc for why there is no identity token
/// and no `PartialEq`. There are no sidecars either: the type's heads point at
/// the declarations that give it meaning, so tracing the type reaches them and
/// dereferencing a head resolves them. A separate definition table would be a
/// second, weaker answer to both questions.
#[derive(Debug, Clone)]
pub struct TypeValue {
    /// The type this value denotes, as spelled.
    pub ty: crate::RealizedTy,
}

impl TypeValue {
    pub fn new(ty: crate::RealizedTy) -> Self {
        Self { ty }
    }
}
