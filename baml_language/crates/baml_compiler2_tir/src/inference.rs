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
use baml_compiler2_ast::{ExprId, PatId};
use baml_compiler2_hir::{
    body::FunctionBody,
    contributions::Definition,
    loc::{ClassLoc, TypeAliasLoc},
    package::{PackageId, PackageItems, package_items},
    scope::{ScopeId, ScopeKind},
};
use rustc_hash::FxHashMap;

use crate::{
    builder::TypeInferenceBuilder,
    infer_context::{InferContext, TypeCheckDiagnostics},
    ty::Ty,
};

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
unsafe impl<'db> salsa::Update for ScopeInference<'db> {
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
                    &*(empty as *const TypeCheckDiagnostics<'static>
                        as *const TypeCheckDiagnostics<'db>)
                }
            })
    }
}

// ── Main Salsa Query: Per-Scope Inference ───────────────────────────────────

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
    let index = baml_compiler2_hir::file_semantic_index(db, file);
    let scope = &index.scopes[file_scope.index() as usize];

    // Get package items for cross-file resolution
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package.clone());
    let pkg_items = package_items(db, pkg_id);

    let aliases = collect_type_aliases(db, pkg_items);
    let context = InferContext::new(db, scope_id);
    let mut builder = TypeInferenceBuilder::new(context, pkg_items, pkg_id, scope_id, aliases);

    // Dispatch based on scope kind
    match &scope.kind {
        ScopeKind::Function => {
            // Find the function by matching scope range against item_tree functions.
            // This works for both top-level functions AND class methods.
            let item_tree = baml_compiler2_hir::file_item_tree(db, file);
            let mut found = false;
            for (local_id, func_data) in &item_tree.functions {
                if func_data.span == scope.range {
                    let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(db, file, *local_id);
                    let body = baml_compiler2_hir::body::function_body(db, func_loc);
                    let sig = baml_compiler2_hir::signature::function_signature(db, func_loc);

                    if let FunctionBody::Expr(expr_body) = body.as_ref() {
                        // Get declared return type
                        let mut diags = Vec::new();
                        let return_ty = sig
                            .return_type
                            .as_ref()
                            .map(|te| {
                                crate::lower_type_expr::lower_type_expr(
                                    db, te, pkg_items, &mut diags,
                                )
                            })
                            .unwrap_or(Ty::Unknown);

                        // Report unresolved type diagnostics for return type
                        if !diags.is_empty() {
                            let sig_sm =
                                baml_compiler2_hir::signature::function_signature_source_map(
                                    db, func_loc,
                                );
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
                        let sig_sm = baml_compiler2_hir::signature::function_signature_source_map(
                            db, func_loc,
                        );
                        for (i, (param_name, param_te)) in sig.params.iter().enumerate() {
                            let param_ty = if param_name.as_str() == "self"
                                && matches!(param_te, baml_compiler2_ast::TypeExpr::Unknown)
                            {
                                // `self` parameter with no type annotation — infer from enclosing class
                                enclosing_class_name
                                    .as_ref()
                                    .and_then(|cn| {
                                        // Look up the class to get its definition's package
                                        pkg_items.lookup_type(&[cn.clone()]).map(|def| {
                                            Ty::Class(crate::lower_type_expr::qualify_def(
                                                db, def, cn,
                                            ))
                                        })
                                    })
                                    .unwrap_or(Ty::Unknown)
                            } else {
                                let mut param_diags = Vec::new();
                                let ty = crate::lower_type_expr::lower_type_expr(
                                    db,
                                    param_te,
                                    pkg_items,
                                    &mut param_diags,
                                );
                                if !param_diags.is_empty() {
                                    let span =
                                        sig_sm.param_spans.get(i).copied().unwrap_or_default();
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
            // Lambda bodies are handled when the enclosing function walks its ExprBody.
            // When the builder encounters a lambda, it stops — the lambda scope
            // gets its own infer_scope_types invocation later.
            // For now, lambda scope inference is a placeholder.
        }
        ScopeKind::Class => {
            // Class scope: no expressions to type-check.
            // Fields are resolved by resolve_class_fields.
            // Methods are child Function scopes with their own infer_scope_types.
        }
        _ => {
            // Project, Package, Namespace, File, Enum, TypeAlias, Block, Item:
            // typically no expressions to infer at these scope levels.
        }
    }

    let (expressions, bindings, diagnostics) = builder.finish();

    let extra = if diagnostics.is_empty() {
        None
    } else {
        Some(Box::new(ScopeInferenceExtra { diagnostics }))
    };

    ScopeInference {
        expressions,
        bindings,
        extra,
    }
}

// ── Type Alias Collection ────────────────────────────────────────────────────

/// Build a map of alias name → resolved Ty from all type aliases in the package.
fn collect_type_aliases<'db>(
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
    let pkg_items = package_items(db, pkg_id);
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
    let pkg_items = package_items(db, pkg_id);
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
                classes.insert(qualified, resolved.fields.clone());
            }
        }
    }
    classes
}

// ── Per-Item Queries ────────────────────────────────────────────────────────

/// Resolved class fields — `TypeExpr` resolved to `Ty` for each field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedClassFields {
    pub fields: Vec<(Name, Ty)>,
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
    let item_tree = baml_compiler2_hir::file_item_tree(db, file);
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package.clone());
    let pkg_items = package_items(db, pkg_id);

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
                    let ty = crate::lower_type_expr::lower_type_expr(
                        db, &te.expr, pkg_items, &mut diags,
                    );
                    for d in diags {
                        all_diags.push((d, te.span));
                    }
                    ty
                })
                .unwrap_or(Ty::Unknown);
            (f.name.clone(), ty)
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
    let item_tree = baml_compiler2_hir::file_item_tree(db, file);
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package.clone());
    let pkg_items = package_items(db, pkg_id);

    let alias_data = &item_tree[alias_loc.id(db)];
    let mut all_diags = Vec::new();
    let ty = alias_data
        .type_expr
        .as_ref()
        .map(|te| {
            let mut diags = Vec::new();
            let ty = crate::lower_type_expr::lower_type_expr(db, &te.expr, pkg_items, &mut diags);
            for d in diags {
                all_diags.push((d, te.span));
            }
            ty
        })
        .unwrap_or(Ty::Unknown);

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
    let index = baml_compiler2_hir::file_semantic_index(db, file);
    let scope = &index.scopes[file_scope.index() as usize];
    let item_tree = baml_compiler2_hir::file_item_tree(db, file);

    let source_map = item_tree
        .functions
        .iter()
        .find(|(_, f)| f.span == scope.range)
        .and_then(|(local_id, _)| {
            let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(db, file, *local_id);
            baml_compiler2_hir::body::function_body_source_map(db, func_loc)
        });

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
pub fn collect_file_diagnostics<'db>(
    db: &'db dyn crate::Db,
    file: baml_base::SourceFile,
) -> TypeCheckDiagnostics<'db> {
    let index = baml_compiler2_hir::file_semantic_index(db, file);
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
