use baml_base::Name;
use baml_type::{RuntimeTy, TyTemplate};
use borsh::{BorshDeserialize, BorshSerialize};
use indexmap::IndexMap;

use crate::{HeapPtr, ObjectIndex, types::interface::InterfaceBound};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
pub struct LocalName {
    pub namespace: Vec<Name>,
    pub name: Name,
}

/// A package object on the heap.
/// Contains lookups for named items defined in the package.
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct Package {
    /// Classes defined in the package.
    pub classes: IndexMap<LocalName, HeapPtr>,
    /// Enums defined in the package.
    pub enums: IndexMap<LocalName, HeapPtr>,
    /// Interfaces defined in the package.
    pub interfaces: IndexMap<LocalName, HeapPtr>,
    /// Implementation rules defined in the package.
    /// May include implementations for interfaces in the package's dependencies.
    /// key references an `Object::Interface` and each value is an `Object::ImplRule`
    pub impl_rules: IndexMap<HeapPtr, Vec<HeapPtr>>,
    pub recursive_type_aliases: IndexMap<LocalName, RuntimeTy>,
}

/// The serialized, global-index-keyed twin of [`Package`]. The `Program` must be
/// `HeapPtr`-free (pointers are runtime-only and there is no heap at emit time),
/// so the emit produces this; the loader allocates the [`Package`] +
/// [`Object::Interface`](super::Object::Interface) /
/// [`Object::ImplRule`](super::Object::ImplRule) from it, resolving each
/// [`ObjectIndex`] to a compile-time `HeapPtr`. Mirrors how classes/enums/
/// functions are carried as pooled objects referenced by index.
#[derive(Debug, Clone, Default, BorshSerialize, BorshDeserialize)]
pub struct ProgramPackage {
    pub classes: IndexMap<LocalName, ObjectIndex>,
    pub enums: IndexMap<LocalName, ObjectIndex>,
    pub interfaces: IndexMap<LocalName, ObjectIndex>,
    /// Implemented-interface `ObjectIndex` → the impl rules of it declared in
    /// this package (may target an interface from a dependency).
    pub impl_rules: IndexMap<ObjectIndex, Vec<ProgramImplRule>>,
    pub recursive_type_aliases: IndexMap<LocalName, RuntimeTy>,
}

/// The global-index-keyed twin of [`RuntimeImplRule`](super::RuntimeImplRule);
/// `interface_head`/`fqn` are `ObjectIndex`es the loader resolves to `HeapPtr`s.
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct ProgramImplRule {
    pub interface_head: ObjectIndex,
    pub for_ty_pattern: TyTemplate,
    pub generic_param_bounds: Vec<Vec<InterfaceBound>>,
    pub interface_args: Vec<TyTemplate>,
    pub interface_assoc: Vec<(Name, TyTemplate)>,
    pub methods: IndexMap<Name, ProgramMethodImpl>,
}

/// The global-index-keyed twin of [`MethodImpl`](super::MethodImpl); `fqn` is the
/// callee function's `ObjectIndex`, resolved to a `HeapPtr` at load.
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct ProgramMethodImpl {
    pub fqn: ObjectIndex,
    pub frame: Vec<TyTemplate>,
}
