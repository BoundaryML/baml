//! The payload of a runtime `type` value: the type it denotes.
//!
//! A `type` value has no identity of its own. Two `type` values denote the
//! same type exactly when the types they carry are **equivalent** — mutual
//! subtypes, decided by structural equality of canonical forms (TYPE_SYSTEM.md,
//! "Equivalence and canonical forms"). Equivalence is a function of the
//! program's facts (alias definitions, interface `requires` edges), not of the
//! payload alone, so [`TypeValue`] deliberately implements neither `PartialEq`
//! nor `Hash`: every comparison threads a `TypeContext` (inside the VM, that is
//! the VM itself). Nominal types compare through their declaration identity, so
//! two runtime declarations are distinct types however alike their shapes.
//!
//! `ty` is stored **as spelled**, never canonicalized in place: unreflection
//! feeds SAP and the LLM boundary, where union member order is semantic.

use baml_type::{Name, QualifiedTypeName, RealizedTy};
use indexmap::IndexMap;

/// Heap-independent definition graph used at host boundaries. It contains no
/// pointers, so serializing it cannot leak engine state. The VM reconstructs
/// ordinary `Class`/`Enum` objects — fresh declarations, distinct from any
/// same-named ones it already has — whenever one arrives from a host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortableTypeDef {
    pub root: baml_type::RuntimeTy,
    pub classes: Vec<PortableClassDef>,
    pub enums: Vec<PortableEnumDef>,
    pub witnesses: Vec<DynWitnessDef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortableMetadata {
    pub description: Option<String>,
    pub alias: Option<String>,
    pub docstring: Option<String>,
    pub other: IndexMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortableClassDef {
    pub name: QualifiedTypeName,
    pub fields: Vec<PortableClassFieldDef>,
    pub metadata: PortableMetadata,
    pub generic_param_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortableClassFieldDef {
    pub name: String,
    pub ty: baml_type::RuntimeTy,
    pub metadata: PortableMetadata,
    pub skip: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortableEnumDef {
    pub name: QualifiedTypeName,
    pub variants: Vec<PortableEnumVariantDef>,
    pub metadata: PortableMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortableEnumVariantDef {
    pub name: String,
    pub metadata: PortableMetadata,
    pub skip: bool,
}

/// Heap-independent witness contribution carried by a minted definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynWitnessDef {
    pub interface: QualifiedTypeName,
    pub interface_args: Vec<RealizedTy>,
    pub associated_types: Vec<(Name, RealizedTy)>,
    /// Interface field name -> physical class field name, in interface order.
    pub field_links: Vec<(Name, Name)>,
}

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
