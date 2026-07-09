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
    loc::{ClassLoc, EnumLoc, FunctionLoc, InterfaceLoc, LetLoc, TypeAliasLoc},
    package::{PackageId, PackageItems},
    scope::{FileScopeId, ScopeId, ScopeKind},
    semantic_index::{BindingId, BindingKind},
};
use rustc_hash::{FxHashMap, FxHashSet};
use text_size::TextRange;

use crate::{
    builder::TypeInferenceBuilder,
    infer_context::{InferContext, TypeCheckDiagnostics},
    ty::{FunctionParamTy, Ty, TyAttr},
};

/// Count of honest `infer_scope_types` bodies walked (Salsa cache misses) since
/// process start. The per-file diagnostics cache serves clean files' diagnostics
/// without querying their scopes, so a warm incremental compile leaves this at
/// only the dirty files' scope count; a cold compile bumps it once per scope.
/// Exposed for the `BAML_CACHE_DEBUG` warm-run evidence, not part of any result.
static SCOPE_INFERENCES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Number of scopes whose bodies `infer_scope_types` walked honestly (not served
/// from a Salsa memo) since process start. Small on a warm incremental compile
/// (dirty scopes only); large on a cold compile.
pub fn scope_inferences() -> usize {
    SCOPE_INFERENCES.load(std::sync::atomic::Ordering::Relaxed)
}

#[derive(Debug, Clone, Default)]
struct GenericEnv {
    params: Vec<Name>,
    bound_param_names: Vec<Name>,
    bound_exprs: Vec<Option<ast::TypeExpr>>,
    concrete_bounds: Vec<(Name, Ty)>,
}

impl GenericEnv {
    fn from_params(params: Vec<Name>) -> Self {
        Self {
            params,
            bound_param_names: Vec::new(),
            bound_exprs: Vec::new(),
            concrete_bounds: Vec::new(),
        }
    }

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
        }
    }

    fn add_concrete_bound(&mut self, param: Name, bound: Ty) {
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
    db: &dyn crate::Db,
    file: SourceFile,
    item_tree: &baml_compiler2_hir::item_tree::ItemTree,
    pkg_items: &PackageItems<'_>,
    ns_context: &[Name],
    type_name: &Name,
) -> Option<(Vec<Name>, Vec<Option<ast::TypeExpr>>)> {
    for class_data in item_tree.classes.values() {
        if class_data.name == *type_name {
            return Some((
                class_data.generic_params.clone(),
                class_data.generic_param_bounds.clone(),
            ));
        }
    }

    for (local_id, iface_data) in &item_tree.interfaces {
        if iface_data.name == *type_name {
            let iface_loc = InterfaceLoc::new(db, file, *local_id);
            return Some(interface_type_level_params_and_bounds(
                db, iface_loc, iface_data, pkg_items, ns_context,
            ));
        }
    }

    None
}

fn interface_type_level_params_and_bounds(
    db: &dyn crate::Db,
    iface_loc: InterfaceLoc<'_>,
    iface_data: &baml_compiler2_hir::item_tree::Interface,
    pkg_items: &PackageItems<'_>,
    ns_context: &[Name],
) -> (Vec<Name>, Vec<Option<ast::TypeExpr>>) {
    let mut params = iface_data.generic_params.clone();
    params.extend(
        iface_data
            .associated_types
            .iter()
            .map(|assoc| assoc.name.clone()),
    );

    let mut bounds = iface_data.generic_param_bounds.clone();
    bounds.extend(
        iface_data
            .associated_types
            .iter()
            .map(|assoc| assoc.bound.clone()),
    );

    let own_associated: FxHashSet<Name> = iface_data
        .associated_types
        .iter()
        .map(|assoc| assoc.name.clone())
        .collect();
    let inherited = inherited_interface_associated_type_names(db, iface_loc, pkg_items, ns_context);
    let mut counts: FxHashMap<Name, usize> = FxHashMap::default();
    for name in &inherited {
        *counts.entry(name.clone()).or_default() += 1;
    }
    for name in inherited {
        if counts.get(&name).copied().unwrap_or_default() == 1
            && !own_associated.contains(&name)
            && !params.contains(&name)
        {
            params.push(name);
            bounds.push(None);
        }
    }

    (params, bounds)
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

/// Signature-scope type bindings for a method declared directly inside an
/// interface (a default method or a required-method stub). `Self` is the rigid
/// type variable bound by the interface, and each associated type — the
/// interface's own plus those inherited through `requires` — maps to the
/// projection `Self.<name>`.
///
/// This mirrors the bindings [`infer_scope_types`] installs for the method
/// *body*, so a signature and its body resolve `Item`/`Error`/`Self`
/// identically. Lowering a signature with these bindings (via
/// [`crate::generics::lower_type_expr_with_generics`]) keeps associated-type
/// references as faithful `Self.<name>` projections instead of letting a bare
/// [`crate::lower_type_expr::lower_type_expr_in_ns`] erase them to `Ty::Unknown`
/// — which would otherwise reach the runtime lowering boundary and panic.
///
/// Callers merge these over the method's in-scope generic parameters (each
/// mapped to its own `TypeVar`); the keys here (`Self` and the associated-type
/// names) take precedence.
pub fn interface_self_projection_bindings(
    db: &dyn crate::Db,
    iface_loc: InterfaceLoc<'_>,
    iface_data: &baml_compiler2_hir::item_tree::Interface,
    pkg_items: &PackageItems<'_>,
    ns_context: &[Name],
) -> FxHashMap<Name, Ty> {
    let self_var = || Ty::TypeVar(Name::new("Self"), TyAttr::default());
    let projection = |member: Name| Ty::AssociatedTypeProjection {
        base: Box::new(self_var()),
        interface: None,
        member,
        attr: TyAttr::default(),
    };

    let mut bindings = FxHashMap::default();
    bindings.insert(Name::new("Self"), self_var());
    for assoc in &iface_data.associated_types {
        bindings.insert(assoc.name.clone(), projection(assoc.name.clone()));
    }
    // Inherited associated types are bound as `Self.<name>` only when the name
    // is unambiguous. A name inherited from more than one `requires` interface
    // is excluded from the interface's type-level params (see
    // `interface_type_level_params_and_bounds`, which applies the same
    // `count == 1` filter) and must be disambiguated explicitly; binding it to a
    // single `Self.<name>` projection here would silently resolve an ambiguous
    // reference and diverge from body inference.
    let inherited = inherited_interface_associated_type_names(db, iface_loc, pkg_items, ns_context);
    let mut counts: FxHashMap<Name, usize> = FxHashMap::default();
    for name in &inherited {
        *counts.entry(name.clone()).or_default() += 1;
    }
    for name in inherited {
        if counts.get(&name).copied().unwrap_or_default() == 1 {
            bindings
                .entry(name.clone())
                .or_insert_with(|| projection(name));
        }
    }
    bindings
}

fn install_generic_param_bounds(
    db: &dyn crate::Db,
    builder: &mut TypeInferenceBuilder<'_>,
    pkg_items: &PackageItems<'_>,
    ns_context: &[Name],
    env: &GenericEnv,
    span: TextRange,
) {
    let mut bounds: FxHashMap<Name, Ty> = FxHashMap::default();
    let type_bindings = type_bindings_for_params(&env.params);
    for (name, bound) in env.bound_param_names.iter().zip(env.bound_exprs.iter()) {
        let Some(bound_te) = bound else {
            continue;
        };
        let mut diags = Vec::new();
        let bound_ty = crate::generics::lower_type_expr_with_generics(
            db,
            bound_te,
            pkg_items,
            ns_context,
            &type_bindings,
            &mut diags,
        );
        for diag in diags {
            builder.report_at_span(diag, span);
        }
        bounds.insert(name.clone(), bound_ty);
    }
    bounds.extend(env.concrete_bounds.iter().cloned());
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
    db: &'a dyn crate::Db,
    file: SourceFile,
    index: &'a baml_compiler2_hir::semantic_index::FileSemanticIndex<'db>,
    item_tree: &'a baml_compiler2_hir::item_tree::ItemTree,
    pkg_items: &'a PackageItems<'db>,
    ns_context: &'a [Name],
}

fn parent_type_generic_env(
    ctx: GenericLookupContext<'_, '_>,
    parent_scope_id: Option<FileScopeId>,
) -> Option<(Name, Vec<Name>, Vec<Option<ast::TypeExpr>>)> {
    let parent = &ctx.index.scopes[parent_scope_id?.index() as usize];
    if !matches!(parent.kind, ScopeKind::Class) {
        return None;
    }
    let type_name = parent.name.clone()?;
    let (params, bounds) = enclosing_type_generics(
        ctx.db,
        ctx.file,
        ctx.item_tree,
        ctx.pkg_items,
        ctx.ns_context,
        &type_name,
    )?;
    Some((type_name, params, bounds))
}

fn prepend_parent_type_generics(
    ctx: GenericLookupContext<'_, '_>,
    env: &mut GenericEnv,
    parent_scope_id: Option<FileScopeId>,
) -> Option<Name> {
    let (type_name, parent_generics, parent_bounds) =
        parent_type_generic_env(ctx, parent_scope_id)?;
    env.prepend_declared(&parent_generics, &parent_bounds);
    Some(type_name)
}

/// The `implements … for …` block whose method list contains `func_data`, if
/// any. A method is identified by its source span (unique per declaration).
fn enclosing_impl_for_func<'a>(
    item_tree: &'a baml_compiler2_hir::item_tree::ItemTree,
    func_data: &baml_compiler2_hir::item_tree::Function,
) -> Option<&'a baml_compiler2_hir::item_tree::ImplementsFor> {
    item_tree.implements_for.iter().find(|imp| {
        imp.methods
            .iter()
            .any(|&mid| item_tree[mid].span == func_data.span)
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
    if let Some(imp) = enclosing_impl_for_func(ctx.item_tree, func_data) {
        env.prepend_declared(&imp.generic_params, &imp.generic_param_bounds);
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

#[derive(Clone, Copy)]
struct TypeExprLoweringOptions<'a> {
    span: TextRange,
    self_replacement: Option<&'a ast::TypeExpr>,
}

fn lower_type_expr_at_span_in_env(
    db: &dyn crate::Db,
    builder: &mut TypeInferenceBuilder<'_>,
    pkg_items: &PackageItems<'_>,
    ns_context: &[Name],
    env: &GenericEnv,
    type_expr: &ast::TypeExpr,
    options: TypeExprLoweringOptions<'_>,
) -> Ty {
    let mut diags = Vec::new();
    let resolved_expr = if let Some(replacement) = options.self_replacement {
        crate::lower_type_expr::substitute_self_in(type_expr, replacement)
    } else {
        type_expr.clone()
    };
    let type_bindings = type_bindings_for_params(&env.params);
    let ty = crate::generics::lower_type_expr_with_generics(
        db,
        &resolved_expr,
        pkg_items,
        ns_context,
        &type_bindings,
        &mut diags,
    );
    for diag in diags {
        builder.report_at_span(diag, options.span);
    }
    builder.validate_type_generic_bounds_at_span(options.span, &ty);
    ty
}

fn validate_spanned_type_expr_generic_bounds(
    db: &dyn crate::Db,
    builder: &mut TypeInferenceBuilder<'_>,
    pkg_items: &PackageItems<'_>,
    ns_context: &[Name],
    env: &GenericEnv,
    type_expr: &ast::TypeExpr,
    self_replacement: Option<&ast::TypeExpr>,
) {
    lower_type_expr_at_span_in_env(
        db,
        builder,
        pkg_items,
        ns_context,
        env,
        type_expr,
        TypeExprLoweringOptions {
            span: type_expr.span,
            self_replacement,
        },
    );
}

fn extend_env_with_lambda_generics(mut env: GenericEnv, func_def: &FunctionDef) -> GenericEnv {
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
    let type_bindings = type_bindings_for_params(&env.params);
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
                crate::generics::lower_type_expr_with_generics(
                    db,
                    ste,
                    pkg_items,
                    ns_context,
                    &type_bindings,
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
    /// An interface default method referenced through the interface type
    /// itself, e.g. `Named.describe(value)`.
    InterfaceDefaultMethod {
        iface_loc: InterfaceLoc<'db>,
        func_loc: FunctionLoc<'db>,
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

/// Point lookups into the default-parameter tables, mirroring the
/// [`ScopeInference`] accessors of the same names. Consumed by MIR lowering,
/// which reads inference facts through per-scope views rather than
/// materializing merged copies.
impl<'db> DefaultParameterInference<'db> {
    /// Look up the type of a default-parameter expression.
    pub fn expression_type(&self, expr_id: ExprId) -> Option<&Ty> {
        self.expressions.get(&expr_id)
    }

    /// Look up the binding type for a default-parameter pattern.
    pub fn binding_type(&self, pat_id: PatId) -> Option<&Ty> {
        self.pattern_types.get(&pat_id)
    }

    /// Look up the member resolution for a default-parameter expression.
    pub fn resolution(&self, expr_id: ExprId) -> Option<&MemberResolution<'db>> {
        self.resolutions.get(&expr_id)
    }

    /// Check whether a default-parameter match expression is exhaustive.
    pub fn is_exhaustive_match(&self, expr_id: ExprId) -> bool {
        self.exhaustive_matches.contains(&expr_id)
    }

    /// Look up the root segment type for a default-parameter path expression.
    pub fn path_root_type(&self, expr_id: ExprId) -> Option<&Ty> {
        self.path_root_types.get(&expr_id)
    }

    /// Look up the type of `segments[..=seg_idx]` for a default-parameter path.
    pub fn path_segment_type(&self, expr_id: ExprId, seg_idx: usize) -> Option<&Ty> {
        self.path_segment_types.get(&(expr_id, seg_idx))
    }

    /// Look up per-segment member resolutions for a default-parameter path.
    pub fn path_member_resolution(&self, expr_id: ExprId) -> Option<&[MemberResolution<'db>]> {
        self.path_member_resolutions
            .get(&expr_id)
            .map(Vec::as_slice)
    }

    /// Look up the argument binding plan for a default-parameter call.
    pub fn call_plan(&self, expr_id: ExprId) -> Option<&CallPlan> {
        self.call_plans.get(&expr_id)
    }

    /// Look up the function adapter for a default-parameter checked coercion.
    pub fn function_coercion(&self, expr_id: ExprId) -> Option<&FunctionCoercion> {
        self.function_coercions.get(&expr_id)
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

    /// Look up the function adapter required by a checked coercion.
    pub fn function_coercion(&self, expr_id: ExprId) -> Option<&FunctionCoercion> {
        self.function_coercions.get(&expr_id)
    }

    /// Iterate over all function adapters required by checked coercions.
    pub fn iter_function_coercions(&self) -> impl Iterator<Item = (&ExprId, &FunctionCoercion)> {
        self.function_coercions.iter()
    }

    /// The default-parameter inference tables for this scope, for point
    /// lookups via [`DefaultParameterInference`]'s accessors.
    pub fn parameter_defaults(&self) -> &DefaultParameterInference<'db> {
        &self.parameter_defaults
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
    // Salsa only enters the query body on a cache miss, so this counts scopes
    // actually re-inferred — the warm-incremental evidence that clean files
    // (never queried, because the diagnostics cache serves them) skip inference.
    SCOPE_INFERENCES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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

                    let enclosing_impl = item_tree
                        .implements_for
                        .iter()
                        .find(|imp| imp.methods.contains(local_id));

                    let mut env = GenericEnv::from_params(sig.user_generic_params.clone());
                    env.params
                        .extend(sig.synthetic_effect_params.iter().cloned());
                    if let Some(imp) = enclosing_impl {
                        env.prepend_declared(&imp.generic_params, &imp.generic_param_bounds);
                    } else if let Some((type_name, parent_generics, parent_bounds)) =
                        parent_type_generic_env(
                            GenericLookupContext {
                                db,
                                file,
                                index,
                                item_tree: &item_tree,
                                pkg_items,
                                ns_context: &pkg_info.namespace_path,
                            },
                            scope.parent,
                        )
                    {
                        for mp in &sig.user_generic_params {
                            if parent_generics.iter().any(|cp| cp == mp) {
                                builder.report_at_span(
                                    crate::infer_context::TirTypeError::TypeParamShadowed {
                                        param_name: mp.clone(),
                                        class_name: type_name.clone(),
                                    },
                                    func_data.span,
                                );
                            }
                        }
                        env.prepend_declared(&parent_generics, &parent_bounds);
                    }
                    // BEP-044 (Self-as-type-variable): inside an interface's own
                    // method, `self` is a `Self` type variable bound by the
                    // interface instead of the interface existential. Register it
                    // in the shared generic env so the normal bound-aware member
                    // resolution path handles `Self`.
                    let interface_self_bound: Option<Ty> = if enclosing_impl.is_none() {
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
                            let associated_bindings = iface
                                .associated_types
                                .iter()
                                .map(|assoc| {
                                    (
                                        assoc.name.clone(),
                                        Ty::AssociatedTypeProjection {
                                            base: Box::new(Ty::TypeVar(
                                                Name::new("Self"),
                                                TyAttr::default(),
                                            )),
                                            interface: None,
                                            member: assoc.name.clone(),
                                            attr: TyAttr::default(),
                                        },
                                    )
                                })
                                .collect();
                            Some(Ty::Interface(
                                qtn,
                                args,
                                associated_bindings,
                                TyAttr::default(),
                            ))
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
                        // BEP-044 `Self` substitution: inside an out-of-body
                        // implementation, `Self` is the rule receiver pattern
                        // (`Box<T>` or `T`). Otherwise it is the enclosing
                        // class/interface type.
                        let self_replacement = if interface_self_bound.is_some() {
                            // Interface method: `Self` is the bound type variable,
                            // not the interface existential.
                            Some(crate::lower_type_expr::type_expr_for_name(Name::new(
                                "Self",
                            )))
                        } else {
                            enclosing_impl
                                .map(|imp| imp.for_target.clone())
                                .or_else(|| {
                                    enclosing_class_name.as_ref().map(|cn| {
                                        crate::lower_type_expr::type_expr_for_name(cn.clone())
                                    })
                                })
                        };
                        let sig_sm = baml_compiler2_ppir::elaborated_function_signature_source_map(
                            db, func_loc,
                        );
                        let mut type_bindings: FxHashMap<Name, Ty> = FxHashMap::default();
                        for param in &env.params {
                            type_bindings.insert(
                                param.clone(),
                                Ty::TypeVar(param.clone(), TyAttr::default()),
                            );
                        }
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
                                let iface_ns =
                                    baml_compiler2_hir::file_package::file_package(db, iface_file)
                                        .namespace_path;
                                let mut iface_type_bindings = type_bindings.clone();
                                if let baml_compiler2_ast::TypeExprKind::Path {
                                    generic_args, ..
                                } = &target.kind
                                {
                                    for (param, arg) in
                                        iface_data.generic_params.iter().zip(generic_args)
                                    {
                                        let mut arg_diags = Vec::new();
                                        let ty = crate::generics::lower_type_expr_with_generics(
                                            db,
                                            arg,
                                            pkg_items,
                                            &pkg_info.namespace_path,
                                            &type_bindings,
                                            &mut arg_diags,
                                        );
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
                                        let resolved = if let Some(replacement) = &self_replacement
                                        {
                                            crate::lower_type_expr::substitute_self_in(
                                                te,
                                                replacement,
                                            )
                                        } else {
                                            te.clone()
                                        };
                                        let mut binding_diags = Vec::new();
                                        let ty = crate::generics::lower_type_expr_with_generics(
                                            db,
                                            &resolved,
                                            pkg_items,
                                            &pkg_info.namespace_path,
                                            &type_bindings,
                                            &mut binding_diags,
                                        );
                                        for diag in binding_diags {
                                            builder.report_at_span(diag, te.span);
                                        }
                                        type_bindings.insert(assoc.name.clone(), ty.clone());
                                        iface_type_bindings.insert(assoc.name.clone(), ty);
                                        continue;
                                    }
                                    if let Some(default) = &assoc.default {
                                        let mut default_diags = Vec::new();
                                        let ty = crate::generics::lower_type_expr_with_generics(
                                            db,
                                            default,
                                            pkg_items,
                                            &iface_ns,
                                            &iface_type_bindings,
                                            &mut default_diags,
                                        );
                                        for diag in default_diags {
                                            builder.report_at_span(diag, default.span);
                                        }
                                        type_bindings.insert(assoc.name.clone(), ty.clone());
                                        iface_type_bindings.insert(assoc.name.clone(), ty);
                                    }
                                }
                            }
                        } else if let Some(iface_name) = &enclosing_class_name
                            && let Some(def) =
                                pkg_items.lookup_type(&pkg_info.namespace_path, iface_name)
                            && let baml_compiler2_hir::contributions::Definition::Interface(
                                iface_loc,
                            ) = def
                        {
                            let iface_file = iface_loc.file(db);
                            let iface_tree = baml_compiler2_hir::file_item_tree(db, iface_file);
                            if let Some(iface_data) = iface_tree.interfaces.get(&iface_loc.id(db)) {
                                let iface_ns =
                                    baml_compiler2_hir::file_package::file_package(db, iface_file)
                                        .namespace_path;
                                let self_assoc_projection =
                                    |member: Name| Ty::AssociatedTypeProjection {
                                        base: Box::new(Ty::TypeVar(
                                            Name::new("Self"),
                                            TyAttr::default(),
                                        )),
                                        interface: None,
                                        member,
                                        attr: TyAttr::default(),
                                    };
                                for assoc in &iface_data.associated_types {
                                    if let Some(default) = &assoc.default {
                                        let mut default_diags = Vec::new();
                                        let _ = crate::generics::lower_type_expr_with_generics(
                                            db,
                                            default,
                                            pkg_items,
                                            &iface_ns,
                                            &type_bindings,
                                            &mut default_diags,
                                        );
                                        for diag in default_diags {
                                            builder.report_at_span(diag, default.span);
                                        }
                                    }
                                    // Inside the interface's own (default) method, `Self` is the
                                    // rigid type variable bound by the interface, not an existential
                                    // interface value. Associated types therefore project onto `Self`
                                    // even when they have defaults. Defaults are for omitted bindings
                                    // at interface type-use sites; a default body must stay
                                    // polymorphic over implementors that override the associated type.
                                    // This also matches how `self.method()` resolves the same
                                    // associated type via `SelfReceiver::RigidVar`.
                                    type_bindings.insert(
                                        assoc.name.clone(),
                                        self_assoc_projection(assoc.name.clone()),
                                    );
                                }
                                let own_associated: FxHashSet<Name> = iface_data
                                    .associated_types
                                    .iter()
                                    .map(|assoc| assoc.name.clone())
                                    .collect();
                                let inherited = inherited_interface_associated_type_names(
                                    db, iface_loc, pkg_items, &iface_ns,
                                );
                                let mut inherited_counts: FxHashMap<Name, usize> =
                                    FxHashMap::default();
                                for name in &inherited {
                                    *inherited_counts.entry(name.clone()).or_default() += 1;
                                }
                                for inherited_name in inherited {
                                    if inherited_counts
                                        .get(&inherited_name)
                                        .copied()
                                        .unwrap_or_default()
                                        == 1
                                        && !own_associated.contains(&inherited_name)
                                    {
                                        type_bindings.insert(
                                            inherited_name.clone(),
                                            self_assoc_projection(inherited_name),
                                        );
                                    }
                                }
                            }
                        }
                        builder.set_type_bindings(type_bindings.clone());
                        let lower_with_self = |te: &baml_compiler2_ast::TypeExpr,
                                               diags: &mut Vec<
                            crate::infer_context::TirTypeError,
                        >| {
                            let resolved = if let Some(replacement) = &self_replacement {
                                crate::lower_type_expr::substitute_self_in(te, replacement)
                            } else {
                                te.clone()
                            };
                            crate::generics::lower_type_expr_with_generics(
                                db,
                                &resolved,
                                pkg_items,
                                &pkg_info.namespace_path,
                                &type_bindings,
                                diags,
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
                                if let Some(imp) = enclosing_impl {
                                    param_ty_validated = true;
                                    lower_type_expr_at_span_in_env(
                                        db,
                                        &mut builder,
                                        pkg_items,
                                        &pkg_info.namespace_path,
                                        &env,
                                        &imp.for_target,
                                        TypeExprLoweringOptions {
                                            span: param_type_span,
                                            self_replacement: None,
                                        },
                                    )
                                } else if interface_self_bound.is_some() {
                                    // BEP-044 Self-as-type-variable: inside an
                                    // interface's own method `self` is the `Self`
                                    // type variable (bound by this interface), not
                                    // the interface existential — so `self.method(
                                    // other: Self)` resolves with `Self` pinned.
                                    // Member resolution walks the registered bound
                                    // to reach the interface contract.
                                    Ty::TypeVar(Name::new("Self"), TyAttr::default())
                                } else {
                                    // `self` parameter with no type annotation — infer from
                                    // enclosing class.
                                    enclosing_class_name
                                        .as_ref()
                                        .and_then(|cn| {
                                            let ns_path = &pkg_info.namespace_path;
                                            pkg_items.lookup_type(ns_path, cn).map(|def| {
                                                let qtn =
                                                    crate::lower_type_expr::qualify_def(db, def, cn);
                                                match def {
                                                    baml_compiler2_hir::contributions::Definition::Interface(_) => {
                                                        // BEP-044 wf3 #1/#5: a generic interface's
                                                        // default method must type `self` as
                                                        // `Interface<T..>` carrying its own params as
                                                        // TypeVars — empty args dropped `T`, so a
                                                        // `self.method()` call lost the reached view's
                                                        // concrete arg (first impl block wins) and
                                                        // cross-`requires` calls found no MIR candidate.
                                                        let iface_args: Vec<Ty> = item_tree
                                                            .interfaces
                                                            .values()
                                                            .find(|i| &i.name == cn)
                                                            .map(|i| {
                                                                i.generic_params
                                                                    .iter()
                                                                    .map(|p| Ty::TypeVar(
                                                                        p.clone(),
                                                                        TyAttr::default(),
                                                                    ))
                                                                    .collect()
                                                            })
                                                            .unwrap_or_default();
                                                        Ty::Interface(
                                                            qtn,
                                                            iface_args,
                                                            vec![],
                                                            TyAttr::default(),
                                                        )
                                                    }
                                                    _ => {
                                                        // Mirror the interface arm: a generic class's
                                                        // method must type `self` as `Class<T..>`
                                                        // carrying the class's own params as TypeVars.
                                                        // Empty args left `self` as a bare `Class`, so
                                                        // the auto-derived `to_json`'s
                                                        // `to_string<Self>(self)` saw `self: StreamCache`
                                                        // against `Self = StreamCache<TStream, TFinal>`,
                                                        // and more generally a bare-class `self` leaked
                                                        // into every generic-class method body.
                                                        let class_args: Vec<Ty> = item_tree
                                                            .classes
                                                            .values()
                                                            .find(|c| &c.name == cn)
                                                            .map(|c| {
                                                                c.generic_params
                                                                    .iter()
                                                                    .map(|p| Ty::TypeVar(
                                                                        p.clone(),
                                                                        TyAttr::default(),
                                                                    ))
                                                                    .collect()
                                                            })
                                                            .unwrap_or_default();
                                                        // The builtin `Array<T>` is the array sugar
                                                        // `T[]` (`Ty::List`), not a nominal class:
                                                        // type its methods' `self` structurally so a
                                                        // body that returns `self` (e.g. the in-place
                                                        // `sort_by`/`sort_by_key`) matches the `T[]`
                                                        // return type. Members still resolve (a `List`
                                                        // receiver dispatches to the `Array` builtins).
                                                        if qtn.is_builtin_root_type("Array")
                                                            && class_args.len() == 1
                                                        {
                                                            Ty::List(
                                                                Box::new(class_args[0].clone()),
                                                                TyAttr::default(),
                                                            )
                                                        } else {
                                                            Ty::Class(qtn, class_args, TyAttr::default())
                                                        }
                                                    }
                                                }
                                            })
                                        })
                                        .unwrap_or(Ty::Unknown {
                                            attr: TyAttr::default(),
                                        })
                                }
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
                                                db,
                                                file,
                                                index,
                                                item_tree: &item_tree,
                                                pkg_items,
                                                ns_context: &pkg_info.namespace_path,
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
                                                db,
                                                file,
                                                index,
                                                item_tree: &item_tree,
                                                pkg_items,
                                                ns_context: &pkg_info.namespace_path,
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
                let (iface_params, iface_bounds) = interface_type_level_params_and_bounds(
                    db,
                    iface_loc,
                    iface_data,
                    pkg_items,
                    &pkg_info.namespace_path,
                );
                let mut iface_env = GenericEnv::from_params(iface_params.clone());
                iface_env.add_bounds_for_declared_params(&iface_params, &iface_bounds);
                let self_replacement =
                    crate::lower_type_expr::type_expr_for_name(iface_data.name.clone());
                apply_generic_env(
                    db,
                    &mut builder,
                    pkg_items,
                    &pkg_info.namespace_path,
                    &iface_env,
                    iface_data.span,
                );
                for field in &iface_data.fields {
                    if let Some(type_expr) = &field.type_expr {
                        validate_spanned_type_expr_generic_bounds(
                            db,
                            &mut builder,
                            pkg_items,
                            &pkg_info.namespace_path,
                            &iface_env,
                            type_expr,
                            Some(&self_replacement),
                        );
                    }
                }
                for sig in &iface_data.required_methods {
                    let mut sig_env = iface_env.clone();
                    sig_env.append_declared(&sig.generic_params, &sig.generic_param_bounds);
                    apply_generic_env(
                        db,
                        &mut builder,
                        pkg_items,
                        &pkg_info.namespace_path,
                        &sig_env,
                        sig.span,
                    );
                    for param in &sig.params {
                        if let Some(type_expr) = &param.type_expr {
                            validate_spanned_type_expr_generic_bounds(
                                db,
                                &mut builder,
                                pkg_items,
                                &pkg_info.namespace_path,
                                &sig_env,
                                type_expr,
                                Some(&self_replacement),
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
                            return_type,
                            Some(&self_replacement),
                        );
                    }
                    if let Some(throws) = &sig.throws {
                        validate_spanned_type_expr_generic_bounds(
                            db,
                            &mut builder,
                            pkg_items,
                            &pkg_info.namespace_path,
                            &sig_env,
                            throws,
                            Some(&self_replacement),
                        );
                    }
                }
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
                    let ty = crate::lower_type_expr::lower_type_expr_in_ns(
                        db,
                        te,
                        pkg_items,
                        &pkg_info.namespace_path,
                        &class_data.generic_params,
                        &mut diags,
                    );
                    for d in diags {
                        let span = d.precise_span().unwrap_or(te.span);
                        all_diags.push((d, span));
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
#[salsa::tracked(returns(ref))]
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
            let ty = crate::lower_type_expr::lower_type_expr_in_ns(
                db,
                te,
                pkg_items,
                &pkg_info.namespace_path,
                &[],
                &mut diags,
            );
            for d in diags {
                let span = d.precise_span().unwrap_or(te.span);
                all_diags.push((d, span));
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
