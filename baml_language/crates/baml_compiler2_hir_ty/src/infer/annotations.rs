//! Minimal body-position annotation lowering: `ast::TypeExpr` -> interned
//! `Ty`, enough for `let` ascriptions in the S6 corpus.
//!
//! This is deliberately NOT the S4 declaration-lowering layer: no name
//! resolution (paths and projections lower to `Error` until S4), no
//! attribute handling (SAP attrs only originate from stream-type
//! generation), no function types yet. `_` holes become fresh inference
//! variables - the ruling-4 semantics - which is the one thing TIR cannot
//! do in these positions. S4 subsumes this module.

use baml_compiler2_ast::{TypeExpr, TypeExprKind};
use baml_type::{
    Freshness, TyAttr,
    interned::{Ty, TyKind},
};

use crate::infer::unify::InferenceTable;

pub(crate) fn lower_annotation(table: &mut InferenceTable, annotation: &TypeExpr) -> Ty {
    let attr = TyAttr::default;
    match &annotation.kind {
        TypeExprKind::Int { .. } => Ty::int(),
        TypeExprKind::Bigint { .. } => Ty::intern(TyKind::Bigint { attr: attr() }),
        TypeExprKind::Float { .. } => Ty::float(),
        TypeExprKind::String { .. } => Ty::string(),
        TypeExprKind::Bool { .. } => Ty::bool(),
        TypeExprKind::Null { .. } => Ty::null(),
        TypeExprKind::Never { .. } => Ty::never(),
        TypeExprKind::Void { .. } => Ty::void(),
        TypeExprKind::Uint8Array { .. } => Ty::intern(TyKind::Uint8Array { attr: attr() }),
        TypeExprKind::Media { kind, .. } => Ty::intern(TyKind::Media(*kind, attr())),
        TypeExprKind::BuiltinUnknown { .. } => Ty::intern(TyKind::Unknown { attr: attr() }),
        TypeExprKind::Type { .. } => Ty::intern(TyKind::Type { attr: attr() }),
        // `T?` is sugar for `T | null` (TYPE_SYSTEM.md subtyping cases).
        TypeExprKind::Optional { inner, .. } => {
            Ty::union([lower_annotation(table, inner), Ty::null()])
        }
        TypeExprKind::List { inner, .. } => Ty::list(lower_annotation(table, inner)),
        TypeExprKind::Map { key, value, .. } => Ty::intern(TyKind::Map {
            key: lower_annotation(table, key),
            value: lower_annotation(table, value),
            attr: attr(),
        }),
        TypeExprKind::Union { variants, .. } => Ty::union(
            variants
                .iter()
                .map(|variant| lower_annotation(table, variant)),
        ),
        // A literal WRITTEN as a type is regular, not fresh: freshness marks
        // literal expressions that widen at binding sites, and an explicit
        // ascription is exactly the user pinning the literal type.
        TypeExprKind::Literal { value, .. } => {
            Ty::intern(TyKind::Literal(value.clone(), Freshness::Regular, attr()))
        }
        // The `_` hole: a fresh inference variable, filled from context.
        TypeExprKind::Infer { .. } => table.new_var_ty(),
        // Needs name resolution (S4) or unimplemented in S6; the sentinel
        // keeps fixtures running without claiming a diagnostic.
        TypeExprKind::Path { .. }
        | TypeExprKind::AssociatedTypeProjection { .. }
        | TypeExprKind::Function { .. }
        | TypeExprKind::Rust { .. }
        | TypeExprKind::Error { .. }
        | TypeExprKind::Unknown { .. } => Ty::error(),
    }
}
