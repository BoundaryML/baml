//! The solver session: the shared state a type-relation derivation carries.
//!
//! One search spans the whole relation family: the co-inductive subtype machinery
//! (assumption path, barriers) and the inductive membership machinery (goal scopes,
//! clause search) live on one stack, spend one step pool, and record into one
//! [store](super::store). That sharing is the point — membership discharging a bound
//! poses subtype comparisons that hit the same tables, and (once facts flow through
//! goals) a subtype derivation consulting membership is one more frame on the same
//! interleaved stack rather than a second, severed search.
//!
//! # Why the fact source is a trait object
//!
//! The session holds `&dyn TypeContext` rather than being generic over the context. The
//! trait is deliberately object-safe for this: its fact methods take `&self` with no
//! type generics and no `Self` in return position, and the algebra methods carry
//! `where Self: Sized`, which keeps them out of the vtable (this field is itself the
//! compile-time pin — the crate stops building if the trait loses object safety). A
//! generic session would instead be a *different type* per
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
//! Membership goals are deliberately *not* barrier-scoped: a goal already in progress
//! must be seen through any interleaving of subtype work, or a membership cycle could
//! smuggle itself past the repeat scan by routing through a comparison. The sorts
//! differ in cycle *semantics* too — a repeated subtype hypothesis closes
//! co-inductively (productive self-support proves the pair), while a repeated
//! membership goal closes inductively (a self-supporting membership has no concrete
//! grounding, so the recurrence contributes `false` — the least-fixpoint reading).
//!
//! # Why answers to scoped goals are recordable, and interior ones are not
//!
//! Tabled solvers normally have to track which answers were derived under a hypothesis
//! about a goal still in progress — such answers are *provisional* and must not outlive
//! the cycle that justified them. Recording [scopes](Scope) make that discipline
//! explicit: an answer enters the store only when its scope closed `Grounded` — no open
//! membership goal leaned on, no limit interfered — at which point it is a pure function
//! of its goal and the world, the same everywhere it is posed. A cycle that closed under
//! its own head is grounded *from outside* (the head discharged its own provisional
//! support), which is what lets cycle heads memoize while interior participants must
//! recompute if asked again elsewhere.
//!
//! Interior subtype steps are never recorded at all: they run *under* the enclosing
//! goal's hypotheses, so their verdicts are conditional. Restricting recording to
//! barriered roots is also what keeps it cheap — probing costs one hash per operand,
//! affordable per absorption probe and not on the structural steps the expanding-arm
//! restriction exists to keep free.
//!
//! # Budget discipline
//!
//! The session carries the per-root step pool ([`Limits`]); the invariants that keep
//! budgets sound alongside the store are enforced here. A verdict derived after any
//! charge was *refused* is budget-relative — it says where the pool ran out, not what
//! the types are — so it never enters the store (the sticky [`SolverSession::exhausted`]
//! flag for step refusals; per-scope [`Support::Refused`] for depth refusals, which are
//! path-local). And a store hit is admitted only by re-verifying and re-charging the
//! recorded [`Cost`], so stored and recomputed runs spend identically and verdicts never
//! depend on cache state or query order — only on the configured limits.

use std::{borrow::Cow, ops::ControlFlow};

use super::{
    Indeterminacy, Limits, NormalTy, Normalized, Truth, TypeContext,
    store::{Answer, Answers, CanonId, Cost, MembershipGoal},
};
use crate::{
    ClauseId, ImplClause, Literal, Name, QualifiedTypeName, RealizedTy, SubstituteError,
    TemplateCompare, Ty, TyTemplate,
};

/// Where a session keeps established knowledge: its own store, a caller's shared
/// one, or — for differential tests only — an identity-only store whose answers
/// are neither admitted nor recorded, so every goal recomputes.
///
/// Identity interning works in every mode (through [`Self::intern`]): canonical ids
/// are search infrastructure (goal keys, the repeat scan), not advisory answers, so
/// no mode is ever without them.
enum Store<'s> {
    Owned(Answers),
    Shared(&'s mut Answers),
    #[cfg(test)]
    IdentityOnly(Answers),
}

impl Store<'_> {
    /// Advisory answer access; `None` when answers are disabled.
    fn answers(&self) -> Option<&Answers> {
        match self {
            Store::Owned(store) => Some(store),
            Store::Shared(store) => Some(store),
            #[cfg(test)]
            Store::IdentityOnly(_) => None,
        }
    }

    /// Advisory answer access; `None` when answers are disabled.
    fn answers_mut(&mut self) -> Option<&mut Answers> {
        match self {
            Store::Owned(store) => Some(store),
            Store::Shared(store) => Some(store),
            #[cfg(test)]
            Store::IdentityOnly(_) => None,
        }
    }

    /// Identity interning, available in every mode.
    fn intern(&mut self, ty: &NormalTy) -> CanonId {
        match self {
            Store::Owned(store) => store.intern(ty),
            Store::Shared(store) => store.intern(ty),
            #[cfg(test)]
            Store::IdentityOnly(store) => store.intern(ty),
        }
    }
}

/// What a completed derivation's answer rests on — the validity summary of its
/// recording scope, a join-semilattice that only degrades as the search explores:
/// `Grounded < Cycle < Refused`.
///
/// A `Grounded` answer is a pure function of its goal and the world — recordable.
/// A `Cycle` answer consumed the provisional `false` of the in-progress membership
/// goal at the carried scope index, so it is valid only inside that cycle — except
/// viewed from the cycle's own head, where the cycle is closed and the answer
/// grounded (the normalization at pop). A `Refused` answer had part of its subtree
/// cut by a limit: it reflects where the budget ran out, not what the types are,
/// and it invalidates everything up its path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Support {
    Grounded,
    /// The outermost (lowest-indexed) in-progress goal this subtree's answer
    /// leans on — the root of the widest cycle it participates in.
    Cycle(usize),
    Refused,
}

impl Support {
    /// Fold a child subtree's summary into its parent's: the worse kind wins, and
    /// two cycle supports lean on the outermost (lower-indexed) of the two roots.
    fn join(self, other: Support) -> Support {
        match (self, other) {
            (Support::Refused, _) | (_, Support::Refused) => Support::Refused,
            (Support::Cycle(a), Support::Cycle(b)) => Support::Cycle(a.min(b)),
            (Support::Cycle(i), Support::Grounded) | (Support::Grounded, Support::Cycle(i)) => {
                Support::Cycle(i)
            }
            (Support::Grounded, Support::Grounded) => Support::Grounded,
        }
    }
}

/// What opened a recording scope. The kinds close cycles differently (a
/// membership goal is inductive and can be a cycle head; the others cannot) and
/// each pop site decides its own recording, but every kind carries identical
/// validity and cost accounting — which is what lets refusals and cycle supports
/// propagate across sort boundaries on one interleaved stack.
#[derive(Debug)]
enum ScopeKind {
    /// A barriered subtype goal ([`SolverSession::prove_subtype`]).
    SubtypeRoot,
    /// An in-progress membership goal; its key is what the repeat scan compares.
    MembershipGoal(MembershipGoal),
    /// A clause selection ([`SolverSession::select`]): records nothing itself, but
    /// must observe refusals in candidates' bound discharge — a starved search
    /// yields *no* selection, never a different one.
    Selection,
    /// An algebra entry point's observation scope: records nothing, but collects
    /// the support state that decides whether a fail-closed interior `false`
    /// surfaces as a definite `No` or an open `Unknown` — and whether a
    /// canonicalization walk produced the full canonical form or a partial one.
    /// The interior stays `bool`; this scope is where its refusals become
    /// three-valued.
    Verdict,
}

/// One recording scope: a point where a completed answer may enter the store, with
/// the validity and cost accounting for everything explored beneath it.
#[derive(Debug)]
struct Scope {
    kind: ScopeKind,
    /// The subtree's validity summary so far; starts `Grounded`, only degrades.
    support: Support,
    /// The step pool at entry (before the scope's own charge, where it has one),
    /// so the pop can measure what the whole derivation consumed.
    steps_at_entry: u64,
    /// The deepest membership-goal depth reached while this scope was live — the
    /// depth half of the recorded [`Cost`], folded upward on pop.
    member_extent: usize,
}

/// The state a single type-relation derivation carries.
///
/// Constructed at the algebra's public entry points and threaded by `&mut` from there
/// down; see the module docs for the invariants it exists to enforce.
pub(super) struct SolverSession<'s> {
    facts: &'s dyn TypeContext,
    limits: Limits,
    /// Co-inductive hypotheses on the current path, innermost last.
    assumptions: Vec<(NormalTy, NormalTy)>,
    /// Barriers, as indices into `assumptions`: hypotheses below the innermost floor
    /// belong to enclosing goals and are invisible here. A floor rather than a marker
    /// entry keeps `assumptions` a flat `Vec` of the pairs actually being scanned.
    floors: Vec<usize>,
    /// Recording scopes, innermost last: membership goals and barriered subtype
    /// roots, interleaved. Deliberately *not* floor-scoped — see the module docs.
    scopes: Vec<Scope>,
    /// How many [`ScopeKind::MembershipGoal`] scopes are live — the depth the
    /// [`Limits::recursion_limit`] bounds.
    member_depth: usize,
    /// The remaining step pool ([`Limits::step_budget`]), charged at the subtype
    /// expanding arms, at membership goal entry, and per pattern comparison inside
    /// clause matching. Refilled at each public membership entry point, so the pool
    /// is per *root*; the algebra's per-call sessions get the same granularity by
    /// construction.
    steps: u64,
    /// Whether a [`Self::charge`] was ever *refused*. From that point every verdict
    /// in this root is budget-relative (fail-closed), so recording stops. Distinct
    /// from "the pool reached zero": a derivation that spent its last step but never
    /// asked for another is still exact.
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
            reason = "first non-test caller arrives with the public session surface"
        )
    )]
    pub(super) fn with_store(facts: &'s dyn TypeContext, store: &'s mut Answers) -> Self {
        Self::over(facts, Store::Shared(store))
    }

    /// A session that recomputes every goal, for differential tests: answers are an
    /// optimization and must never be observable in a verdict.
    #[cfg(test)]
    pub(super) fn new_uncached(facts: &'s dyn TypeContext) -> Self {
        Self::over(facts, Store::IdentityOnly(Answers::default()))
    }

    fn over(facts: &'s dyn TypeContext, store: Store<'s>) -> Self {
        let limits = facts.limits();
        Self {
            facts,
            limits,
            assumptions: Vec::new(),
            floors: Vec::new(),
            scopes: Vec::new(),
            member_depth: 0,
            steps: limits.step_budget,
            exhausted: false,
            store,
        }
    }

    /// The nominal facts this derivation reasons against.
    pub(super) fn facts(&self) -> &'s dyn TypeContext {
        self.facts
    }

    /// Spend one step, or refuse: `false` means the pool is exhausted and the
    /// caller must fail closed. A refusal marks the whole root
    /// [exhausted](Self::exhausted), which stops answer recording — every verdict
    /// from here on is an artifact of where the budget ran out, not of the types.
    #[must_use]
    pub(super) fn charge(&mut self) -> bool {
        if self.steps == 0 {
            self.exhausted = true;
            return false;
        }
        self.steps -= 1;
        true
    }

    // ── the assumption path (co-inductive subtype hypotheses) ──────────────

    /// Whether `sub <: sup` is already assumed on the current path.
    ///
    /// A linear scan, not a hash probe, and deliberately so: the path is bounded by the
    /// expanding-arm recursion depth (a handful of entries in practice), while hashing
    /// would mean a full-tree walk of both operands at every probe — the cost the
    /// expanding-arm restriction exists to avoid.
    #[must_use]
    pub(super) fn assumes(&self, sub: &NormalTy, sup: &NormalTy) -> bool {
        let floor = self.floors.last().copied().unwrap_or(0);
        self.assumptions[floor..]
            .iter()
            .any(|(a, b)| a == sub && b == sup)
    }

    /// Run `derive` under the hypothesis `sub <: sup`. The push/pop pairing is
    /// structural: the hypothesis cannot outlive the derivation it justifies.
    pub(super) fn with_assumption<R>(
        &mut self,
        sub: NormalTy,
        sup: NormalTy,
        derive: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.assumptions.push((sub, sup));
        let result = derive(self);
        let popped = self.assumptions.pop();
        debug_assert!(popped.is_some(), "an assumption vanished mid-derivation");
        result
    }

    /// Run `derive` as an independent relate goal: hypotheses it records are
    /// invisible to enclosing derivations, and enclosing hypotheses are invisible
    /// to it. Structural, like [`Self::with_assumption`] — a derivation cannot
    /// leave its hypotheses on the path.
    fn barriered<R>(&mut self, derive: impl FnOnce(&mut Self) -> R) -> R {
        self.floors.push(self.assumptions.len());
        let result = derive(self);
        let floor = self.floors.pop();
        debug_assert_eq!(
            floor,
            Some(self.assumptions.len()),
            "a goal left hypotheses on the path"
        );
        result
    }

    // ── recording scopes ───────────────────────────────────────────────────

    /// Run `explore` inside a recording scope, returning its result alongside the
    /// closed scope (validity summary + cost accounting, with the depth extent
    /// already folded into the parent). Pairing is structural: a scope cannot
    /// stay open past the exploration it accounts for.
    fn in_scope<R>(
        &mut self,
        kind: ScopeKind,
        steps_at_entry: u64,
        explore: impl FnOnce(&mut Self) -> R,
    ) -> (R, Scope) {
        self.push_scope(kind, steps_at_entry);
        let result = explore(self);
        (result, self.pop_scope())
    }

    fn push_scope(&mut self, kind: ScopeKind, steps_at_entry: u64) {
        match kind {
            ScopeKind::MembershipGoal(_) => self.member_depth += 1,
            ScopeKind::SubtypeRoot | ScopeKind::Selection | ScopeKind::Verdict => {}
        }
        self.scopes.push(Scope {
            kind,
            support: Support::Grounded,
            steps_at_entry,
            member_extent: self.member_depth,
        });
    }

    /// Pop the innermost scope, folding its depth exploration into the parent (the
    /// parent explored through this child either way — its own admission data must
    /// cover the whole excursion, recorded or not). Support folds separately at the
    /// pop sites, which first normalize cycle heads.
    fn pop_scope(&mut self) -> Scope {
        let scope = self
            .scopes
            .pop()
            .unwrap_or_else(|| unreachable!("scope push/pop mismatch"));
        match scope.kind {
            ScopeKind::MembershipGoal(_) => self.member_depth -= 1,
            ScopeKind::SubtypeRoot | ScopeKind::Selection | ScopeKind::Verdict => {}
        }
        if let Some(parent) = self.scopes.last_mut() {
            parent.member_extent = parent.member_extent.max(scope.member_extent);
        }
        scope
    }

    fn fold_into_parent(&mut self, outward: Support) {
        if let Some(parent) = self.scopes.last_mut() {
            parent.support = parent.support.join(outward);
        }
    }

    /// Whether the remaining budget covers a recorded derivation: depth headroom
    /// re-verified, steps not yet charged ([`Self::admit`] charges on admission).
    #[must_use]
    fn admissible(&self, cost: Cost) -> bool {
        self.member_depth
            .checked_add(cost.member_depth)
            .is_some_and(|extent| extent <= self.limits.recursion_limit)
            && self.steps >= cost.steps
    }

    /// Admit a recorded answer: re-charge its steps, and fold the depth its
    /// recompute would explore into the enclosing scope — a hit must be
    /// observationally identical to that recompute in *both* budget dimensions,
    /// and the enclosing goal's own recorded cost must cover the excursion the
    /// hit stood in for (steps flow through the shared pool automatically;
    /// depth does not).
    fn admit(&mut self, cost: Cost) {
        debug_assert!(self.steps >= cost.steps, "admission without admissibility");
        self.steps -= cost.steps;
        if let Some(top) = self.scopes.last_mut() {
            top.member_extent = top.member_extent.max(self.member_depth + cost.member_depth);
        }
    }

    // ── the three-valued surface ───────────────────────────────────────────

    /// Whether a completed derivation was untouched by any limit, judged from
    /// its scope's support plus the sticky step-refusal flag. Only then is a
    /// fail-closed `false` a fact about the types, and only then is a
    /// canonicalization result the full canonical form.
    fn refusal_free(&self, support: Support) -> bool {
        match (support, self.exhausted) {
            (Support::Refused, _) | (_, true) => false,
            // A cycle support cannot outlive its head's scope (heads normalize it
            // on pop), so at an entry point it is defensive; grounded either way.
            (Support::Grounded | Support::Cycle(_), false) => true,
        }
    }

    /// Map an interior fail-closed verdict onto the three-valued surface: `true`
    /// is `Yes` unconditionally — a refusal can only *hide* proofs (absorption
    /// keeps members, and every positive rule consumes positives), never
    /// manufacture one — while `false` is a definite `No` only when
    /// [refusal-free](Self::refusal_free).
    fn truth_of(&self, proven: bool, support: Support) -> Truth {
        if proven {
            Truth::Yes
        } else if self.refusal_free(support) {
            Truth::No
        } else {
            Truth::Unknown(Indeterminacy::BudgetExhausted)
        }
    }

    // ── the subtype sort ───────────────────────────────────────────────────

    /// Decide `sub <: sup` as an independent goal, under its own barrier.
    ///
    /// This is the entry point for every probe that is not a continuation of the current
    /// derivation — union-member absorption, the automaton's per-state algebra. Those
    /// callers re-pose the same pairs across the automaton's fixpoint rounds and across
    /// structurally identical unions elsewhere in a term, which is what the store serves;
    /// see the module docs for why a verdict recorded here is unconditional.
    ///
    /// This is an *interior* probe, not a root: it spends its caller's pool rather
    /// than refilling one (the algebra's per-call sessions make it per-root by
    /// construction; a long-lived session's refill points are the public entries).
    pub(super) fn prove_subtype(&mut self, sub: &NormalTy, sup: &NormalTy) -> bool {
        if let Some(answers) = self.store.answers()
            && let Some(a) = answers.lookup(sub)
            && let Some(b) = answers.lookup(sup)
            && let Some(Answer { proven, cost }) = answers.subtype_answer(a, b)
            && self.admissible(cost)
        {
            self.admit(cost);
            return proven;
        }
        let member_depth_at_entry = self.member_depth;
        let (proven, scope) = self.barriered(|session| {
            session.in_scope(ScopeKind::SubtypeRoot, session.steps, |session| {
                sub.is_subtype_of(sup, session)
            })
        });
        // A subtype root is never a membership cycle head, so its support needs no
        // normalization: anything but `Grounded` is conditional here.
        if scope.support == Support::Grounded
            && !self.exhausted
            && let Some(answers) = self.store.answers_mut()
        {
            let (a, b) = (answers.intern(sub), answers.intern(sup));
            answers.record_subtype(
                a,
                b,
                Answer {
                    proven,
                    cost: Cost {
                        member_depth: scope.member_extent - member_depth_at_entry,
                        steps: scope.steps_at_entry - self.steps,
                    },
                },
            );
        }
        self.fold_into_parent(scope.support);
        proven
    }

    // ── the algebra entry points ───────────────────────────────────────────

    /// Decide `sub <: sup` on the three-valued surface — the body of
    /// [`TypeContext::is_subtype`].
    ///
    /// There is no relational fallback here: the walk *is* the relational
    /// procedure, so an `Unknown` from it is genuine — nothing cheaper is left
    /// to try.
    pub(super) fn decide_subtype(&mut self, sub: &Ty, sup: &Ty) -> Truth {
        self.begin_phase();
        let (proven, scope) = self.in_scope(ScopeKind::Verdict, self.steps, |s| {
            let sub = NormalTy::canonical(sub, s);
            let sup = NormalTy::canonical(sup, s);
            s.prove_subtype(&sub, &sup)
        });
        self.truth_of(proven, scope.support)
    }

    /// Decide `a ≡ b` on the three-valued surface — the body of
    /// [`TypeContext::equivalent`].
    ///
    /// Canonical identity first; when a limit cut the identity phase, fall back
    /// to the relation itself: mutual inclusion over the partial forms, under a
    /// fresh pool (the root's second and last — see [`Self::begin_phase`]).
    pub(super) fn decide_equivalent(&mut self, a: &Ty, b: &Ty) -> Truth {
        let (identity, ca, cb) = self.equivalent_by_identity(a, b);
        match identity {
            Truth::Unknown(_) => {}
            definite => return definite,
        }
        // The fallback is sound over partial forms because they denote the same
        // sets as the inputs (every rewrite a partial form *did* apply is
        // fact-justified; a refusal only skips rewrites) and are ε-closed
        // contractive as the walk requires (the structural stages never charge,
        // so no refusal can interrupt them). A definite `No` in either
        // direction refutes equivalence outright — Kleene `and`.
        self.begin_phase();
        let forward = self.subtype_verdict(&ca, &cb);
        let backward = self.subtype_verdict(&cb, &ca);
        forward.and(backward)
    }

    /// The identity phase of [`Self::decide_equivalent`]: canonicalize both
    /// operands and compare. Equal forms are `Yes` even when a limit left them
    /// partial — positive identity is sound, since both forms denote their
    /// inputs' sets. Unequal forms are `No` only when nothing was cut (partial
    /// forms under-absorb, so their inequality proves nothing); otherwise
    /// `Unknown`, handing the caller the partial forms for the fallback walk.
    pub(super) fn equivalent_by_identity(&mut self, a: &Ty, b: &Ty) -> (Truth, NormalTy, NormalTy) {
        self.begin_phase();
        let ((ca, cb), scope) = self.in_scope(ScopeKind::Verdict, self.steps, |s| {
            (NormalTy::canonical(a, s), NormalTy::canonical(b, s))
        });
        let identity = if ca == cb {
            Truth::Yes
        } else {
            self.truth_of(false, scope.support)
        };
        (identity, ca, cb)
    }

    /// Canonicalize and render `ty`, with its identity token when the walk earned
    /// one — the identity-carrying form of [`TypeContext::normalize`].
    ///
    /// The token is minted only for canonical-tier forms (no limit touched the
    /// walk): a partial form is still a faithful, equivalent rendering, but it is
    /// never an identity — [`Normalized::identity`] stays `None`, and relations
    /// over the type are decided by goals rather than token comparison.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the runtime delegation is this entry point's first \
                                    production caller"
        )
    )]
    pub(super) fn normalize(&mut self, ty: &Ty) -> Normalized {
        self.begin_phase();
        let ((form, mu_render), scope) = self.in_scope(ScopeKind::Verdict, self.steps, |s| {
            NormalTy::canonical_and_render(ty, s)
        });
        let identity = self
            .refusal_free(scope.support)
            .then(|| self.store.intern(&form));
        let ty = match mu_render {
            Some(rendered) => rendered,
            None => form.into_ty(),
        };
        Normalized { ty, identity }
    }

    /// The canonical form of `ty`, or `None` if a limit touched the walk.
    ///
    /// This is the gate for the definite-conclusion collapses
    /// ([`TypeContext::definitely_disjoint`] and friends): they conclude from
    /// structural comparison of forms, which is meaningful only on the canonical
    /// tier, so they uniformly decline to conclude from a partial form — even
    /// where a particular conclusion (positive identity) would happen to be
    /// sound. A `None` is not a failure to normalize; it is the signal that this
    /// caller's conclusion style must not consume the result.
    pub(super) fn canonical_definite(&mut self, ty: &Ty) -> Option<NormalTy> {
        let (form, scope) = self.in_scope(ScopeKind::Verdict, self.steps, |s| {
            NormalTy::canonical(ty, s)
        });
        self.refusal_free(scope.support).then_some(form)
    }

    /// One direction of [`Self::decide_equivalent`]'s relational fallback, with
    /// its own observation scope so each direction's verdict carries its own
    /// support (the shared pool makes a starved first direction taint the
    /// second through the sticky refusal flag, which is the conservative
    /// direction).
    fn subtype_verdict(&mut self, sub: &NormalTy, sup: &NormalTy) -> Truth {
        let (proven, scope) = self.in_scope(ScopeKind::Verdict, self.steps, |s| {
            s.prove_subtype(sub, sup)
        });
        self.truth_of(proven, scope.support)
    }

    // ── the membership sort ────────────────────────────────────────────────

    /// Whether `subject` implements the interface at exactly this instantiation.
    ///
    /// An empty `args` request matches any instantiation — the legitimate case for a
    /// non-generic interface, which has nothing to specify. `assoc` may be narrower
    /// than the implementation provides (bindings are outputs; requesting a subset is
    /// asking less, not something different).
    ///
    /// `No` is definite — a refutation over this world's clauses, including a cycle
    /// closed inductively. `Unknown` means a limit cut the search.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the runtime delegation is this entry point's first \
                                    production caller"
        )
    )]
    pub(super) fn implements(
        &mut self,
        subject: &RealizedTy,
        interface: &QualifiedTypeName,
        args: &[RealizedTy],
        assoc: &[(Name, RealizedTy)],
    ) -> Truth {
        self.begin_phase();
        let (proven, outward) = self.solve_member(subject, interface, args, assoc);
        self.truth_of(proven, outward)
    }

    /// Select the single applicable clause for `(subject, interface, args)` —
    /// membership's *resolution* twin: first match in the supplier's contractual
    /// order wins, and the returned bindings realize the clause's payload.
    ///
    /// Associated bindings never participate in selection: they are outputs of the
    /// impl, functionally determined once `(Self, Iface<Args>)` is fixed. A search
    /// any limit interfered with yields no selection at all (fail-closed `None`, by
    /// ruling): a refused candidate might have applied, so selecting a *later* one
    /// would let the budget pick which implementation runs.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the runtime delegation is this entry point's first \
                                    production caller"
        )
    )]
    pub(super) fn select(
        &mut self,
        subject: &RealizedTy,
        interface: &QualifiedTypeName,
        args: &[RealizedTy],
    ) -> Option<(ClauseId, Vec<RealizedTy>)> {
        self.begin_phase();
        let base = dispatch_base(subject);
        let facts = self.facts;
        let (selected, scope) = self.in_scope(ScopeKind::Selection, self.steps, |session| {
            let mut selected = None;
            facts.for_each_clause(interface, &mut |clause| {
                let Some(bindings) = session.clause_applies(&clause, &base) else {
                    return ControlFlow::Continue(());
                };
                if session.instantiation_matches(&clause, &bindings, args, &[]) {
                    selected = Some((clause.id, bindings));
                    return ControlFlow::Break(());
                }
                ControlFlow::Continue(())
            });
            selected
        });
        if scope.support == Support::Refused || self.exhausted {
            return None;
        }
        selected
    }

    /// Whether one specific clause applies to `subject` — its pattern matches the
    /// subject's dispatch base and its bounds discharge. On success returns the
    /// bound generic args in de Bruijn order.
    ///
    /// [`Self::implements`] asks whether *any* clause admits a subject; this is the
    /// per-clause view an enumeration consumer needs, where *which* clause admitted
    /// a subject is the answer itself. A refusal fails the clause — fail-closed
    /// `None`, with no ordering hazard since only one clause is in question.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the runtime delegation is this entry point's first \
                                    production caller"
        )
    )]
    pub(super) fn applies(
        &mut self,
        clause: &ImplClause<'_>,
        subject: &RealizedTy,
    ) -> Option<Vec<RealizedTy>> {
        self.begin_phase();
        self.clause_applies(clause, &dispatch_base(subject))
    }

    /// Begin a fresh pool phase: refill the step pool and clear the refusal flag.
    /// Every public entry point opens one, so the pool is per root; the one root
    /// with a second phase is [`Self::decide_equivalent`], whose relational
    /// fallback re-pools after a starved identity phase — a hard ≤2× ceiling per
    /// root, reachable only on inputs that exhausted the first pool. The store
    /// deliberately survives across phases — its entries are budget-qualified, so
    /// reuse is safe by admission, not by luck.
    fn begin_phase(&mut self) {
        debug_assert!(
            self.scopes.is_empty()
                && self.assumptions.is_empty()
                && self.floors.is_empty()
                && self.member_depth == 0,
            "a pool phase began inside a live derivation"
        );
        self.steps = self.limits.step_budget;
        self.exhausted = false;
    }

    /// Prove the membership goal `subject: interface<args, assoc>` by clause
    /// search. Returns the verdict plus the outward support the enclosing scope was
    /// folded with.
    fn solve_member(
        &mut self,
        subject: &RealizedTy,
        interface: &QualifiedTypeName,
        args: &[RealizedTy],
        assoc: &[(Name, RealizedTy)],
    ) -> (bool, Support) {
        // The canonical-keyed form of the question: any respelling of the subject,
        // arguments, or bindings canonicalizes to the same forms — computed against
        // *this* session, so the work shares this root's tables and pool — and
        // interns to the same ids.
        let base = dispatch_base(subject);
        let goal = self.membership_goal(&base, interface, args, assoc);

        // A recorded answer is admitted only when the remaining budget covers its
        // cost; an inadmissible entry falls through to the recompute, which fails
        // closed exactly where a first computation would.
        if let Some(answers) = self.store.answers()
            && let Some(answer) = answers.membership_answer(&goal)
            && self.admissible(answer.cost)
        {
            self.admit(answer.cost);
            return (answer.proven, Support::Grounded);
        }
        // A goal already in progress is an inductive cycle: membership is only ever
        // grounded in a concrete impl, so a self-supporting derivation proves
        // nothing. The provisional `false` reaches every scope inside the cycle as
        // the pops between here and the head fold it upward. Canonical keys make
        // the scan exact — any respelling collides.
        if let Some(head) = self.scopes.iter().position(|scope| {
            matches!(&scope.kind, ScopeKind::MembershipGoal(in_progress) if *in_progress == goal)
        }) {
            self.fold_into_parent(Support::Cycle(head));
            return (false, Support::Cycle(head));
        }
        // Limit refusals fail closed and degrade the enclosing subtree to
        // `Refused`, which both blocks recording up the path and reports outward.
        if self.member_depth >= self.limits.recursion_limit {
            self.fold_into_parent(Support::Refused);
            return (false, Support::Refused);
        }
        // The goal's own step charge (deliberately not spent on a depth refusal
        // above). The sticky `exhausted` flag is what blocks recording; the fold
        // reports the refusal outward like any other.
        let steps_at_entry = self.steps;
        if !self.charge() {
            self.fold_into_parent(Support::Refused);
            return (false, Support::Refused);
        }

        let member_depth_at_entry = self.member_depth;
        let facts = self.facts;
        let (proven, scope) =
            self.in_scope(ScopeKind::MembershipGoal(goal), steps_at_entry, |session| {
                let mut proven = false;
                facts.for_each_clause(interface, &mut |clause| {
                    if let Some(bindings) = session.clause_applies(&clause, &base)
                        && session.instantiation_matches(&clause, &bindings, args, assoc)
                    {
                        proven = true;
                        return ControlFlow::Break(());
                    }
                    ControlFlow::Continue(())
                });
                proven
            });

        // A cycle that closed under this goal is invisible from outside it: the head
        // discharges its own provisional support, so the answer is grounded — and,
        // per the inductive reading, definite.
        let own_index = self.scopes.len();
        let outward = match scope.support {
            Support::Cycle(head) if head >= own_index => {
                debug_assert!(head == own_index, "a cycle rooted above its own scope");
                Support::Grounded
            }
            other => other,
        };
        if outward == Support::Grounded
            && !self.exhausted
            && let Some(answers) = self.store.answers_mut()
        {
            let ScopeKind::MembershipGoal(goal) = scope.kind else {
                unreachable!("pushed as a membership goal above")
            };
            answers.record_membership(
                goal,
                Answer {
                    proven,
                    cost: Cost {
                        member_depth: scope.member_extent - member_depth_at_entry,
                        steps: scope.steps_at_entry - self.steps,
                    },
                },
            );
        }
        self.fold_into_parent(outward);
        (proven, outward)
    }

    /// The canonical, identity-keyed form of a membership question, over the
    /// subject's already-folded dispatch base.
    fn membership_goal(
        &mut self,
        base: &RealizedTy,
        interface: &QualifiedTypeName,
        args: &[RealizedTy],
        assoc: &[(Name, RealizedTy)],
    ) -> MembershipGoal {
        let form = NormalTy::canonical(base.as_ty(), self);
        let subject = self.store.intern(&form);
        let args = args
            .iter()
            .map(|arg| {
                let form = NormalTy::canonical(arg.as_ty(), self);
                self.store.intern(&form)
            })
            .collect();
        let mut assoc: Vec<(Name, CanonId)> = assoc
            .iter()
            .map(|(name, ty)| {
                let form = NormalTy::canonical(ty.as_ty(), self);
                (name.clone(), self.store.intern(&form))
            })
            .collect();
        assoc.sort_by(|(a, _), (b, _)| a.cmp(b));
        MembershipGoal {
            subject,
            interface: interface.clone(),
            args,
            assoc,
        }
    }

    /// Match the clause's `for`-pattern and discharge its bounds. `Some(bindings)`
    /// means the clause applies to `base`, with every parameter bound.
    fn clause_applies(
        &mut self,
        clause: &ImplClause<'_>,
        base: &RealizedTy,
    ) -> Option<Vec<RealizedTy>> {
        let mut slots: Vec<Option<RealizedTy>> = vec![None; clause.num_vars];
        if !clause
            .self_pattern
            .match_against(base, &mut slots, &mut SessionCompare(self))
        {
            return None;
        }
        // Every parameter must be bound by the pattern: one that is not can never be
        // realized, so the clause is inapplicable rather than partially applicable.
        let bindings: Vec<RealizedTy> = slots.into_iter().collect::<Option<_>>()?;

        for (binding, bounds) in bindings.iter().zip(clause.bounds) {
            for bound in bounds {
                let args: Vec<RealizedTy> = bound
                    .generics
                    .iter()
                    .map(|template| self.realize_template(template, &bindings))
                    .collect();
                let assoc: Vec<(Name, RealizedTy)> = bound
                    .associated_types
                    .iter()
                    .map(|(name, template)| {
                        (name.clone(), self.realize_template(template, &bindings))
                    })
                    .collect();
                if self.existential_satisfies(binding, &bound.name, &args, &assoc) {
                    continue;
                }
                let (holds, _) = self.solve_member(binding, &bound.name, &args, &assoc);
                if !holds {
                    return None;
                }
            }
        }
        Some(bindings)
    }

    /// Whether an applicable clause's interface instantiation satisfies the request
    /// — an empty `args` request matches any instantiation, and `assoc` may be
    /// narrower than the clause provides. Templates are realized only for the
    /// dimensions the request actually constrains.
    fn instantiation_matches(
        &mut self,
        clause: &ImplClause<'_>,
        bindings: &[RealizedTy],
        args: &[RealizedTy],
        assoc: &[(Name, RealizedTy)],
    ) -> bool {
        if !args.is_empty() {
            let instantiation: Vec<RealizedTy> = clause
                .iface_args
                .iter()
                .map(|template| self.realize_template(template, bindings))
                .collect();
            if !self.args_equivalent(&instantiation, args) {
                return false;
            }
        }
        if assoc.is_empty() {
            return true;
        }
        let provided: Vec<(Name, RealizedTy)> = clause
            .iface_assoc
            .iter()
            .map(|(name, template)| (name.clone(), self.realize_template(template, bindings)))
            .collect();
        self.assoc_satisfied(&provided, assoc)
    }

    /// Whether an interface-existential `binding` satisfies a bound naming its own
    /// interface directly (same head; equivalent args where requested; bindings at
    /// least as wide as requested).
    ///
    /// This is *dispatchability*, not membership: the checker only forms such a
    /// value for a type already known to implement the interface, and there is no
    /// concrete impl to find for an interface *type* — an interface never
    /// implements itself. (No dispatch-base fold here: a literal or enum-variant
    /// folds to a primitive or an enum, never to an existential, so the fold
    /// cannot change this check's outcome.)
    fn existential_satisfies(
        &mut self,
        binding: &RealizedTy,
        interface: &QualifiedTypeName,
        args: &[RealizedTy],
        assoc: &[(Name, RealizedTy)],
    ) -> bool {
        let RealizedTy::Interface(name, existential_args, existential_assoc, _) = binding else {
            return false;
        };
        name == interface
            && (args.is_empty() || self.args_equivalent(existential_args, args))
            && self.assoc_satisfied(existential_assoc, assoc)
    }

    // ── comparisons and realization ────────────────────────────────────────

    /// In-session equivalence: the facade's fast paths, then canonical identity —
    /// with the canonicalization work running against this session's tables and
    /// pool rather than a throwaway one.
    ///
    /// One comparison costs one step (charged after the free identity fast path, so
    /// monomorphic dispatch stays zero-cost): comparisons are the unit the union
    /// matcher's backtracking multiplies, and charging every comparison uniformly —
    /// matcher, argument, and binding positions alike — is what bounds that
    /// factorial worst case. A refused comparison answers "not the same type",
    /// fail-closed.
    fn types_equivalent(&mut self, a: &Ty, b: &Ty) -> bool {
        if a == b {
            return true;
        }
        if !self.charge() {
            return false;
        }
        if super::heads_definitely_differ(a, b) {
            return false;
        }
        NormalTy::canonical(a, self) == NormalTy::canonical(b, self)
    }

    /// Positional, equal-length, order-sensitive: interface arguments are inputs,
    /// and a different instantiation is a different interface.
    fn args_equivalent(&mut self, provided: &[RealizedTy], requested: &[RealizedTy]) -> bool {
        provided.len() == requested.len()
            && provided
                .iter()
                .zip(requested)
                .all(|(p, r)| self.types_equivalent(p.as_ty(), r.as_ty()))
    }

    /// Name-keyed and asymmetric: every *requested* binding must be provided, and
    /// the provider may be wider (bindings are outputs; asking about fewer is
    /// asking less). Trivially satisfied by an empty request.
    fn assoc_satisfied(
        &mut self,
        provided: &[(Name, RealizedTy)],
        requested: &[(Name, RealizedTy)],
    ) -> bool {
        requested.iter().all(|(name, requested_ty)| {
            provided.iter().any(|(provided_name, provided_ty)| {
                provided_name == name
                    && self.types_equivalent(provided_ty.as_ty(), requested_ty.as_ty())
            })
        })
    }

    /// Realize a clause template against the match's bindings — substitution plus
    /// projection reduction over this session's facts, the same facts normalization
    /// consumes.
    fn realize_template(&mut self, template: &TyTemplate, bindings: &[RealizedTy]) -> RealizedTy {
        match template.substitute(bindings, self.facts) {
            Ok(realized) => realized,
            // Candidate filtering hands a clause's templates bindings from a match
            // that may not survive its bounds, so a projection those bindings
            // cannot reduce — directly, through an unrealized reduction, or by
            // running out of fuel on a cyclic binding — is a legitimate outcome
            // here, unlike at the strict value-materialization boundary. The
            // sentinel compares equal only to itself, so whatever consumes it
            // conservatively fails: fail-closed, never a wrong selection.
            Err(
                SubstituteError::UnreducibleProjection { .. }
                | SubstituteError::ProjectionNotRealized { .. }
                | SubstituteError::ProjectionFuelExhausted { .. },
            ) => RealizedTy::unknown(),
            // The pattern match bound every parameter, so an out-of-range
            // reference means a malformed clause or mixed suppliers' data — never
            // a legitimate outcome. Loud, in every build profile: silently
            // mis-selecting an implementation is the worst failure available here.
            Err(refused @ SubstituteError::TypeArgRefOutOfRange { .. }) => {
                unreachable!("malformed clause template: {refused}")
            }
        }
    }
}

/// Pattern-position comparisons, resolved through the session (which charges each
/// one — see [`SolverSession::types_equivalent`]).
struct SessionCompare<'a, 's>(&'a mut SolverSession<'s>);

impl TemplateCompare for SessionCompare<'_, '_> {
    fn same_type(&mut self, pattern: &Ty, concrete: &Ty) -> bool {
        self.0.types_equivalent(pattern, concrete)
    }
}

/// A literal or enum-variant subject dispatches through its base type — `1` uses
/// `int`'s impls, `Color.Red` uses `Color`'s. Top level only: nested arguments keep
/// their literal form, so invariance holds (`Box<1>` is not `Box<int>`). Owned only
/// at the folded leaves; everything else passes through borrowed.
fn dispatch_base(ty: &RealizedTy) -> Cow<'_, RealizedTy> {
    match ty {
        RealizedTy::Literal(lit, _, attr) => Cow::Owned(match lit {
            Literal::Int(_) => RealizedTy::Int { attr: attr.clone() },
            Literal::Bigint(_) => RealizedTy::Bigint { attr: attr.clone() },
            Literal::Float(_) => RealizedTy::Float { attr: attr.clone() },
            Literal::String(_) => RealizedTy::String { attr: attr.clone() },
            Literal::Bool(_) => RealizedTy::Bool { attr: attr.clone() },
        }),
        RealizedTy::EnumVariant(name, _, attr) => {
            Cow::Owned(RealizedTy::Enum(name.clone(), attr.clone()))
        }
        _ => Cow::Borrowed(ty),
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
        s.with_assumption(NormalTy::Int, NormalTy::String, |s| {
            assert!(s.assumes(&NormalTy::Int, &NormalTy::String));

            // An independent goal must not inherit the hypothesis: assuming
            // `a <: b` is sound only for the derivation of `a <: b` itself, and a
            // sibling that saw it would prove the pair unconditionally.
            s.barriered(|s| assert!(!s.assumes(&NormalTy::Int, &NormalTy::String)));

            // …and it is restored for the enclosing derivation afterwards.
            assert!(s.assumes(&NormalTy::Int, &NormalTy::String));
        });
        assert!(!s.assumes(&NormalTy::Int, &NormalTy::String));
    }

    #[test]
    fn hypotheses_match_on_both_operands_and_direction() {
        let mut s = session();
        s.with_assumption(NormalTy::Int, NormalTy::String, |s| {
            assert!(!s.assumes(&NormalTy::String, &NormalTy::Int));
            assert!(!s.assumes(&NormalTy::Int, &NormalTy::Bool));
        });
    }

    #[test]
    fn support_joins_to_the_worse_side_and_the_outermost_cycle() {
        use Support::{Cycle, Grounded, Refused};
        // The full lattice: `Refused` absorbs, `Grounded` is the identity, and two
        // cycle supports lean on the outermost (lower-indexed) root.
        assert_eq!(Grounded.join(Grounded), Grounded);
        assert_eq!(Grounded.join(Cycle(3)), Cycle(3));
        assert_eq!(Cycle(3).join(Grounded), Cycle(3));
        assert_eq!(Cycle(3).join(Cycle(1)), Cycle(1));
        assert_eq!(Cycle(1).join(Cycle(3)), Cycle(1));
        assert_eq!(Grounded.join(Refused), Refused);
        assert_eq!(Refused.join(Grounded), Refused);
        assert_eq!(Cycle(3).join(Refused), Refused);
        assert_eq!(Refused.join(Cycle(3)), Refused);
        assert_eq!(Refused.join(Refused), Refused);
    }
}
