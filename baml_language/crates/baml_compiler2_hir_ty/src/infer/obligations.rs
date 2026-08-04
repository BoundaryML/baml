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
use baml_type::{
    interned::{InterfaceRef, Ty, TyKind},
};

use super::InferenceContext;
use crate::impls::InterfaceTarget;

/// One registered obligation.
pub(super) enum Obligation {
    /// `ty` must implement `interface` (call-site bound checks). Failure
    /// records a mismatch at `at`; there is no output to bind.
    Implements {
        ty: Ty,
        interface: InterfaceRef,
        at: ExprId,
    },
    /// `lhs <op-interface> rhs` deferred: discharge re-runs the SAME
    /// ground operator dispatch (union distribution, literal widening,
    /// carried bounds included) and unifies `out` with its result.
    Operator {
        interface: &'static str,
        lhs: Ty,
        rhs: Option<Ty>,
        out: Ty,
        /// The registering expression - S17's anchor for the no-impl and
        /// still-ambiguous diagnostics (the mismatch is recorded through
        /// the discharge result today).
        #[allow(dead_code)]
        at: ExprId,
    },
}

enum Attempt {
    /// Live variables remain: retry next round.
    Stalled,
    /// Discharged (successfully or with a recorded failure).
    Done,
}

impl InferenceContext<'_> {
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
            Obligation::Implements { ty, interface, at } => {
                let ty = self.table.resolve_completely(ty);
                let interface = self.resolve_interface_ref(interface);
                if ty.has_infer() || interface_has_infer(&interface) {
                    return Attempt::Stalled;
                }
                if ty.has_error() {
                    return Attempt::Done;
                }
                if !self.implements_holds(&ty, &interface) {
                    let expected = interface_as_existential(&interface);
                    self.result
                        .type_mismatches
                        .insert(*at, (expected, ty.clone()));
                }
                Attempt::Done
            }
            Obligation::Operator {
                interface,
                lhs,
                rhs,
                out,
                ..
            } => {
                let lhs = self.table.resolve_completely(lhs);
                let rhs = rhs.as_ref().map(|rhs| self.table.resolve_completely(rhs));
                if lhs.has_infer() || rhs.as_ref().is_some_and(Ty::has_infer) {
                    return Attempt::Stalled;
                }
                let result = self.dispatch_operator(interface, &lhs, rhs.as_ref());
                let _ = self.table.unify(out, &result);
                Attempt::Done
            }
        }
    }

    /// Whether ground `ty` implements `interface`: carried bounds for a
    /// rigid var (directly or through the requires closure), interface
    /// identity/requires for an existential, the impl registry for
    /// concrete types. The spec's concreteness rule holds by
    /// construction: a union reaches the registry and no impl subject is
    /// a union, so it fails - never "passes as a subtype".
    fn implements_holds(&mut self, ty: &Ty, interface: &InterfaceRef) -> bool {
        let target = target_of(interface);
        match ty.kind() {
            TyKind::TypeVar(param, _) => {
                let carried =
                    baml_type::normalize::TypeContext::type_var_bound(&self.facts, param);
                carried.iter().any(|have| {
                    let have = InterfaceTarget::from_constraint(have);
                    carried_satisfies(&have, &target)
                        || crate::impls::interface_requires(self.db, &have, &target, 8)
                })
            }
            TyKind::Interface(name, args, pins, _) => {
                let have = InterfaceTarget {
                    name: name.clone(),
                    args: args.to_vec(),
                    pins: pins.to_vec(),
                };
                carried_satisfies(&have, &target)
                    || crate::impls::interface_requires(self.db, &have, &target, 8)
            }
            // A projection: a reducible one reduces inside the canonical
            // algebra (the oracle is live since I5); a still-symbolic one
            // proves against its declared bound through the algebra's
            // projection-subtype rule (`associated_type_bound`).
            // Fail-closed - TIR's vacuous rule retired with I5.
            TyKind::AssociatedTypeProjection { .. } => {
                let existential = interface_as_existential(interface);
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

fn target_of(interface: &InterfaceRef) -> InterfaceTarget {
    InterfaceTarget {
        name: interface.name.clone(),
        args: interface.generics.to_vec(),
        pins: interface.associated_types.to_vec(),
    }
}

fn interface_as_existential(interface: &InterfaceRef) -> Ty {
    Ty::intern(TyKind::Interface(
        interface.name.clone(),
        interface.generics.to_vec().into(),
        interface.associated_types.to_vec().into(),
        baml_type::TyAttr::default(),
    ))
}

/// `have` satisfies `want` when heads and args agree and `have` pins
/// everything `want` pins to the same type (it may pin MORE; a bare
/// `have` does not satisfy a pinned requirement).
fn carried_satisfies(have: &InterfaceTarget, want: &InterfaceTarget) -> bool {
    have.name == want.name
        && have.args.len() == want.args.len()
        && have.args.iter().zip(&want.args).all(|(a, b)| a == b)
        && want.pins.iter().all(|(name, want_pin)| {
            have.pins
                .iter()
                .any(|(have_name, have_pin)| have_name == name && have_pin == want_pin)
        })
}
