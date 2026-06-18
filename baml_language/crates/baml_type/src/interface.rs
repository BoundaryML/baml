//! Interface *constraints* — the subject of generic bounds and `implements`
//! lookups, as distinct from the interface-existential [`crate::Ty::Interface`].

use std::collections::HashMap;

use crate::{Name, QualifiedTypeName, Ty};

/// Used, for example, in generic bounds to represent an interface constraint.
///
/// This does NOT represent an interface-existential type-- it is a pure interface definition.
/// Unlike this struct, interface-existentials require all associated types be defined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interface<T = Ty> {
    pub name: QualifiedTypeName,
    /// All generic parameters must be specified in order.
    pub generics: Vec<T>,
    /// Associated types on the interface are *optional*.
    /// If provided, they apply an additional constraint to the type(s) implementing the interface.
    pub associated_types: HashMap<Name, T>,
}
