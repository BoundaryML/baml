//! The obligation system (I4) - rust-analyzer's fulfillment semantics
//! over BAML's facts:
//!
//! - An obligation REGISTERS during the walk when a decision needs
//!   information inference has not produced yet (an operator on a
//!   still-unsolved generic, a call-site bound on an argument variable).
//!   Registration never fails and never guesses.
//! - Discharge runs at `finish`, INTERLEAVED with bound resolution to
//!   fixpoint: each round resolves what the ground bounds determine,
//!   then attempts every pending obligation. An attempt with live
//!   variables STALLS (rust-analyzer's Ambiguous - retried next round,
//!   never an early failure); a ground attempt succeeds (possibly
//!   unifying its output variable, which is what un-stalls other work)
//!   or definitively fails (a reported diagnostic, the output erased).
//! - After fixpoint, still-stalled obligations fail CLOSED: their
//!   outputs erase through the ordinary finalize rules (ruling 2), and a
//!   goal that stalled because SEVERAL candidates proved it reports that
//!   ambiguity ([`InferenceContext::settle_stalled_obligations`]). Each
//!   stall records which of the two it was ([`Stall`]), so settling reads
//!   the reason instead of re-deriving one from the leftover type.
//!
//! The interleave exists because of a real deadlock: an operator
//! obligation's output can be a LOWER BOUND of the very variable its
//! operand waits on (`?A`'s bounds contain `?O`; `?O`'s operand is
//! `?A`). Bound resolution therefore decides from the GROUND SUBSET of
//! a class's bounds, deferring variable-carrying bounds to post-hoc
//! verification - the doc-inference "one solve budget" shape.
//!
//! Two kinds. Projections discharge through the canonical algebra since
//! I5 (reduction + declared-bound proving); PROBE mode (a speculative
//! attempt with no committed bindings, for candidate selection) is the
//! table's existing snapshot/rollback when a consumer arrives.

use baml_compiler2_ast::ExprId;
use baml_type::interned::{InferInterface, InferTy, Ty, TyVocabulary};

use super::InferenceContext;

/// Why a type must implement an interface, which decides what proves it.
///
/// `TYPE_SYSTEM.md` derives subtyping FROM implements (a concrete `C <: I`
/// exactly when `C` implements `I`), and the arrow never runs backwards. A
/// goal that licenses dispatch on a TYPE is answered by the implements
/// relation, which is nominal: an impl row exists or it fails, so `never`
/// implements nothing it has no impl for. A goal that asks whether a VALUE
/// may be used through the interface is answered by subtyping, where `never`
/// holds vacuously and a union holds when every member does.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum GoalPurpose {
    /// A value used through the interface: flowing into an existential,
    /// iterated, or the receiver a method call dispatches on. Dispatch is
    /// keyed by the value at run time, so this is value inhabitation.
    Coercion,
    /// A declared bound on a generic param — a function's or class's own, or
    /// an impl header's, replayed when the impl is selected. Code under the
    /// bound may dispatch on the TYPE (`(T as I).make()`), so it is nominal,
    /// and the argument must be concrete (E0001): an abstract type has no
    /// single runtime type to dispatch on, and no impl makes an existential
    /// an implementor (`TYPE_SYSTEM.md`, "Generics on Functions"). The impl
    /// header is no exception — a blanket impl applies "to all concrete
    /// types satisfying the bounds" — and the ground resolver already rejects
    /// the candidate there, so selection must too or the two disagree.
    Bound,
}

/// One registered obligation.
pub(super) enum Obligation {
    /// `ty` must implement `interface`. There is no output to bind, but
    /// selection unifies the goal against the one impl (or bound) that
    /// proves it, which is how a goal's open args and pins solve. A failure
    /// reports the nominal does-not-implement diagnostic at `at`.
    Implements {
        ty: Ty,
        interface: InferInterface,
        at: ExprId,
        purpose: GoalPurpose,
        /// The goals whose confirmed impls required this one, the program's
        /// own goal first; empty for a goal the program asked directly. A
        /// report names the goal that failed and notes each of these, so a
        /// bound replayed from an impl header is tied back to what the
        /// program wrote (rustc's `ImplDerivedObligation`). Its length is how
        /// many impl headers were confirmed to reach this goal: a bounded
        /// blanket impl can be satisfied by another, so confirming one
        /// registers its own bounds, and this length is the budget that stops
        /// a self-satisfying impl from doing that forever (see
        /// [`InferenceContext::attempt`]).
        required_for: Vec<RequiredFor>,
    },
    /// `lhs <op-interface> rhs` deferred: discharge re-runs the SAME
    /// ground operator dispatch (union distribution, literal widening,
    /// carried bounds included) and unifies `out` with its result.
    Operator {
        interface: &'static str,
        lhs: Ty,
        rhs: Option<Ty>,
        out: Ty,
        /// The registering expression - the no-impl diagnostic's anchor.
        at: ExprId,
    },
}

impl Obligation {
    /// A goal the program asks directly: `ty` must implement `interface` for
    /// `purpose`, reported at `at`.
    pub(super) fn implements(
        ty: Ty,
        interface: InferInterface,
        at: ExprId,
        purpose: GoalPurpose,
    ) -> Self {
        Obligation::Implements {
            ty,
            interface,
            at,
            purpose,
            required_for: Vec::new(),
        }
    }
}

/// A goal that confirming an impl for it made another goal of: the impl's
/// bounds had to hold for `subject` to implement `interface`.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct RequiredFor {
    /// As the goal spelled it.
    pub(super) subject: Ty,
    pub(super) interface: InferInterface,
}

/// An impl's interface reference (its own, or one of its bounds) at a
/// confirmation's `instantiation` of the impl's params.
fn instantiate_interface(
    interface: &baml_type::interned::ClosedInterface,
    instantiation: &rustc_hash::FxHashMap<baml_type::ParamTy, Ty>,
) -> InferInterface {
    InferInterface::new(
        interface.name.clone(),
        interface
            .generics
            .iter()
            .map(|ty| ty.substitute_bindings(instantiation))
            .collect::<Vec<_>>()
            .into_boxed_slice(),
        interface
            .associated_types
            .iter()
            .map(|(name, ty)| (name.clone(), ty.substitute_bindings(instantiation)))
            .collect(),
    )
}

/// Whether a bound must refuse `head` before consulting any implementation:
/// the type has no single runtime type for code under the bound to dispatch
/// on (`TYPE_SYSTEM.md`, "Generics on Functions"), because it spans several
/// (a union, an existential, `unknown`) or none (`never`, `void`).
fn is_abstract_head(head: &InferTy) -> bool {
    match head {
        InferTy::Union(..)
        | InferTy::Interface(..)
        | InferTy::Unknown
        | InferTy::Never
        | InferTy::Void => true,
        InferTy::Int
        | InferTy::Bigint
        | InferTy::Float
        | InferTy::String
        | InferTy::Bool
        | InferTy::Null
        | InferTy::Uint8Array
        | InferTy::Media(..)
        | InferTy::Class(..)
        | InferTy::Enum(..)
        | InferTy::List(..)
        | InferTy::Map { .. }
        | InferTy::Function { .. }
        | InferTy::Future(..)
        | InferTy::Type
        | InferTy::Resource
        | InferTy::PromptAst
        | InferTy::RustType => false,
        // Literal types are not refused here but left to the implements
        // verdict (a literal was widened to its base before this is asked).
        InferTy::Literal(..) | InferTy::EnumVariant(..) => false,
        // Symbolic: concreteness is the variable's own bound, which
        // caller-bound selection proves.
        InferTy::TypeVar(..) | InferTy::AssociatedTypeProjection { .. } => false,
        // Not decided here: an open variable stalls, an alias head is
        // expanded before this is asked, and an error was reported already.
        InferTy::InferVar { .. } | InferTy::TypeAlias(..) | InferTy::Error => false,
    }
}

enum Attempt {
    /// Undecided this round: retry next.
    Stalled(Stall),
    /// Discharged (successfully or with a recorded failure).
    Done,
}

/// Why an attempt could not decide its goal - and so, at quiescence, which
/// diagnostic owns it.
#[derive(Clone, Copy)]
enum Stall {
    /// Part of the goal is still unsolved: the subject is a variable, a
    /// union has not closed, a projection's base has not resolved, or a
    /// conditional default needs resolved components. This is not candidate
    /// ambiguity; the diagnostics for an undetermined type own it.
    Unresolved,
    /// The goal is decidable and MORE THAN ONE candidate decides it. More
    /// solving may still prune the set, so it retries; if nothing does, it
    /// is rustc's E0283 ("type annotations needed").
    Ambiguous,
}

/// A registered obligation and what its last attempt learned about it.
pub(super) struct Pending {
    obligation: Obligation,
    /// Why the last attempt left it pending, or `None` before its first.
    /// Settling treats `None` as undecided rather than ambiguous: a goal
    /// registered after the fixpoint's last round was never asked, so
    /// nothing observed more than one candidate for it.
    stall: Option<Stall>,
}

/// The outcome of candidate selection for a goal whose subject is known.
enum Selection {
    /// Exactly one candidate applied, and confirming it committed its
    /// unifications.
    Confirmed,
    /// Several candidates apply; more solving may prune them.
    Ambiguous,
    /// A conditional default needs the subject's remaining variables resolved.
    Unresolved,
    /// No candidate can apply, whatever the goal's open variables become.
    NoCandidate,
}

impl<'db> InferenceContext<'db> {
    pub(super) fn register_obligation(&mut self, obligation: Obligation) {
        self.obligations.push(Pending {
            obligation,
            stall: None,
        });
    }

    /// One discharge round over every pending obligation. Returns whether
    /// anything discharged (progress for the fixpoint driver).
    pub(super) fn discharge_obligations_once(&mut self) -> bool {
        let pending = std::mem::take(&mut self.obligations);
        let mut progressed = false;
        for Pending { obligation, .. } in pending {
            match self.attempt(&obligation) {
                Attempt::Done => progressed = true,
                Attempt::Stalled(stall) => self.obligations.push(Pending {
                    obligation,
                    stall: Some(stall),
                }),
            }
        }
        progressed
    }

    fn attempt(&mut self, obligation: &Obligation) -> Attempt {
        match obligation {
            Obligation::Implements {
                ty,
                interface,
                at,
                purpose,
                required_for,
            } => {
                let at = *at;
                let purpose = *purpose;
                let ty = self.table.resolve_completely(ty);
                let interface = self.resolve_interface_ref(interface);
                if ty.has_error() {
                    return Attempt::Done;
                }
                // An alias is a spelling device and a literal implements
                // what its base primitive does, so the goal is JUDGED for
                // the type the alias denotes, widened; a report names the
                // type as written.
                let subject = ty;
                let mut ty = widen_literal(&self.expand_alias_ty(&subject));
                // One spelling, one verdict (B-1576): resolution can ground a
                // syntactic union after this obligation registered, and the
                // canonical form may be a single member. An OPEN union may
                // still collapse, so only a coercion (which needs every
                // member either way) acts on it before it closes.
                if let InferTy::Union(..) = ty.kind() {
                    match baml_type::interned::ClosedTy::try_from(&ty) {
                        // The canonical form of a recursive alias's
                        // unfolding is the alias again.
                        Ok(closed) => {
                            ty = self.canonicalize_unions(&closed).into_ty();
                            ty = self.expand_alias_ty(&ty);
                        }
                        Err(_) if purpose != GoalPurpose::Coercion => {
                            return Attempt::Stalled(Stall::Unresolved);
                        }
                        Err(_) => {}
                    }
                }
                if purpose == GoalPurpose::Coercion {
                    match ty.kind() {
                        // `never` inhabits every type.
                        InferTy::Never => return Attempt::Done,
                        // A union value is used through the interface by
                        // whichever member it holds, so every member must
                        // implement it (the spec's Variance rule 2.1).
                        InferTy::Union(members) => {
                            for member in members {
                                self.register_obligation(Obligation::Implements {
                                    ty: member.clone(),
                                    interface: interface.clone(),
                                    at,
                                    purpose,
                                    required_for: required_for.clone(),
                                });
                            }
                            return Attempt::Done;
                        }
                        _ => {}
                    }
                }
                // An abstract argument has no single runtime type to
                // dispatch on (E0001), whatever its args become. Checked
                // before the shared verdict, whose existential arm answers by
                // the existential's own reference — right for a coercion
                // (a `Show` value is usable as a `Show`), never for a bound.
                if purpose == GoalPurpose::Bound
                    && is_abstract_head(ty.kind())
                    && !crate::impls::is_structural_interface(self.db, &interface)
                    && !(crate::impls::structural_interface(self.db, &interface.name)
                        == Some(baml_type::StructuralInterface::Hash)
                        && crate::impls::hash_eligible(self.db, &self.facts, &ty, &interface))
                {
                    self.pending_diags
                        .push(super::PendingDiag::BoundedArgNotConcrete {
                            expr: at,
                            arg: ty,
                            bound: interface,
                            required_for: required_for.clone(),
                        });
                    return Attempt::Done;
                }
                if let Ok(closed) = baml_type::interned::ClosedTy::try_from(&ty)
                    && !interface_has_infer(&interface)
                {
                    // Every purpose shares the verdict: past `never` and
                    // unions (handled above for a coercion), a value
                    // inhabits an existential exactly when its type
                    // implements it.
                    let judged = self.canonicalize_unions(&closed).into_ty();
                    if !self.implements_holds(&judged, &interface) {
                        self.report_not_implemented(at, subject, &interface, required_for);
                    }
                    return Attempt::Done;
                }
                // The ground resolver bounds the same recursion with
                // `BLANKET_IMPL_BOUND_DEPTH` and fails the candidate when it
                // runs out (`bounds_hold`), so an exhausted chain reports
                // "does not implement" on both paths. Selection needs the
                // budget as well as the cycle check that resolver has: each
                // round instantiates the impl header with FRESH variables, so
                // a self-satisfying impl (`implements<X, T extends Cyc<X>>
                // Cyc<X> for T`) never re-registers an EQUAL goal - it grows
                // one, and equality alone would never stop it.
                if required_for.len() > crate::impls::BLANKET_IMPL_BOUND_DEPTH as usize {
                    self.report_not_implemented(at, subject, &interface, required_for);
                    return Attempt::Done;
                }
                let selection = match ty.kind() {
                    // Nothing filters the candidate set yet: a head still
                    // unknown, or a projection whose base has not resolved.
                    // Guessing is the one thing fulfillment never does.
                    InferTy::InferVar { .. } | InferTy::AssociatedTypeProjection { .. } => {
                        return Attempt::Stalled(Stall::Unresolved);
                    }
                    // Object candidates (rustc's
                    // `assemble_candidates_from_object_ty`): an existential
                    // subject has no impl to select - it proves the goal
                    // from its OWN reference plus its `requires` closure.
                    InferTy::Interface(..) => self.select_object(&ty, &interface),
                    // Caller-bound candidates (rustc's
                    // `assemble_candidates_from_caller_bounds`): a rigid
                    // var proves the goal from the bounds it carries.
                    InferTy::TypeVar(..) => self.select_param_env(&ty, &interface),
                    // Every other head is known, and filters the impls. A
                    // (closed) union finds none: no impl subject is a union.
                    _ => self.select_impl(&ty, &subject, &interface, at, required_for),
                };
                match selection {
                    Selection::Confirmed => Attempt::Done,
                    Selection::Ambiguous => Attempt::Stalled(Stall::Ambiguous),
                    Selection::Unresolved => Attempt::Stalled(Stall::Unresolved),
                    Selection::NoCandidate => {
                        self.report_not_implemented(at, subject, &interface, required_for);
                        Attempt::Done
                    }
                }
            }
            Obligation::Operator {
                interface,
                lhs,
                rhs,
                out,
                at,
            } => {
                let at = *at;
                let lhs = self.table.resolve_completely(lhs);
                let rhs = rhs.as_ref().map(|rhs| self.table.resolve_completely(rhs));
                if lhs.has_infer() || rhs.as_ref().is_some_and(Ty::has_infer) {
                    return Attempt::Stalled(Stall::Unresolved);
                }
                let result = self.dispatch_operator(interface, &lhs, rhs.as_ref());
                if result.has_error() {
                    self.report_operator_failure(at, interface, &lhs, rhs.as_ref());
                }
                let _ = self.table.unify(out, &result);
                Attempt::Done
            }
        }
    }

    /// rustc's impl SELECTION for a variable-bearing goal (the impl
    /// inversion B-898 needs): every candidate is tried under a table
    /// snapshot and rolled back; EXACTLY ONE applying confirms it - the
    /// header unification commits, constraining the goal's inference
    /// variables. Zero applicable falls back to structural defaults, whose
    /// eligibility may still be unresolved; otherwise it is a definite failure.
    /// Several is genuine ambiguity - Stalled, retried once more
    /// information may prune, reported at quiescence if it never does
    /// (rustc's "type annotations needed"). Committing on uniqueness is
    /// sound because coherence (I7) guarantees at most one impl per
    /// realized instance.
    fn select_impl(
        &mut self,
        goal: &Ty,
        subject: &Ty,
        interface: &InferInterface,
        at: ExprId,
        required_for: &[RequiredFor],
    ) -> Selection {
        let candidates = crate::impls::impl_candidates(self.db, goal, &interface.name);
        let mut applicable: Option<&crate::impls::ImplFacts<'_>> = None;
        for facts in candidates {
            let probe = self.probe();
            let applies = self.confirm_impl(goal, interface, facts).is_some();
            self.rollback_probe(probe);
            if applies {
                if applicable.is_some() {
                    return Selection::Ambiguous;
                }
                applicable = Some(facts);
            }
        }
        let Some(facts) = applicable else {
            if crate::impls::is_structural_interface(self.db, interface) {
                return Selection::Confirmed;
            }
            if crate::impls::structural_interface(self.db, &interface.name)
                == Some(baml_type::StructuralInterface::Hash)
            {
                if goal.has_infer() || interface_has_infer(interface) {
                    return Selection::Unresolved;
                }
                if crate::impls::hash_eligible(self.db, &self.facts, subject, interface) {
                    return Selection::Confirmed;
                }
            }
            return Selection::NoCandidate;
        };
        let instantiation = self
            .confirm_impl(goal, interface, facts)
            .expect("the unique applicable candidate re-confirms");
        let mut nested = required_for.to_vec();
        nested.push(RequiredFor {
            subject: subject.clone(),
            interface: interface.clone(),
        });
        self.register_impl_bound_obligations(facts, &instantiation, at, &nested);
        Selection::Confirmed
    }

    /// Object selection: the existential subject's own reference and its
    /// `requires` closure are the candidate heads - a matching head
    /// confirms by unifying args and the goal's pins (`Iterator<Error =
    /// never>` proving `Iterable<Error = ?E2>` commits `?E2 := never`).
    /// As in impl selection: exactly one applicable head commits,
    /// several stall, none reports.
    fn select_object(&mut self, subject: &Ty, goal: &InferInterface) -> Selection {
        let InferTy::Interface(name, args, pins) = subject.kind() else {
            unreachable!("object selection is for an existential subject")
        };
        let subject_target = InferInterface::new(name.clone(), args.clone(), pins.clone());
        let heads = crate::impls::requires_heads(self.db, &subject_target, subject, 8);
        let goal_target = goal.clone();
        let mut applicable = None;
        for head in &heads {
            let probe = self.probe();
            let applies = self.confirm_object(head, &goal_target);
            self.rollback_probe(probe);
            if applies {
                if applicable.is_some() {
                    return Selection::Ambiguous;
                }
                applicable = Some(head);
            }
        }
        let Some(head) = applicable else {
            return Selection::NoCandidate;
        };
        let confirmed = self.confirm_object(head, &goal_target);
        debug_assert!(confirmed, "the unique applicable head re-confirms");
        Selection::Confirmed
    }

    /// Caller-bound selection: a rigid var implements what its carried
    /// bounds and their `requires` closure say. A matching head confirms
    /// by unifying args, and each goal pin with the head's pin - or, where
    /// the bound leaves the member unpinned, with the rigid projection
    /// `(subject as Head).Member` (rustc normalizes the same member to its
    /// placeholder projection). As in impl selection: exactly one
    /// applicable head commits, several stall, none reports.
    fn select_param_env(&mut self, subject: &Ty, goal: &InferInterface) -> Selection {
        let InferTy::TypeVar(param) = subject.kind() else {
            unreachable!("caller-bound selection is for a rigid var subject")
        };
        let carried = baml_type::normalize::TypeContext::type_var_bound(&self.facts, param);
        // One head, one candidate: two bounds whose `requires` closures meet
        // (a diamond, or `T extends C & A` where `A requires C`) reach the
        // same head twice, and a head is not ambiguous with itself.
        let mut heads: Vec<InferInterface> = Vec::new();
        for bound in &carried {
            for head in crate::impls::requires_heads(
                self.db,
                &InferInterface::from_constraint(bound),
                subject,
                8,
            ) {
                if !heads.contains(&head) {
                    heads.push(head);
                }
            }
        }
        let mut applicable = None;
        for head in &heads {
            let probe = self.probe();
            let applies = self.confirm_param_head(subject, head, goal);
            self.rollback_probe(probe);
            if applies {
                if applicable.is_some() {
                    return Selection::Ambiguous;
                }
                applicable = Some(head);
            }
        }
        let Some(head) = applicable else {
            return Selection::NoCandidate;
        };
        let confirmed = self.confirm_param_head(subject, head, goal);
        debug_assert!(confirmed, "the unique applicable bound re-confirms");
        Selection::Confirmed
    }

    /// One carried head against the goal: [`Self::confirm_object`]'s args
    /// and pins, except that a member the bound does not pin is the rigid
    /// var's own projection rather than a failure.
    fn confirm_param_head(
        &mut self,
        subject: &Ty,
        head: &InferInterface,
        goal: &InferInterface,
    ) -> bool {
        if head.name != goal.name || head.generics.len() != goal.generics.len() {
            return false;
        }
        for (have, want) in head.generics.iter().zip(&goal.generics) {
            if self.table.unify(have, want).is_err() {
                return false;
            }
        }
        for (name, want_pin) in &goal.associated_types {
            let have_pin = head
                .associated_types
                .iter()
                .find(|(have_name, _)| have_name == name)
                .map_or_else(
                    || {
                        Ty::intern(InferTy::AssociatedTypeProjection {
                            base: subject.clone(),
                            interface: head.clone(),
                            member: name.clone(),
                        })
                    },
                    |(_, have_pin)| have_pin.clone(),
                );
            if self.table.unify(&have_pin, want_pin).is_err() {
                return false;
            }
        }
        true
    }

    /// Records that `subject` does not implement `interface`: the nominal
    /// verdict, reported as such rather than as a subtyping mismatch (which
    /// the finalize filter would re-judge, and `never <: I` holds
    /// vacuously). One report per anchor, subject, and interface - a goal
    /// can register more than once for the same expression.
    fn report_not_implemented(
        &mut self,
        at: ExprId,
        subject: Ty,
        interface: &InferInterface,
        required_for: &[RequiredFor],
    ) {
        let interface = interface.existential();
        let already = self.pending_diags.iter().any(|pending| {
            matches!(
                pending,
                super::PendingDiag::DoesNotImplement { expr, value, interface: reported, .. }
                    if *expr == at && *value == subject && *reported == interface
            )
        });
        if !already {
            self.pending_diags
                .push(super::PendingDiag::DoesNotImplement {
                    expr: at,
                    value: subject,
                    interface,
                    required_for: required_for.to_vec(),
                });
        }
    }

    /// Settles the goals still pending at quiescence, when no more solving
    /// can happen. The recorded stall REASON decides, not a guess re-derived
    /// from the leftover type: "more than one implementation could make this
    /// work" is rustc's E0283 and is only true of a goal that actually found
    /// more than one. Everything else stalled because part of it never
    /// resolved - no candidate was ever consulted - and the diagnostics for
    /// an undetermined type own it, as they own the deferred subs' residue.
    pub(super) fn settle_stalled_obligations(&mut self) {
        for Pending { obligation, stall } in std::mem::take(&mut self.obligations) {
            // An operator stalls only on an unsolved operand, so one pattern
            // covers both kinds: ambiguity is impl selection's outcome alone.
            let (
                Obligation::Implements {
                    ty,
                    interface,
                    at,
                    required_for,
                    ..
                },
                Some(Stall::Ambiguous),
            ) = (obligation, stall)
            else {
                continue;
            };
            let ty = self.table.resolve_completely(&ty);
            let interface = self.resolve_interface_ref(&interface).existential();
            self.pending_diags
                .push(super::PendingDiag::AmbiguousImplementation {
                    expr: at,
                    value: ty,
                    interface,
                    required_for,
                });
        }
    }

    /// One object head against the goal: names equal, args unify
    /// pairwise, and every goal pin unifies with the head's realization
    /// of that member (the head may pin MORE; a member the head does not
    /// realize cannot prove a pinned requirement).
    fn confirm_object(&mut self, head: &InferInterface, goal: &InferInterface) -> bool {
        if head.name != goal.name || head.generics.len() != goal.generics.len() {
            return false;
        }
        for (have, want) in head.generics.iter().zip(&goal.generics) {
            if self.table.unify(have, want).is_err() {
                return false;
            }
        }
        for (name, want_pin) in &goal.associated_types {
            let Some((_, have_pin)) = head
                .associated_types
                .iter()
                .find(|(have_name, _)| have_name == name)
            else {
                return false;
            };
            if self.table.unify(have_pin, want_pin).is_err() {
                return false;
            }
        }
        true
    }

    /// The impl's declared bounds at `instantiation` become NESTED
    /// obligations (rustc's confirmation side conditions) - the
    /// fulfillment loop discharges them next round. Shared by selection
    /// and the method probe; `required_for` ends with the goal the impl was
    /// confirmed for.
    fn register_impl_bound_obligations(
        &mut self,
        facts: &crate::impls::ImplFacts<'_>,
        instantiation: &rustc_hash::FxHashMap<baml_type::ParamTy, Ty>,
        at: ExprId,
        required_for: &[RequiredFor],
    ) {
        debug_assert!(
            !required_for.is_empty(),
            "a header bound is required for the goal its impl was confirmed for"
        );
        for (param, bounds) in &facts.generic_params {
            let Some(arg) = instantiation.get(param) else {
                continue;
            };
            for bound in bounds {
                self.register_obligation(Obligation::Implements {
                    ty: arg.clone(),
                    interface: instantiate_interface(bound, instantiation),
                    at,
                    purpose: GoalPurpose::Bound,
                    required_for: required_for.to_vec(),
                });
            }
        }
    }

    /// CONFIRMATION (rustc's shape): instantiate the impl header with
    /// FRESH inference variables for its params and unify the goal
    /// against it - the for-target, each interface arg, then every
    /// requested pin against the impl's binding-else-default. Returns
    /// the param instantiation on success; any failed unification (or a
    /// pin the impl can neither bind nor default) rejects the candidate,
    /// and the caller's snapshot discards the partial bindings.
    fn confirm_impl(
        &mut self,
        goal: &Ty,
        interface: &InferInterface,
        facts: &crate::impls::ImplFacts<'_>,
    ) -> Option<rustc_hash::FxHashMap<baml_type::ParamTy, Ty>> {
        if facts.interface.generics.len() != interface.generics.len() {
            return None;
        }
        let instantiation = self.confirm_impl_subject(goal, facts)?;
        for (pattern, requested) in facts
            .interface
            .generics
            .iter()
            .zip(interface.generics.iter())
        {
            let pattern = pattern.substitute_bindings(&instantiation);
            let requested = crate::impls::unfold_aliases_against(&pattern, requested, &self.facts);
            self.table.unify(&requested, &pattern).ok()?;
        }
        for (name, requested) in &interface.associated_types {
            let supplied = facts
                .associated_types
                .iter()
                .find(|(declared, _)| declared == name)
                .map(|(_, ty)| ty.as_ty().substitute_bindings(&instantiation))
                .or_else(|| {
                    let implemented = InferInterface::new(
                        facts.interface.name.clone(),
                        facts
                            .interface
                            .generics
                            .iter()
                            .map(|ty| ty.substitute_bindings(&instantiation))
                            .collect::<Vec<_>>()
                            .into_boxed_slice(),
                        facts
                            .interface
                            .associated_types
                            .iter()
                            .map(|(pin, ty)| (pin.clone(), ty.substitute_bindings(&instantiation)))
                            .collect(),
                    );
                    crate::impls::realized_assoc_default(self.db, &implemented, goal, name)
                })?;
            // Normalize-then-unify (the `sub` entry's discipline): a
            // GROUND requested pin may be a reducible projection -
            // `chain`'s `Item = (.. as Iterator).Item` IS `int` - and
            // structural unification would reject the candidate on
            // spelling. Var-carrying pins skip reduction (the oracle's
            // plain conversion erases inference vars) and unify as
            // variables.
            let requested = if requested.has_projection()
                && let Ok(closed) = baml_type::interned::ClosedTy::try_from(requested)
            {
                self.reduce_projections(&closed, super::PROJECTION_FINALIZE_FUEL)
                    .into_ty()
            } else {
                requested.clone()
            };
            let supplied = if supplied.has_projection()
                && let Ok(closed) = baml_type::interned::ClosedTy::try_from(&supplied)
            {
                self.reduce_projections(&closed, super::PROJECTION_FINALIZE_FUEL)
                    .into_ty()
            } else {
                supplied
            };
            self.table.unify(&requested, &supplied).ok()?;
        }
        Some(instantiation)
    }

    /// The SUBJECT half of confirmation, shared with the method probe:
    /// the impl's params instantiate as FRESH inference variables and
    /// the goal unifies against the for-target - which merely LINKS the
    /// goal's variables to the impl's, committing nothing the caller's
    /// snapshot cannot discard. Bare-blanket guard as in the ground
    /// matcher: `implement<T> I for T` applies only to concrete
    /// receivers.
    fn confirm_impl_subject(
        &mut self,
        goal: &Ty,
        facts: &crate::impls::ImplFacts<'_>,
    ) -> Option<rustc_hash::FxHashMap<baml_type::ParamTy, Ty>> {
        if let InferTy::TypeVar(param) = facts.for_ty_pattern.kind()
            && facts.generic_params.iter().any(|(p, _)| p == param)
            && !crate::impls::is_concrete_receiver(goal)
        {
            return None;
        }
        let instantiation: rustc_hash::FxHashMap<baml_type::ParamTy, Ty> = facts
            .generic_params
            .iter()
            .map(|(param, _)| (param.clone(), self.fresh_generic_arg(param)))
            .collect();
        let for_ty = facts
            .for_ty_pattern
            .as_ty()
            .substitute_bindings(&instantiation);
        // Unification is structural: an alias nested where the header has
        // structure meets it unfolded.
        let goal = crate::impls::unfold_aliases_against(&for_ty, goal, &self.facts);
        self.table.unify(&goal, &for_ty).ok()?;
        Some(instantiation)
    }

    /// rust-analyzer's METHOD PROBE for a receiver still carrying
    /// inference variables (`ArrayIterator<?T>.filter`): the ground
    /// registry fails safe on such types, so candidates are tried
    /// NON-COMMITTALLY under a table snapshot - subject confirmation
    /// plus "does its interface declare the member" - and exactly ONE
    /// applying re-confirms for real, its header unification LINKING
    /// the receiver's variables to the impl's fresh ones (rustc's
    /// probe-in-snapshot, then confirm-the-pick; the variables are
    /// never forced early, so the deferral model is untouched). Zero
    /// or several candidates resolve nothing - several is rustc's
    /// "type annotations needed" family, S17's diagnostic. Licensed by
    /// a concrete head, like variable-bearing selection.
    pub(super) fn probe_impl_member(
        &mut self,
        receiver: &Ty,
        name: &baml_type::Name,
        at: ExprId,
    ) -> Option<crate::method_resolution::InterfaceMember<'db>> {
        if !crate::impls::is_concrete_receiver(receiver) {
            return None;
        }
        let mut applicable: Option<&crate::impls::ImplFacts<'_>> = None;
        for facts in crate::impls::all_impl_facts(self.db, self.viewer()) {
            if !crate::impls::provides_concrete_members(
                baml_compiler2_hir::package::lang_roots(self.db),
                &facts.interface.name,
            ) {
                continue;
            }
            let probe = self.probe();
            let applies = self.probe_candidate(receiver, name, facts).is_some();
            self.rollback_probe(probe);
            if applies {
                if applicable.is_some() {
                    return None;
                }
                applicable = Some(facts);
            }
        }
        let Some(facts) = applicable else {
            let roots = if receiver.has_infer() {
                crate::impls::structural_interface_roots(self.db)
                    .into_iter()
                    .chain(crate::impls::hash_interface_root(self.db))
                    .collect()
            } else {
                crate::impls::applicable_structural_interface_roots(self.db, &self.facts, receiver)
            };
            let mut candidates = roots.into_iter().filter_map(|interface| {
                crate::method_resolution::member_on_interface(
                    self.db,
                    &self.facts,
                    &interface,
                    receiver,
                    name,
                    false,
                )
                .map(|member| (interface, member))
            });
            let (interface, member) = candidates.next()?;
            if candidates.next().is_some() {
                return None;
            }
            // Discovering a conditional default's signature does not prove
            // conformance. Fulfillment checks it once the receiver resolves.
            if crate::impls::structural_interface(self.db, &interface.name)
                == Some(baml_type::StructuralInterface::Hash)
            {
                self.register_obligation(Obligation::implements(
                    receiver.clone(),
                    interface,
                    at,
                    GoalPurpose::Bound,
                ));
            }
            return Some(member);
        };
        let (member, instantiation) = self
            .probe_candidate(receiver, name, facts)
            .expect("the unique applicable candidate re-confirms");
        let required_for = [RequiredFor {
            subject: receiver.clone(),
            interface: instantiate_interface(&facts.interface, &instantiation),
        }];
        self.register_impl_bound_obligations(facts, &instantiation, at, &required_for);
        Some(member)
    }

    /// One probe candidate: subject confirmation, then the member
    /// resolved on the interface this impl provides, realized through
    /// the confirmation's bindings (args and pins in terms of the
    /// now-linked variables - `filter`'s signature comes back MENTIONING
    /// the receiver's `?T`, and later argument checks bound it like any
    /// other evidence).
    fn probe_candidate(
        &mut self,
        receiver: &Ty,
        name: &baml_type::Name,
        facts: &crate::impls::ImplFacts<'_>,
    ) -> Option<(
        crate::method_resolution::InterfaceMember<'db>,
        rustc_hash::FxHashMap<baml_type::ParamTy, Ty>,
    )> {
        let instantiation = self.confirm_impl_subject(receiver, facts)?;
        // The target's pins carry the impl's OWN associated bindings
        // (`type Item = T`) alongside any header pins: the member's
        // `Self.Item` slots must realize through the confirmation's
        // variables, not stay symbolic projections over a receiver the
        // oracle refuses (it is var-carrying by construction here).
        let mut pins: Vec<(baml_type::Name, Ty)> = facts
            .interface
            .associated_types
            .iter()
            .map(|(pin, ty)| (pin, ty))
            .chain(facts.associated_types.iter().map(|(pin, ty)| (pin, &**ty)))
            .map(|(pin, ty)| (pin.clone(), ty.substitute_bindings(&instantiation)))
            .collect();
        pins.dedup_by(|(a, _), (b, _)| a == b);
        let implemented = InferInterface::new(
            facts.interface.name.clone(),
            facts
                .interface
                .generics
                .iter()
                .map(|arg| arg.substitute_bindings(&instantiation))
                .collect(),
            pins.into_boxed_slice(),
        );
        let member = crate::method_resolution::member_on_interface(
            self.db,
            &self.facts,
            &implemented,
            receiver,
            name,
            false,
        )?;
        Some((member, instantiation))
    }

    /// Whether ground `ty` implements `interface`: carried bounds for a
    /// rigid var (directly or through the requires closure), interface
    /// identity/requires for an existential, the impl registry for
    /// concrete types. The spec's concreteness rule holds by
    /// construction: a union reaches the registry and no impl subject is
    /// a union, so it fails - never "passes as a subtype".
    fn implements_holds(&mut self, ty: &Ty, interface: &InferInterface) -> bool {
        if crate::impls::is_structural_interface(self.db, interface) {
            return true;
        }
        if crate::impls::structural_interface(self.db, &interface.name)
            == Some(baml_type::StructuralInterface::Hash)
        {
            return crate::impls::hash_eligible(self.db, &self.facts, ty, interface);
        }
        let target = interface.clone();
        let eq = crate::impls::AliasOnlyFacts::new(self.db);
        match ty.kind() {
            InferTy::TypeVar(param) => {
                let carried = baml_type::normalize::TypeContext::type_var_bound(&self.facts, param);
                carried.iter().any(|have| {
                    let have = InferInterface::from_constraint(have);
                    crate::impls::head_satisfies(self.db, &have, &target, &eq)
                        || crate::impls::interface_requires(self.db, &have, &target, ty, 8)
                })
            }
            InferTy::Interface(name, args, pins) => {
                let have = InferInterface::new(name.clone(), args.clone(), pins.clone());
                crate::impls::head_satisfies(self.db, &have, &target, &eq)
                    || crate::impls::interface_requires(self.db, &have, &target, ty, 8)
            }
            // A projection: a reducible one reduces inside the canonical
            // algebra (the oracle is live since I5); a still-symbolic one
            // proves against its declared bound through the algebra's
            // projection-subtype rule (`associated_type_bound`).
            // Fail-closed - TIR's vacuous rule retired with I5.
            InferTy::AssociatedTypeProjection { .. } => {
                let existential = interface.existential();
                self.sub(ty, &existential)
            }
            _ => crate::impls::implements_interface(self.db, ty, &target),
        }
    }

    fn resolve_interface_ref(&mut self, interface: &InferInterface) -> InferInterface {
        InferInterface::new(
            interface.name.clone(),
            interface
                .generics
                .iter()
                .map(|arg| self.table.resolve_completely(arg))
                .collect(),
            interface
                .associated_types
                .iter()
                .map(|(name, ty)| (name.clone(), self.table.resolve_completely(ty)))
                .collect(),
        )
    }
}

/// A literal type is judged as its base primitive: `1` implements what `int`
/// does. Only the JUDGMENT widens - a report names the subject as the user
/// wrote it (`1`), which is why [`InferenceContext::attempt`] keeps both.
fn widen_literal(ty: &Ty) -> Ty {
    match ty.kind() {
        InferTy::Literal(literal, _) => Ty::intern(super::literal_base(literal)),
        _ => ty.clone(),
    }
}

fn interface_has_infer(interface: &InferInterface) -> bool {
    interface.generics.iter().any(Ty::has_infer)
        || interface
            .associated_types
            .iter()
            .any(|(_, ty)| ty.has_infer())
}
