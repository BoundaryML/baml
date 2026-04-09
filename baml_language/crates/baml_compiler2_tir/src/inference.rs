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

use std::{collections::HashMap, sync::Arc};

use baml_base::Name;
use baml_compiler2_ast::{AstSourceMap, Expr as AstExpr, ExprBody, ExprId, FunctionDef, PatId};
use baml_compiler2_hir::{
    body::{FunctionBody, LetBody},
    contributions::Definition,
    loc::{ClassLoc, EnumLoc, FunctionLoc, LetLoc, TypeAliasLoc},
    package::{PackageId, PackageItems},
    scope::{ScopeId, ScopeKind},
};
use rustc_hash::{FxHashMap, FxHashSet};
use text_size::TextRange;

use crate::{
    builder::TypeInferenceBuilder,
    infer_context::{InferContext, TypeCheckDiagnostics},
    ty::{Ty, TyAttr},
};

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
    /// e.g. `env.get` → package="env", namespace=[], name="get"
    Free { func_loc: FunctionLoc<'db> },
    /// A method on a class (user-defined or builtin).
    /// e.g. `arr.length` → package="baml", namespace=[], class="Array", name="length"
    /// e.g. `baz.Greeting` → package="user", namespace=[], class="Baz", name="Greeting"
    Method {
        class_loc: ClassLoc<'db>,
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
    /// Binding types: the type a variable is bound to after widening/annotation.
    /// May differ from the initializer expression type (e.g. `let x = 1` has
    /// expression type `Literal(1, Fresh)` but binding type `int`).
    bindings: FxHashMap<PatId, Ty>,
    /// Member resolutions: for field-access expressions that resolved to a
    /// class field, enum variant, method, or free function — records the
    /// structural path so MIR can emit the correct `QualifiedName` and LSP
    /// can navigate to the definition.
    resolutions: FxHashMap<ExprId, MemberResolution<'db>>,
    /// Match expressions that the exhaustiveness checker determined cover all cases.
    exhaustive_matches: FxHashSet<ExprId>,
    /// Diagnostics and other rare data. Heap-allocated only when non-empty.
    extra: Option<Box<ScopeInferenceExtra<'db>>>,
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

    /// Look up the binding type for a pattern (the type the variable is bound to,
    /// which may differ from the initializer expression type due to widening).
    pub fn binding_type(&self, pat_id: PatId) -> Option<&Ty> {
        self.bindings.get(&pat_id)
    }

    /// Iterate over all (`ExprId`, Ty) pairs for expressions in this scope.
    pub fn iter_expressions(&self) -> impl Iterator<Item = (&ExprId, &Ty)> {
        self.expressions.iter()
    }

    /// Iterate over all (`PatId`, Ty) pairs for pattern bindings in this scope.
    pub fn iter_bindings(&self) -> impl Iterator<Item = (&PatId, &Ty)> {
        self.bindings.iter()
    }

    /// Look up the member resolution for an expression in this scope.
    pub fn resolution(&self, expr_id: ExprId) -> Option<&MemberResolution<'db>> {
        self.resolutions.get(&expr_id)
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
#[salsa::tracked(returns(ref))]
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
    let pkg_items = res_ctx.own_items;

    let mut aliases = collect_type_aliases(db, pkg_items);
    // Also collect type aliases from dependency packages so that e.g.
    // `testing.TestRunner` can be resolved during subtype checking.
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
                    let sig = baml_compiler2_ppir::function_signature(db, func_loc);

                    // Compute the generic params for this function scope.
                    // If this is a method inside a class, also include the class's generic params.
                    let mut generic_params = func_data.generic_params.clone();
                    if let Some(parent_idx) = scope.parent {
                        let parent = &index.scopes[parent_idx.index() as usize];
                        if matches!(parent.kind, ScopeKind::Class) {
                            if let Some(class_name) = &parent.name {
                                for class_data in item_tree.classes.values() {
                                    if class_data.name == *class_name {
                                        let mut merged = class_data.generic_params.clone();
                                        merged.extend(generic_params);
                                        generic_params = merged;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    builder.set_generic_params(generic_params.clone());

                    if let FunctionBody::Expr(expr_body) = body.as_ref() {
                        // Get declared return type
                        let mut diags = Vec::new();
                        let return_ty = sig
                            .return_type
                            .as_ref()
                            .map(|te| {
                                crate::lower_type_expr::lower_type_expr_in_ns(
                                    db,
                                    te,
                                    pkg_items,
                                    &pkg_info.namespace_path,
                                    &generic_params,
                                    &mut diags,
                                )
                            })
                            .unwrap_or(Ty::Unknown {
                                attr: TyAttr::default(),
                            });

                        // Report unresolved type diagnostics for return type
                        if !diags.is_empty() {
                            let sig_sm =
                                baml_compiler2_ppir::function_signature_source_map(db, func_loc);
                            if let Some(ret_span) = sig_sm.return_type_span {
                                for diag in diags.drain(..) {
                                    builder.report_at_span(diag, ret_span);
                                }
                            }
                        }

                        // Set declared return type for return statement checking
                        builder.set_return_type(return_ty.clone());

                        // Determine enclosing class name for `self` parameter resolution
                        let enclosing_class_name: Option<Name> =
                            scope.parent.and_then(|parent_idx| {
                                let parent = &index.scopes[parent_idx.index() as usize];
                                if matches!(parent.kind, ScopeKind::Class) {
                                    parent.name.clone()
                                } else {
                                    None
                                }
                            });

                        // Add parameter bindings as locals
                        let sig_sm =
                            baml_compiler2_ppir::function_signature_source_map(db, func_loc);
                        for (i, (param_name, param_te)) in sig.params.iter().enumerate() {
                            let param_ty = if param_name.as_str() == "self"
                                && matches!(param_te, baml_compiler2_ast::TypeExpr::Unknown { .. })
                            {
                                // `self` parameter with no type annotation — infer from enclosing class
                                enclosing_class_name
                                    .as_ref()
                                    .and_then(|cn| {
                                        let ns_path = &pkg_info.namespace_path;
                                        pkg_items.lookup_type(ns_path, cn).map(|def| {
                                            Ty::Class(
                                                crate::lower_type_expr::qualify_def(db, def, cn),
                                                vec![],
                                                TyAttr::default(),
                                            )
                                        })
                                    })
                                    .unwrap_or(Ty::Unknown {
                                        attr: TyAttr::default(),
                                    })
                            } else {
                                let mut param_diags = Vec::new();
                                let ty = crate::lower_type_expr::lower_type_expr_in_ns(
                                    db,
                                    param_te,
                                    pkg_items,
                                    &pkg_info.namespace_path,
                                    &generic_params,
                                    &mut param_diags,
                                );
                                if !param_diags.is_empty() {
                                    let span = sig_sm
                                        .param_type_spans
                                        .get(i)
                                        .copied()
                                        .flatten()
                                        .or_else(|| sig_sm.param_spans.get(i).copied())
                                        .unwrap_or_default();
                                    for diag in param_diags {
                                        builder.report_at_span(diag, span);
                                    }
                                }
                                ty
                            };
                            builder.add_local(param_name.clone(), param_ty);
                        }

                        // Check root expression against declared return type
                        if let Some(root_expr) = expr_body.root_expr {
                            builder.check_expr(root_expr, expr_body, &return_ty);
                        }

                        // Validate declared `throws` against effective escaping throws.
                        builder.check_throws_contract(
                            expr_body,
                            sig.throws.as_ref(),
                            sig_sm.throws_type_span,
                            func_data.span,
                        );
                    }
                    found = true;
                    break;
                }
            }
            let _ = found;
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
            // diagnostics. Proper capture types will be propagated in a later phase.
            let captures = &index.scope_bindings[file_scope.index() as usize].captures;
            for (capture_name, _def_site) in captures {
                builder.add_local(
                    capture_name.clone(),
                    Ty::Unknown {
                        attr: TyAttr::default(),
                    },
                );
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
                                    // Seed builder with lambda params
                                    let generic_params: Vec<Name> = func_def.generic_params.clone();
                                    builder.set_generic_params(generic_params.clone());
                                    for param in &func_def.params {
                                        let param_ty = param
                                            .type_expr
                                            .as_ref()
                                            .map(|ste| {
                                                crate::lower_type_expr::lower_type_expr_in_ns(
                                                    db,
                                                    &ste.expr,
                                                    pkg_items,
                                                    &pkg_info.namespace_path,
                                                    &generic_params,
                                                    &mut Vec::new(),
                                                )
                                            })
                                            .unwrap_or(Ty::Unknown {
                                                attr: TyAttr::default(),
                                            });
                                        builder.add_local(param.name.clone(), param_ty);
                                    }
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
                                    // Seed builder with lambda params
                                    let generic_params: Vec<Name> = func_def.generic_params.clone();
                                    builder.set_generic_params(generic_params.clone());
                                    for param in &func_def.params {
                                        let param_ty = param
                                            .type_expr
                                            .as_ref()
                                            .map(|ste| {
                                                crate::lower_type_expr::lower_type_expr_in_ns(
                                                    db,
                                                    &ste.expr,
                                                    pkg_items,
                                                    &pkg_info.namespace_path,
                                                    &generic_params,
                                                    &mut Vec::new(),
                                                )
                                            })
                                            .unwrap_or(Ty::Unknown {
                                                attr: TyAttr::default(),
                                            });
                                        builder.add_local(param.name.clone(), param_ty);
                                    }
                                    if let Some(root_expr) = lambda_body.root_expr {
                                        builder.infer_expr(root_expr, lambda_body);
                                    }
                                }
                            }
                            break 'ancestor_walk;
                        }
                    }
                    _ => {
                        // Continue walking up ancestors (e.g., nested lambda inside lambda)
                    }
                }
            }
        }
        ScopeKind::Class => {
            // Class scope: no expressions to type-check.
            // Fields are resolved by resolve_class_fields.
            // Methods are child Function scopes with their own infer_scope_types.
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
        _ => {
            // Project, Package, Namespace, File, Enum, TypeAlias, Block, Item:
            // typically no expressions to infer at these scope levels.
        }
    }

    let (expressions, bindings, resolutions, exhaustive_matches, diagnostics) = builder.finish();

    let extra = if diagnostics.is_empty() {
        None
    } else {
        Some(Box::new(ScopeInferenceExtra { diagnostics }))
    };

    ScopeInference {
        expressions,
        bindings,
        resolutions,
        exhaustive_matches,
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
                        &te.expr,
                        pkg_items,
                        &pkg_info.namespace_path,
                        &class_data.generic_params,
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
                &te.expr,
                pkg_items,
                &pkg_info.namespace_path,
                &[],
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
        .map(|d| d.render(source_map.as_ref()))
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
