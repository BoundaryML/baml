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
//! # Why answers to barriered goals are recordable, and interior ones are not
//!
//! Tabled solvers normally have to track which answers were derived under a hypothesis
//! about a goal still in progress — such answers are *provisional* and must not outlive
//! the cycle that justified them. Barriers make that bookkeeping unnecessary at exactly
//! the points where answers are recorded: a barriered goal cannot observe any hypothesis
//! from an enclosing derivation, and every hypothesis it raises itself is discharged
//! before it returns. Its verdict is therefore a pure function of its two operands and the
//! fact source — the same everywhere it is posed — so it can be recorded in the
//! [store](super::store) without qualifying it by the path that produced it.
//!
//! Interior steps have no such guarantee: they run *under* the enclosing goal's
//! hypotheses, so their verdicts are conditional and are deliberately not recorded.
//! Restricting recording this way is also what keeps it cheap — probing costs one hash
//! of each operand, which is affordable per absorption probe and would not be affordable
//! on the structural steps the expanding-arm restriction exists to keep free.
//!
//! # Budget discipline
//!
//! The session carries the per-root step pool ([`Limits`](super::Limits)); the two
//! invariants that keep it sound alongside the store are enforced here. A verdict
//! derived after any charge was *refused* is budget-relative — it says where the pool
//! ran out, not what the types are — so recording stops at the first refusal. And a
//! store hit is admitted only by re-charging the recorded cost, so cached and
//! recomputed runs spend identically and verdicts never depend on cache state.

use super::{
    NormalTy, TypeContext,
    store::{Answer, Answers, Cost},
};

/// Where a session keeps established knowledge: its own store, a caller's shared one,
/// or — for differential tests only — none, so every goal recomputes.
enum Store<'s> {
    Owned(Answers),
    Shared(&'s mut Answers),
    #[cfg(test)]
    Disabled,
}

impl Store<'_> {
    fn get(&self) -> Option<&Answers> {
        match self {
            Store::Owned(store) => Some(store),
            Store::Shared(store) => Some(store),
            #[cfg(test)]
            Store::Disabled => None,
        }
    }

    fn get_mut(&mut self) -> Option<&mut Answers> {
        match self {
            Store::Owned(store) => Some(store),
            Store::Shared(store) => Some(store),
            #[cfg(test)]
            Store::Disabled => None,
        }
    }
}

/// The state a single type-relation derivation carries.
///
/// Constructed at the algebra's public entry points and threaded by `&mut` from there
/// down; see the module docs for the invariants it exists to enforce.
pub(super) struct SolverSession<'s> {
    facts: &'s dyn TypeContext,
    /// Co-inductive hypotheses on the current path, innermost last.
    assumptions: Vec<(NormalTy, NormalTy)>,
    /// Barriers, as indices into `assumptions`: hypotheses below the innermost floor
    /// belong to enclosing goals and are invisible here. A floor rather than a marker
    /// entry keeps `assumptions` a flat `Vec` of the pairs actually being scanned.
    floors: Vec<usize>,
    /// The remaining step pool ([`Limits::step_budget`](super::Limits::step_budget)),
    /// charged at the subtype expanding arms. One pool per session = one pool per
    /// algebra root, the granularity the design names "per root goal".
    steps: u64,
    /// Whether a [`Self::charge`] was ever *refused*. From that point every verdict
    /// in this session is budget-relative (fail-closed), so recording stops — see
    /// [`Self::prove_subtype`]. Distinct from "the pool reached zero": a derivation
    /// that spent its last step but never asked for another is still exact.
    exhausted: bool,
    store: Store<'s>,
}

impl<'s> SolverSession<'s> {
    /// A session with its own fresh store, dropped when the session is.
    pub(super) fn new(facts: &'s dyn TypeContext) -> Self {
        Self::over(facts, Store::Owned(Answers::default()))
    }

    /// A session recording into (and admitting from) a caller-owned store, so
    /// knowledge survives the session. The caller owns the store↔world association:
    /// a store must never be shared across fact sources.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "first non-test caller arrives with the public \
                                    session surface"
        )
    )]
    pub(super) fn with_store(facts: &'s dyn TypeContext, store: &'s mut Answers) -> Self {
        Self::over(facts, Store::Shared(store))
    }

    /// A session that recomputes every goal, for differential tests: the store is an
    /// optimization and must never be observable in a verdict.
    #[cfg(test)]
    pub(super) fn new_uncached(facts: &'s dyn TypeContext) -> Self {
        Self::over(facts, Store::Disabled)
    }

    fn over(facts: &'s dyn TypeContext, store: Store<'s>) -> Self {
        Self {
            facts,
            assumptions: Vec::new(),
            floors: Vec::new(),
            steps: facts.limits().step_budget,
            exhausted: false,
            store,
        }
    }

    /// The nominal facts this derivation reasons against.
    pub(super) fn facts(&self) -> &'s dyn TypeContext {
        self.facts
    }

    /// Spend one step, or refuse: `false` means the pool is exhausted and the
    /// caller must fail closed. A refusal marks the whole session
    /// [exhausted](Self::exhausted), which stops answer recording — every verdict
    /// from here on is an artifact of where the budget ran out, not of the types.
    pub(super) fn charge(&mut self) -> bool {
        if self.steps == 0 {
            self.exhausted = true;
            return false;
        }
        self.steps -= 1;
        true
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
    /// structurally identical unions elsewhere in a term, which is what the store serves;
    /// see the module docs for why a verdict recorded here is unconditional.
    ///
    /// Budget discipline in both directions: a hit is admitted only by re-charging the
    /// recorded cost (an insufficient pool falls through to recompute, which fails
    /// closed exactly where a first computation would — never earlier, never later),
    /// and a verdict derived after any refusal is not recorded. Together these make
    /// stored and recomputed runs verdict-identical at every budget.
    pub(super) fn prove_subtype(&mut self, sub: &NormalTy, sup: &NormalTy) -> bool {
        if let Some(store) = self.store.get()
            && let Some(a) = store.lookup(sub)
            && let Some(b) = store.lookup(sup)
            && let Some(Answer { proven, cost }) = store.subtype_answer(a, b)
            && self.steps >= cost.steps
        {
            self.steps -= cost.steps;
            return proven;
        }
        let steps_before = self.steps;
        self.push_barrier();
        let proven = sub.is_subtype_of(sup, self);
        self.pop_barrier();
        if !self.exhausted
            && let Some(store) = self.store.get_mut()
        {
            let (a, b) = (store.intern(sub), store.intern(sup));
            store.record_subtype(
                a,
                b,
                Answer {
                    proven,
                    cost: Cost {
                        steps: steps_before - self.steps,
                    },
                },
            );
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
