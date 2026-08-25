//! What the type algebra requires of a nominal head.
//!
//! A *head* is what a `Class`, `Enum`, `Interface`, or `TypeAlias` refers to —
//! the `N` parameter of the [`Ty`](crate::Ty) family. The compiler spells it as
//! a [`QualifiedTypeName`](crate::QualifiedTypeName); the runtime substitutes a
//! heap-anchored handle. The algebra is written against this trait so one
//! implementation serves both.

/// A nominal type head the algebra can canonicalize over.
///
/// Purely a bundle of bounds — a head is opaque to the algebra, which only ever
/// threads one through and hands it back in the resulting `Ty`. That opacity is
/// what lets a head be a runtime handle at all; anything the algebra needs to
/// *know* about a particular head comes from
/// [`TypeContext`](crate::normalize::TypeContext), which owns the state needed
/// to answer (a registry, a heap), rather than from the head itself.
///
/// Each bound is load-bearing, not defensive:
///
/// - `Clone` — the family requires it (a `Ty` is a cloneable value tree).
/// - `Ord` — heads are interned into a `BTreeMap` for integer comparison, union
///   members are sorted into canonical form by it, and μ-canonicalization picks
///   the least head of a state as its rendering representative. Canonical form
///   is therefore only as deterministic as this ordering.
/// - `'static` — a head is an owned identity, never a borrow into something
///   else. Lets the μ-automaton's special leaves be `'static` constants
///   (see `mu`'s `never`/`unknown_top`/…), keeping its interner borrowing.
/// - `Eq` + `Hash` — the normalized form is compared and memoized by value, and
///   head identity is decided by `==` against a head obtained from the context
///   (see [`TypeContext::head_lookup`](crate::normalize::TypeContext::head_lookup)).
///
/// Note what is absent: nothing that recovers a display name, and nothing that
/// recognizes a particular builtin. Both would force every head representation
/// to understand names, which is exactly what the runtime's handle cannot do.
///
/// Blanket-implemented, so a representation opts in by satisfying the bounds.
pub trait Head: Clone + Ord + Eq + std::hash::Hash + std::fmt::Debug + 'static {}

impl<T: Clone + Ord + Eq + std::hash::Hash + std::fmt::Debug + 'static> Head for T {}

/// A nominal head that carries both its identity and its spelling.
///
/// The boundary layers — sys-ops, SAP, output-format rendering — need two
/// things at once that neither of the other heads provides. They need an
/// *identity* that tells two declarations apart even when a user spelled both
/// `Widget`, because a name-keyed definition table silently merges them (and a
/// runtime declaration can shadow a compiled one of the same name). And they
/// need a *name*, because a rendered prompt and a diagnostic both say what the
/// type is called.
///
/// A [`QualifiedTypeName`](crate::QualifiedTypeName) gives only the second, and
/// an anonymous declaration has none at all; a runtime `TypeHead` gives the
/// first but reaches its name through a heap pointer, which these layers cannot
/// hold — SAP runs after the heap permit is released, so its types must be
/// plain owned data.
///
/// The name is a [`DeclarationName`](crate::DeclarationName) rather than a
/// qualified name precisely so an anonymous declaration can say it has none.
/// Flattening it to a `user.`-local spelling would fabricate the very
/// collidable name that keying by identity exists to avoid.
///
/// So identity is the [`TypeTag`](crate::typetag::TypeTag) and nothing else —
/// `Eq`, `Ord`, and `Hash` all key on it, exactly as the runtime head does with
/// its own tag, which keeps those relations mutually consistent and stable
/// under collection. The name rides along as *data*: rendered, never compared.
#[derive(Clone, Debug)]
pub struct TaggedTypeName {
    tag: crate::typetag::TypeTag,
    name: crate::DeclarationName,
}

impl TaggedTypeName {
    #[must_use]
    pub fn new(tag: crate::typetag::TypeTag, name: crate::DeclarationName) -> Self {
        Self { tag, name }
    }

    /// This head's identity.
    #[must_use]
    pub fn tag(&self) -> crate::typetag::TypeTag {
        self.tag
    }

    /// What this head is called. Display data — never an identity; compare
    /// [`tag`](Self::tag) instead.
    #[must_use]
    pub fn name(&self) -> &crate::DeclarationName {
        &self.name
    }

    /// What this head is *called*, for output labels and diagnostics. Two
    /// distinct declarations can share one display name; that is exactly why it
    /// is not the identity.
    #[must_use]
    pub fn display_name(&self) -> crate::Name {
        self.name.display_name()
    }

    /// The qualified name, when this head names a declaration that has one.
    /// `None` for an anonymous declaration — which has no spelling any lookup
    /// could resolve, and must not be handed a fabricated one.
    #[must_use]
    pub fn declared(&self) -> Option<&crate::TypeName> {
        self.name.declared()
    }
}

/// Identity is the tag alone, so two heads that name the same declaration are
/// equal however either was spelled.
impl PartialEq for TaggedTypeName {
    fn eq(&self, other: &Self) -> bool {
        self.tag == other.tag
    }
}

impl Eq for TaggedTypeName {}

impl std::hash::Hash for TaggedTypeName {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.tag.hash(state);
    }
}

/// Consistent with [`PartialEq`] as the `Ord` contract requires: `cmp` returns
/// `Equal` exactly when `eq` is true. Ordering by name instead would let two
/// references to one declaration compare unequal.
impl Ord for TaggedTypeName {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.tag.cmp(&other.tag)
    }
}

impl PartialOrd for TaggedTypeName {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::fmt::Display for TaggedTypeName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.name.fmt(f)
    }
}
