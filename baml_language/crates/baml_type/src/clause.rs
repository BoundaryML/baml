//! Impl rules as *clauses* — the form the type-relation solver reasons over.
//!
//! An `implements` block is, to a solver, a rule with a head and a body: it applies to
//! whatever its `for`-pattern matches, and it applies *only if* the bounds on its generic
//! parameters hold at that match. Both halves are pure type data — patterns over the
//! impl's own parameters, addressed positionally — which is what lets one representation
//! serve a compiler reading declarations and a runtime reading a baked table.
//!
//! Nothing here describes what an impl *provides* (its methods, its field layout). That
//! is dispatch payload, retrieved once selection has already picked a clause, and keeping
//! it out is what makes the clause form shareable across embeddings that disagree entirely
//! about how a method is called.
//!
//! Clauses are supplied by the same context that supplies the nominal facts —
//! [`TypeContext::for_each_clause`](crate::normalize::TypeContext::for_each_clause) — because
//! both are non-re-entrant enumerations of one world's data; the fact/clause distinction is
//! about how the solver *uses* them (facts consulted as leaves, clauses searched as rules),
//! not about who supplies them.

use crate::{Name, TyTemplate, TyTemplateInterface};

/// A supplier's handle for one clause, opaque to the solver.
///
/// The solver only ever compares these and hands them back, so each embedding is free to
/// encode whatever it needs to recover the clause's payload — a position in a baked table,
/// an interned declaration handle. It travels with a decision so that "which impl applied"
/// survives to whoever needs to act on it, and so a failure can name the clause that
/// failed rather than only the goal that did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClauseId(pub u64);

/// One `implements` rule, borrowed from whatever the supplier keeps it in.
///
/// Borrowed rather than owned because clause enumeration sits on the dispatch path: a
/// receiver is matched against every candidate for its interface, and copying each
/// candidate's patterns to ask whether it applies would allocate proportionally to the
/// candidate set on every call. The solver reads patterns and never retains them.
pub struct ImplClause<'a> {
    /// The supplier's handle for this clause.
    pub id: ClauseId,
    /// How many generic parameters the patterns address, i.e. the size of the binding
    /// frame a match against this clause must be given.
    pub num_vars: usize,
    /// The implementor pattern — what `for` was written against.
    pub self_pattern: &'a TyTemplate,
    /// The implemented interface's input arguments. Selection keys on these; associated
    /// bindings are outputs of the impl and never narrow which impl applies.
    pub iface_args: &'a [TyTemplate],
    /// The implemented interface's associated-type bindings.
    pub iface_assoc: &'a [(Name, TyTemplate)],
    /// Per generic parameter, positionally, the set of bounds it must satisfy — the
    /// clause's body, each element an obligation once the match has bound it.
    ///
    /// A bound is an [interface constraint](TyTemplateInterface) over the impl's own
    /// parameters, so discharging one means substituting the match's bindings into it
    /// first: `T extends Container<U>` at a match binding `U = int` is the obligation
    /// `T: Container<int>`. Bounds are interfaces rather than types, so an intersection
    /// (`T extends A & B`) is a *set* of them, and an empty set is unbounded.
    pub bounds: &'a [Vec<TyTemplateInterface>],
}
