//! The type-system contract: every `baml_compiler2_hir_ty` fact this crate
//! reads.
//!
//! This is deliberately the **only** module in `baml_surface` that imports
//! from `baml_compiler2_hir_ty`. The handle layer's syntactic methods read
//! PPIR/HIR item data directly; everything *resolved* funnels through here.
//!
//! ## The contract
//!
//! The type provider is free to change how these facts are computed — what it
//! must preserve is this query surface: **loc-keyed inputs, `Ty`-shaped
//! outputs**. If a rework changes one of these shapes, this file is the
//! complete list of what breaks.
//!
//! | Fact | Query |
//! |---|---|
//! | resolved fn/method signature | `callable::function_signature_ty(db, FunctionLoc)` |
//! | effective (inferred-or-declared) throws | `callable::callable_throws(db, FunctionLoc)` |
//! | resolved class fields | `lower::resolve_class_fields(db, ClassLoc)` |
//! | resolved type-alias RHS | `lower::type_alias_value(db, TypeAliasLoc)` |
//! | resolved impl header + members | `interfaces::impl_data(db, ImplLoc)` (+ source map) |
//! | resolved interface fields | `interfaces::resolve_interface_fields(db, InterfaceLoc)` |
//! | resolved interface required methods | `interfaces::resolve_interface_required_methods(db, InterfaceLoc)` |
//! | associated-type default | `interfaces::interface_associated_type_default(db, InterfaceLoc, Name)` |
//! | in-scope generic bounds | `lower::{class,function}_generic_bounds`, `interfaces::interface_declared_param_bounds` |
//! | throws `Ty` → leaf set | `package_interface::flatten_ty_to_facts(&Ty)` (pure) |
//!
//! The output vocabulary (`baml_type::Ty` and friends) is owned by
//! `baml_type` and is not part of any rework.
//!
//! ## Known out-of-contract consumers
//!
//! `baml_project::client_codegen::build_symbol_pool` still performs its own
//! raw lowering walk with codegen's *deliberately* divergent policies (empty
//! bounds maps, alias inlining for non-recursive aliases). It is the one
//! consumer a rework must either keep those entry points alive for, or
//! migrate onto this contract (at which point its policies need re-encoding
//! against the new queries). Everything else that reads resolved types goes
//! through this module.

use baml_base::Name;
use baml_compiler2_hir::loc::{ClassLoc, FunctionLoc, ImplLoc, InterfaceLoc, TypeAliasLoc};
// Contract types, re-exported under the surface's name so consumers never
// import `baml_compiler2_hir_ty` themselves.
pub use baml_compiler2_hir_ty::{
    callable::FunctionSignatureTy,
    interfaces::{ImplData, ResolvedInterfaceFields, ResolvedInterfaceMethod},
};
pub use baml_type::pattern_overlap::TypeVarBoundsMap;
use baml_type::{ParamTy, Ty};

use crate::Db;

/// Declaration-site resolved signature of a function or method.
pub(crate) fn function_signature<'db>(
    db: &'db dyn Db,
    func: FunctionLoc<'db>,
) -> &'db FunctionSignatureTy {
    baml_compiler2_hir_ty::callable::function_signature_ty(db, func)
}

/// The effective throws contract — the declared clause when written,
/// otherwise inferred from the body.
pub(crate) fn effective_throws<'db>(db: &'db dyn Db, func: FunctionLoc<'db>) -> Ty {
    baml_compiler2_hir_ty::callable::callable_throws(db, func).0
}

/// Decompose a throws `Ty` into its leaf set (unions flattened,
/// `never`/`void` dropped).
pub(crate) fn throws_leaves(ty: &Ty) -> Vec<Ty> {
    baml_compiler2_hir_ty::package_interface::flatten_ty_to_facts(ty)
        .into_iter()
        .collect()
}

/// Resolved class field types: `(name, type, field attributes)`, in
/// declaration order.
pub(crate) fn class_fields<'db>(
    db: &'db dyn Db,
    class: ClassLoc<'db>,
) -> &'db [(Name, Ty, Vec<baml_compiler2_hir::item_tree::Attribute>)] {
    baml_compiler2_hir_ty::lower::resolve_class_fields(db, class)
}

/// Resolved type-alias RHS.
pub(crate) fn type_alias_resolved<'db>(db: &'db dyn Db, alias: TypeAliasLoc<'db>) -> Ty {
    baml_compiler2_hir_ty::lower::type_alias_value(db, alias).to_plain()
}

/// Resolved interface field types (symbolic `Self` scope).
pub(crate) fn interface_fields<'db>(
    db: &'db dyn Db,
    iface: InterfaceLoc<'db>,
) -> &'db ResolvedInterfaceFields {
    baml_compiler2_hir_ty::interfaces::resolve_interface_fields(db, iface)
}

/// Resolved required-method signatures (symbolic `Self` scope), parallel to
/// `InterfaceData::required_methods`.
pub(crate) fn interface_required_methods<'db>(
    db: &'db dyn Db,
    iface: InterfaceLoc<'db>,
) -> &'db [ResolvedInterfaceMethod] {
    baml_compiler2_hir_ty::interfaces::resolve_interface_required_methods(db, iface)
}

/// An associated type's default, lowered once against the interface's own
/// scope (symbolic `Self`); `None` when the associated type has no default.
pub(crate) fn interface_associated_type_default<'db>(
    db: &'db dyn Db,
    iface: InterfaceLoc<'db>,
    name: Name,
) -> Option<Ty> {
    baml_compiler2_hir_ty::interfaces::interface_associated_type_default(db, iface, name)
        .map(|(ty, _diags)| ty)
}

/// An interned bounds map (the lowering layer's shape) as the contract's
/// plain conjunction map. Sparse: an unbounded parameter has no entry.
fn plain_bounds(
    interned: impl IntoIterator<Item = (ParamTy, Vec<baml_type::interned::InterfaceRef>)>,
) -> TypeVarBoundsMap {
    interned
        .into_iter()
        .map(|(param, refs)| {
            (
                param,
                refs.iter()
                    .map(|bound| baml_type::Interface {
                        name: bound.name.clone(),
                        generics: bound
                            .generics
                            .iter()
                            .map(baml_type::interned::Ty::to_plain)
                            .collect(),
                        associated_types: bound
                            .associated_types
                            .iter()
                            .map(|(name, t)| (name.clone(), t.to_plain()))
                            .collect(),
                    })
                    .collect(),
            )
        })
        .collect()
}

/// A class's in-scope generic bounds, keyed by parameter.
pub(crate) fn class_generic_bounds<'db>(db: &'db dyn Db, class: ClassLoc<'db>) -> TypeVarBoundsMap {
    plain_bounds(baml_compiler2_hir_ty::lower::class_generic_bounds(
        db, class,
    ))
}

/// A function's in-scope generic bounds (its own parameters plus the
/// enclosing type's), keyed by parameter.
pub(crate) fn function_generic_bounds<'db>(
    db: &'db dyn Db,
    func: FunctionLoc<'db>,
) -> TypeVarBoundsMap {
    plain_bounds(baml_compiler2_hir_ty::lower::function_generic_bounds(
        db, func,
    ))
}

/// Resolved impl header and members. `None` when the block is malformed
/// (unresolvable interface target, cyclic header) — broken impls carry
/// diagnostics through the check paths and are omitted from surface listings.
pub(crate) fn impl_data<'db>(db: &'db dyn Db, imp: ImplLoc<'db>) -> Option<&'db ImplData<'db>> {
    baml_compiler2_hir_ty::interfaces::impl_data(db, imp)
        .as_ref()
        .ok()
}

/// A class's declared generic parameters, in order. (Safe for classes: the
/// class frame has no interface parent, so the declared and in-scope views
/// coincide.)
pub(crate) fn class_generic_params<'db>(db: &'db dyn Db, class: ClassLoc<'db>) -> Vec<ParamTy> {
    baml_compiler2_hir_ty::lower::class_generic_frame(db, class)
}

/// An interface's *declared* generic parameters, in order — the in-scope view
/// would lead with the implicit `Self`, which every interface has and which is
/// therefore not part of what this one declares.
pub(crate) fn interface_generic_params<'db>(
    db: &'db dyn Db,
    iface: InterfaceLoc<'db>,
) -> Vec<ParamTy> {
    baml_compiler2_hir_ty::lower::interface_declared_params(db, iface)
}

/// An interface's declared generic bounds, keyed by parameter.
pub(crate) fn interface_generic_bounds<'db>(
    db: &'db dyn Db,
    iface: InterfaceLoc<'db>,
) -> TypeVarBoundsMap {
    baml_compiler2_hir_ty::interfaces::interface_declared_param_bounds(db, iface)
}
