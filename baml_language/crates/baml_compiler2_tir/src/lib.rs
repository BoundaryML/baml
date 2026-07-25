//! `baml_compiler2_tir` — Per-scope type inference for the compiler2 pipeline.
//!
//! Provides:
//! - `Ty` — the resolved type representation
//! - `ScopeInference` — per-scope expression type map with optional diagnostics
//! - `infer_scope_types(db, ScopeId)` — per-scope Salsa tracked query
//! - `TypeInferenceBuilder` — walks `ExprBody` within a scope, infers types
//! - `resolve_name_at(db, file, offset, name)` — on-demand name resolution
//! - `resolve_class_fields`, `resolve_type_alias` — per-item structural queries
//!
//! ## Architecture
//!
//! The main query is `infer_scope_types(db, ScopeId) -> ScopeInference`, which
//! returns expression types for a single scope — NOT a monolithic per-function
//! result. This gives fine-grained incrementality: editing a lambda body only
//! recomputes that lambda's `ScopeInference`, not the enclosing function's.

/// Smallest representable BAML `int` (i63): `-2^62`. Mirrors
/// `bex_vm_types::Value::INT_MIN` (`!(i64::MAX >> 1)`), redefined here to keep
/// the type-checker free of a dependency on the VM crate.
pub(crate) const INT_MIN: i64 = !(i64::MAX >> 1);
/// Largest representable BAML `int` (i63): `2^62 - 1`. Mirrors
/// `bex_vm_types::Value::INT_MAX` (`i64::MAX >> 1`).
pub(crate) const INT_MAX: i64 = i64::MAX >> 1;

pub mod analysis;
pub mod builder;
pub mod callable;
pub mod exhaustiveness;
mod generic_env;
pub mod generics;
pub mod infer_context;
pub mod inference;
pub mod interfaces;
pub mod lower_type_expr;
pub mod narrowing;
pub mod normalize;
pub mod package_interface;
pub mod pattern_lowering;
pub mod resolve;
pub mod self_type;
pub mod signature;
pub mod throw_inference;
pub mod throws_analysis;
pub mod ty;
pub mod type_context;
pub mod user_facing;

pub fn class_generic_params(
    db: &dyn Db,
    class: baml_compiler2_hir::loc::ClassLoc<'_>,
) -> Vec<ty::ParamTy> {
    generic_env::class_generic_env(db, class).params().to_vec()
}

pub fn interface_generic_params(
    db: &dyn Db,
    interface: baml_compiler2_hir::loc::InterfaceLoc<'_>,
) -> Vec<ty::ParamTy> {
    generic_env::interface_generic_env(db, interface)
        .params()
        .to_vec()
}

pub fn interface_declared_generic_params(
    db: &dyn Db,
    interface: baml_compiler2_hir::loc::InterfaceLoc<'_>,
) -> Vec<ty::ParamTy> {
    generic_env::interface_declared_params(db, interface)
}

pub fn impl_generic_params(
    db: &dyn Db,
    block: baml_compiler2_hir::loc::ImplLoc<'_>,
) -> Vec<ty::ParamTy> {
    generic_env::impl_generic_env(db, block).params().to_vec()
}

pub fn function_generic_params(
    db: &dyn Db,
    function: baml_compiler2_hir::loc::FunctionLoc<'_>,
) -> Vec<ty::ParamTy> {
    generic_env::function_generic_env(db, function)
        .params()
        .to_vec()
}

// ── Db trait ──────────────────────────────────────────────────────────────────

/// Database trait for `compiler2_tir` queries.
///
/// Extends `baml_compiler2_hir::Db`. Use `infer_scope_types` for type
/// inference queries, `resolve_name_at` for name resolution.
#[salsa::db]
pub trait Db: baml_compiler2_ppir::Db {}
