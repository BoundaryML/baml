//! The solver session: the shared state a type-relation derivation carries.
//!
//! Today this owns exactly one thing — the co-inductive assumption *path* that
//! [`NormalTy::is_subtype_of`](super::NormalTy::is_subtype_of) previously threaded as a
//! bare `HashSet` — plus the fact source it was previously handed alongside. Collapsing
//! the two into one parameter is what lets later work grow an answer table, a step
//! budget, and canonicalization frames without touching every signature again.
//!
//! # Why the fact source is a trait object
//!
//! The session holds `&dyn TypeContext` rather than being generic over the context. The
//! trait is deliberately object-safe for this: its seven fact methods take `&self` with no
//! generics and no `Self` in return position, and the algebra methods carry
//! `where Self: Sized`, which keeps them out of the vtable (a compile-time assertion in
//! the parent module pins this). A generic session would instead be a *different type* per
//! context, so a derivation that needs to re-run part of itself against a restricted fact
//! source could not reuse the same code paths, and the whole canonicalization machinery
//! would monomorphize once per embedding. Fact lookups already allocate
//! (`type_var_bound` builds a `Vec`, `alias_def` clones a `Ty`), so the virtual call is
//! noise against work the algebra was already doing.
//!
//! # Hypotheses are path-scoped, and barriers are what keep them honest
//!
//! An entry on the assumption path is a co-inductive *hypothesis*: "assume `a <: b` while
//! proving `a <: b`". It is sound only for derivations that sit underneath it. Sibling
//! derivations are not under that hypothesis, so leaking one into a sibling would prove
//! `a <: b` unconditionally. Callers that begin an independent relate goal therefore push
//! a [barrier](SolverSession::push_barrier); [`SolverSession::assumes`] never scans past
//! one. This is the structural form of what per-call `&mut HashSet::new()` used to do by
//! construction.
//!
//! # Why answers to barriered goals are cacheable, and interior ones are not
//!
//! Tabled solvers normally have to track which answers were derived under a hypothesis
//! about a goal still in progress — such answers are *provisional* and must not outlive
//! the cycle that justified them. Barriers make that bookkeeping unnecessary at exactly
//! the points where answers are recorded: a barriered goal cannot observe any hypothesis
//! from an enclosing derivation, and every hypothesis it raises itself is discharged
//! before it returns. Its verdict is therefore a pure function of its two operands and the
//! fact source — the same everywhere it is posed — so it can be cached without qualifying
//! it by the path that produced it.
//!
//! Interior steps have no such guarantee: they run *under* the enclosing goal's
//! hypotheses, so their verdicts are conditional and are deliberately not recorded.
//! Restricting the cache this way is also what keeps it cheap — probing costs a hash of
//! both operands, which is affordable per absorption probe and would not be affordable on
//! the structural steps the expanding-arm restriction exists to keep free.

use rustc_hash::FxHashMap;

use super::{NormalTy, TypeContext};

/// The state a single type-relation derivation carries.
///
/// Constructed at the algebra's public entry points and threaded by `&mut` from there
/// down; see the module docs for the two invariants it exists to enforce.
pub(super) struct SolverSession<'s> {
    facts: &'s dyn TypeContext,
    /// Co-inductive hypotheses on the current path, innermost last.
    assumptions: Vec<(NormalTy, NormalTy)>,
    /// Barriers, as indices into `assumptions`: hypotheses below the innermost floor
    /// belong to enclosing goals and are invisible here. A floor rather than a marker
    /// entry keeps `assumptions` a flat `Vec` of the pairs actually being scanned.
    floors: Vec<usize>,
    /// Verdicts of barriered goals already decided in this session, keyed subtype-first,
    /// or `None` in a session that recomputes every goal.
    ///
    /// Nested rather than keyed by a `(NormalTy, NormalTy)` tuple so a probe can be made
    /// from two `&NormalTy`s: a tuple key would force a deep clone of both operands just
    /// to ask the question, which is most of what the cache is meant to save. `Option`
    /// rather than a separate enable flag so "caching disabled" and "cache holds entries"
    /// cannot both be true — there is no map to record into when it is off.
    answers: Option<FxHashMap<NormalTy, FxHashMap<NormalTy, bool>>>,
}

impl<'s> SolverSession<'s> {
    pub(super) fn new(facts: &'s dyn TypeContext) -> Self {
        Self {
            facts,
            assumptions: Vec::new(),
            floors: Vec::new(),
            answers: Some(FxHashMap::default()),
        }
    }

    /// A session that recomputes every goal, for differential tests: the cache is an
    /// optimization and must never be observable in a verdict.
    #[cfg(test)]
    pub(super) fn new_uncached(facts: &'s dyn TypeContext) -> Self {
        Self {
            answers: None,
            ..Self::new(facts)
        }
    }

    /// The nominal facts this derivation reasons against.
    pub(super) fn facts(&self) -> &'s dyn TypeContext {
        self.facts
    }

    /// Whether `sub <: sup` is already assumed on the current path.
    ///
    /// A linear scan, not a hash probe, and deliberately so: the path is bounded by the
    /// expanding-arm recursion depth (a handful of entries in practice), while hashing
    /// would mean a full-tree walk of both operands at every probe — the cost the
    /// expanding-arm restriction exists to avoid.
    pub(super) fn assumes(&self, sub: &NormalTy, sup: &NormalTy) -> bool {
        let floor = self.floors.last().copied().unwrap_or(0);
        self.assumptions[floor..]
            .iter()
            .any(|(a, b)| a == sub && b == sup)
    }

    /// Record `sub <: sup` as a hypothesis for the derivation about to run.
    /// Every call must be matched by [`Self::pop_assumption`].
    pub(super) fn push_assumption(&mut self, sub: NormalTy, sup: NormalTy) {
        self.assumptions.push((sub, sup));
    }

    pub(super) fn pop_assumption(&mut self) {
        let popped = self.assumptions.pop();
        debug_assert!(popped.is_some(), "assumption push/pop mismatch");
    }

    /// Begin an independent relate goal: hypotheses recorded above this point are
    /// invisible to enclosing derivations, and enclosing hypotheses are invisible here.
    /// Every call must be matched by [`Self::pop_barrier`].
    pub(super) fn push_barrier(&mut self) {
        self.floors.push(self.assumptions.len());
    }

    pub(super) fn pop_barrier(&mut self) {
        let floor = self.floors.pop();
        debug_assert_eq!(
            floor,
            Some(self.assumptions.len()),
            "a goal left hypotheses on the path"
        );
    }

    /// Decide `sub <: sup` as an independent goal, under its own barrier.
    ///
    /// This is the entry point for every probe that is not a continuation of the current
    /// derivation — union-member absorption and the automaton's per-state algebra. Those
    /// callers re-pose the same pairs across the automaton's fixpoint rounds and across
    /// structurally identical unions elsewhere in a term, which is what the cache serves;
    /// see the module docs for why a verdict recorded here is unconditional.
    pub(super) fn prove_subtype(&mut self, sub: &NormalTy, sup: &NormalTy) -> bool {
        if let Some(answers) = &self.answers
            && let Some(&proven) = answers.get(sub).and_then(|sups| sups.get(sup))
        {
            return proven;
        }
        self.push_barrier();
        let proven = sub.is_subtype_of(sup, self);
        self.pop_barrier();
        if let Some(answers) = &mut self.answers {
            answers
                .entry(sub.clone())
                .or_default()
                .insert(sup.clone(), proven);
        }
        proven
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[expect(deprecated, reason = "path bookkeeping only; no facts are consulted")]
    use crate::normalize::NoFacts;

    #[expect(deprecated, reason = "path bookkeeping only; no facts are consulted")]
    fn session() -> SolverSession<'static> {
        SolverSession::new(&NoFacts)
    }

    #[test]
    fn a_barrier_hides_enclosing_hypotheses() {
        let mut s = session();
        s.push_assumption(NormalTy::Int, NormalTy::String);
        assert!(s.assumes(&NormalTy::Int, &NormalTy::String));

        // An independent goal must not inherit the hypothesis: assuming `a <: b`
        // is sound only for the derivation of `a <: b` itself, and a sibling that
        // saw it would prove the pair unconditionally.
        s.push_barrier();
        assert!(!s.assumes(&NormalTy::Int, &NormalTy::String));
        s.pop_barrier();

        // …and it is restored for the enclosing derivation afterwards.
        assert!(s.assumes(&NormalTy::Int, &NormalTy::String));
        s.pop_assumption();
        assert!(!s.assumes(&NormalTy::Int, &NormalTy::String));
    }

    #[test]
    fn hypotheses_match_on_both_operands_and_direction() {
        let mut s = session();
        s.push_assumption(NormalTy::Int, NormalTy::String);
        assert!(!s.assumes(&NormalTy::String, &NormalTy::Int));
        assert!(!s.assumes(&NormalTy::Int, &NormalTy::Bool));
    }
}
