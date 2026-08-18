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

use crate::HeapPtr;

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

/// Runtime schema definitions carried by a `type` value.
///
/// The map is a per-value overlay, never a process/global registry. Heap
/// pointers keep the ordinary `Object::Class` and `Object::Enum` definitions
/// authoritative for reflection and parsed values; the owning `Object::Type`
/// traces them.
#[derive(Debug, Clone, Default)]
pub struct DynTypeDefs {
    pub classes: IndexMap<QualifiedTypeName, HeapPtr>,
    pub enums: IndexMap<QualifiedTypeName, HeapPtr>,
    /// Structured interface witnesses are part of a runtime definition's
    /// equivalence tuple (BEP-066 I-6).  They deliberately contain no heap
    /// pointers: dispatchable rules live in the engine's dynamic side table.
    pub witnesses: Vec<DynWitnessDef>,
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

impl DynTypeDefs {
    pub fn with_class(name: QualifiedTypeName, ptr: HeapPtr) -> Self {
        Self {
            classes: IndexMap::from([(name, ptr)]),
            enums: IndexMap::new(),
            witnesses: Vec::new(),
        }
    }

    pub fn with_enum(name: QualifiedTypeName, ptr: HeapPtr) -> Self {
        Self {
            classes: IndexMap::new(),
            enums: IndexMap::from([(name, ptr)]),
            witnesses: Vec::new(),
        }
    }

    pub fn merge_from(&mut self, other: &Self) {
        for (name, ptr) in &other.classes {
            self.classes.entry(name.clone()).or_insert(*ptr);
        }
        for (name, ptr) in &other.enums {
            self.enums.entry(name.clone()).or_insert(*ptr);
        }
        for witness in &other.witnesses {
            if !self.witnesses.contains(witness) {
                self.witnesses.push(witness.clone());
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.classes.is_empty() && self.enums.is_empty() && self.witnesses.is_empty()
    }
}

/// Provenance retained by a runtime-created nominal definition.
///
/// The definition itself cannot include its own pointer, so `defs` contains
/// only dependencies. `type.of_value` adds the instance/variant's definition
/// pointer back when reconstructing the declaration's `type` value.
#[derive(Debug, Clone)]
pub struct RuntimeTypeProvenance {
    pub defs: DynTypeDefs,
    /// Runtime package that owns this nominal definition. Runtime-constructed
    /// standalone types use a null pointer. This is a GC edge.
    pub owner: HeapPtr,
}

/// What an `Object::Type` wraps: the type this value denotes.
///
/// Carried data only — see the module doc for why there is no identity token
/// and no `PartialEq`.
#[derive(Debug, Clone)]
pub struct TypeValue {
    /// The type this value denotes, as spelled.
    pub ty: crate::RealizedTy,
    defs: DynTypeDefs,
    /// Runtime package whose definitions give this type meaning. Static type
    /// values use a null pointer. This is a GC edge.
    pub owner: HeapPtr,
}

impl TypeValue {
    /// A type value over declarations the program already knows: a static
    /// spelling (`type.of<T>()`, a reflected signature, a decoded wire type).
    pub fn new(ty: crate::RealizedTy) -> Self {
        Self {
            ty,
            defs: DynTypeDefs::default(),
            owner: HeapPtr::null(),
        }
    }

    /// A type value carrying the runtime definitions its type refers to, with
    /// no owning package (standalone typebuilder constructions).
    pub fn with_defs(ty: crate::RealizedTy, defs: DynTypeDefs) -> Self {
        Self {
            ty,
            defs,
            owner: HeapPtr::null(),
        }
    }

    /// A type value whose meaning comes from a runtime package's declarations.
    pub fn owned(ty: crate::RealizedTy, defs: DynTypeDefs, owner: HeapPtr) -> Self {
        debug_assert!(!owner.is_null(), "an owned type value needs its package");
        Self { ty, defs, owner }
    }

    pub fn defs(&self) -> &DynTypeDefs {
        &self.defs
    }

    pub fn defs_mut(&mut self) -> &mut DynTypeDefs {
        &mut self.defs
    }
}
