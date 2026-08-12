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

impl ProgramPackage {
    /// Sort every per-kind map and each impl-rule list into the content-determined
    /// order the serialized `Program` requires, so the bytes are reproducible
    /// regardless of the source maps' iteration order (`recursive_type_aliases` in
    /// particular is sourced from a per-process-seeded `std::HashMap`).
    ///
    /// Impl rules key on their rendered `for_ty_pattern`; that `Display` drops
    /// module paths, so `{:?}` (module-qualified identity) breaks ties, and the
    /// interface instantiation (args + associated bindings) is folded in last so
    /// the same for-type implementing one interface at several instantiations
    /// orders by content rather than declaration order.
    ///
    /// The full-compile emit and the incremental linker both apply this so their
    /// `Program`s stay byte-identical.
    pub fn sort_maps(&mut self) {
        self.classes.sort_keys();
        self.enums.sort_keys();
        self.recursive_type_aliases.sort_keys();
        self.interfaces.sort_keys();
        self.impl_rules.sort_keys();
        for rules in self.impl_rules.values_mut() {
            rules.sort_by_cached_key(|rule| {
                (
                    rule.for_ty_pattern.to_string(),
                    format!("{:?}", rule.for_ty_pattern),
                    format!("{:?}", rule.interface_args),
                    format!("{:?}", rule.interface_assoc),
                )
            });
        }
    }
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
    /// See [`RuntimeImplRule::field_links`](super::RuntimeImplRule::field_links).
    /// Positional, so — unlike the name-keyed maps — it needs no canonical ordering
    /// pass in [`ProgramPackage::sort_maps`].
    pub field_links: Box<[u32]>,
}

/// The global-index-keyed twin of [`MethodImpl`](super::MethodImpl); `fqn` is the
/// callee function's `ObjectIndex`, resolved to a `HeapPtr` at load.
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct ProgramMethodImpl {
    pub fqn: ObjectIndex,
    pub frame: Vec<TyTemplate>,
}
