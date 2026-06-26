use baml_base::Name;
use baml_type::RuntimeTy;
use borsh::{BorshDeserialize, BorshSerialize};
use indexmap::IndexMap;

use crate::HeapPtr;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
pub struct LocalName {
    pub namespace: Vec<Name>,
    pub name: Name,
}

/// A package object on the heap.
/// Contains lookups for named items defined in the package.
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct Package {
    /// The package's dependencies as pointers to other `Package` objects.
    /// May not contain circular references.
    pub dependencies: Vec<HeapPtr>,
    /// Classes defined in the package.
    pub classes: IndexMap<LocalName, HeapPtr>,
    /// Enums defined in the package.
    pub enums: IndexMap<LocalName, HeapPtr>,
    /// Free functions defined in the package.
    pub functions: IndexMap<LocalName, HeapPtr>,
    /// Interfaces defined in the package.
    pub interfaces: IndexMap<LocalName, HeapPtr>,
    /// Implementation rules defined in the package.
    /// May include implementations for interfaces in the package's dependencies.
    /// key references an `Object::Interface` and each value is an `Object::ImplRule`
    pub impl_rules: IndexMap<HeapPtr, Vec<HeapPtr>>,
    pub recursive_type_aliases: IndexMap<LocalName, RuntimeTy>,
}
