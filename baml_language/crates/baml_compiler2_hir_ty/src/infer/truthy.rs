//! Truthiness (B-1563) - condition positions coerce any value to `bool`
//! instead of requiring one.
//!
//! The falsy set is uniform and type-agnostic (Python's rule): `false`,
//! `null`, `0`, `0n`, `0.0`/`-0.0`, `""`, `[]`, `{}`, and an empty byte
//! array. Everything else - including `NaN`, class instances, enum
//! variants, functions, and media - is truthy. `void` conditions are an
//! error (the value does not exist), and a condition whose STATIC type
//! decides the branch is a warning (TS 5.6's 2872/2873) unless the
//! condition is a written literal (`while (true)` stays idiomatic).
//!
//! Layering follows the `FunctionAdapter` precedent exactly: the checker
//! DECIDES here and records an [`Adjust::Truthy`] adjustment on the
//! condition expression; MIR synthesizes the coercion structurally from
//! `expr_adjustments`; the VM's branch opcodes stay strict-bool. A
//! condition already typed `bool` records nothing and lowers exactly as
//! before.

use baml_compiler2_ast::{Expr, ExprBody, ExprId};
use baml_type::{
    Literal,
    interned::{InferTy, Ty},
};

use super::{Adjust, Adjustment, Expectation, InferenceContext, InferenceResult};

/// What a value's static type says about its runtime truthiness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Truthiness {
    /// Every runtime inhabitant is truthy (an instance, a function, a
    /// non-falsy literal).
    AlwaysTruthy,
    /// Every runtime inhabitant is falsy (`null`, `false`, `0`, `""`).
    AlwaysFalsy,
    /// Decided per value at runtime.
    Runtime,
}

/// Truthiness of a RESOLVED type. Conservative: anything open (vars,
/// projections, `unknown`) answers `Runtime`.
pub(crate) fn truthiness(ty: &Ty) -> Truthiness {
    match ty.kind() {
        InferTy::Null { .. } => Truthiness::AlwaysFalsy,
        InferTy::Literal(lit, _, _) => literal_truthiness(lit),
        // Runtime-decided scalars and containers: any of them can hold a
        // falsy value.
        InferTy::Bool { .. }
        | InferTy::Int { .. }
        | InferTy::Bigint { .. }
        | InferTy::Float { .. }
        | InferTy::String { .. }
        | InferTy::Uint8Array { .. }
        | InferTy::List(..)
        | InferTy::Map { .. } => Truthiness::Runtime,
        // Heap values with no falsy inhabitant.
        InferTy::Class(..)
        | InferTy::Interface(..)
        | InferTy::Enum(..)
        | InferTy::EnumVariant(..)
        | InferTy::Media(..)
        | InferTy::Function { .. }
        | InferTy::Future(..)
        | InferTy::Type { .. }
        | InferTy::Resource { .. }
        | InferTy::PromptAst { .. }
        | InferTy::RustType { .. } => Truthiness::AlwaysTruthy,
        InferTy::Union(members, _) => {
            let mut all_truthy = true;
            let mut all_falsy = true;
            for member in members {
                match truthiness(member) {
                    Truthiness::AlwaysTruthy => all_falsy = false,
                    Truthiness::AlwaysFalsy => all_truthy = false,
                    Truthiness::Runtime => return Truthiness::Runtime,
                }
            }
            match (all_truthy, all_falsy) {
                (true, false) => Truthiness::AlwaysTruthy,
                (false, true) => Truthiness::AlwaysFalsy,
                // A union is non-empty; both flags survive only if the
                // union mixes polarities, which the arms above cleared.
                _ => Truthiness::Runtime,
            }
        }
        // Open or sentinel types: no static claim.
        InferTy::Unknown { .. }
        | InferTy::TypeVar(..)
        | InferTy::AssociatedTypeProjection { .. }
        | InferTy::TypeAlias(..)
        | InferTy::Never { .. }
        | InferTy::Void { .. }
        | InferTy::Error { .. }
        | InferTy::InferVar { .. } => Truthiness::Runtime,
    }
}

/// Literal truthiness: the zero of each base and the empty string are
/// falsy; every other literal is truthy. Float literals carry their
/// SOURCE text, so `-0.0` and `0e5` classify by parsed value; a literal
/// that fails to parse (unreachable for checked source) stays truthy.
fn literal_truthiness(lit: &Literal) -> Truthiness {
    let falsy = match lit {
        Literal::Bool(b) => !b,
        Literal::Int(v) => *v == 0,
        Literal::Bigint(v) => *v == num_bigint::BigInt::ZERO,
        Literal::Float(text) => text.parse::<f64>().is_ok_and(|v| v == 0.0),
        Literal::String(s) => s.is_empty(),
    };
    if falsy {
        Truthiness::AlwaysFalsy
    } else {
        Truthiness::AlwaysTruthy
    }
}

impl<'db> InferenceContext<'db> {
    /// Type a condition position (`if`/`while`/guard, and `&&`/`||`
    /// operands): any type is accepted, a non-`bool` records the Truthy
    /// adjustment for MIR, `void` is a mismatch (there is no value to
    /// test), and a statically-decided branch warns.
    ///
    /// Inference runs with NO expectation - a `bool` expectation would
    /// wrongly pin type variables the condition is free to leave open
    /// under truthiness.
    pub(super) fn check_condition(&mut self, body: &ExprBody, condition: ExprId) -> Ty {
        self.check_truthy_operand(body, condition, true)
    }

    /// Types the operand of `!` (B-1563): same acceptance and warnings as
    /// a condition position, but NO `Adjust::Truthy` is recorded -
    /// `OpCode::Not` performs the truthiness coercion itself, so a
    /// recorded adjustment would be dead metadata with no MIR consumer.
    pub(super) fn check_not_operand(&mut self, body: &ExprBody, operand: ExprId) -> Ty {
        self.check_truthy_operand(body, operand, false)
    }

    /// The shared operand walk behind both positions: infer with no
    /// expectation, bail on error, defer open types to `finish`, decide
    /// closed ones. `coerce` distinguishes a branch condition (records
    /// `Adjust::Truthy`) from a `!` operand (records nothing -
    /// `OpCode::Not` coerces itself); everything else - acceptance, the
    /// `void` mismatch, the always-constant warning - is identical, and
    /// keeping it in one place is what stops the two paths drifting
    /// apart again.
    fn check_truthy_operand(&mut self, body: &ExprBody, operand: ExprId, coerce: bool) -> Ty {
        let ty = self.infer_expr(body, operand, &Expectation::None);
        let resolved = self.table.resolve_completely(&ty);
        if resolved.has_error() {
            return ty;
        }
        let is_literal = matches!(body.exprs[operand], Expr::Literal(_) | Expr::Null);
        if resolved.has_infer() {
            // The type is still open (a generic call's var solves at the
            // fixpoint): defer the decision to `finish`, where the FINAL
            // type is known - bailing here would pass the raw value to
            // the strict-bool branch.
            self.pending_truthy_conditions.push(PendingCondition {
                expr: operand,
                is_literal,
                coerce,
            });
            return ty;
        }
        if let Some(decision) = Self::decide_condition(&resolved) {
            self.apply_condition_decision(operand, resolved, is_literal, coerce, decision);
        }
        ty
    }

    /// Deferred half of `check_condition` (B-1563): conditions whose type
    /// was still open at check time decide here on the finalized
    /// `type_of_expr` entry. Runs in `finish` after type finalization and
    /// before the S17 diagnostic materialization, writing into the SAME
    /// tables the eager path writes (`result` is already taken from
    /// `self.result` at this point in `finish`).
    pub(super) fn decide_deferred_conditions(&mut self, result: &mut InferenceResult<'db>) {
        for pending in std::mem::take(&mut self.pending_truthy_conditions) {
            let PendingCondition {
                expr: condition,
                is_literal,
                coerce,
            } = pending;
            let Some(resolved) = result.type_of_expr.get(&condition).cloned() else {
                continue;
            };
            if resolved.has_error() || resolved.has_infer() {
                continue;
            }
            match Self::decide_condition(&resolved) {
                Some(ConditionDecision::Mismatch) => {
                    result
                        .type_mismatches
                        .entry(condition)
                        .or_insert((Ty::bool(), resolved));
                }
                Some(ConditionDecision::Coerce) => {
                    // A `!` operand (`coerce: false`) records nothing:
                    // `OpCode::Not` coerces itself. It still owes the
                    // warning.
                    if coerce {
                        result.expr_adjustments.insert(
                            condition,
                            Box::new([Adjustment {
                                kind: Adjust::Truthy,
                                target: Ty::bool(),
                            }]),
                        );
                    }
                    self.push_always_const_warning(condition, resolved, is_literal);
                }
                None => {}
            }
        }
    }

    /// The shared judgment: `None` for an already-boolean condition
    /// (including `never`, which produces no value to coerce),
    /// `Mismatch` for `void` (no value exists to test), `Coerce`
    /// otherwise.
    fn decide_condition(resolved: &Ty) -> Option<ConditionDecision> {
        match resolved.kind() {
            InferTy::Bool { .. }
            | InferTy::Literal(Literal::Bool(_), _, _)
            | InferTy::Never { .. } => None,
            InferTy::Void { .. } => Some(ConditionDecision::Mismatch),
            _ => Some(ConditionDecision::Coerce),
        }
    }

    /// Eager-path application of a decision, writing through `self.result`.
    fn apply_condition_decision(
        &mut self,
        condition: ExprId,
        resolved: Ty,
        is_literal: bool,
        coerce: bool,
        decision: ConditionDecision,
    ) {
        match decision {
            ConditionDecision::Mismatch => {
                self.result
                    .type_mismatches
                    .insert(condition, (Ty::bool(), resolved));
            }
            ConditionDecision::Coerce => {
                if coerce {
                    self.result.expr_adjustments.insert(
                        condition,
                        Box::new([Adjustment {
                            kind: Adjust::Truthy,
                            target: Ty::bool(),
                        }]),
                    );
                }
                self.push_always_const_warning(condition, resolved, is_literal);
            }
        }
    }

    /// A statically-decided NON-literal condition is a likely bug
    /// (`if (some_fn)`, `if (instance)`); a written literal is the
    /// author's deliberate constant.
    fn push_always_const_warning(&mut self, condition: ExprId, resolved: Ty, is_literal: bool) {
        if is_literal {
            return;
        }
        let always_true = match truthiness(&resolved) {
            Truthiness::AlwaysTruthy => true,
            Truthiness::AlwaysFalsy => false,
            Truthiness::Runtime => return,
        };
        self.pending_diags
            .push(super::PendingDiag::ConditionAlwaysConst {
                expr: condition,
                ty: resolved,
                always_true,
            });
    }
}

/// A condition or `!` operand whose truthiness decision waits for the
/// inference fixpoint.
pub(crate) struct PendingCondition {
    pub(crate) expr: ExprId,
    /// Written-literal conditions are exempt from the always-constant
    /// warning.
    pub(crate) is_literal: bool,
    /// Branch conditions record `Adjust::Truthy`; `!` operands do not
    /// (`OpCode::Not` coerces itself).
    pub(crate) coerce: bool,
}

/// What a condition position does with its (closed) type.
#[derive(Clone, Copy)]
enum ConditionDecision {
    /// `void`: no value exists to test.
    Mismatch,
    /// Any non-boolean value: record the truthiness coercion.
    Coerce,
}
