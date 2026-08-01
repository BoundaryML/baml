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
