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
//!   stalled `Implements` goal on a known subject reports its ambiguity
//!   ([`InferenceContext::settle_stalled_obligations`]).
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
use baml_type::interned::{InferInterface, InferTy, Ty};

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
    /// A bound an impl's header places on its own generic param, replayed
    /// when the impl is selected. The body may dispatch on the TYPE
    /// (`(T as I).make()`), so it is nominal; an existential argument proves
    /// it by its own reference and `requires` closure.
    Bound,
    /// A declared bound on a function's or class's own generic param:
    /// [`GoalPurpose::Bound`], and the argument must be concrete (E0001) -
    /// an abstract type has no single runtime type to dispatch on.
    ConcreteBound,
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

enum Attempt {
    /// Live variables remain: retry next round.
    Stalled,
    /// Discharged (successfully or with a recorded failure).
    Done,
}

/// The outcome of candidate selection for a goal whose subject is known.
enum Selection {
    /// Exactly one candidate applied, and confirming it committed its
    /// unifications.
    Confirmed,
    /// Several candidates apply; more solving may prune them.
    Ambiguous,
    /// No candidate can apply, whatever the goal's open variables become.
    NoCandidate,
}

impl<'db> InferenceContext<'db> {
    pub(super) fn register_obligation(&mut self, obligation: Obligation) {
        self.obligations.push(obligation);
    }

    /// One discharge round over every pending obligation. Returns whether
    /// anything discharged (progress for the fixpoint driver).
    pub(super) fn discharge_obligations_once(&mut self) -> bool {
        let pending = std::mem::take(&mut self.obligations);
        let mut progressed = false;
        for obligation in pending {
            match self.attempt(&obligation) {
                Attempt::Done => progressed = true,
                Attempt::Stalled => self.obligations.push(obligation),
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
            } => {
                let at = *at;
                let purpose = *purpose;
                let ty = self.table.resolve_completely(ty);
                let interface = self.resolve_interface_ref(interface);
                if ty.has_error() {
                    return Attempt::Done;
                }
                // A literal implements what its base primitive does, so it
                // is JUDGED as the base; a report names the type as written.
                let subject = ty;
                let mut ty = widen_literal(&subject);
                // One spelling, one verdict (B-1576): resolution can ground a
                // syntactic union after this obligation registered, and the
                // canonical form may be a single member. An OPEN union may
                // still collapse, so only a coercion (which needs every
                // member either way) acts on it before it closes.
                if let InferTy::Union(..) = ty.kind() {
                    match baml_type::interned::ClosedTy::try_from(&ty) {
                        Ok(closed) => ty = self.canonicalize_unions(&closed).into_ty(),
                        Err(_) if purpose != GoalPurpose::Coercion => return Attempt::Stalled,
                        Err(_) => {}
                    }
                }
                if purpose == GoalPurpose::Coercion {
                    match ty.kind() {
                        // `never` inhabits every type.
                        InferTy::Never { .. } => return Attempt::Done,
                        // A union value is used through the interface by
                        // whichever member it holds, so every member must
                        // implement it (the spec's Variance rule 2.1).
                        InferTy::Union(members, _) => {
                            for member in members {
                                self.register_obligation(Obligation::Implements {
                                    ty: member.clone(),
                                    interface: interface.clone(),
                                    at,
                                    purpose,
                                });
                            }
                            return Attempt::Done;
                        }
                        _ => {}
                    }
                }
                // An abstract argument has no single runtime type to
                // dispatch on (E0001), whatever its args become.
                if purpose == GoalPurpose::ConcreteBound
                    && matches!(ty.kind(), InferTy::Interface(..) | InferTy::Union(..))
                {
                    self.pending_diags
                        .push(super::PendingDiag::BoundedArgNotConcrete {
                            expr: at,
                            arg: ty,
                            bound: interface,
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
                        self.report_not_implemented(at, subject, &interface);
                    }
                    return Attempt::Done;
                }
                let selection = match ty.kind() {
                    // Nothing filters the candidate set yet: a head still
                    // unknown, or a projection whose base has not resolved.
                    // Guessing is the one thing fulfillment never does.
                    InferTy::InferVar { .. } | InferTy::AssociatedTypeProjection { .. } => {
                        return Attempt::Stalled;
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
                    _ => self.select_impl(&ty, &interface, at),
                };
                match selection {
                    Selection::Confirmed => Attempt::Done,
                    Selection::Ambiguous => Attempt::Stalled,
                    Selection::NoCandidate => {
                        self.report_not_implemented(at, subject, &interface);
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
                    return Attempt::Stalled;
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
    /// variables. Zero applicable is a definite failure (reported);
    /// several is genuine ambiguity - Stalled, retried once more
    /// information may prune, reported at quiescence if it never does
    /// (rustc's "type annotations needed"). Committing on uniqueness is
    /// sound because coherence (I7) guarantees at most one impl per
    /// realized instance.
    fn select_impl(&mut self, goal: &Ty, interface: &InferInterface, at: ExprId) -> Selection {
        let candidates = crate::impls::impl_candidates(self.db, goal, &interface.name);
        let mut applicable = None;
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
            return Selection::NoCandidate;
        };
        let instantiation = self
            .confirm_impl(goal, interface, facts)
            .expect("the unique applicable candidate re-confirms");
        self.register_impl_bound_obligations(facts, &instantiation, at);
        Selection::Confirmed
    }

    /// Object selection: the existential subject's own reference and its
    /// `requires` closure are the candidate heads - a matching head
    /// confirms by unifying args and the goal's pins (`Iterator<Error =
    /// never>` proving `Iterable<Error = ?E2>` commits `?E2 := never`).
    /// As in impl selection: exactly one applicable head commits,
    /// several stall, none reports.
    fn select_object(&mut self, subject: &Ty, goal: &InferInterface) -> Selection {
        let InferTy::Interface(name, args, pins, _) = subject.kind() else {
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
        let InferTy::TypeVar(param, _) = subject.kind() else {
            unreachable!("caller-bound selection is for a rigid var subject")
        };
        let carried = baml_type::normalize::TypeContext::type_var_bound(&self.facts, param);
        let heads: Vec<InferInterface> = carried
            .iter()
            .flat_map(|bound| {
                crate::impls::requires_heads(
                    self.db,
                    &InferInterface::from_constraint(bound),
                    subject,
                    8,
                )
            })
            .collect();
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
                            attr: baml_type::TyAttr::default(),
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
    fn report_not_implemented(&mut self, at: ExprId, subject: Ty, interface: &InferInterface) {
        let interface = interface.existential();
        let already = self.pending_diags.iter().any(|pending| {
            matches!(
                pending,
                super::PendingDiag::DoesNotImplement { expr, value, interface: reported }
                    if *expr == at && *value == subject && *reported == interface
            )
        });
        if !already {
            self.pending_diags
                .push(super::PendingDiag::DoesNotImplement {
                    expr: at,
                    value: subject,
                    interface,
                });
        }
    }

    /// Settles the goals still pending at quiescence, when no more solving
    /// can happen. A goal whose subject never resolved, or is a projection
    /// whose base never did, is left to the diagnostics for those (reporting
    /// it too would only repeat them), as the deferred subs' residue is. A
    /// known subject still pending had more than one impl or bound that
    /// could prove it - rustc's "type annotations needed" (E0283).
    pub(super) fn settle_stalled_obligations(&mut self) {
        for obligation in std::mem::take(&mut self.obligations) {
            let Obligation::Implements {
                ty, interface, at, ..
            } = obligation
            else {
                // A stalled operator's operand is an unsolved variable;
                // its own diagnostic covers it.
                continue;
            };
            let ty = self.table.resolve_completely(&ty);
            if ty.has_error()
                || ty.has_projection()
                || matches!(ty.kind(), InferTy::InferVar { .. })
            {
                continue;
            }
            let interface = self.resolve_interface_ref(&interface).existential();
            self.pending_diags
                .push(super::PendingDiag::AmbiguousImplementation {
                    expr: at,
                    value: ty,
                    interface,
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
    /// and the method probe.
    fn register_impl_bound_obligations(
        &mut self,
        facts: &crate::impls::ImplFacts<'_>,
        instantiation: &rustc_hash::FxHashMap<baml_type::ParamTy, Ty>,
        at: ExprId,
    ) {
        for (param, bounds) in &facts.generic_params {
            let Some(arg) = instantiation.get(param) else {
                continue;
            };
            for bound in bounds {
                let interface = InferInterface::new(
                    bound.name.clone(),
                    bound
                        .generics
                        .iter()
                        .map(|ty| crate::impls::substitute_bindings(ty, instantiation))
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                    bound
                        .associated_types
                        .iter()
                        .map(|(name, ty)| {
                            (
                                name.clone(),
                                crate::impls::substitute_bindings(ty, instantiation),
                            )
                        })
                        .collect(),
                );
                self.register_obligation(Obligation::Implements {
                    ty: arg.clone(),
                    interface,
                    at,
                    purpose: GoalPurpose::Bound,
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
            let pattern = crate::impls::substitute_bindings(pattern, &instantiation);
            self.table.unify(requested, &pattern).ok()?;
        }
        for (name, requested) in &interface.associated_types {
            let supplied = facts
                .associated_types
                .iter()
                .find(|(declared, _)| declared == name)
                .map(|(_, ty)| crate::impls::substitute_bindings(ty, &instantiation))
                .or_else(|| {
                    let implemented = InferInterface::new(
                        facts.interface.name.clone(),
                        facts
                            .interface
                            .generics
                            .iter()
                            .map(|ty| crate::impls::substitute_bindings(ty, &instantiation))
                            .collect::<Vec<_>>()
                            .into_boxed_slice(),
                        facts
                            .interface
                            .associated_types
                            .iter()
                            .map(|(pin, ty)| {
                                (
                                    pin.clone(),
                                    crate::impls::substitute_bindings(ty, &instantiation),
                                )
                            })
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
        if let InferTy::TypeVar(param, _) = facts.for_ty_pattern.kind()
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
        let for_ty = crate::impls::substitute_bindings(&facts.for_ty_pattern, &instantiation);
        self.table.unify(goal, &for_ty).ok()?;
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
        let mut applicable = None;
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
        let facts = applicable?;
        let (member, instantiation) = self
            .probe_candidate(receiver, name, facts)
            .expect("the unique applicable candidate re-confirms");
        self.register_impl_bound_obligations(facts, &instantiation, at);
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
            .map(|(pin, ty)| {
                (
                    pin.clone(),
                    crate::impls::substitute_bindings(ty, &instantiation),
                )
            })
            .collect();
        pins.dedup_by(|(a, _), (b, _)| a == b);
        let implemented = InferInterface::new(
            facts.interface.name.clone(),
            facts
                .interface
                .generics
                .iter()
                .map(|arg| crate::impls::substitute_bindings(arg, &instantiation))
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
        let target = interface.clone();
        let eq = crate::impls::AliasOnlyFacts::new(self.db);
        match ty.kind() {
            InferTy::TypeVar(param, _) => {
                let carried = baml_type::normalize::TypeContext::type_var_bound(&self.facts, param);
                carried.iter().any(|have| {
                    let have = InferInterface::from_constraint(have);
                    crate::impls::head_satisfies(self.db, &have, &target, &eq)
                        || crate::impls::interface_requires(self.db, &have, &target, ty, 8)
                })
            }
            InferTy::Interface(name, args, pins, _) => {
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
/// does, and reports as `int`.
fn widen_literal(ty: &Ty) -> Ty {
    match ty.kind() {
        InferTy::Literal(literal, _, attr) => {
            Ty::intern(super::literal_base(literal, attr.clone()))
        }
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
