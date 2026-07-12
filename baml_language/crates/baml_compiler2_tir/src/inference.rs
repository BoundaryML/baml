//! Per-scope type inference — Salsa entry queries.
//!
//! `infer_scope_types(db, ScopeId)` is the main query: it returns
//! `ScopeInference`, which maps `ExprId → Ty` for a single scope.
//!
//! Lambda/closure bodies are separate scopes with their own `infer_scope_types`
//! invocation — editing a lambda body only re-runs that scope's query, not
//! the enclosing function's.
//!
//! Per-item queries (`resolve_class_fields`, `resolve_type_alias`) provide
//! Salsa-cached structural type resolution for class fields and type aliases.

use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

use baml_base::{Name, SourceFile};
use baml_compiler2_ast::{
    self as ast, AstSourceMap, Expr as AstExpr, ExprBody, ExprId, FunctionDef, PatId,
};
use baml_compiler2_hir::{
    body::{FunctionBody, LetBody},
    contributions::Definition,
    loc::{ClassLoc, EnumLoc, FunctionLoc, ImplLoc, InterfaceLoc, LetLoc, TypeAliasLoc},
    package::{PackageId, PackageItems},
    scope::{FileScopeId, ScopeId, ScopeKind},
    semantic_index::{BindingId, BindingKind},
};
use rustc_hash::{FxHashMap, FxHashSet};
use text_size::TextRange;

use crate::{
    builder::TypeInferenceBuilder,
    infer_context::{InferContext, TypeCheckDiagnostics},
    lower_type_expr::TypeVarBoundsMap,
    ty::{FunctionParamTy, Ty, TyAttr},
};

#[derive(Debug, Clone, Default)]
struct GenericEnv {
    params: Vec<Name>,
    bound_param_names: Vec<Name>,
    bound_exprs: Vec<Option<ast::TypeExpr>>,
    /// Parallel to `bound_param_names`/`bound_exprs`: whether this env's scope
    /// *owns* the bound's declaration. Bound-expr diagnostics (unresolved names,
    /// arity, non-interface bounds) are reported only by the owning scope — an
    /// env that merely inherits a parent declaration's bounds (a method env
    /// carrying its class's, a lambda env carrying its function's) still lowers
    /// them for enforcement/projection but stays silent, so each declaration
    /// error is reported exactly once, at its declaration.
    owned_bounds: Vec<bool>,
    /// A parameter pinned to an interface *constraint* (currently only `Self` inside an
    /// interface's own default method). A `baml_type::Interface`, never a `Ty::Interface`
    /// existential — a constraint pins only some associated types.
    concrete_bounds: Vec<(Name, baml_type::Interface)>,
}

impl GenericEnv {
    fn from_params(params: Vec<Name>) -> Self {
        Self {
            params,
            bound_param_names: Vec::new(),
            bound_exprs: Vec::new(),
            owned_bounds: Vec::new(),
            concrete_bounds: Vec::new(),
        }
    }

    /// Prepend an *enclosing* declaration's parameters (a class's onto a method
    /// env, an impl block's onto its method env). Their bounds are inherited —
    /// diagnosed at the enclosing declaration, not re-reported by this env.
    fn prepend_declared(&mut self, params: &[Name], bounds: &[Option<ast::TypeExpr>]) {
        let mut merged_params = params.to_vec();
        merged_params.extend(std::mem::take(&mut self.params));
        self.params = merged_params;

        let mut merged_bound_names = params.to_vec();
        merged_bound_names.extend(std::mem::take(&mut self.bound_param_names));
        self.bound_param_names = merged_bound_names;

        let mut merged_bounds = bounds.to_vec();
        merged_bounds.extend(std::mem::take(&mut self.bound_exprs));
        self.bound_exprs = merged_bounds;

        let mut merged_owned = vec![false; params.len()];
        merged_owned.extend(std::mem::take(&mut self.owned_bounds));
        self.owned_bounds = merged_owned;
    }

    fn add_bounds_for_declared_params(
        &mut self,
        params: &[Name],
        bounds: &[Option<ast::TypeExpr>],
    ) {
        self.bound_param_names.extend(params.iter().cloned());
        self.bound_exprs.extend(
            params
                .iter()
                .enumerate()
                .map(|(idx, _)| bounds.get(idx).cloned().unwrap_or(None)),
        );
        self.owned_bounds.extend(params.iter().map(|_| true));
    }

    fn append_declared(&mut self, params: &[Name], bounds: &[Option<ast::TypeExpr>]) {
        self.params.extend(params.iter().cloned());
        self.add_bounds_for_declared_params(params, bounds);
    }

    fn append_unique_declared(&mut self, params: &[Name], bounds: &[Option<ast::TypeExpr>]) {
        for (idx, param) in params.iter().enumerate() {
            if self.params.contains(param) {
                continue;
            }
            self.params.push(param.clone());
            self.bound_param_names.push(param.clone());
            self.bound_exprs
                .push(bounds.get(idx).cloned().unwrap_or(None));
            self.owned_bounds.push(true);
        }
    }

    /// Demote every bound to inherited: the env is being extended for an inner
    /// scope (a lambda, a required-method signature) that must not re-report the
    /// enclosing declaration's bound diagnostics.
    fn mark_all_bounds_inherited(&mut self) {
        self.owned_bounds.fill(false);
    }

    fn add_concrete_bound(&mut self, param: Name, bound: baml_type::Interface) {
        self.concrete_bounds.push((param, bound));
    }
}

pub(crate) fn inference_owner_scope(
    index: &baml_compiler2_hir::semantic_index::FileSemanticIndex<'_>,
    mut scope_id: FileScopeId,
) -> FileScopeId {
    loop {
        let scope = &index.scopes[scope_id.index() as usize];
        // A synthetic tagged-template body (BEP-049) is a `ScopeKind::Lambda`
        // scope, but it carries no standalone inference — its body is typed
        // inline in the enclosing function/lambda, so bindings declared inside
        // it (e.g. `${for}` loop locals) have their types recorded there, not
        // here. Treat it as transparent so capture resolution finds the real
        // owner; otherwise `binding_type` misses and the capture degrades to
        // `Ty::Unknown` (which then panics at the runtime-lowering boundary).
        if matches!(
            scope.kind,
            ScopeKind::Function | ScopeKind::Let | ScopeKind::Lambda
        ) && !scope.is_template_body
        {
            return scope_id;
        }
        let Some(parent) = scope.parent else {
            return scope_id;
        };
        scope_id = parent;
    }
}

fn enclosing_type_generics(
    item_tree: &baml_compiler2_hir::item_tree::ItemTree,
    type_name: &Name,
) -> Option<(
    crate::infer_context::ShadowedParamOwner,
    Vec<Name>,
    Vec<Option<ast::TypeExpr>>,
)> {
    for class_data in item_tree.classes.values() {
        if class_data.name == *type_name {
            return Some((
                crate::infer_context::ShadowedParamOwner::Class,
                class_data.generic_params.clone(),
                class_data.generic_param_bounds.clone(),
            ));
        }
    }

    for iface_data in item_tree.interfaces.values() {
        if iface_data.name == *type_name {
            // Associated types are NOT type-level parameters: a bare associated-type
            // name (`Item`) is illegal and must be written `Self.Item`, so it never
            // resolves as an in-scope type variable. Only the interface's declared
            // generics are.
            return Some((
                crate::infer_context::ShadowedParamOwner::Interface,
                iface_data.generic_params.clone(),
                iface_data.generic_param_bounds.clone(),
            ));
        }
    }

    None
}

/// Every associated-type name the interface named `qtn` declares — its own plus
/// each one transitively inherited through `requires`. Empty if `qtn` does not
/// resolve to an interface. Used to recognise a bare associated-type reference
/// (illegal: it must be written `Self.<name>`) so lowering can suggest the fix.
pub(crate) fn interface_associated_type_names_for_qtn(
    db: &dyn crate::Db,
    qtn: &crate::ty::QualifiedTypeName,
) -> FxHashSet<Name> {
    let pkg_id = PackageId::new(db, qtn.package().clone());
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
    let Some(Definition::Interface(iface_loc)) = pkg_items.lookup_type(qtn.namespace(), qtn.name())
    else {
        return FxHashSet::default();
    };
    let iface_pkg = baml_compiler2_hir::file_package::file_package(db, iface_loc.file(db));
    let iface_pkg_id = PackageId::new(db, iface_pkg.package.clone());
    let iface_pkg_items = baml_compiler2_ppir::package_items(db, iface_pkg_id);

    let mut names: FxHashSet<Name> = FxHashSet::default();
    let item_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
    if let Some(iface_data) = item_tree.interfaces.get(&iface_loc.id(db)) {
        names.extend(iface_data.associated_types.iter().map(|a| a.name.clone()));
    }
    names.extend(inherited_interface_associated_type_names(
        db,
        iface_loc,
        iface_pkg_items,
        &iface_pkg.namespace_path,
    ));
    names
}

fn inherited_interface_associated_type_names(
    db: &dyn crate::Db,
    iface_loc: InterfaceLoc<'_>,
    pkg_items: &PackageItems<'_>,
    ns_context: &[Name],
) -> Vec<Name> {
    type InterfaceVisitKey = (
        SourceFile,
        baml_compiler2_hir::ids::LocalItemId<baml_compiler2_hir::ids::InterfaceMarker>,
    );

    fn walk(
        db: &dyn crate::Db,
        iface_loc: InterfaceLoc<'_>,
        pkg_items: &PackageItems<'_>,
        ns_context: &[Name],
        seen: &mut FxHashSet<InterfaceVisitKey>,
        out: &mut Vec<Name>,
    ) {
        if !seen.insert((iface_loc.file(db), iface_loc.id(db))) {
            return;
        }
        let item_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
        let Some(iface_data) = item_tree.interfaces.get(&iface_loc.id(db)) else {
            return;
        };
        for required in &iface_data.requires {
            let Some(required_loc) =
                crate::interfaces::resolve_path_to_interface(db, required, pkg_items, ns_context)
            else {
                continue;
            };
            let required_tree = baml_compiler2_hir::file_item_tree(db, required_loc.file(db));
            if let Some(required_iface) = required_tree.interfaces.get(&required_loc.id(db)) {
                out.extend(
                    required_iface
                        .associated_types
                        .iter()
                        .map(|assoc| assoc.name.clone()),
                );
            }
            let required_pkg =
                baml_compiler2_hir::file_package::file_package(db, required_loc.file(db));
            let required_pkg_id = PackageId::new(db, required_pkg.package.clone());
            let required_pkg_items = baml_compiler2_ppir::package_items(db, required_pkg_id);
            walk(
                db,
                required_loc,
                required_pkg_items,
                &required_pkg.namespace_path,
                seen,
                out,
            );
        }
    }

    let mut seen = FxHashSet::default();
    let mut out = Vec::new();
    walk(db, iface_loc, pkg_items, ns_context, &mut seen, &mut out);
    out
}

fn type_bindings_for_params(params: &[Name]) -> FxHashMap<Name, Ty> {
    params
        .iter()
        .map(|param| (param.clone(), Ty::TypeVar(param.clone(), TyAttr::default())))
        .collect()
}

/// Lower one generic parameter's `extends` bound expression to its `Ty`, in the
/// declaration's own scope with the sibling parameters (`params`) in scope as
/// rigid type variables, and the env's *concrete* constraints (e.g. `Self`'s
/// interface inside a default method) visible so a projection bound
/// (`U extends Self.Item`) resolves through them. Sibling *declared* bounds are
/// not threaded (that would be order-dependent/circular); shared by the
/// enforcement table and [`env_interface_bounds`].
#[expect(clippy::too_many_arguments)]
fn lower_env_generic_bound(
    db: &dyn crate::Db,
    pkg_items: &PackageItems<'_>,
    ns_context: &[Name],
    params: &[Name],
    concrete_bounds: &TypeVarBoundsMap,
    self_ty: Option<&Ty>,
    bound_te: &ast::TypeExpr,
    diags: &mut Vec<crate::infer_context::TirTypeError>,
) -> Ty {
    crate::lower_type_expr::lower_type_expr(
        bound_te,
        &crate::lower_type_expr::ScopeCtx {
            db,
            package_items: pkg_items,
            ns_context,
            generic_params: params,
            bounds: concrete_bounds,
            self_ty: self_ty.cloned(),
        },
        diags,
    )
}

/// The lowering view of a [`GenericEnv`]'s concrete constraints: the bounds map
/// (each concrete constraint as a single-conjunct entry) and, when the env
/// carries a `Self` constraint, the symbolic `Self` type — so a declared bound
/// mentioning `Self.Item` lowers inside the same scope its enforcement runs in.
fn env_concrete_lowering_scope(env: &GenericEnv) -> (TypeVarBoundsMap, Option<Ty>) {
    let bounds: TypeVarBoundsMap = env
        .concrete_bounds
        .iter()
        .map(|(name, constraint)| (name.clone(), vec![constraint.clone()]))
        .collect();
    let self_ty = bounds
        .contains_key(&Name::new("Self"))
        .then(crate::self_type::self_type_for_interface_default);
    (bounds, self_ty)
}

/// A [`GenericEnv`]'s interface-constraint bounds, for resolving a `T.member`
/// projection in a type expression lowered against it — the env's lowered
/// `extends` bounds (interface ones only) plus its `concrete_bounds` (e.g. the
/// `Self` constraint inside an interface default). The projection view of the
/// same env that [`install_generic_param_bounds`] installs as the `Ty`-typed
/// enforcement table; bound-lowering diagnostics are the enforcement path's to
/// report, so they are discarded here.
fn env_interface_bounds(
    db: &dyn crate::Db,
    pkg_items: &PackageItems<'_>,
    ns_context: &[Name],
    env: &GenericEnv,
) -> TypeVarBoundsMap {
    let mut bounds = TypeVarBoundsMap::default();
    let (concrete_lowering_bounds, lowering_self_ty) = env_concrete_lowering_scope(env);
    for (name, bound) in env.bound_param_names.iter().zip(env.bound_exprs.iter()) {
        let Some(bound_te) = bound else {
            continue;
        };
        let mut diags = Vec::new();
        let bound_ty = lower_env_generic_bound(
            db,
            pkg_items,
            ns_context,
            &env.params,
            &concrete_lowering_bounds,
            lowering_self_ty.as_ref(),
            bound_te,
            &mut diags,
        );
        if let Some(constraint) = bound_ty.as_interface() {
            // Last-wins, matching `install_generic_param_bounds`'s `insert`: a name repeated
            // across `bound_param_names` is a shadowing artifact (an inner-scope parameter
            // reusing an outer name), not an intersection conjunction, so the inner bound
            // replaces the outer one. Keeping both would make this map disagree with the
            // enforcement table it is meant to mirror.
            bounds.insert(name.clone(), vec![constraint]);
        }
    }
    for (name, constraint) in &env.concrete_bounds {
        bounds.insert(name.clone(), vec![constraint.clone()]);
    }
    bounds
}

/// Report each generic parameter declared more than once in a single declaration
/// list (`<T, T>`). A name reused across *nested* scopes is not a duplicate but a
/// shadow, reported as `TypeParamShadowed` at the inner declaration instead.
fn report_duplicate_generic_params(
    builder: &TypeInferenceBuilder<'_>,
    params: &[Name],
    span: TextRange,
) {
    for (idx, param) in params.iter().enumerate() {
        if params[..idx].contains(param) {
            builder.report_at_span(
                crate::infer_context::TirTypeError::DuplicateGenericParam {
                    name: param.clone(),
                },
                span,
            );
        }
    }
}

/// The number of generic parameters the interface named `qtn` declares, or `None`
/// if `qtn` does not resolve to an interface.
pub(crate) fn interface_declared_generic_arity(
    db: &dyn crate::Db,
    qtn: &crate::ty::QualifiedTypeName,
) -> Option<usize> {
    let pkg_id = baml_compiler2_hir::package::PackageId::new(db, qtn.package().clone());
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
    let baml_compiler2_hir::contributions::Definition::Interface(loc) =
        pkg_items.lookup_type(qtn.namespace(), qtn.name())?
    else {
        return None;
    };
    let tree = baml_compiler2_ppir::file_item_tree(db, loc.file(db));
    Some(tree.interfaces.get(&loc.id(db))?.generic_params.len())
}

/// Lower one declared interface bound — from a generic parameter's or an
/// associated type's `extends` clause — to its interface constraint(s), in
/// `params` scope. A bound *is* an interface conjunction; the intermediate `Ty`
/// is used only to classify it for diagnostics. When `report` is true (the owning
/// declaration's scope), emits its lowering diagnostics plus a bare-generic arity
/// error (`extends Outer` where `interface Outer<X>` — a bound cannot infer the
/// missing argument, unlike a value position where the bare form is a wildcard)
/// and a non-interface bound error (a sibling type variable or an associated-type
/// projection); an inherited bound passes `false`, since the owner already
/// reported them.
#[expect(clippy::too_many_arguments)]
fn lower_declared_interface_bound(
    db: &dyn crate::Db,
    builder: &mut TypeInferenceBuilder<'_>,
    pkg_items: &PackageItems<'_>,
    ns_context: &[Name],
    params: &[Name],
    bounds: &crate::lower_type_expr::TypeVarBoundsMap,
    self_ty: Option<&Ty>,
    bound_te: &ast::TypeExpr,
    span: TextRange,
    report: bool,
) -> Box<[baml_type::Interface]> {
    // `Self` and the scope's bounds are threaded so a projection bound
    // (`extends Self.Item`, `extends T.Item`) resolves to an `AssociatedTypeProjection`
    // — which is then correctly rejected as a non-interface bound below, rather than
    // failing to resolve and masquerading as an "unresolved type".
    let mut diags = Vec::new();
    let bound_ty = crate::lower_type_expr::lower_type_expr(
        bound_te,
        &crate::lower_type_expr::ScopeCtx {
            db,
            package_items: pkg_items,
            ns_context,
            generic_params: params,
            bounds,
            self_ty: self_ty.cloned(),
        },
        &mut diags,
    );
    if report {
        for diag in diags {
            builder.report_at_span(diag, span);
        }
        match &bound_ty {
            Ty::Interface(qtn, generics, assoc, _) => {
                if generics.is_empty()
                    && let Some(arity) = interface_declared_generic_arity(db, qtn)
                    && arity > 0
                {
                    builder.report_at_span(
                        crate::infer_context::TirTypeError::WrongNumberOfTypeArgs {
                            type_name: qtn.name().clone(),
                            expected: arity,
                            got: 0,
                        },
                        span,
                    );
                }
                // An explicit associated binding written on the bound (`P extends
                // Parser<Output = V>`) must implement that assoc's own declared bound
                // (`type Output extends Named`) — the same implements relation the
                // impl-side binding check enforces. Only *written* bindings: a default
                // is the interface's own obligation, checked at its declaration; a
                // symbolic value resolves at instantiation and fails open. Cycle-safe:
                // scope inference runs strictly downstream of `impl_data`.
                if let baml_compiler2_ast::TypeExprKind::Path {
                    associated_type_bindings,
                    ..
                } = &bound_te.kind
                {
                    let head =
                        baml_type::Interface::new(qtn.clone(), generics.clone(), assoc.clone());
                    for written in associated_type_bindings {
                        let Some((_, value)) = assoc.iter().find(|(n, _)| *n == written.name)
                        else {
                            // Unknown binding name — lowering reported it already.
                            continue;
                        };
                        if crate::generics::contains_typevar(value) {
                            continue;
                        }
                        let normalized = baml_type::normalize::normalize(value, &*builder);
                        for declared in
                            crate::builder::associated_projection::associated_type_declared_bound(
                                db,
                                &head,
                                &written.name,
                            )
                        {
                            if !crate::interfaces::normalized_arg_implements_bound(
                                &*builder,
                                &normalized,
                                &declared,
                            ) {
                                builder.report_at_span(
                                    crate::infer_context::TirTypeError::AssociatedTypeBindingViolatesBound {
                                        interface: qtn.clone(),
                                        name: written.name.clone(),
                                        binding: value.clone(),
                                        bound: declared,
                                    },
                                    span,
                                );
                            }
                        }
                    }
                }
            }
            // Already diagnosed by lowering the bound expression itself — a second
            // "not an interface" here would be redundant.
            Ty::Unknown { .. } | Ty::Error { .. } | Ty::BuiltinUnknown { .. } => {}
            // Mirrors the impl-bound check in `lower_generic_param_interface_bounds`.
            other => builder.report_at_span(
                crate::infer_context::TirTypeError::GenericBoundNotInterface {
                    bound: other.clone(),
                },
                span,
            ),
        }
    }
    bound_ty.as_interface().into_iter().collect()
}

fn install_generic_param_bounds(
    db: &dyn crate::Db,
    builder: &mut TypeInferenceBuilder<'_>,
    pkg_items: &PackageItems<'_>,
    ns_context: &[Name],
    env: &GenericEnv,
    span: TextRange,
) {
    let mut bounds = crate::lower_type_expr::TypeVarBoundsMap::default();
    let (concrete_lowering_bounds, lowering_self_ty) = env_concrete_lowering_scope(env);
    debug_assert_eq!(env.bound_param_names.len(), env.owned_bounds.len());
    for ((name, bound), owned) in env
        .bound_param_names
        .iter()
        .zip(env.bound_exprs.iter())
        .zip(env.owned_bounds.iter().copied())
    {
        let Some(bound_te) = bound else {
            continue;
        };
        // Inherited bounds (an enclosing declaration's) are lowered for the
        // enforcement table but their diagnostics belong to — and were already
        // reported by — the owning declaration's scope. The env's *concrete*
        // constraints (e.g. `Self`'s interface inside a default method) are
        // visible so `U extends Self.Item` resolves; sibling declared bounds
        // are not threaded (that would be order-dependent).
        let constraint = lower_declared_interface_bound(
            db,
            builder,
            pkg_items,
            ns_context,
            &env.params,
            &concrete_lowering_bounds,
            lowering_self_ty.as_ref(),
            bound_te,
            span,
            owned,
        );
        bounds.insert(name.clone(), constraint.into_vec());
    }
    bounds.extend(
        env.concrete_bounds
            .iter()
            .map(|(name, constraint)| (name.clone(), vec![constraint.clone()])),
    );
    builder.set_generic_param_bounds(bounds);
}

fn apply_generic_env(
    db: &dyn crate::Db,
    builder: &mut TypeInferenceBuilder<'_>,
    pkg_items: &PackageItems<'_>,
    ns_context: &[Name],
    env: &GenericEnv,
    span: TextRange,
) {
    builder.set_generic_params(env.params.clone());
    install_generic_param_bounds(db, builder, pkg_items, ns_context, env, span);
}

#[derive(Clone, Copy)]
struct GenericLookupContext<'a, 'db> {
    index: &'a baml_compiler2_hir::semantic_index::FileSemanticIndex<'db>,
    item_tree: &'a baml_compiler2_hir::item_tree::ItemTree,
}

/// The enclosing class/interface declaration whose type-level parameters a method's
/// signature env extends — with the declaration kind for shadowing diagnostics.
struct ParentTypeGenerics {
    type_name: Name,
    owner: crate::infer_context::ShadowedParamOwner,
    params: Vec<Name>,
    bounds: Vec<Option<ast::TypeExpr>>,
}

fn parent_type_generic_env(
    ctx: GenericLookupContext<'_, '_>,
    parent_scope_id: Option<FileScopeId>,
) -> Option<ParentTypeGenerics> {
    let parent = &ctx.index.scopes[parent_scope_id?.index() as usize];
    if !matches!(parent.kind, ScopeKind::Class) {
        return None;
    }
    let type_name = parent.name.clone()?;
    let (owner, params, bounds) = enclosing_type_generics(ctx.item_tree, &type_name)?;
    Some(ParentTypeGenerics {
        type_name,
        owner,
        params,
        bounds,
    })
}

fn prepend_parent_type_generics(
    ctx: GenericLookupContext<'_, '_>,
    env: &mut GenericEnv,
    parent_scope_id: Option<FileScopeId>,
) -> Option<Name> {
    let parent = parent_type_generic_env(ctx, parent_scope_id)?;
    env.prepend_declared(&parent.params, &parent.bounds);
    Some(parent.type_name)
}

/// An out-of-body impl block's declared generics as `(param names, each's single optional
/// bound)`. The legacy flat form the generic env consumes; the `ImplBlock` holds the full
/// `&`-bound set per param, of which only the first is used here (matching the old single-bound
/// representation). Empty for an in-body (`InClass`) block.
fn free_impl_generics(
    block: &baml_compiler2_hir::item_tree::ImplBlock,
) -> (Vec<Name>, Vec<Option<baml_compiler2_ast::TypeExpr>>) {
    match &block.subject {
        baml_compiler2_hir::item_tree::ImplSubject::Free { generics, .. } => (
            generics.iter().map(|g| g.name.clone()).collect(),
            generics.iter().map(|g| g.bounds.first().cloned()).collect(),
        ),
        baml_compiler2_hir::item_tree::ImplSubject::InClass { .. } => {
            debug_assert!(false, "Should only be called for a free impl");
            (Vec::new(), Vec::new())
        }
    }
}

/// The out-of-body `implement … for …` block whose method list contains `func_data`, as its
/// declared generics, if any. A method is identified by its source span (unique per declaration).
/// Reads the unified `impls` store via the `free_impls` index.
fn enclosing_impl_generics_for_func(
    item_tree: &baml_compiler2_hir::item_tree::ItemTree,
    func_data: &baml_compiler2_hir::item_tree::Function,
) -> Option<(Vec<Name>, Vec<Option<baml_compiler2_ast::TypeExpr>>)> {
    item_tree.free_impls.iter().find_map(|impl_id| {
        let block = item_tree.impls.get(impl_id)?;
        block
            .methods
            .iter()
            .any(|&mid| item_tree[mid].span == func_data.span)
            .then(|| free_impl_generics(block))
    })
}

fn generic_env_for_function_data(
    ctx: GenericLookupContext<'_, '_>,
    function_scope: &baml_compiler2_hir::scope::Scope,
    func_data: &baml_compiler2_hir::item_tree::Function,
) -> GenericEnv {
    let mut env = GenericEnv::from_params(func_data.generic_params.clone());
    env.add_bounds_for_declared_params(&func_data.generic_params, &func_data.generic_param_bounds);
    // A method in a generic `implements<T …> Iface for Target` block sees the
    // block's type params (e.g. `T` in `implements<T extends Comparable>
    // Sortable for T[]`), which live on the impl block rather than a parent
    // scope. Mirror the `ScopeKind::Function` arm: impl generics take the place
    // of parent-type generics so a nested lambda body can resolve them too.
    if let Some((generic_params, generic_param_bounds)) =
        enclosing_impl_generics_for_func(ctx.item_tree, func_data)
    {
        env.prepend_declared(&generic_params, &generic_param_bounds);
    } else {
        prepend_parent_type_generics(ctx, &mut env, function_scope.parent);
    }
    env
}

fn enclosing_function_generic_env_from_let(
    ctx: GenericLookupContext<'_, '_>,
    let_scope: &baml_compiler2_hir::scope::Scope,
) -> Option<GenericEnv> {
    let mut current = let_scope.parent;
    while let Some(fsi) = current {
        let scope = &ctx.index.scopes[fsi.index() as usize];
        match scope.kind {
            ScopeKind::Function => {
                let func_data =
                    ctx.item_tree.functions.values().find(|fd| {
                        fd.span == scope.range && scope.name.as_ref() == Some(&fd.name)
                    })?;
                return Some(generic_env_for_function_data(ctx, scope, func_data));
            }
            ScopeKind::Let => current = scope.parent,
            _ => current = scope.parent,
        }
    }
    None
}

struct TypeExprLoweringOptions {
    span: TextRange,
    self_ty: Option<Ty>,
}

#[expect(clippy::too_many_arguments)]
fn lower_type_expr_at_span_in_env(
    db: &dyn crate::Db,
    builder: &mut TypeInferenceBuilder<'_>,
    pkg_items: &PackageItems<'_>,
    ns_context: &[Name],
    env: &GenericEnv,
    env_bounds: &crate::lower_type_expr::TypeVarBoundsMap,
    type_expr: &ast::TypeExpr,
    options: TypeExprLoweringOptions,
) -> Ty {
    // The env's params are the in-scope type variables; `Self` resolves through `self_ty`,
    // and the env's interface bounds (`env_bounds`, computed once per env via
    // `env_interface_bounds`) let a `T.member` / `Self.member` projection resolve.
    let ctx = crate::lower_type_expr::ScopeCtx {
        db,
        package_items: pkg_items,
        ns_context,
        generic_params: &env.params,
        bounds: env_bounds,
        self_ty: options.self_ty,
    };
    let mut diags = Vec::new();
    let ty = crate::lower_type_expr::lower_type_expr(type_expr, &ctx, &mut diags);
    for diag in diags {
        builder.report_at_span(diag, options.span);
    }
    builder.validate_type_generic_bounds_at_span(options.span, &ty);
    ty
}

#[expect(clippy::too_many_arguments)]
fn validate_spanned_type_expr_generic_bounds(
    db: &dyn crate::Db,
    builder: &mut TypeInferenceBuilder<'_>,
    pkg_items: &PackageItems<'_>,
    ns_context: &[Name],
    env: &GenericEnv,
    env_bounds: &crate::lower_type_expr::TypeVarBoundsMap,
    type_expr: &ast::TypeExpr,
    self_ty: Option<Ty>,
) {
    lower_type_expr_at_span_in_env(
        db,
        builder,
        pkg_items,
        ns_context,
        env,
        env_bounds,
        type_expr,
        TypeExprLoweringOptions {
            span: type_expr.span,
            self_ty,
        },
    );
}

fn extend_env_with_lambda_generics(mut env: GenericEnv, func_def: &FunctionDef) -> GenericEnv {
    // The enclosing scope's bounds were already diagnosed there; the lambda's
    // scope reports only its own.
    env.mark_all_bounds_inherited();
    env.append_unique_declared(&func_def.generic_params, &func_def.generic_param_bounds);
    env
}

fn add_lambda_params_to_builder(
    db: &dyn crate::Db,
    builder: &mut TypeInferenceBuilder<'_>,
    pkg_items: &PackageItems<'_>,
    ns_context: &[Name],
    env: &GenericEnv,
    func_def: &FunctionDef,
    contextual_param_tys: Option<&[FunctionParamTy]>,
) {
    // The lambda's own generic bounds (its env extends the enclosing scope's) let a
    // `T.member` projection in a parameter type resolve `T`'s declaring interface.
    let bounds = env_interface_bounds(db, pkg_items, ns_context, env);
    for (i, param) in func_def.params.iter().enumerate() {
        let param_ty = param
            .type_expr
            .as_ref()
            // The parent scope already lowers and validates the lambda
            // function type. Here we only need local parameter types for the
            // lambda body; reporting again would duplicate diagnostics in the
            // child lambda scope.
            .map(|ste| {
                let mut diags = Vec::new();
                crate::lower_type_expr::lower_type_expr(
                    ste,
                    &crate::lower_type_expr::ScopeCtx {
                        db,
                        package_items: pkg_items,
                        ns_context,
                        generic_params: &env.params,
                        bounds: &bounds,
                        self_ty: None,
                    },
                    &mut diags,
                )
            })
            .or_else(|| {
                contextual_param_tys
                    .and_then(|pts| pts.get(i))
                    .map(|param| param.ty.clone())
            })
            .unwrap_or(Ty::Unknown {
                attr: TyAttr::default(),
            });
        builder.add_local(param.name.clone(), param_ty.clone());
        builder.param_types.push((param.name.clone(), param_ty));
    }
}

// ── Member Resolution ─────────────────────────────────────────────────────

/// Records what a field-access expression resolved to during type inference.
///
/// Stored per-ExprId alongside the `Ty`, so MIR can emit the correct
/// `Constant::Function(QualifiedName)` without re-doing resolution, and so
/// LSP can navigate to the definition of the accessed member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemberResolution<'db> {
    /// A class field access (e.g. `p.name`).
    Field {
        class_loc: ClassLoc<'db>,
        field_name: Name,
    },
    /// An enum variant access (e.g. `Status.Active`).
    Variant {
        enum_loc: EnumLoc<'db>,
        variant_name: Name,
    },
    /// A free item accessed via a package/namespace path.
    /// e.g. `baml.env.get` → package=`baml`, namespace=[`env`], name=`get`
    Free { func_loc: FunctionLoc<'db> },
    /// A bound method reference: root is a value (local variable or field chain).
    /// e.g. `p.get_name` where `p` is a local — type has `self` stripped.
    BoundMethod {
        class_loc: ClassLoc<'db>,
        func_loc: FunctionLoc<'db>,
    },
    /// An unbound method reference: root is a type name.
    /// e.g. `Person.get_name` where `Person` is a class type — type keeps `self`.
    UnboundMethod {
        class_loc: ClassLoc<'db>,
        func_loc: FunctionLoc<'db>,
    },
    /// A **virtual** interface-method call: the receiver's concrete type is unknown — an
    /// interface-existential value (`named.describe()`) or a `T extends I` type variable —
    /// so dispatch resolves to the receiver's runtime impl. Only the *slot* is known
    /// statically — the interface and the method name — so no `FunctionLoc`: there is no
    /// statically-known body (the interface's default is just one possible target, and a
    /// required method has none). Recorded for every virtual call, required and default
    /// alike; the contract (signature / generics / throws) for type-checking is the
    /// interface's declaration of `method`. Contrast
    /// [`MemberResolution::InterfaceConcreteMethod`], where the impl — and thus the called
    /// body — is statically known.
    InterfaceVirtualMethod {
        iface_loc: InterfaceLoc<'db>,
        method: Name,
    },
    /// A **concrete** interface-method call: the receiver's concrete type is known, so the
    /// `impl` block is resolved statically (`foo.describe()` on a class implementing the
    /// interface). `func_loc` is the impl's override, or — when the impl inherits it — the
    /// interface's default body; `impl_loc` identifies the impl (and recovers the interface,
    /// the implementor, and the impl's bindings via `impl_data`).
    InterfaceConcreteMethod {
        impl_loc: ImplLoc<'db>,
        func_loc: FunctionLoc<'db>,
    },
    /// A **virtual** interface-field access (`named.field` on an interface-existential, or
    /// the projected `obj.as<I>.field`): the concrete type is unknown, so the field is read
    /// through the interface. A *concrete* receiver's interface field instead resolves to
    /// the linked class field it backs ([`MemberResolution::Field`]), so only the virtual
    /// case needs its own variant.
    InterfaceVirtualField {
        iface_loc: InterfaceLoc<'db>,
        field: Name,
    },
}

// ── Per-Scope Inference Result ─────────────────────────────────────────────

/// Per-scope type inference result.
///
/// Each scope (function body, lambda, class method, block) gets its own
/// `ScopeInference` cached independently by Salsa. This is the Ty-style
/// decomposed approach — NOT a monolithic per-function struct.
///
/// Modeled after Ty's `ScopeInference<'db>` (`infer.rs:557-563`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeInference<'db> {
    /// Type of every expression within this scope (NOT nested child scopes).
    expressions: FxHashMap<ExprId, Ty>,
    /// Pattern types: the type each pattern is associated with. Used both for
    /// `Pattern::Bind` (the variable's bound type, post widening) and for
    /// `Pattern::Type` / `Pattern::Class` (the type to runtime-test against).
    pattern_types: FxHashMap<PatId, Ty>,
    /// Member resolutions: for field-access expressions that resolved to a
    /// class field, enum variant, method, or free function — records the
    /// structural path so MIR can emit the correct `QualifiedName` and LSP
    /// can navigate to the definition.
    resolutions: FxHashMap<ExprId, MemberResolution<'db>>,
    /// Residual throw facts for each catch expression after its arms have been
    /// applied. This lets downstream throw-surface queries reuse the same catch
    /// semantics as the main type-checking builder instead of over-approximating.
    catch_residual_throws: FxHashMap<ExprId, BTreeSet<Ty>>,
    /// Match expressions that the exhaustiveness checker determined cover all cases.
    exhaustive_matches: FxHashSet<ExprId>,
    /// TIR-inferred root segment type for each multi-segment `Path` expression.
    /// Populated in `infer_path` so that MIR can chain field projections even
    /// when the MIR local was declared with a coarser type (e.g. catch variables
    /// are declared as `BuiltinUnknown` by `lower_catch` before `bind_pattern`
    /// has a chance to refine them).
    path_root_types: FxHashMap<ExprId, Ty>,
    /// TIR-inferred type of every prefix `segments[..=i]` for multi-segment
    /// local-rooted `Path` expressions. Index `0` mirrors `path_root_types`;
    /// later indices are produced by chaining `resolve_member` over each
    /// segment. MIR uses this to thread receiver-prefix class type-args
    /// through method-call paths of depth ≥ 3 (e.g. `holder.box.describe()`).
    path_segment_types: FxHashMap<(ExprId, usize), Ty>,
    /// Per-segment member resolutions for multi-segment local-rooted `Path` expressions.
    ///
    /// For `obj.a.b` (`Path(["obj", "a", "b"])`), contains resolutions for segments
    /// [1..] i.e., "a" (index 0) and "b" (index 1). Parallel to `segments[1..]`.
    ///
    /// Used by MIR to emit chained `Place::Field` projections and by LSP to
    /// navigate to field definitions from within multi-segment paths.
    path_member_resolutions: FxHashMap<ExprId, Vec<MemberResolution<'db>>>,
    /// Lambda span → `Ty::Function` for every lambda expression encountered
    /// during inline body inference (including nested lambdas). Allows nested
    /// lambda scopes to look up their contextual param types without calling
    /// `infer_scope_types` on intermediate Lambda ancestors (which would cycle).
    nested_lambda_types: FxHashMap<FileScopeId, Ty>,
    /// Synthetic tagged-template body Lambda scope → its tag's body-lambda
    /// params (BEP-049 §10). A real lambda nested in the interpolations looks
    /// these up (via this owning Function/Let scope) to seed params that have no
    /// HIR binding — see the `ScopeKind::Lambda` arm of `infer_scope_types`.
    template_body_params: FxHashMap<FileScopeId, Vec<FunctionParamTy>>,
    /// Lambda/function parameter types by index (name, inferred type).
    /// Populated for lambda scopes so LSP can resolve unannotated lambda
    /// parameter types (e.g. `items.map((item) -> { item. })`).
    param_types: Vec<(Name, Ty)>,
    /// Full parameter binding plan for checked calls.
    call_plans: FxHashMap<ExprId, CallPlan>,
    /// Generic instantiation for checked calls whose callee declares type
    /// params, in declared De Bruijn order ([class params...] ++ [fn
    /// params...]). Values may contain the *caller's* rigid `TypeVar`s
    /// (generic→generic calls); MIR lowers those to `TypeArgRef` templates
    /// resolved against the caller's `frame.type_args` at runtime.
    call_type_instantiations: FxHashMap<ExprId, Vec<Ty>>,
    /// Function value adapters required after structural function subtyping.
    ///
    /// Optional parameters are matched by name in TIR types, but runtime calls
    /// are positional and exact-arity. MIR uses this metadata to synthesize a
    /// wrapper that drops/reorders optional parameters before the VM sees the
    /// call.
    function_coercions: FxHashMap<ExprId, FunctionCoercion>,
    /// Expression metadata produced while checking parameter defaults.
    ///
    /// Defaults live in a separate AST arena from the function body, so their
    /// `ExprId`s and `PatId`s are not safe to merge into the normal per-scope
    /// maps above.
    parameter_defaults: DefaultParameterInference<'db>,
    /// Diagnostics and other rare data. Heap-allocated only when non-empty.
    extra: Option<Box<ScopeInferenceExtra<'db>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultParameterInference<'db> {
    pub(crate) expressions: FxHashMap<ExprId, Ty>,
    pub(crate) pattern_types: FxHashMap<PatId, Ty>,
    pub(crate) resolutions: FxHashMap<ExprId, MemberResolution<'db>>,
    pub(crate) catch_residual_throws: FxHashMap<ExprId, BTreeSet<Ty>>,
    pub(crate) exhaustive_matches: FxHashSet<ExprId>,
    pub(crate) path_root_types: FxHashMap<ExprId, Ty>,
    pub(crate) path_segment_types: FxHashMap<(ExprId, usize), Ty>,
    pub(crate) path_member_resolutions: FxHashMap<ExprId, Vec<MemberResolution<'db>>>,
    pub(crate) call_plans: FxHashMap<ExprId, CallPlan>,
    pub(crate) call_type_instantiations: FxHashMap<ExprId, Vec<Ty>>,
    pub(crate) function_coercions: FxHashMap<ExprId, FunctionCoercion>,
}

impl DefaultParameterInference<'_> {
    pub(crate) fn empty() -> Self {
        Self {
            expressions: FxHashMap::default(),
            pattern_types: FxHashMap::default(),
            resolutions: FxHashMap::default(),
            catch_residual_throws: FxHashMap::default(),
            exhaustive_matches: FxHashSet::default(),
            path_root_types: FxHashMap::default(),
            path_segment_types: FxHashMap::default(),
            path_member_resolutions: FxHashMap::default(),
            call_plans: FxHashMap::default(),
            call_type_instantiations: FxHashMap::default(),
            function_coercions: FxHashMap::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallPlan {
    pub bindings: Vec<ParamBinding>,
    pub type_args: Vec<Ty>,
}

impl CallPlan {
    pub fn provided_arg_count(&self) -> usize {
        self.bindings
            .iter()
            .filter(|binding| matches!(binding, ParamBinding::Provided { .. }))
            .count()
    }

    pub fn provided_args(&self) -> impl Iterator<Item = ExprId> + '_ {
        self.bindings.iter().filter_map(|binding| match binding {
            ParamBinding::Provided { arg, .. } => Some(*arg),
            ParamBinding::OmittedDefault { .. } => None,
        })
    }

    pub fn provided_param_args(&self) -> impl Iterator<Item = (usize, ExprId)> + '_ {
        self.bindings.iter().filter_map(|binding| match binding {
            ParamBinding::Provided { param_index, arg } => Some((*param_index, *arg)),
            ParamBinding::OmittedDefault { .. } => None,
        })
    }

    pub fn provided_arg_for_param(&self, param_index: usize) -> Option<ExprId> {
        self.bindings.iter().find_map(|binding| match binding {
            ParamBinding::Provided {
                param_index: binding_param_index,
                arg,
            } if *binding_param_index == param_index => Some(*arg),
            ParamBinding::Provided { .. } | ParamBinding::OmittedDefault { .. } => None,
        })
    }

    pub fn matches_provided_args(&self, args: &[ExprId]) -> bool {
        self.provided_arg_count() == args.len()
            && args
                .iter()
                .all(|arg| self.provided_args().any(|provided| provided == *arg))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamBinding {
    Provided {
        param_index: usize,
        arg: ExprId,
    },
    OmittedDefault {
        param_index: usize,
        param_name: Name,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionCoercion {
    pub source_params: Vec<FunctionParamTy>,
    pub target_params: Vec<FunctionParamTy>,
    pub target_return: Ty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeInferenceExtra<'db> {
    pub diagnostics: TypeCheckDiagnostics<'db>,
}

// Safety: `ScopeInference<'db>` contains `ExprId` (arena indices) and `Ty`
// (which contains `Name`, a Salsa-interned type). The `FxHashMap` doesn't
// implement `salsa::Update` automatically; we provide the impl manually.
#[allow(unsafe_code)]
unsafe impl salsa::Update for ScopeInference<'_> {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        #[allow(unsafe_code)]
        let old = unsafe { &*old_pointer };
        if old == &new_value {
            false
        } else {
            #[allow(unsafe_code)]
            unsafe {
                std::ptr::drop_in_place(old_pointer);
                std::ptr::write(old_pointer, new_value);
            }
            true
        }
    }
}

impl<'db> ScopeInference<'db> {
    /// Look up the type of an expression in this scope.
    pub fn expression_type(&self, expr_id: ExprId) -> Option<&Ty> {
        self.expressions.get(&expr_id)
    }

    /// Look up the `Ty::Function` type assigned to a nested lambda by its span.
    /// Used by nested Lambda scopes to get contextual param types without
    /// calling `infer_scope_types` on intermediate Lambda ancestors.
    pub fn nested_lambda_type(&self, fsi: FileScopeId) -> Option<&Ty> {
        self.nested_lambda_types.get(&fsi)
    }

    /// Look up the tag's body-lambda params recorded for a synthetic
    /// tagged-template body scope (`is_template_body`). Used by a nested lambda's
    /// standalone scope inference to seed params that have no HIR binding.
    pub fn template_body_params(&self, fsi: FileScopeId) -> Option<&[FunctionParamTy]> {
        self.template_body_params.get(&fsi).map(Vec::as_slice)
    }

    /// Look up the binding type for a pattern (the type the variable is bound to,
    /// which may differ from the initializer expression type due to widening).
    pub fn binding_type(&self, pat_id: PatId) -> Option<&Ty> {
        self.pattern_types.get(&pat_id)
    }

    /// Look up the type of a parameter by index.
    pub fn param_type(&self, param_idx: usize) -> Option<&Ty> {
        self.param_types.get(param_idx).map(|(_, ty)| ty)
    }

    /// Look up the full argument binding plan for a call expression.
    pub fn call_plan(&self, expr_id: ExprId) -> Option<&CallPlan> {
        self.call_plans.get(&expr_id)
    }

    pub fn call_plan_for_provided_args(&self, args: &[ExprId]) -> Option<&CallPlan> {
        self.call_plans
            .values()
            .find(|plan| plan.matches_provided_args(args))
    }

    /// Iterate over all call binding plans in this scope.
    pub fn iter_call_plans(&self) -> impl Iterator<Item = (&ExprId, &CallPlan)> {
        self.call_plans.iter()
    }

    /// Iterate over the generic instantiations recorded for checked calls
    /// (callee's declared type params, in De Bruijn order).
    pub fn iter_call_type_instantiations(&self) -> impl Iterator<Item = (&ExprId, &Vec<Ty>)> {
        self.call_type_instantiations.iter()
    }

    /// Iterate over all function adapters required by checked coercions.
    pub fn iter_function_coercions(&self) -> impl Iterator<Item = (&ExprId, &FunctionCoercion)> {
        self.function_coercions.iter()
    }

    /// Look up the function adapter required for a coerced expression in this scope.
    pub fn function_coercion(&self, expr_id: ExprId) -> Option<&FunctionCoercion> {
        self.function_coercions.get(&expr_id)
    }

    /// Iterate over all default-parameter expression types for this scope.
    pub fn iter_default_expressions(&self) -> impl Iterator<Item = (&ExprId, &Ty)> {
        self.parameter_defaults.expressions.iter()
    }

    /// Iterate over all default-parameter pattern types for this scope.
    pub fn iter_default_bindings(&self) -> impl Iterator<Item = (&PatId, &Ty)> {
        self.parameter_defaults.pattern_types.iter()
    }

    /// Iterate over all default-parameter member resolutions for this scope.
    pub fn iter_default_resolutions(
        &self,
    ) -> impl Iterator<Item = (&ExprId, &MemberResolution<'db>)> {
        self.parameter_defaults.resolutions.iter()
    }

    /// Iterate over all exhaustive default-parameter match expressions.
    pub fn iter_default_exhaustive_matches(&self) -> impl Iterator<Item = &ExprId> {
        self.parameter_defaults.exhaustive_matches.iter()
    }

    /// Iterate over all default-parameter path root types.
    pub fn iter_default_path_root_types(&self) -> impl Iterator<Item = (&ExprId, &Ty)> {
        self.parameter_defaults.path_root_types.iter()
    }

    /// Iterate over all default-parameter path prefix types.
    pub fn iter_default_path_segment_types(&self) -> impl Iterator<Item = (&(ExprId, usize), &Ty)> {
        self.parameter_defaults.path_segment_types.iter()
    }

    /// Iterate over all default-parameter per-segment path member resolutions.
    pub fn iter_default_path_member_resolutions(
        &self,
    ) -> impl Iterator<Item = (&ExprId, &Vec<MemberResolution<'db>>)> {
        self.parameter_defaults.path_member_resolutions.iter()
    }

    /// Iterate over all default-parameter call binding plans.
    pub fn iter_default_call_plans(&self) -> impl Iterator<Item = (&ExprId, &CallPlan)> {
        self.parameter_defaults.call_plans.iter()
    }

    /// Iterate over all default-parameter call generic instantiations.
    pub fn iter_default_call_type_instantiations(
        &self,
    ) -> impl Iterator<Item = (&ExprId, &Vec<Ty>)> {
        self.parameter_defaults.call_type_instantiations.iter()
    }

    /// Iterate over all default-parameter function adapters.
    pub fn iter_default_function_coercions(
        &self,
    ) -> impl Iterator<Item = (&ExprId, &FunctionCoercion)> {
        self.parameter_defaults.function_coercions.iter()
    }

    // ── Parameter-default point lookups ────────────────────────────────────────
    // Mirror the body-scope accessors above, but read the per-scope
    // default-parameter inference sub-result (a default's expressions live in a
    // separate metadata scope). Keep `parameter_defaults` encapsulated: consumers
    // look up by id here rather than reaching into the sub-struct.

    /// Look up a default-parameter expression's type.
    pub fn default_expression_type(&self, expr_id: ExprId) -> Option<&Ty> {
        self.parameter_defaults.expressions.get(&expr_id)
    }

    /// Look up a default-parameter pattern binding's type.
    pub fn default_binding_type(&self, pat_id: PatId) -> Option<&Ty> {
        self.parameter_defaults.pattern_types.get(&pat_id)
    }

    /// Look up a default-parameter expression's member resolution.
    pub fn default_resolution(&self, expr_id: ExprId) -> Option<&MemberResolution<'db>> {
        self.parameter_defaults.resolutions.get(&expr_id)
    }

    /// Whether a default-parameter match expression was determined exhaustive.
    pub fn default_is_exhaustive_match(&self, expr_id: ExprId) -> bool {
        self.parameter_defaults
            .exhaustive_matches
            .contains(&expr_id)
    }

    /// Look up a default-parameter path's root segment type.
    pub fn default_path_root_type(&self, expr_id: ExprId) -> Option<&Ty> {
        self.parameter_defaults.path_root_types.get(&expr_id)
    }

    /// Look up a default-parameter path's `segments[..=seg_idx]` type.
    pub fn default_path_segment_type(&self, expr_id: ExprId, seg_idx: usize) -> Option<&Ty> {
        self.parameter_defaults
            .path_segment_types
            .get(&(expr_id, seg_idx))
    }

    /// Look up a default-parameter path's per-segment member resolutions.
    pub fn default_path_member_resolution(
        &self,
        expr_id: ExprId,
    ) -> Option<&[MemberResolution<'db>]> {
        self.parameter_defaults
            .path_member_resolutions
            .get(&expr_id)
            .map(Vec::as_slice)
    }

    /// Look up a default-parameter call's argument binding plan.
    pub fn default_call_plan(&self, expr_id: ExprId) -> Option<&CallPlan> {
        self.parameter_defaults.call_plans.get(&expr_id)
    }

    /// Look up a default-parameter expression's function adapter.
    pub fn default_function_coercion(&self, expr_id: ExprId) -> Option<&FunctionCoercion> {
        self.parameter_defaults.function_coercions.get(&expr_id)
    }

    /// Iterate over all (`ExprId`, Ty) pairs for expressions in this scope.
    pub fn iter_expressions(&self) -> impl Iterator<Item = (&ExprId, &Ty)> {
        self.expressions.iter()
    }

    /// Iterate over all (`PatId`, Ty) pairs for pattern bindings in this scope.
    pub fn iter_bindings(&self) -> impl Iterator<Item = (&PatId, &Ty)> {
        self.pattern_types.iter()
    }

    /// Look up the member resolution for an expression in this scope.
    pub fn resolution(&self, expr_id: ExprId) -> Option<&MemberResolution<'db>> {
        self.resolutions.get(&expr_id)
    }

    /// Look up residual throw facts for a catch expression after handled arms
    /// have been removed.
    pub fn catch_residual_throws(&self, expr_id: ExprId) -> Option<&BTreeSet<Ty>> {
        self.catch_residual_throws.get(&expr_id)
    }

    /// Iterate over all (`ExprId`, `MemberResolution`) pairs for this scope.
    pub fn iter_resolutions(&self) -> impl Iterator<Item = (&ExprId, &MemberResolution<'db>)> {
        self.resolutions.iter()
    }

    /// Check whether a match expression was determined to be exhaustive by TIR.
    pub fn is_exhaustive_match(&self, expr_id: ExprId) -> bool {
        self.exhaustive_matches.contains(&expr_id)
    }

    /// Iterate over all exhaustive match `ExprIds` in this scope.
    pub fn iter_exhaustive_matches(&self) -> impl Iterator<Item = &ExprId> {
        self.exhaustive_matches.iter()
    }

    /// Look up the TIR-inferred root segment type for a multi-segment Path expression.
    pub fn path_root_type(&self, expr_id: ExprId) -> Option<&Ty> {
        self.path_root_types.get(&expr_id)
    }

    /// Iterate over all (`ExprId`, root `Ty`) pairs for multi-segment paths in this scope.
    pub fn iter_path_root_types(&self) -> impl Iterator<Item = (&ExprId, &Ty)> {
        self.path_root_types.iter()
    }

    /// Look up the type of `segments[..=seg_idx]` for a multi-segment
    /// local-rooted `Path` expression. Index `0` mirrors `path_root_type`.
    pub fn path_segment_type(&self, expr_id: ExprId, seg_idx: usize) -> Option<&Ty> {
        self.path_segment_types.get(&(expr_id, seg_idx))
    }

    /// Iterate over all `((ExprId, seg_idx), Ty)` entries for multi-segment
    /// local-rooted paths in this scope.
    pub fn iter_path_segment_types(&self) -> impl Iterator<Item = (&(ExprId, usize), &Ty)> {
        self.path_segment_types.iter()
    }

    /// Look up per-segment member resolutions for a multi-segment local-rooted
    /// `Path` expression. Returns `None` if not recorded (e.g. package-rooted
    /// paths or paths with only a single segment).
    pub fn path_member_resolution(&self, expr_id: ExprId) -> Option<&[MemberResolution<'db>]> {
        self.path_member_resolutions
            .get(&expr_id)
            .map(Vec::as_slice)
    }

    /// Iterate over all (`ExprId`, per-segment resolutions) for multi-segment
    /// local-rooted paths in this scope.
    pub fn iter_path_member_resolutions(
        &self,
    ) -> impl Iterator<Item = (&ExprId, &Vec<MemberResolution<'db>>)> {
        self.path_member_resolutions.iter()
    }

    /// Get diagnostics for this scope (empty slice if none).
    pub fn diagnostics(&self) -> &TypeCheckDiagnostics<'db> {
        self.extra
            .as_ref()
            .map(|e| &e.diagnostics)
            .unwrap_or_else(|| {
                // Use a static empty diagnostics — safe since TypeCheckDiagnostics
                // with no diagnostics is logically equivalent to the default.
                static EMPTY: std::sync::OnceLock<TypeCheckDiagnostics<'static>> =
                    std::sync::OnceLock::new();
                // SAFETY: we return a reference with lifetime tied to 'db.
                // The static EMPTY has no 'db-tied data (empty Vec).
                #[allow(unsafe_code)]
                unsafe {
                    let empty = EMPTY.get_or_init(TypeCheckDiagnostics::default);
                    // Extend the lifetime — safe because the data is empty and 'static.
                    &*std::ptr::from_ref::<TypeCheckDiagnostics<'static>>(empty)
                        .cast::<TypeCheckDiagnostics<'db>>()
                }
            })
    }
}

// ── Main Salsa Query: Per-Scope Inference ───────────────────────────────────

/// Seed a nested lambda's builder with the tag's body-lambda params of any
/// `is_template_body` ancestor scope (BEP-049 §10). Those params are injected
/// into the owning scope's locals while typing the tagged-template body, but
/// have no HIR binding — so the capture seeding above can't reach them, and a
/// real lambda nested in the interpolations would otherwise report them as
/// "unresolved name" (and leave them `Unknown`, which MIR forbids).
/// `owner_inference` is the enclosing Function/Let scope inference, which
/// recorded them keyed by the template-body scope.
fn seed_template_body_params(
    builder: &mut TypeInferenceBuilder<'_>,
    index: &baml_compiler2_hir::semantic_index::FileSemanticIndex<'_>,
    lambda_fsi: FileScopeId,
    owner_inference: &ScopeInference<'_>,
) {
    for anc_fsi in index.ancestor_scopes(lambda_fsi) {
        if index.scopes[anc_fsi.index() as usize].is_template_body
            && let Some(params) = owner_inference.template_body_params(anc_fsi)
        {
            for p in params {
                if let Some(name) = &p.name {
                    builder.add_local(name.clone(), p.ty.clone());
                }
            }
        }
    }
}

/// Search for a `Lambda` expression whose source span matches `target_span` in
/// `body`/`source_map`, recursively descending into nested lambda bodies.
///
/// Returns `Some((func_def, lambda_body, lambda_source_map, lambda_expr_id))` when
/// found; `None` otherwise.
fn find_lambda_by_span<'a>(
    body: &'a ExprBody,
    source_map: &AstSourceMap,
    target_span: TextRange,
) -> Option<(&'a FunctionDef, &'a ExprBody, &'a AstSourceMap, ExprId)> {
    for (expr_id, expr) in body.exprs.iter() {
        if let AstExpr::Lambda(ref func_def) = *expr {
            let span = source_map.expr_span(expr_id);
            if span == target_span {
                // Found the matching lambda
                if let Some(baml_compiler2_ast::FunctionBodyDef::Expr(
                    ref lambda_body,
                    ref lambda_sm,
                )) = func_def.body
                {
                    return Some((func_def, lambda_body, lambda_sm, expr_id));
                }
            }
            // Recurse into nested lambda bodies
            if let Some(baml_compiler2_ast::FunctionBodyDef::Expr(ref nested_body, ref nested_sm)) =
                func_def.body
            {
                if let Some(found) = find_lambda_by_span(nested_body, nested_sm, target_span) {
                    return Some(found);
                }
            }
        }
    }
    None
}

/// Per-scope type inference — the primary Salsa query for type checking.
///
/// Returns expression types for a single scope. Lambda/closure bodies are
/// separate scopes with their own query invocation.
///
/// Keyed by `ScopeId<'db>` (tracked: `File + FileScopeId`), so Salsa caches
/// independently per scope. Editing lambda A does NOT invalidate the enclosing
/// function's `ScopeInference`.
fn infer_scope_types_cycle_initial<'db>(
    _db: &'db dyn crate::Db,
    _id: salsa::Id,
    _scope_id: ScopeId<'db>,
) -> ScopeInference<'db> {
    ScopeInference {
        expressions: FxHashMap::default(),
        pattern_types: FxHashMap::default(),
        resolutions: FxHashMap::default(),
        catch_residual_throws: FxHashMap::default(),
        exhaustive_matches: FxHashSet::default(),
        path_root_types: FxHashMap::default(),
        path_segment_types: FxHashMap::default(),
        path_member_resolutions: FxHashMap::default(),
        nested_lambda_types: FxHashMap::default(),
        template_body_params: FxHashMap::default(),
        param_types: Vec::new(),
        call_plans: FxHashMap::default(),
        call_type_instantiations: FxHashMap::default(),
        function_coercions: FxHashMap::default(),
        parameter_defaults: DefaultParameterInference::empty(),
        extra: None,
    }
}

#[salsa::tracked(returns(ref), cycle_initial=infer_scope_types_cycle_initial)]
pub fn infer_scope_types<'db>(
    db: &'db dyn crate::Db,
    scope_id: ScopeId<'db>,
) -> ScopeInference<'db> {
    let file = scope_id.file(db);
    let file_scope = scope_id.file_scope_id(db);
    let index = baml_compiler2_ppir::file_semantic_index(db, file);
    let scope = &index.scopes[file_scope.index() as usize];

    // Get package items for cross-file resolution
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package.clone());
    let res_ctx = crate::package_interface::package_resolution_context(db, pkg_id);
    let pkg_items = &res_ctx.own_items;

    let aliases = package_alias_map(db, res_ctx);
    let context = InferContext::new(db, scope_id);
    let mut builder = TypeInferenceBuilder::new(context, res_ctx, pkg_id, scope_id, aliases);

    // Dispatch based on scope kind
    match &scope.kind {
        ScopeKind::Function => {
            // Find the function by matching scope range AND name against item_tree functions.
            // Both checks are required to disambiguate companion functions that
            // share the parent's span.
            let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
            let mut found = false;
            for (local_id, func_data) in &item_tree.functions {
                if func_data.span == scope.range && scope.name.as_ref() == Some(&func_data.name) {
                    let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(db, file, *local_id);
                    let body = baml_compiler2_ppir::function_body(db, func_loc);
                    let sig = baml_compiler2_ppir::elaborated_function_signature(db, func_loc);

                    let enclosing_impl = item_tree.free_impls.iter().find_map(|impl_id| {
                        let block = item_tree.impls.get(impl_id)?;
                        block.methods.contains(local_id).then_some(block)
                    });

                    let mut env = GenericEnv::from_params(sig.user_generic_params.clone());
                    env.params
                        .extend(sig.synthetic_effect_params.iter().cloned());
                    report_duplicate_generic_params(
                        &builder,
                        &sig.user_generic_params,
                        func_data.span,
                    );
                    if let Some(imp) = enclosing_impl {
                        let (impl_generic_params, impl_generic_bounds) = free_impl_generics(imp);
                        for mp in &sig.user_generic_params {
                            if impl_generic_params.iter().any(|cp| cp == mp) {
                                builder.report_at_span(
                                    crate::infer_context::TirTypeError::TypeParamShadowedImplParam {
                                        param_name: mp.clone(),
                                    },
                                    func_data.span,
                                );
                            }
                        }
                        env.prepend_declared(&impl_generic_params, &impl_generic_bounds);
                    } else if let Some(parent) = parent_type_generic_env(
                        GenericLookupContext {
                            index,
                            item_tree: &item_tree,
                        },
                        scope.parent,
                    ) {
                        for mp in &sig.user_generic_params {
                            if parent.params.iter().any(|cp| cp == mp) {
                                builder.report_at_span(
                                    crate::infer_context::TirTypeError::TypeParamShadowed {
                                        param_name: mp.clone(),
                                        type_name: parent.type_name.clone(),
                                        owner: parent.owner,
                                    },
                                    func_data.span,
                                );
                            }
                        }
                        env.prepend_declared(&parent.params, &parent.bounds);
                    }
                    // BEP-044 (Self-as-type-variable): inside an interface's own
                    // method, `self` is a `Self` type variable bound by the
                    // interface instead of the interface existential. Register it
                    // in the shared generic env so the normal bound-aware member
                    // resolution path handles `Self`.
                    let interface_self_bound: Option<baml_type::Interface> = if enclosing_impl
                        .is_none()
                    {
                        scope.parent.and_then(|parent_idx| {
                            let parent = &index.scopes[parent_idx.index() as usize];
                            if !matches!(parent.kind, ScopeKind::Class) {
                                return None;
                            }
                            let cn = parent.name.as_ref()?;
                            let def = pkg_items.lookup_type(&pkg_info.namespace_path, cn)?;
                            if !matches!(
                                def,
                                baml_compiler2_hir::contributions::Definition::Interface(_)
                            ) {
                                return None;
                            }
                            let qtn = crate::lower_type_expr::qualify_def(db, def, cn);
                            let iface = item_tree.interfaces.values().find(|i| &i.name == cn)?;
                            let args = iface
                                .generic_params
                                .iter()
                                .map(|p| Ty::TypeVar(p.clone(), TyAttr::default()))
                                .collect();
                            // `Self`'s bound is the interface as a *constraint* (a
                            // `baml_type::Interface`, not a `Ty::Interface` existential): it pins
                            // none of its associated types — they are `Self`'s own, abstract in a
                            // default body, so `Self.Item` stays symbolic. Empty bindings, never
                            // `Ty::Error` placeholders which would wrongly pin `Item = Error`.
                            Some(baml_type::Interface::new(qtn, args, Vec::new()))
                        })
                    } else {
                        None
                    };
                    if let Some(bound) = interface_self_bound.clone() {
                        let self_param = Name::new("Self");
                        if !env.params.iter().any(|p| p == &self_param) {
                            env.params.push(self_param.clone());
                        }
                        env.add_concrete_bound(self_param, bound);
                    }
                    env.add_bounds_for_declared_params(
                        &func_data.generic_params,
                        &func_data.generic_param_bounds,
                    );
                    apply_generic_env(
                        db,
                        &mut builder,
                        pkg_items,
                        &pkg_info.namespace_path,
                        &env,
                        func_data.span,
                    );
                    if let Some(sm) = baml_compiler2_ppir::function_body_source_map(db, func_loc) {
                        builder.set_body_source_map(sm);
                    }
                    builder.set_auto_derived(matches!(
                        func_data.origin,
                        ast::FunctionOrigin::AutoDerive
                    ));
                    // BEP-044: if this function lives inside an
                    // `implements I { ... }` block, attach `I`'s QTN so
                    // `default.<method>(...)` resolves against I's
                    // contract.
                    if let Some(target) = item_tree.method_to_iface_target.get(local_id)
                        && let baml_compiler2_ast::TypeExprKind::Path { segments, .. } =
                            &target.kind
                        && let Some((head, name)) = segments
                            .split_last()
                            .map(|(last, head)| (head, last.clone()))
                    {
                        let lookup_ns: &[Name] = if head.is_empty() {
                            &pkg_info.namespace_path
                        } else {
                            head
                        };
                        if let Some(def) = pkg_items.lookup_type(lookup_ns, &name)
                            && let baml_compiler2_hir::contributions::Definition::Interface(_) = def
                        {
                            let qtn = crate::lower_type_expr::qualify_def(db, def, &name);
                            builder.set_implements_block_interface(Some(qtn));
                        }
                    }

                    if let FunctionBody::Expr(expr_body) = body.as_ref() {
                        // The method's in-scope interface bounds (its own params, the
                        // enclosing class/interface's, and `Self`'s constraint) so a
                        // `T.member` / `Self.member` projection in the receiver pattern,
                        // interface arguments, or a binding value resolves nominally.
                        let env_bounds =
                            env_interface_bounds(db, pkg_items, &pkg_info.namespace_path, &env);
                        // Determine enclosing class name for `self` parameter
                        // resolution and BEP-044 `Self`-type substitution.
                        let enclosing_class_name: Option<Name> =
                            scope.parent.and_then(|parent_idx| {
                                let parent = &index.scopes[parent_idx.index() as usize];
                                if matches!(parent.kind, ScopeKind::Class) {
                                    parent.name.clone()
                                } else {
                                    None
                                }
                            });
                        // `Self`'s type for this body — resolved through the lowering context
                        // below, never a bare-name substitution: the rigid `Self` type variable
                        // (interface's own default method), the impl's receiver pattern, or the
                        // enclosing class's full receiver type (`Foo<T>`, carrying its generics).
                        let self_ty: Option<Ty> = if interface_self_bound.is_some() {
                            Some(crate::self_type::self_type_for_interface_default())
                        } else if let Some(imp) = enclosing_impl
                            && let baml_compiler2_hir::item_tree::ImplSubject::Free {
                                for_target,
                                ..
                            } = &imp.subject
                        {
                            let mut diags = Vec::new();
                            Some(crate::lower_type_expr::lower_type_expr(
                                for_target,
                                &crate::lower_type_expr::ScopeCtx {
                                    db,
                                    package_items: pkg_items,
                                    ns_context: &pkg_info.namespace_path,
                                    generic_params: &env.params,
                                    bounds: &env_bounds,
                                    self_ty: None,
                                },
                                &mut diags,
                            ))
                        } else {
                            enclosing_class_name.as_ref().and_then(|cn| {
                                let baml_compiler2_hir::contributions::Definition::Class(class_loc) =
                                    pkg_items.lookup_type(&pkg_info.namespace_path, cn)?
                                else {
                                    return None;
                                };
                                let class_file = class_loc.file(db);
                                let class_pkg =
                                    baml_compiler2_hir::file_package::file_package(db, class_file);
                                let class_tree = baml_compiler2_hir::file_item_tree(db, class_file);
                                let class_data = class_tree.classes.get(&class_loc.id(db))?;
                                Some(crate::self_type::self_type_for_class(
                                    class_data,
                                    &class_pkg.namespace_path,
                                    class_pkg.package.clone(),
                                ))
                            })
                        };
                        // The interface bound `Self.Assoc` projects through, as a constraint
                        // (interface's own default method); `None` when `Self` is concrete.
                        let self_bound: Option<baml_type::Interface> = interface_self_bound;

                        let sig_sm = baml_compiler2_ppir::elaborated_function_signature_source_map(
                            db, func_loc,
                        );
                        let mut type_bindings: FxHashMap<Name, Ty> =
                            type_bindings_for_params(&env.params);
                        if let Some(target) = item_tree.method_to_iface_target.get(local_id)
                            && let Some(iface_loc) = crate::interfaces::resolve_path_to_interface(
                                db,
                                target,
                                pkg_items,
                                &pkg_info.namespace_path,
                            )
                        {
                            let iface_file = iface_loc.file(db);
                            let iface_tree = baml_compiler2_hir::file_item_tree(db, iface_file);
                            if let Some(iface_data) = iface_tree.interfaces.get(&iface_loc.id(db)) {
                                let mut iface_type_bindings = type_bindings.clone();
                                if let baml_compiler2_ast::TypeExprKind::Path {
                                    generic_args, ..
                                } = &target.kind
                                {
                                    for (param, arg) in
                                        iface_data.generic_params.iter().zip(generic_args)
                                    {
                                        let mut arg_diags = Vec::new();
                                        let ty = {
                                            let generic_params: Vec<_> =
                                                type_bindings.keys().cloned().collect();
                                            crate::generics::substitute_ty(
                                                &crate::lower_type_expr::lower_type_expr(
                                                    arg,
                                                    &crate::lower_type_expr::ScopeCtx {
                                                        db,
                                                        package_items: pkg_items,
                                                        ns_context: &pkg_info.namespace_path,
                                                        generic_params: &generic_params,
                                                        bounds: &env_bounds,
                                                        self_ty: None,
                                                    },
                                                    &mut arg_diags,
                                                ),
                                                &type_bindings,
                                            )
                                        };
                                        for diag in arg_diags {
                                            builder.report_at_span(diag, target.span);
                                        }
                                        iface_type_bindings.insert(param.clone(), ty.clone());
                                        type_bindings.entry(param.clone()).or_insert(ty);
                                    }
                                }
                                let explicit_bindings = item_tree
                                    .method_to_iface_associated_type_bindings
                                    .get(local_id)
                                    .cloned()
                                    .unwrap_or_default();
                                for assoc in &iface_data.associated_types {
                                    if let Some(binding) =
                                        explicit_bindings.iter().find(|b| b.name == assoc.name)
                                        && let Some(te) = &binding.type_expr
                                    {
                                        // Lower the binding's type through a context that resolves
                                        // `Self`, then substitute the in-scope generics /
                                        // associated types accumulated so far.
                                        let binding_generic_params: Vec<Name> =
                                            type_bindings.keys().cloned().collect();
                                        let binding_ctx = crate::lower_type_expr::ScopeCtx {
                                            db,
                                            package_items: pkg_items,
                                            ns_context: &pkg_info.namespace_path,
                                            generic_params: &binding_generic_params,
                                            bounds: &env_bounds,
                                            self_ty: self_ty.clone(),
                                        };
                                        let mut binding_diags = Vec::new();
                                        let ty = crate::generics::substitute_ty(
                                            &crate::lower_type_expr::lower_type_expr(
                                                te,
                                                &binding_ctx,
                                                &mut binding_diags,
                                            ),
                                            &type_bindings,
                                        );
                                        for diag in binding_diags {
                                            builder.report_at_span(diag, te.span);
                                        }
                                        // Into `iface_type_bindings` only — the binding realizes
                                        // later defaults, but the bare name is NOT in scope
                                        // (banned: the method must write `Self.Item`).
                                        iface_type_bindings.insert(assoc.name.clone(), ty);
                                        continue;
                                    }
                                    if let Some((default_ty, _diags)) =
                                        crate::interfaces::interface_associated_type_default(
                                            db,
                                            iface_loc,
                                            assoc.name.clone(),
                                        )
                                    {
                                        // The default is lowered once (symbolic `Self`) by the
                                        // shared query; substitute the generics / associated
                                        // types accumulated so far. `Self` stays symbolic here
                                        // (an interface method body's receiver is rigid), so a
                                        // Self-referencing default resolves to `(Self as I).X`.
                                        // Diagnostics surface at the interface declaration.
                                        // Into `iface_type_bindings` only (see the explicit-
                                        // binding arm above): bare names are banned.
                                        let ty = crate::generics::substitute_ty(
                                            &default_ty,
                                            &iface_type_bindings,
                                        );
                                        iface_type_bindings.insert(assoc.name.clone(), ty);
                                    }
                                }
                            }
                        }
                        // Inside an interface's own default body, associated types are
                        // deliberately NOT registered as bare names: a bare `Item` is
                        // banned everywhere — the body writes `Self.Item`, which lowers
                        // through the `Self` bound installed below.
                        builder.set_type_bindings(type_bindings.clone());
                        // Lower body type annotations through the shared context: `Self` and
                        // `Self.Assoc` resolve via `self_ty` and the `Self` bound; interface /
                        // method generics and associated types then substitute in. A bare
                        // associated name is an in-scope type variable (it is a `type_bindings`
                        // key) substituted to its symbolic projection, matching `Self.Assoc`.
                        let body_generic_params: Vec<Name> =
                            type_bindings.keys().cloned().collect();
                        let mut body_bounds =
                            crate::lower_type_expr::function_in_scope_generic_param_bounds(
                                db, func_loc,
                            )
                            .clone();
                        if let Some(bound) = &self_bound {
                            body_bounds.insert(Name::new("Self"), vec![bound.clone()]);
                        }
                        let ctx = crate::lower_type_expr::ScopeCtx {
                            db,
                            package_items: pkg_items,
                            ns_context: &pkg_info.namespace_path,
                            generic_params: &body_generic_params,
                            bounds: &body_bounds,
                            self_ty: self_ty.clone(),
                        };
                        let lower_with_self = |te: &baml_compiler2_ast::TypeExpr,
                                               diags: &mut Vec<
                            crate::infer_context::TirTypeError,
                        >| {
                            crate::generics::substitute_ty(
                                &crate::lower_type_expr::lower_type_expr(te, &ctx, diags),
                                &type_bindings,
                            )
                        };

                        // Get declared return type
                        let return_ty = sig
                            .return_type
                            .as_ref()
                            .map(|te| {
                                let span = sig_sm.return_type_span.unwrap_or(func_data.span);
                                let mut diags = Vec::new();
                                let ty = lower_with_self(te, &mut diags);
                                for diag in diags {
                                    builder.report_at_span(diag, span);
                                }
                                builder.validate_type_generic_bounds_at_span(span, &ty);
                                ty
                            })
                            .unwrap_or(Ty::Unknown {
                                attr: TyAttr::default(),
                            });

                        // Set declared return type for return statement checking
                        builder.set_return_type(return_ty.clone());

                        // Add parameter bindings as locals
                        for (i, param) in sig.params.iter().enumerate() {
                            let param_type_span = sig_sm
                                .param_type_spans
                                .get(i)
                                .copied()
                                .flatten()
                                .or_else(|| sig_sm.param_spans.get(i).copied())
                                .unwrap_or_default();
                            let mut param_ty_validated = false;
                            let param_ty = if param.name.as_str() == "self"
                                && matches!(
                                    param.ty.kind,
                                    baml_compiler2_ast::TypeExprKind::Unknown { .. }
                                ) {
                                // `self`'s type is the method's `Self` receiver, resolved once
                                // above (rigid `Self` var for an interface's own default method,
                                // the impl's receiver pattern, or the enclosing class's `Foo<T>`).
                                self_ty.clone().unwrap_or(Ty::Unknown {
                                    attr: TyAttr::default(),
                                })
                            } else {
                                param_ty_validated = true;
                                let mut param_diags = Vec::new();
                                let ty = lower_with_self(&param.ty, &mut param_diags);
                                for diag in param_diags {
                                    builder.report_at_span(diag, param_type_span);
                                }
                                builder.validate_type_generic_bounds_at_span(param_type_span, &ty);
                                ty
                            };
                            if !param_ty_validated {
                                builder.validate_type_generic_bounds_at_span(
                                    param_type_span,
                                    &param_ty,
                                );
                            }
                            builder.add_local(param.name.clone(), param_ty.clone());
                            builder.param_types.push((param.name.clone(), param_ty));
                        }

                        let param_types = builder.param_types.clone();
                        let parameter_defaults =
                            baml_compiler2_ppir::function_parameter_defaults(db, func_loc);
                        builder.check_function_parameter_defaults(
                            &func_data.params,
                            &parameter_defaults,
                            &param_types,
                        );

                        // Check root expression against declared return type
                        if let Some(root_expr) = expr_body.root_expr {
                            builder.check_expr(root_expr, expr_body, &return_ty);
                        }

                        // Validate declared `throws` against effective escaping throws.
                        // Auto-derived methods (synthesized `to_json` /
                        // `from_json`) use a conservative pre-baked throws
                        // clause; the body's actual escaping throws can be
                        // wider when fields have malformed/unknown types.
                        // The user can't fix the synthesized contract, so
                        // skip the entire check for auto-derive bodies.
                        let is_auto_derive =
                            matches!(func_data.origin, ast::FunctionOrigin::AutoDerive);
                        if !is_auto_derive {
                            builder.check_throws_contract(
                                expr_body,
                                sig.throws.as_ref(),
                                sig_sm.throws_type_span,
                                func_data.span,
                                true,
                            );
                        }
                    }
                    found = true;
                    break;
                }
            }
            if !found {
                // Template strings create ScopeKind::Function scopes but are
                // stored in item_tree.template_strings, not item_tree.functions.
                // They have no expression body to type-check, so skip silently.
                let is_template_string = item_tree
                    .template_strings
                    .values()
                    .any(|ts| scope.name.as_ref() == Some(&ts.name));
                debug_assert!(
                    is_template_string,
                    "TIR: no item_tree function matched scope (name={:?}, range={:?})",
                    scope.name, scope.range
                );
            }
        }
        ScopeKind::Lambda => {
            // Find the enclosing Function (or Let) scope by walking ancestors.
            // The Lambda scope does not directly store its body — we must find
            // the top-level body (Function or Let) and then locate the lambda
            // expression within it by matching spans.
            let lambda_span = scope.range;
            let item_tree = baml_compiler2_ppir::file_item_tree(db, file);

            // Seed captured variables as Ty::Unknown so that the lambda's builder
            // can resolve references to captures without reporting "unresolved name"
            // diagnostics. The loop below will override these with proper types.
            let captures = &index.scope_bindings[file_scope.index() as usize].captures;
            for (capture_name, _binding_id) in captures {
                builder.add_local(
                    capture_name.clone(),
                    Ty::Unknown {
                        attr: TyAttr::default(),
                    },
                );
            }

            // Re-seed captured variables with their actual types by walking ALL
            // ancestor scopes (including Lambda ancestors). Each ancestor scope's
            // ScopeInference holds types for declarations in that specific scope.
            // A capture's DefinitionSite identifies which ancestor scope owns it:
            // - PatternBinding(pat_id): pat_id valid in the ancestor's inference
            // - Parameter(idx): ancestor's param_types[idx]
            // - Statement(stmt_id): stmt_id in the ancestor's body, but we detect
            //   ownership by checking the ancestor's scope_bindings.
            //
            // We walk all ancestors so that captures from intermediate lambda scopes
            // (not just the enclosing Function/Let) are also resolved correctly.
            {
                let captures = &index.scope_bindings[file_scope.index() as usize].captures;
                let mut inferred_owner_scopes = Vec::new();
                for ancestor_fsi in index.ancestor_scopes(file_scope) {
                    let anc_bindings = &index.scope_bindings[ancestor_fsi.index() as usize];
                    let inference_fsi = inference_owner_scope(index, ancestor_fsi);
                    let inference_scope_id = index.scope_ids[inference_fsi.index() as usize];
                    let capture_declared_in_ancestor =
                        |_capture_name: &Name, binding_id: &BindingId| -> bool {
                            binding_id.scope == ancestor_fsi
                                && match binding_id.kind {
                                    BindingKind::Parameter(idx) => {
                                        anc_bindings.params.iter().any(|(_, i)| *i == idx)
                                    }
                                    BindingKind::Local(idx) => {
                                        anc_bindings.bindings.get(idx as usize).is_some()
                                    }
                                }
                        };
                    // Only call infer_scope_types if this ancestor has any of
                    // the captures we still need (avoids unnecessary Salsa calls).
                    // For efficiency, check if any capture is declared in this scope.
                    let has_relevant_capture = captures
                        .iter()
                        .any(|(name, binding_id)| capture_declared_in_ancestor(name, binding_id));
                    if !has_relevant_capture {
                        continue;
                    }
                    let anc_inference = if let Some(idx) = inferred_owner_scopes
                        .iter()
                        .position(|(scope_id, _)| scope_id == &inference_scope_id)
                    {
                        inferred_owner_scopes[idx].1
                    } else {
                        let inference = infer_scope_types(db, inference_scope_id);
                        inferred_owner_scopes.push((inference_scope_id, inference));
                        inference
                    };
                    for (capture_name, binding_id) in captures {
                        let is_declared_here =
                            capture_declared_in_ancestor(capture_name, binding_id);
                        if !is_declared_here {
                            continue;
                        }
                        let actual_ty = match binding_id.kind {
                            BindingKind::Parameter(idx) => anc_inference.param_type(idx).cloned(),
                            BindingKind::Local(idx) => {
                                anc_bindings.bindings.get(idx as usize).and_then(|binding| {
                                    anc_inference.binding_type(binding.pattern).cloned()
                                })
                            }
                        };
                        if let Some(ty) = actual_ty {
                            builder.add_local(capture_name.clone(), ty);
                        }
                    }
                }
            }

            // Walk ancestors to find a Function or Let scope that has a body.
            'ancestor_walk: for ancestor_fsi in index.ancestor_scopes(file_scope) {
                let ancestor_scope = &index.scopes[ancestor_fsi.index() as usize];
                match &ancestor_scope.kind {
                    ScopeKind::Function => {
                        // Find the function by span + name in the item tree
                        for func_data in item_tree.functions.values() {
                            if func_data.span != ancestor_scope.range {
                                continue;
                            }
                            if ancestor_scope.name.as_ref() != Some(&func_data.name) {
                                continue;
                            }
                            // Get the function body from item_tree (includes source map)
                            if let Some(baml_compiler2_ast::FunctionBodyDef::Expr(
                                ref func_body,
                                ref func_sm,
                            )) = func_data.body
                            {
                                if let Some((func_def, lambda_body, _lambda_sm, _lambda_expr_id)) =
                                    find_lambda_by_span(func_body, func_sm, lambda_span)
                                {
                                    // Look up contextual param types via the lambda's FileScopeId
                                    // in the parent scope's nested_lambda_types map. This works
                                    // for arbitrarily nested lambdas without calling
                                    // infer_scope_types on intermediate Lambda ancestors (which
                                    // would create a Salsa cycle through package_interface).
                                    let parent_scope_id =
                                        index.scope_ids[ancestor_fsi.index() as usize];
                                    let parent_inference = infer_scope_types(db, parent_scope_id);
                                    let contextual_param_tys = parent_inference
                                        .nested_lambda_type(file_scope)
                                        .and_then(|ty| {
                                            if let Ty::Function { params, .. } = ty {
                                                Some(params.clone())
                                            } else {
                                                None
                                            }
                                        });

                                    let env = extend_env_with_lambda_generics(
                                        generic_env_for_function_data(
                                            GenericLookupContext {
                                                index,
                                                item_tree: &item_tree,
                                            },
                                            ancestor_scope,
                                            func_data,
                                        ),
                                        func_def,
                                    );
                                    apply_generic_env(
                                        db,
                                        &mut builder,
                                        pkg_items,
                                        &pkg_info.namespace_path,
                                        &env,
                                        lambda_span,
                                    );
                                    add_lambda_params_to_builder(
                                        db,
                                        &mut builder,
                                        pkg_items,
                                        &pkg_info.namespace_path,
                                        &env,
                                        func_def,
                                        contextual_param_tys.as_deref(),
                                    );
                                    seed_template_body_params(
                                        &mut builder,
                                        index,
                                        file_scope,
                                        parent_inference,
                                    );
                                    // Infer the lambda body
                                    if let Some(root_expr) = lambda_body.root_expr {
                                        builder.infer_expr(root_expr, lambda_body);
                                    }
                                }
                            }
                            break 'ancestor_walk;
                        }
                    }
                    ScopeKind::Let => {
                        // Find the let binding by span + name in the item tree
                        for (local_id, let_data) in &item_tree.lets {
                            if let_data.span != ancestor_scope.range {
                                continue;
                            }
                            if ancestor_scope.name.as_ref() != Some(&let_data.name) {
                                continue;
                            }
                            let let_loc = LetLoc::new(db, file, *local_id);
                            let body = baml_compiler2_hir::body::let_body(db, let_loc);
                            let source_map_opt =
                                baml_compiler2_hir::body::let_body_source_map(db, let_loc);
                            if let (LetBody::Expr(let_body), Some(let_sm)) =
                                (body.as_ref(), source_map_opt)
                            {
                                if let Some((func_def, lambda_body, _lambda_sm, _lambda_expr_id)) =
                                    find_lambda_by_span(let_body, &let_sm, lambda_span)
                                {
                                    // Look up contextual param types via FileScopeId (same as Function branch).
                                    let parent_scope_id =
                                        index.scope_ids[ancestor_fsi.index() as usize];
                                    let parent_inference = infer_scope_types(db, parent_scope_id);
                                    let contextual_param_tys = parent_inference
                                        .nested_lambda_type(file_scope)
                                        .and_then(|ty| {
                                            if let Ty::Function { params, .. } = ty {
                                                Some(params.clone())
                                            } else {
                                                None
                                            }
                                        });

                                    let env = extend_env_with_lambda_generics(
                                        enclosing_function_generic_env_from_let(
                                            GenericLookupContext {
                                                index,
                                                item_tree: &item_tree,
                                            },
                                            ancestor_scope,
                                        )
                                        .unwrap_or_default(),
                                        func_def,
                                    );
                                    apply_generic_env(
                                        db,
                                        &mut builder,
                                        pkg_items,
                                        &pkg_info.namespace_path,
                                        &env,
                                        lambda_span,
                                    );
                                    add_lambda_params_to_builder(
                                        db,
                                        &mut builder,
                                        pkg_items,
                                        &pkg_info.namespace_path,
                                        &env,
                                        func_def,
                                        contextual_param_tys.as_deref(),
                                    );
                                    seed_template_body_params(
                                        &mut builder,
                                        index,
                                        file_scope,
                                        parent_inference,
                                    );
                                    if let Some(root_expr) = lambda_body.root_expr {
                                        builder.infer_expr(root_expr, lambda_body);
                                    }
                                }
                            }
                            break 'ancestor_walk;
                        }
                    }
                    _ => {
                        continue;
                    }
                }
            }
        }
        ScopeKind::Class => {
            let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
            for (local_id, class_data) in &item_tree.classes {
                if class_data.span != scope.range || scope.name.as_ref() != Some(&class_data.name) {
                    continue;
                }
                report_duplicate_generic_params(
                    &builder,
                    &class_data.generic_params,
                    class_data.span,
                );
                let mut env = GenericEnv::from_params(class_data.generic_params.clone());
                env.add_bounds_for_declared_params(
                    &class_data.generic_params,
                    &class_data.generic_param_bounds,
                );
                apply_generic_env(
                    db,
                    &mut builder,
                    pkg_items,
                    &pkg_info.namespace_path,
                    &env,
                    class_data.span,
                );
                let class_loc = ClassLoc::new(db, file, *local_id);
                let resolved = resolve_class_fields(db, class_loc);
                for (field, (_, ty, _)) in class_data.fields.iter().zip(resolved.fields.iter()) {
                    if let Some(type_expr) = &field.type_expr {
                        builder.validate_type_generic_bounds_at_span(type_expr.span, ty);
                    }
                }
                break;
            }
            for (local_id, iface_data) in &item_tree.interfaces {
                if iface_data.span != scope.range || scope.name.as_ref() != Some(&iface_data.name) {
                    continue;
                }
                let iface_loc = InterfaceLoc::new(db, file, *local_id);
                report_duplicate_generic_params(
                    &builder,
                    &iface_data.generic_params,
                    iface_data.span,
                );
                // Associated types share the interface's type-level namespace with its
                // generic parameters (a bare `Assoc` reference lowers as a type variable),
                // so a name collision — with a parameter or another associated type —
                // would silently alias the two. Both are declaration errors.
                for (idx, assoc) in iface_data.associated_types.iter().enumerate() {
                    if iface_data.generic_params.contains(&assoc.name) {
                        builder.report_at_span(
                            crate::infer_context::TirTypeError::AssociatedTypeConflictsWithGenericParam {
                                name: assoc.name.clone(),
                            },
                            assoc.name_span,
                        );
                    }
                    if iface_data.associated_types[..idx]
                        .iter()
                        .any(|prior| prior.name == assoc.name)
                    {
                        builder.report_at_span(
                            crate::infer_context::TirTypeError::DuplicateAssociatedType {
                                name: assoc.name.clone(),
                            },
                            assoc.name_span,
                        );
                    }
                }
                // Interface-declaration well-formedness (BEP-044). `iface_qtn` names the
                // interface for each diagnostic.
                let iface_qtn = crate::lower_type_expr::qualify_def(
                    db,
                    baml_compiler2_hir::contributions::Definition::Interface(iface_loc),
                    &iface_data.name,
                );
                // E0118: a `requires` graph that cycles back to this interface.
                if let Some(chain) = crate::interfaces::interface_requires_cycle(db, iface_loc) {
                    builder.report_at_span(
                        crate::infer_context::TirTypeError::InterfaceRequiresCycle { chain },
                        iface_data.span,
                    );
                }
                // E0136: a field type may not name the *bare* `Self` type (a recursive field
                // must name the interface itself). A `Self.Assoc` projection is allowed —
                // once the implementor binds the associated type it denotes a concrete field
                // type (`value: Self.Item` with `type Item = int` is an `int` field).
                for field in &iface_data.fields {
                    if let Some(type_expr) = &field.type_expr
                        && crate::builder::TypeInferenceBuilder::type_expr_contains_bare_self(
                            type_expr,
                        )
                    {
                        builder.report_at_span(
                            crate::infer_context::TirTypeError::SelfInInterfaceField {
                                interface: iface_qtn.clone(),
                                field: field.name.clone(),
                            },
                            type_expr.span,
                        );
                    }
                }
                // E0133: a `requires` clause may only name interfaces. A resolved non-interface
                // type is rejected here; an unknown name forwards its own lowering error.
                let requires_scope = crate::lower_type_expr::ScopeCtx {
                    db,
                    package_items: pkg_items,
                    ns_context: &pkg_info.namespace_path,
                    generic_params: &iface_data.generic_params,
                    bounds: &crate::lower_type_expr::TypeVarBoundsMap::default(),
                    self_ty: None,
                };
                for requires_te in &iface_data.requires {
                    if crate::interfaces::resolve_path_to_interface(
                        db,
                        requires_te,
                        pkg_items,
                        &pkg_info.namespace_path,
                    )
                    .is_some()
                    {
                        continue;
                    }
                    let mut requires_diags = Vec::new();
                    let lowered = crate::lower_type_expr::lower_type_expr(
                        requires_te,
                        &requires_scope,
                        &mut requires_diags,
                    );
                    match &lowered {
                        Ty::Class(qtn, ..) | Ty::Enum(qtn, ..) => builder.report_at_span(
                            crate::infer_context::TirTypeError::InterfaceRequiresNonInterface {
                                interface: iface_qtn.clone(),
                                target: qtn.name().clone(),
                            },
                            requires_te.span,
                        ),
                        _ => {
                            for e in requires_diags {
                                builder.report_at_span(e, requires_te.span);
                            }
                        }
                    }
                }
                // Every interface method (required or default) must declare an explicit `throws`
                // clause: a signature is the contract, and unlike a free function its error type is
                // never inferred (TYPE_SYSTEM.md rule 1). (The return type is required for *all*
                // functions, not just interface ones — a universal syntax-layer rule, not enforced
                // here.)
                for (method, throws, span) in iface_data
                    .required_methods
                    .iter()
                    .map(|s| (&s.name, s.throws.as_ref(), s.span))
                    .chain(iface_data.default_methods.iter().filter_map(|id| {
                        item_tree
                            .functions
                            .get(id)
                            .map(|f| (&f.name, f.throws.as_ref(), f.span))
                    }))
                {
                    if throws.is_none() {
                        builder.report_at_span(
                            crate::infer_context::TirTypeError::InterfaceMethodMissingThrows {
                                interface: iface_qtn.clone(),
                                method: method.clone(),
                            },
                            span,
                        );
                    }
                }
                // Associated types are NOT type-level parameters — a bare associated-type
                // name is illegal (`Self.X` required), so only the interface's declared
                // generics enter the signature-lowering env. The associated-type names are
                // still collected below, for the method-generic shadowing check.
                let iface_params = iface_data.generic_params.clone();
                let iface_bounds = iface_data.generic_param_bounds.clone();
                let iface_assoc_names: Vec<Name> = iface_data
                    .associated_types
                    .iter()
                    .map(|assoc| assoc.name.clone())
                    .collect();
                let mut iface_env = GenericEnv::from_params(iface_params.clone());
                iface_env.add_bounds_for_declared_params(&iface_params, &iface_bounds);
                // `Self` in an interface's own signatures is its rigid `Self` type variable,
                // bounded by the interface itself (as a constraint, no associated pins) — so a
                // `Self.member` projection in a field or method signature resolves through it,
                // mirroring the interface-default-body path.
                let self_param = Name::new("Self");
                if !iface_env.params.iter().any(|p| p == &self_param) {
                    iface_env.params.push(self_param.clone());
                }
                iface_env.add_concrete_bound(
                    self_param,
                    baml_type::Interface::new(
                        crate::lower_type_expr::qualify_def(
                            db,
                            baml_compiler2_hir::contributions::Definition::Interface(iface_loc),
                            &iface_data.name,
                        ),
                        iface_data
                            .generic_params
                            .iter()
                            .map(|p| Ty::TypeVar(p.clone(), TyAttr::default()))
                            .collect(),
                        Vec::new(),
                    ),
                );
                let self_ty = crate::self_type::self_type_for_interface_default();
                apply_generic_env(
                    db,
                    &mut builder,
                    pkg_items,
                    &pkg_info.namespace_path,
                    &iface_env,
                    iface_data.span,
                );
                // Computed once per env — every signature type expr below shares it.
                let iface_env_bounds =
                    env_interface_bounds(db, pkg_items, &pkg_info.namespace_path, &iface_env);
                // Each associated type's `extends` bound must be a well-formed interface (same
                // arity / non-interface checks as a generic-param bound). Lowered in the
                // interface's env with `Self` in scope so a projection bound (`extends
                // Self.Item`) forms and is rejected as non-interface. Only checked for
                // diagnostics — associated types are not type-level params, so nothing is
                // threaded into the enforcement table.
                for assoc in &iface_data.associated_types {
                    if let Some(bound_te) = &assoc.bound {
                        let _ = lower_declared_interface_bound(
                            db,
                            &mut builder,
                            pkg_items,
                            &pkg_info.namespace_path,
                            &iface_env.params,
                            &iface_env_bounds,
                            Some(&self_ty),
                            bound_te,
                            bound_te.span,
                            true,
                        );
                    }
                }
                for field in &iface_data.fields {
                    if let Some(type_expr) = &field.type_expr {
                        validate_spanned_type_expr_generic_bounds(
                            db,
                            &mut builder,
                            pkg_items,
                            &pkg_info.namespace_path,
                            &iface_env,
                            &iface_env_bounds,
                            type_expr,
                            Some(self_ty.clone()),
                        );
                    }
                }
                for sig in &iface_data.required_methods {
                    // Required methods have no body scope, so the method-generic
                    // hygiene checks that the function arm runs for default methods
                    // happen here: no `<T, T>`, and no shadowing of the interface's
                    // type-level parameters (generics and associated types alike).
                    report_duplicate_generic_params(&builder, &sig.generic_params, sig.span);
                    for mp in &sig.generic_params {
                        if iface_params.iter().any(|ip| ip == mp) || iface_assoc_names.contains(mp)
                        {
                            builder.report_at_span(
                                crate::infer_context::TirTypeError::TypeParamShadowed {
                                    param_name: mp.clone(),
                                    type_name: iface_data.name.clone(),
                                    owner: crate::infer_context::ShadowedParamOwner::Interface,
                                },
                                sig.span,
                            );
                        }
                    }
                    let mut sig_env = iface_env.clone();
                    // The interface's own bounds were diagnosed above at the
                    // interface span; each signature env reports only its
                    // method's own.
                    sig_env.mark_all_bounds_inherited();
                    sig_env.append_declared(&sig.generic_params, &sig.generic_param_bounds);
                    apply_generic_env(
                        db,
                        &mut builder,
                        pkg_items,
                        &pkg_info.namespace_path,
                        &sig_env,
                        sig.span,
                    );
                    // Once per signature env (the interface's bounds plus this
                    // method's own), shared by its params, return, and throws.
                    let sig_env_bounds =
                        env_interface_bounds(db, pkg_items, &pkg_info.namespace_path, &sig_env);
                    for param in &sig.params {
                        if let Some(type_expr) = &param.type_expr {
                            validate_spanned_type_expr_generic_bounds(
                                db,
                                &mut builder,
                                pkg_items,
                                &pkg_info.namespace_path,
                                &sig_env,
                                &sig_env_bounds,
                                type_expr,
                                Some(self_ty.clone()),
                            );
                        }
                    }
                    if let Some(return_type) = &sig.return_type {
                        validate_spanned_type_expr_generic_bounds(
                            db,
                            &mut builder,
                            pkg_items,
                            &pkg_info.namespace_path,
                            &sig_env,
                            &sig_env_bounds,
                            return_type,
                            Some(self_ty.clone()),
                        );
                    }
                    if let Some(throws) = &sig.throws {
                        validate_spanned_type_expr_generic_bounds(
                            db,
                            &mut builder,
                            pkg_items,
                            &pkg_info.namespace_path,
                            &sig_env,
                            &sig_env_bounds,
                            throws,
                            Some(self_ty.clone()),
                        );
                    }
                }
                // An associated type's default must implement its declared bound (`type Item
                // extends J = V` requires `V` to implement `J`) — the decl-side analogue of the
                // impl-side binding check, via the same shared bound-satisfaction helper. Cycle-safe
                // (the bound resolves without a `requires`-closure walk). A self-referential default
                // bound realizes only partially (`Self` stays symbolic) — rare.
                let default_bound_iface = baml_type::Interface::new(
                    iface_qtn.clone(),
                    iface_data
                        .generic_params
                        .iter()
                        .map(|p| Ty::TypeVar(p.clone(), TyAttr::default()))
                        .collect(),
                    Vec::new(),
                );
                for assoc in &iface_data.associated_types {
                    let Some(default_te) = &assoc.default else {
                        continue;
                    };
                    // The default is lowered once — with a symbolic `Self` — by the shared
                    // query; its lowering diagnostics surface here, at the interface
                    // declaration, the single reporting site, so no referencing site (value
                    // type, projection reducer, method body) re-reports them.
                    let Some((default_ty, default_diags)) =
                        crate::interfaces::interface_associated_type_default(
                            db,
                            iface_loc,
                            assoc.name.clone(),
                        )
                    else {
                        continue;
                    };
                    for diag in default_diags {
                        builder.report_at_span(diag, default_te.span);
                    }
                    // A bounded default (`type Item extends J = V`) must implement its bound.
                    if assoc.bound.is_none() {
                        continue;
                    }
                    let normalized = baml_type::normalize::normalize(&default_ty, &builder);
                    for bound in
                        crate::builder::associated_projection::associated_type_declared_bound(
                            db,
                            &default_bound_iface,
                            &assoc.name,
                        )
                    {
                        if !crate::interfaces::normalized_arg_implements_bound(
                            &builder,
                            &normalized,
                            &bound,
                        ) {
                            builder.report_at_span(
                                crate::infer_context::TirTypeError::AssociatedTypeDefaultViolatesBound {
                                    interface: iface_qtn.clone(),
                                    name: assoc.name.clone(),
                                    default: default_ty.clone(),
                                    bound,
                                },
                                default_te.span,
                            );
                        }
                    }
                }
                // NOTE: interfaces are traits, not inheritance — `Foo.x` and `Bar.x` are distinct,
                // per-interface obligations (like `<T as Foo>::Item` vs `<T as Bar>::Item`), and a
                // type satisfies each independently (via `field as class_field` links mapping them
                // to different class fields). So two interfaces sharing a field name with different
                // types is NOT a `requires`-declaration conflict. A genuine clash surfaces only at
                // the impl site when one class field is forced to satisfy two of them (E0116), and
                // ambiguous unqualified access is a use-site check. There is deliberately no
                // declaration-level inherited-field-conflict check.
                break;
            }
        }
        ScopeKind::Let => {
            // Top-level let binding — find the matching let in the item tree
            // and type-infer its initializer expression.
            let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
            for (local_id, let_data) in &item_tree.lets {
                if let_data.span == scope.range && scope.name.as_ref() == Some(&let_data.name) {
                    let let_loc = LetLoc::new(db, file, *local_id);
                    let body = baml_compiler2_hir::body::let_body(db, let_loc);

                    if let LetBody::Expr(expr_body) = body.as_ref() {
                        // Infer the root expression type bottom-up.
                        if let Some(root_expr) = expr_body.root_expr {
                            builder.infer_expr(root_expr, expr_body);
                        }
                    }
                    break;
                }
            }
        }
        ScopeKind::TypeAlias => {
            let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
            for (local_id, alias_data) in &item_tree.type_aliases {
                if alias_data.span != scope.range || scope.name.as_ref() != Some(&alias_data.name) {
                    continue;
                }
                let alias_loc = TypeAliasLoc::new(db, file, *local_id);
                let resolved = resolve_type_alias(db, alias_loc);
                if let Some(type_expr) = &alias_data.type_expr {
                    builder.validate_type_generic_bounds_at_span(type_expr.span, &resolved.ty);
                }
                break;
            }
        }
        _ => {
            // Project, Package, Namespace, File, Enum, Block, Item:
            // typically no expressions to infer at these scope levels.
        }
    }

    let (
        expressions,
        pattern_types,
        resolutions,
        catch_residual_throws,
        exhaustive_matches,
        diagnostics,
        path_root_types,
        path_segment_types,
        path_member_resolutions,
        param_types,
        call_plans,
        call_type_instantiations,
        function_coercions,
        nested_lambda_types,
        template_body_params,
        parameter_defaults,
    ) = builder.finish();

    let extra = if diagnostics.is_empty() {
        None
    } else {
        Some(Box::new(ScopeInferenceExtra { diagnostics }))
    };

    ScopeInference {
        expressions,
        pattern_types,
        resolutions,
        catch_residual_throws,
        exhaustive_matches,
        path_root_types,
        path_segment_types,
        path_member_resolutions,
        nested_lambda_types,
        template_body_params,
        param_types,
        call_plans,
        call_type_instantiations,
        function_coercions,
        parameter_defaults,
        extra,
    }
}

// ── Type Alias Collection ────────────────────────────────────────────────────

/// Build a map of alias name → resolved Ty from all type aliases in the package.
pub fn collect_type_aliases<'db>(
    db: &'db dyn crate::Db,
    pkg_items: &PackageItems<'db>,
) -> HashMap<crate::ty::QualifiedTypeName, Ty> {
    let mut aliases = HashMap::new();
    for ns in pkg_items.namespaces.values() {
        for (name, def) in &ns.types {
            if let Definition::TypeAlias(loc) = def {
                let resolved = resolve_type_alias(db, *loc);
                let qualified =
                    crate::lower_type_expr::qualify_def(db, Definition::TypeAlias(*loc), name);
                aliases.insert(qualified, resolved.ty.clone());
            }
        }
    }
    aliases
}

/// Resolve a type alias by qualified name to the type it expands to (one level).
///
/// A global function of the name alone — `qtn` carries its package, so the alias's
/// definition is found without any scope or prebuilt alias map, composing the cached
/// [`baml_compiler2_ppir::package_items`] and [`resolve_type_alias`] queries. Returns
/// `None` when `qtn` does not name a type alias. Equivalent to a [`collect_type_aliases`]
/// lookup, but resolved on demand and reaching every dependency (not only the aliases a
/// package happens to re-export).
pub fn alias_def(db: &dyn crate::Db, qtn: &crate::ty::QualifiedTypeName) -> Option<Ty> {
    let pkg_id = PackageId::new(db, qtn.package().clone());
    let items = baml_compiler2_ppir::package_items(db, pkg_id);
    match items.lookup_type(qtn.namespace(), qtn.name())? {
        Definition::TypeAlias(loc) => Some(resolve_type_alias(db, loc).ty.clone()),
        _ => None,
    }
}

/// Look up an enum's variant names by qualified name, resolving the owning
/// package through `res_ctx`.
///
/// The global counterpart to per-scope enum lookup: a pure function of the
/// program's declarations plus the resolution context that bounds which packages
/// are visible from the current one. Returns `None` when `enum_name` does not
/// resolve to an enum in an accessible package — distinct from `Some(vec![])`,
/// a resolved enum that declares no variants.
pub fn enum_variants<'db>(
    db: &'db dyn crate::Db,
    res_ctx: &'db crate::package_interface::PackageResolutionContext<'db>,
    enum_name: &crate::ty::QualifiedTypeName,
) -> Option<Vec<Name>> {
    let items = res_ctx.items_for_package(db, enum_name.package())?;
    let Some(Definition::Enum(enum_loc)) =
        items.lookup_type(enum_name.namespace(), enum_name.name())
    else {
        return None;
    };
    let file = enum_loc.file(db);
    let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
    let enum_data = &item_tree[enum_loc.id(db)];
    Some(enum_data.variants.iter().map(|v| v.name.clone()).collect())
}

/// Build the type-alias map visible from a package: its own aliases plus those
/// re-exported by its dependencies. Shared by per-scope inference and throws
/// analysis so both expand the same aliases (e.g. `testing.TestSetBody`).
pub(crate) fn package_alias_map<'db>(
    db: &'db dyn crate::Db,
    res_ctx: &crate::package_interface::PackageResolutionContext<'db>,
) -> HashMap<crate::ty::QualifiedTypeName, Ty> {
    let mut aliases = collect_type_aliases(db, &res_ctx.own_items);
    for (_dep_name, dep_iface) in &res_ctx.dep_interfaces {
        for types_in_ns in dep_iface.types.values() {
            for exported in types_in_ns.values() {
                if let crate::package_interface::ExportedType::TypeAlias { qtn, resolved } =
                    exported
                {
                    aliases.insert(qtn.clone(), resolved.clone());
                }
            }
        }
    }
    aliases
}

/// Follow a chain of `Ty::TypeAlias` to the first non-alias type it resolves to,
/// leaving any other type untouched. The depth bound guards against cyclic
/// aliases (which are a separate diagnostic).
pub(crate) fn expand_alias_chains(
    mut ty: Ty,
    aliases: &HashMap<crate::ty::QualifiedTypeName, Ty>,
) -> Ty {
    for _ in 0..64 {
        match &ty {
            Ty::TypeAlias(qtn, _) => match aliases.get(qtn) {
                Some(expanded) => ty = expanded.clone(),
                None => break,
            },
            _ => break,
        }
    }
    ty
}

/// Detect invalid (unguarded) type alias cycles in a package.
///
/// Returns the set of `QualifiedTypeName`s that participate in invalid cycles.
/// Valid recursion through containers (e.g. `type JSON = string | JSON[]`) is
/// NOT flagged.
pub fn detect_invalid_alias_cycles<'db>(
    db: &'db dyn crate::Db,
    pkg_id: PackageId<'db>,
) -> std::collections::HashSet<crate::ty::QualifiedTypeName> {
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
    let aliases = collect_type_aliases(db, pkg_items);
    crate::normalize::find_invalid_alias_cycles(&aliases)
}

/// Detect invalid required-field class cycles in a package.
///
/// Returns a list of `ClassCycleInfo`, one per unconstructable cycle found.
/// Cycles through optional, list, or map fields are valid (can be null/empty).
pub fn detect_invalid_class_cycles<'db>(
    db: &'db dyn crate::Db,
    pkg_id: PackageId<'db>,
) -> Vec<crate::normalize::ClassCycleInfo> {
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
    let aliases = collect_type_aliases(db, pkg_items);
    let class_fields = collect_class_fields(db, pkg_items);
    crate::normalize::find_invalid_class_cycles(&class_fields, &aliases)
}

/// Build a map of class qualified name → resolved fields from all classes in the package.
fn collect_class_fields<'db>(
    db: &'db dyn crate::Db,
    pkg_items: &PackageItems<'db>,
) -> HashMap<crate::ty::QualifiedTypeName, Vec<(Name, crate::ty::Ty)>> {
    let mut classes = HashMap::new();
    for ns in pkg_items.namespaces.values() {
        for (name, def) in &ns.types {
            if let Definition::Class(loc) = def {
                let resolved = resolve_class_fields(db, *loc);
                let qualified =
                    crate::lower_type_expr::qualify_def(db, Definition::Class(*loc), name);
                let fields_without_attrs: Vec<(Name, crate::ty::Ty)> = resolved
                    .fields
                    .iter()
                    .map(|(n, ty, _attrs)| (n.clone(), ty.clone()))
                    .collect();
                classes.insert(qualified, fields_without_attrs);
            }
        }
    }
    classes
}

// ── Per-Item Queries ────────────────────────────────────────────────────────

/// Resolved class fields — `TypeExpr` resolved to `Ty` for each field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedClassFields {
    /// (field name, resolved type, field-level attributes)
    pub fields: Vec<(Name, Ty, Vec<baml_compiler2_hir::item_tree::Attribute>)>,
    /// Type lowering diagnostics: (error, span of the type annotation).
    pub diagnostics: Vec<(crate::infer_context::TirTypeError, text_size::TextRange)>,
}

// Safety: `ResolvedClassFields` contains `Ty` (which has `Name`, a Salsa
// interned type). Manual `Update` impl uses `PartialEq` for early-cutoff.
#[allow(unsafe_code)]
unsafe impl salsa::Update for ResolvedClassFields {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        #[allow(unsafe_code)]
        let old = unsafe { &*old_pointer };
        if old == &new_value {
            false
        } else {
            #[allow(unsafe_code)]
            unsafe {
                std::ptr::drop_in_place(old_pointer);
                std::ptr::write(old_pointer, new_value);
            }
            true
        }
    }
}

/// Resolved type alias body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTypeAlias {
    pub ty: Ty,
    /// Type lowering diagnostics: (error, span of the type annotation).
    pub diagnostics: Vec<(crate::infer_context::TirTypeError, text_size::TextRange)>,
}

#[allow(unsafe_code)]
unsafe impl salsa::Update for ResolvedTypeAlias {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        #[allow(unsafe_code)]
        let old = unsafe { &*old_pointer };
        if old == &new_value {
            false
        } else {
            #[allow(unsafe_code)]
            unsafe {
                std::ptr::drop_in_place(old_pointer);
                std::ptr::write(old_pointer, new_value);
            }
            true
        }
    }
}

/// Salsa query: resolved class fields for a specific class.
///
/// Cached per `ClassLoc` — re-runs only when the class definition changes.
#[salsa::tracked(returns(ref))]
pub fn resolve_class_fields<'db>(
    db: &'db dyn crate::Db,
    class_loc: ClassLoc<'db>,
) -> Arc<ResolvedClassFields> {
    let file = class_loc.file(db);
    let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package.clone());
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);

    let class_data = &item_tree[class_loc.id(db)];
    let field_scope = crate::lower_type_expr::ScopeCtx {
        db,
        package_items: pkg_items,
        ns_context: &pkg_info.namespace_path,
        generic_params: &class_data.generic_params,
        bounds: crate::lower_type_expr::class_generic_param_bounds(db, class_loc),
        self_ty: None,
    };
    let mut all_diags = Vec::new();
    let fields = class_data
        .fields
        .iter()
        .map(|f| {
            let ty = f
                .type_expr
                .as_ref()
                .map(|te| {
                    let mut diags = Vec::new();
                    let ty = crate::lower_type_expr::lower_type_expr(te, &field_scope, &mut diags);
                    for d in diags {
                        all_diags.push((d, te.span));
                    }
                    ty
                })
                .unwrap_or(Ty::Unknown {
                    attr: TyAttr::default(),
                });
            (f.name.clone(), ty, f.attributes.clone())
        })
        .collect();

    Arc::new(ResolvedClassFields {
        fields,
        diagnostics: all_diags,
    })
}

/// Salsa query: resolved type alias body.
///
/// Cached per `TypeAliasLoc` — re-runs only when the alias definition changes.
/// Cycle-recovery seed for [`resolve_type_alias`].
///
/// A type alias whose body is an associated-type projection (`type A = T.Member`)
/// makes `resolve_type_alias` a Salsa cycle head: lowering the projection consults
/// the package's impls and its alias map — to find and realize the declaring
/// interface — and building that alias map resolves every alias in the package,
/// this one included. The projection's resolution never depends on this alias's own
/// value, so the fixpoint converges in a single step; this seeds it with the
/// "still inferring" sentinel and no diagnostics (the converged iteration owns them).
fn resolve_type_alias_cycle_initial<'db>(
    _db: &'db dyn crate::Db,
    _id: salsa::Id,
    _alias_loc: TypeAliasLoc<'db>,
) -> Arc<ResolvedTypeAlias> {
    Arc::new(ResolvedTypeAlias {
        ty: Ty::Unknown {
            attr: TyAttr::default(),
        },
        diagnostics: Vec::new(),
    })
}

#[salsa::tracked(returns(ref), cycle_initial = resolve_type_alias_cycle_initial)]
pub fn resolve_type_alias<'db>(
    db: &'db dyn crate::Db,
    alias_loc: TypeAliasLoc<'db>,
) -> Arc<ResolvedTypeAlias> {
    let file = alias_loc.file(db);
    let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package.clone());
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);

    let alias_data = &item_tree[alias_loc.id(db)];
    let mut all_diags = Vec::new();
    let ty = alias_data
        .type_expr
        .as_ref()
        .map(|te| {
            let mut diags = Vec::new();
            let ty = crate::lower_type_expr::lower_type_expr(
                te,
                &crate::lower_type_expr::ScopeCtx {
                    db,
                    package_items: pkg_items,
                    ns_context: &pkg_info.namespace_path,
                    generic_params: &[],
                    bounds: &crate::lower_type_expr::TypeVarBoundsMap::default(),
                    self_ty: None,
                },
                &mut diags,
            );
            for d in diags {
                all_diags.push((d, te.span));
            }
            ty
        })
        .unwrap_or(Ty::Unknown {
            attr: TyAttr::default(),
        });

    Arc::new(ResolvedTypeAlias {
        ty,
        diagnostics: all_diags,
    })
}

// ── Rendered Diagnostics ─────────────────────────────────────────────────────

/// Render all diagnostics for a single scope, resolving arena IDs to source
/// `TextRange` via the function body's `AstSourceMap`.
///
/// This is NOT a Salsa query — it's a convenience function that combines the
/// cached `infer_scope_types` result with the `function_body_source_map` to
/// produce display-ready diagnostics.
pub fn render_scope_diagnostics<'db>(
    db: &'db dyn crate::Db,
    scope_id: ScopeId<'db>,
) -> Vec<crate::infer_context::RenderedTirDiagnostic> {
    let inference = infer_scope_types(db, scope_id);
    let diags = inference.diagnostics();
    if diags.is_empty() {
        return Vec::new();
    }

    // Find the source map by matching scope range against item_tree functions.
    let file = scope_id.file(db);
    let file_scope = scope_id.file_scope_id(db);
    let index = baml_compiler2_ppir::file_semantic_index(db, file);
    let scope = &index.scopes[file_scope.index() as usize];
    let item_tree = baml_compiler2_ppir::file_item_tree(db, file);

    let source_map = match &scope.kind {
        ScopeKind::Lambda => {
            // For lambda scopes, walk ancestors to find the parent Function/Let body,
            // then use find_lambda_by_span to get the lambda's own source map.
            let lambda_span = scope.range;
            let mut found_sm = None;
            'ancestor: for ancestor_fsi in index.ancestor_scopes(file_scope) {
                let ancestor = &index.scopes[ancestor_fsi.index() as usize];
                match &ancestor.kind {
                    ScopeKind::Function => {
                        for func_data in item_tree.functions.values() {
                            if func_data.span == ancestor.range
                                && ancestor.name.as_ref() == Some(&func_data.name)
                            {
                                if let Some(baml_compiler2_ast::FunctionBodyDef::Expr(
                                    ref body,
                                    ref sm,
                                )) = func_data.body
                                {
                                    if let Some((_, _, lambda_sm, _)) =
                                        find_lambda_by_span(body, sm, lambda_span)
                                    {
                                        found_sm = Some(lambda_sm.clone());
                                    }
                                }
                                break 'ancestor;
                            }
                        }
                    }
                    ScopeKind::Let => {
                        for let_data in item_tree.lets.values() {
                            if let_data.span == ancestor.range
                                && ancestor.name.as_ref() == Some(&let_data.name)
                            {
                                if let Some((ref body, ref sm)) = let_data.initializer {
                                    if let Some((_, _, lambda_sm, _)) =
                                        find_lambda_by_span(body, sm, lambda_span)
                                    {
                                        found_sm = Some(lambda_sm.clone());
                                    }
                                }
                                break 'ancestor;
                            }
                        }
                    }
                    _ => {}
                }
            }
            found_sm
        }
        _ => {
            // For Function/Let scopes, find the source map directly.
            // Use PPIR's canonical version so PPIR-synthesized functions are found.
            item_tree
                .functions
                .iter()
                .find(|(_, f)| f.span == scope.range && scope.name.as_ref() == Some(&f.name))
                .and_then(|(local_id, _)| {
                    let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(db, file, *local_id);
                    baml_compiler2_ppir::function_body_source_map(db, func_loc)
                })
                .or_else(|| {
                    // Also search let bindings.
                    item_tree
                        .lets
                        .iter()
                        .find(|(_, l)| l.span == scope.range)
                        .and_then(|(local_id, _)| {
                            let let_loc = baml_compiler2_hir::loc::LetLoc::new(db, file, *local_id);
                            baml_compiler2_hir::body::let_body_source_map(db, let_loc)
                        })
                })
        }
    };

    diags
        .diagnostics
        .iter()
        .map(|d| d.render(db, file, source_map.as_ref()))
        .collect()
}

// ── File-Level Diagnostic Collection ────────────────────────────────────────

/// Collect all type-check diagnostics for a file by iterating all scopes.
///
/// Modeled after Ty's `check_types` (`types.rs:127-168`).
pub fn collect_file_diagnostics(
    db: &dyn crate::Db,
    file: baml_base::SourceFile,
) -> TypeCheckDiagnostics<'_> {
    let index = baml_compiler2_ppir::file_semantic_index(db, file);
    let mut all_diagnostics = TypeCheckDiagnostics::default();

    for scope_id in &index.scope_ids {
        let scope_result = infer_scope_types(db, *scope_id);
        all_diagnostics.extend(scope_result.diagnostics());
    }

    // Collect diagnostics from structural items (class fields, type aliases)
    for (_name, contrib) in &index.symbol_contributions.types {
        match contrib.definition {
            Definition::Class(class_loc) => {
                let resolved = resolve_class_fields(db, class_loc);
                for (error, span) in &resolved.diagnostics {
                    all_diagnostics
                        .diagnostics
                        .push(crate::infer_context::TirDiagnostic {
                            error: error.clone(),
                            severity: crate::infer_context::DiagnosticSeverity::Error,
                            primary: crate::infer_context::DiagnosticLocation::Span(*span),
                            related: Vec::new(),
                        });
                }
            }
            Definition::TypeAlias(alias_loc) => {
                let resolved = resolve_type_alias(db, alias_loc);
                for (error, span) in &resolved.diagnostics {
                    all_diagnostics
                        .diagnostics
                        .push(crate::infer_context::TirDiagnostic {
                            error: error.clone(),
                            severity: crate::infer_context::DiagnosticSeverity::Error,
                            primary: crate::infer_context::DiagnosticLocation::Span(*span),
                            related: Vec::new(),
                        });
                }
            }
            _ => {}
        }
    }

    all_diagnostics
}
