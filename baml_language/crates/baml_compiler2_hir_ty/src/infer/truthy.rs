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
    interned::{Ty, TyKind},
};

use super::{Adjust, Adjustment, Expectation, InferenceContext};

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
        TyKind::Null { .. } => Truthiness::AlwaysFalsy,
        TyKind::Literal(lit, _, _) => literal_truthiness(lit),
        // Runtime-decided scalars and containers: any of them can hold a
        // falsy value.
        TyKind::Bool { .. }
        | TyKind::Int { .. }
        | TyKind::Bigint { .. }
        | TyKind::Float { .. }
        | TyKind::String { .. }
        | TyKind::Uint8Array { .. }
        | TyKind::List(..)
        | TyKind::Map { .. } => Truthiness::Runtime,
        // Heap values with no falsy inhabitant.
        TyKind::Class(..)
        | TyKind::Interface(..)
        | TyKind::Enum(..)
        | TyKind::EnumVariant(..)
        | TyKind::Media(..)
        | TyKind::Function { .. }
        | TyKind::Future(..)
        | TyKind::Type { .. }
        | TyKind::Resource { .. }
        | TyKind::PromptAst { .. }
        | TyKind::RustType { .. } => Truthiness::AlwaysTruthy,
        TyKind::Union(members, _) => {
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
        TyKind::Unknown { .. }
        | TyKind::TypeVar(..)
        | TyKind::AssociatedTypeProjection { .. }
        | TyKind::TypeAlias(..)
        | TyKind::Never { .. }
        | TyKind::Void { .. }
        | TyKind::Error { .. }
        | TyKind::Infer { .. } => Truthiness::Runtime,
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

impl InferenceContext<'_> {
    /// Type a condition position (`if`/`while`/guard, and `&&`/`||`
    /// operands): any type is accepted, a non-`bool` records the Truthy
    /// adjustment for MIR, `void` is a mismatch (there is no value to
    /// test), and a statically-decided branch warns.
    ///
    /// Inference runs with NO expectation - a `bool` expectation would
    /// wrongly pin type variables the condition is free to leave open
    /// under truthiness.
    pub(super) fn check_condition(&mut self, body: &ExprBody, condition: ExprId) -> Ty {
        let ty = self.infer_expr(body, condition, &Expectation::None);
        let resolved = self.table.resolve_completely(&ty);
        if resolved.has_error() || resolved.has_infer() {
            return ty;
        }
        match resolved.kind() {
            // Already boolean (including literal bools and `never`, which
            // produces no value to coerce): today's exact lowering.
            TyKind::Bool { .. }
            | TyKind::Literal(Literal::Bool(_), _, _)
            | TyKind::Never { .. } => {}
            // No value exists to test.
            TyKind::Void { .. } => {
                self.result
                    .type_mismatches
                    .insert(condition, (Ty::bool(), resolved));
                return ty;
            }
            _ => {
                self.result.expr_adjustments.insert(
                    condition,
                    Box::new([Adjustment {
                        kind: Adjust::Truthy,
                        target: Ty::bool(),
                    }]),
                );
                // A statically-decided NON-literal condition is a likely
                // bug (`if (some_fn)`, `if (instance)`); a written
                // literal is the author's deliberate constant.
                if !matches!(body.exprs[condition], Expr::Literal(_) | Expr::Null) {
                    match truthiness(&resolved) {
                        Truthiness::AlwaysTruthy => {
                            self.pending_diags
                                .push(super::PendingDiag::ConditionAlwaysConst {
                                    expr: condition,
                                    ty: resolved,
                                    always_true: true,
                                });
                        }
                        Truthiness::AlwaysFalsy => {
                            self.pending_diags
                                .push(super::PendingDiag::ConditionAlwaysConst {
                                    expr: condition,
                                    ty: resolved,
                                    always_true: false,
                                });
                        }
                        Truthiness::Runtime => {}
                    }
                }
            }
        }
        ty
    }
}
