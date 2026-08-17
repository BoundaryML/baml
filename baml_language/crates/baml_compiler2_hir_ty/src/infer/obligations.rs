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
//!   or definitively fails (a recorded mismatch, the output erased).
//! - After fixpoint, still-stalled obligations fail CLOSED: their
//!   outputs erase through the ordinary finalize rules (ruling 2), and
//!   S17 renders the ambiguity.
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
use baml_type::interned::{InterfaceRef, Ty, TyKind};

use super::InferenceContext;

/// One registered obligation.
pub(super) enum Obligation {
    /// `ty` must implement `interface` (call-site bound checks). Failure
    /// records a mismatch at `at`; there is no output to bind.
    Implements {
        ty: Ty,
        interface: InterfaceRef,
        at: ExprId,
        /// True for a GENERIC-PARAM bound obligation: only a concrete
        /// type can instantiate an interface-bounded parameter (an
        /// abstract arg - union or interface existential - has no single
        /// runtime type to dispatch on), so those reject with
        /// `BoundedTypeArgNotConcrete` even when the implements relation
        /// would hold. False for coercion/iterability goals, where an
        /// existential legitimately satisfies the interface.
        not_concrete_rejects: bool,
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
                not_concrete_rejects,
            } => {
                let ty = self.table.resolve_completely(ty);
                let interface = self.resolve_interface_ref(interface);
                if ty.has_error() {
                    return Attempt::Done;
                }
                if !ty.has_infer() && !interface_has_infer(&interface) {
                    if *not_concrete_rejects
                        && matches!(ty.kind(), TyKind::Interface(..) | TyKind::Union(..))
                    {
                        self.pending_diags
                            .push(super::PendingDiag::BoundedArgNotConcrete {
                                expr: *at,
                                arg: ty.clone(),
                                bound: interface.clone(),
                            });
                        return Attempt::Done;
                    }
                    if !self.implements_holds(&ty, &interface) {
                        let expected = interface.existential();
                        self.result
                            .type_mismatches
                            .insert(*at, (expected, ty.clone()));
                    }
                    return Attempt::Done;
                }
                // Object candidates (rustc's
                // `assemble_candidates_from_object_ty`): an existential
                // subject has no impl to select - it proves the goal
                // from its OWN reference plus its `requires` closure.
                if matches!(ty.kind(), TyKind::Interface(..)) {
                    return self.select_object(&ty, &interface, *at);
                }
                // Variable-bearing goal: rustc's SELECTION, licensed by a
                // known concrete head (candidates filter by it). A goal
                // whose head is itself unknown - a bare inference var or
                // a rigid var - stays Stalled: nothing filters the
                // candidate set, and guessing is the one thing
                // fulfillment never does.
                if crate::impls::is_concrete_receiver(&ty) {
                    return self.select_impl(&ty, &interface, *at);
                }
                Attempt::Stalled
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
    /// variables. Zero applicable is a definite failure (the mismatch
    /// records); several is genuine ambiguity - Stalled, retried once
    /// more information may prune, failing closed through finalize
    /// (rustc's "type annotations needed"). Committing on uniqueness is
    /// sound because coherence (I7) guarantees at most one impl per
    /// realized instance.
    fn select_impl(&mut self, goal: &Ty, interface: &InterfaceRef, at: ExprId) -> Attempt {
        let candidates = crate::impls::impl_candidates(self.db, goal, &interface.name);
        let mut applicable = None;
        for facts in candidates {
            let snapshot = self.table.snapshot();
            let applies = self.confirm_impl(goal, interface, facts).is_some();
            self.table.rollback_to(snapshot);
            if applies {
                if applicable.is_some() {
                    return Attempt::Stalled;
                }
                applicable = Some(facts);
            }
        }
        let Some(facts) = applicable else {
            let expected = interface.existential();
            self.result
                .type_mismatches
                .insert(at, (expected, goal.clone()));
            return Attempt::Done;
        };
        let instantiation = self
            .confirm_impl(goal, interface, facts)
            .expect("the unique applicable candidate re-confirms");
        self.register_impl_bound_obligations(facts, &instantiation, at);
        Attempt::Done
    }

    /// Object selection: the existential subject's own reference and its
    /// `requires` closure are the candidate heads - a matching head
    /// confirms by unifying args and the goal's pins (`Iterator<Error =
    /// never>` proving `Iterable<Error = ?E2>` commits `?E2 := never`).
    /// As in impl selection: exactly one applicable head commits,
    /// several stall, none records the mismatch.
    fn select_object(&mut self, subject: &Ty, goal: &InterfaceRef, at: ExprId) -> Attempt {
        let TyKind::Interface(name, args, pins, _) = subject.kind() else {
            return Attempt::Stalled;
        };
        let subject_target = InterfaceRef::new(name.clone(), (args.to_vec()).into(), pins.to_vec());
        let heads = crate::impls::requires_heads(self.db, &subject_target, subject, 8);
        let goal_target = goal.clone();
        let mut applicable = None;
        for head in &heads {
            let snapshot = self.table.snapshot();
            let applies = self.confirm_object(head, &goal_target);
            self.table.rollback_to(snapshot);
            if applies {
                if applicable.is_some() {
                    return Attempt::Stalled;
                }
                applicable = Some(head);
            }
        }
        let Some(head) = applicable else {
            let expected = goal.existential();
            self.result
                .type_mismatches
                .insert(at, (expected, subject.clone()));
            return Attempt::Done;
        };
        let confirmed = self.confirm_object(head, &goal_target);
        debug_assert!(confirmed, "the unique applicable head re-confirms");
        Attempt::Done
    }

    /// One object head against the goal: names equal, args unify
    /// pairwise, and every goal pin unifies with the head's realization
    /// of that member (the head may pin MORE; a member the head does not
    /// realize cannot prove a pinned requirement).
    fn confirm_object(&mut self, head: &InterfaceRef, goal: &InterfaceRef) -> bool {
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
                let interface = InterfaceRef::new(
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
                    // Impl-header confirmation replays the impl's own
                    // bounds; selection already admitted the receiver, so
                    // keep the plain implements judgement.
                    not_concrete_rejects: false,
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
        interface: &InterfaceRef,
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
                    let implemented = InterfaceRef::new(
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
            let requested = if requested.has_projection() && !requested.has_infer() {
                self.reduce_projections(requested, super::PROJECTION_FINALIZE_FUEL)
            } else {
                requested.clone()
            };
            let supplied = if supplied.has_projection() && !supplied.has_infer() {
                self.reduce_projections(&supplied, super::PROJECTION_FINALIZE_FUEL)
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
        if let TyKind::TypeVar(param, _) = facts.for_ty_pattern.kind()
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
        for facts in crate::impls::all_impl_facts(self.db) {
            // Name-only reject before any snapshot/unification: a
            // candidate whose interface never declares `name` cannot
            // produce a member (`member_on_interface` would return None),
            // so it can't apply and can't create ambiguity. The probe
            // previously cloned the whole bounds table per impl block in
            // the project just to discover this.
            if crate::method_resolution::interface_declares_member(
                self.db,
                &self.facts,
                &facts.interface,
                name,
            )
            .is_none()
            {
                continue;
            }
            let snapshot = self.table.snapshot();
            let applies = self.probe_candidate(receiver, name, facts).is_some();
            self.table.rollback_to(snapshot);
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
            .chain(facts.associated_types.iter())
            .map(|(pin, ty)| {
                (
                    pin.clone(),
                    crate::impls::substitute_bindings(ty, &instantiation),
                )
            })
            .collect();
        pins.dedup_by(|(a, _), (b, _)| a == b);
        let implemented = InterfaceRef::new(
            facts.interface.name.clone(),
            facts
                .interface
                .generics
                .iter()
                .map(|arg| crate::impls::substitute_bindings(arg, &instantiation))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            pins,
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
    fn implements_holds(&mut self, ty: &Ty, interface: &InterfaceRef) -> bool {
        let target = interface.clone();
        let eq = crate::impls::AliasOnlyFacts::new(self.db);
        match ty.kind() {
            TyKind::TypeVar(param, _) => {
                let carried = baml_type::normalize::TypeContext::type_var_bound(&self.facts, param);
                carried.iter().any(|have| {
                    let have = InterfaceRef::from_constraint(have);
                    carried_satisfies(&have, &target, &eq)
                        || crate::impls::interface_requires(self.db, &have, &target, ty, 8)
                })
            }
            TyKind::Interface(name, args, pins, _) => {
                let have = InterfaceRef::new(name.clone(), (args.to_vec()).into(), pins.to_vec());
                carried_satisfies(&have, &target, &eq)
                    || crate::impls::interface_requires(self.db, &have, &target, ty, 8)
            }
            // A projection: a reducible one reduces inside the canonical
            // algebra (the oracle is live since I5); a still-symbolic one
            // proves against its declared bound through the algebra's
            // projection-subtype rule (`associated_type_bound`).
            // Fail-closed - TIR's vacuous rule retired with I5.
            TyKind::AssociatedTypeProjection { .. } => {
                let existential = interface.existential();
                self.sub(ty, &existential)
            }
            _ => crate::impls::implements_interface(self.db, ty, &target),
        }
    }

    fn resolve_interface_ref(&mut self, interface: &InterfaceRef) -> InterfaceRef {
        InterfaceRef::new(
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

fn interface_has_infer(interface: &InterfaceRef) -> bool {
    interface.generics.iter().any(Ty::has_infer)
        || interface
            .associated_types
            .iter()
            .any(|(_, ty)| ty.has_infer())
}

/// `have` satisfies `want` when heads and args agree and `have` pins
/// everything `want` pins to the same type (it may pin MORE; a bare
/// `have` does not satisfy a pinned requirement).
fn carried_satisfies(
    have: &InterfaceRef,
    want: &InterfaceRef,
    eq: &crate::impls::AliasOnlyFacts<'_>,
) -> bool {
    use baml_type::normalize::equivalent_interned;
    // The shared head relation, plus the pin-superset this consumer
    // layers on (a bare carried bound does not satisfy a pinned
    // requirement). Args and pins compare under the alias oracle -
    // the `==` drift an alias-spelled bound used to trip is gone.
    crate::impls::head_matches(have, want, eq)
        && want.associated_types.iter().all(|(name, want_pin)| {
            have.associated_types.iter().any(|(have_name, have_pin)| {
                have_name == name && equivalent_interned(have_pin, want_pin, eq)
            })
        })
}
