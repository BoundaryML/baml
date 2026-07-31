//! The type-system contract: every `baml_compiler2_tir` fact this crate reads.
//!
//! This is deliberately the **only** module in `baml_surface` that imports
//! from `baml_compiler2_tir`. The handle layer's syntactic methods read
//! PPIR/HIR item data directly; everything *resolved* funnels through here.
//!
//! ## The contract
//!
//! The type-system internals are being rebuilt (obligation-based inference,
//! rust-analyzer-style). That rebuild is free to change how these facts are
//! computed — what it must preserve is this query surface: **loc-keyed
//! inputs, `Ty`-shaped outputs**. If a rebuild changes one of these shapes,
//! this file is the complete list of what breaks.
//!
//! | Fact | Query |
//! |---|---|
//! | resolved fn/method signature | `callable::function_signature_ty(db, FunctionLoc)` |
//! | effective (inferred-or-declared) throws | `callable::callable_throws(db, FunctionLoc)` |
//! | resolved class fields | `inference::resolve_class_fields(db, ClassLoc)` |
//! | resolved type-alias RHS | `inference::resolve_type_alias(db, TypeAliasLoc)` |
//! | resolved impl header + members | `interfaces::impl_data(db, ImplLoc)` (+ source map) |
//! | resolved interface fields | `interfaces::resolve_interface_fields(db, InterfaceLoc)` |
//! | resolved interface required methods | `interfaces::resolve_interface_required_methods(db, InterfaceLoc)` |
//! | associated-type default | `interfaces::interface_associated_type_default(db, InterfaceLoc, Name)` |
//! | in-scope generic bounds | `lower_type_expr::{class,interface,impl,function_in_scope}_generic_param_bounds` |
//! | throws `Ty` → leaf set | `throw_inference::flatten_ty_to_facts(&Ty)` (pure) |
//!
//! The output vocabulary (`baml_type::Ty` and friends) is owned by
//! `baml_type` and is not part of the rebuild.

use baml_base::Name;
use baml_compiler2_hir::loc::{ClassLoc, FunctionLoc, ImplLoc, InterfaceLoc, TypeAliasLoc};
// Contract types, re-exported under the surface's name so consumers never
// import `baml_compiler2_tir` themselves.
pub use baml_compiler2_tir::{
    callable::FunctionSignatureTy,
    inference::{ResolvedClassFields, ResolvedTypeAlias},
    interfaces::{ImplData, ResolvedInterfaceFields, ResolvedInterfaceMethod},
    lower_type_expr::TypeVarBoundsMap,
};
use baml_type::{ParamTy, Ty};

use crate::Db;

/// Declaration-site resolved signature of a function or method.
pub(crate) fn function_signature<'db>(
    db: &'db dyn Db,
    func: FunctionLoc<'db>,
) -> &'db FunctionSignatureTy {
    baml_compiler2_tir::callable::function_signature_ty(db, func)
}

/// The effective throws contract — the declared clause when written,
/// otherwise inferred from the body.
pub(crate) fn effective_throws<'db>(db: &'db dyn Db, func: FunctionLoc<'db>) -> &'db Ty {
    baml_compiler2_tir::callable::callable_throws(db, func)
}

/// Decompose a throws `Ty` into its leaf set (unions flattened, literals
/// widened, `never`/`void` dropped).
pub(crate) fn throws_leaves(ty: &Ty) -> Vec<Ty> {
    baml_compiler2_tir::throw_inference::flatten_ty_to_facts(ty)
        .into_iter()
        .collect()
}

/// Resolved class field types.
pub(crate) fn class_fields<'db>(db: &'db dyn Db, class: ClassLoc<'db>) -> &'db ResolvedClassFields {
    baml_compiler2_tir::inference::resolve_class_fields(db, class)
}

/// Resolved type-alias RHS.
pub(crate) fn type_alias_resolved<'db>(
    db: &'db dyn Db,
    alias: TypeAliasLoc<'db>,
) -> &'db ResolvedTypeAlias {
    baml_compiler2_tir::inference::resolve_type_alias(db, alias)
}

/// Resolved interface field types (symbolic `Self` scope).
pub(crate) fn interface_fields<'db>(
    db: &'db dyn Db,
    iface: InterfaceLoc<'db>,
) -> &'db ResolvedInterfaceFields {
    baml_compiler2_tir::interfaces::resolve_interface_fields(db, iface)
}

/// Resolved required-method signatures (symbolic `Self` scope), parallel to
/// `InterfaceData::required_methods`.
pub(crate) fn interface_required_methods<'db>(
    db: &'db dyn Db,
    iface: InterfaceLoc<'db>,
) -> &'db [ResolvedInterfaceMethod] {
    baml_compiler2_tir::interfaces::resolve_interface_required_methods(db, iface)
}

/// An associated type's default, lowered once against the interface's own
/// scope (symbolic `Self`); `None` when the associated type has no default.
pub(crate) fn interface_associated_type_default<'db>(
    db: &'db dyn Db,
    iface: InterfaceLoc<'db>,
    name: Name,
) -> Option<Ty> {
    baml_compiler2_tir::interfaces::interface_associated_type_default(db, iface, name)
        .map(|(ty, _diags)| ty)
}

/// A class's in-scope generic bounds, keyed by parameter.
pub(crate) fn class_generic_bounds<'db>(
    db: &'db dyn Db,
    class: ClassLoc<'db>,
) -> &'db TypeVarBoundsMap {
    baml_compiler2_tir::lower_type_expr::class_generic_param_bounds(db, class)
}

/// A function's in-scope generic bounds (its own parameters plus the
/// enclosing type's), keyed by parameter.
pub(crate) fn function_generic_bounds<'db>(
    db: &'db dyn Db,
    func: FunctionLoc<'db>,
) -> &'db TypeVarBoundsMap {
    baml_compiler2_tir::lower_type_expr::function_in_scope_generic_param_bounds(db, func)
}

/// Resolved impl header and members. `None` when the block is malformed
/// (unresolvable interface target, cyclic header) — broken impls carry
/// diagnostics through the check paths and are omitted from surface listings.
pub(crate) fn impl_data<'db>(db: &'db dyn Db, imp: ImplLoc<'db>) -> Option<&'db ImplData<'db>> {
    baml_compiler2_tir::interfaces::impl_data(db, imp)
        .as_ref()
        .ok()
}

/// A class's declared generic parameters, in order. (Safe for classes: the
/// class env has no interface parent, so the declared and in-scope views
/// coincide.)
pub(crate) fn class_generic_params<'db>(db: &'db dyn Db, class: ClassLoc<'db>) -> Vec<ParamTy> {
    baml_compiler2_tir::class_generic_params(db, class)
}
