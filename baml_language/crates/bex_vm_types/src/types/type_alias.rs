use borsh::{BorshDeserialize, BorshSerialize};

/// Runtime type-alias representation.
///
/// A declaration object like [`Class`](crate::Class) and [`Enum`](crate::Enum),
/// rather than an entry in a name-keyed map: that is what lets a nominal
/// reference point *at* an alias instead of carrying a name to look it up by,
/// and it gives the alias a [`TypeTag`](baml_type::typetag::TypeTag) identity of
/// its own.
///
/// Only *recursive* aliases reach the runtime — a non-recursive alias is
/// expanded inline at lowering, and an irreducible one is why the indirection
/// has to survive at all.
///
/// # Transparency
///
/// An alias is not a nominal type. `type A = int` and `type B = int` are the
/// same type with different tags, so an alias tag is a *definition reference*,
/// not an identity: tag equality implies same type, but tag inequality implies
/// nothing. Equivalence must expand through the alias — see the equirecursive
/// canonicalization, which is deliberately invariant over binder names. What the
/// alias must nonetheless preserve is the indirection itself, so reflection can
/// report `MyAlias` rather than whatever it expands to.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct TypeAliasDef {
    /// Type identity: carries short name, module path, and display name.
    pub name: baml_type::TypeName,

    /// This alias's head identity, content-addressed from its fully-qualified
    /// name at emit time, in the same space as every other declared head.
    pub type_tag: baml_type::typetag::TypeTag,

    /// The aliased type.
    ///
    /// [`RealizedTy`](baml_type::RealizedTy) rather than
    /// [`RuntimeTy`](baml_type::RuntimeTy) because aliases cannot be generic —
    /// the declaration has no type-parameter list, so nothing is in scope for
    /// the right-hand side to reference. That rules out the `typevar` and
    /// `projection` axes structurally instead of by convention.
    pub definition: baml_type::RealizedTy,

    /// Runtime package that declared this alias; null for a static (or
    /// standalone) declaration. A member back-edge: reaching the alias keeps
    /// its package — globals, dependencies, sibling declarations — alive, the
    /// same ownership shape `RuntimeTypeProvenance::owner` gives classes and
    /// enums. This is a GC edge, never serialized.
    #[borsh(skip)]
    pub owner: crate::HeapPtr,
}

impl std::fmt::Display for TypeAliasDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<type {}>", self.name)
    }
}
