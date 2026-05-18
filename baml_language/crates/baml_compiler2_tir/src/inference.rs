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

use baml_base::Name;
use baml_compiler2_ast::{
    self as ast, AstSourceMap, Expr as AstExpr, ExprBody, ExprId, FunctionDef, PatId,
};
use baml_compiler2_hir::{
    body::{FunctionBody, LetBody},
    contributions::Definition,
    loc::{ClassLoc, EnumLoc, FunctionLoc, LetLoc, TypeAliasLoc},
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

fn inference_owner_scope(
    index: &baml_compiler2_hir::semantic_index::FileSemanticIndex<'_>,
    mut scope_id: FileScopeId,
) -> FileScopeId {
    loop {
        let scope = &index.scopes[scope_id.index() as usize];
        if matches!(
            scope.kind,
            ScopeKind::Function | ScopeKind::Let | ScopeKind::Lambda
        ) {
            return scope_id;
        }
        let Some(parent) = scope.parent else {
            return scope_id;
        };
        scope_id = parent;
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
    /// Lambda/function parameter types by index (name, inferred type).
    /// Populated for lambda scopes so LSP can resolve unannotated lambda
    /// parameter types (e.g. `items.map((item) -> { item. })`).
    param_types: Vec<(Name, Ty)>,
    /// Full parameter binding plan for checked calls.
    call_plans: FxHashMap<ExprId, CallPlan>,
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
            function_coercions: FxHashMap::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallPlan {
    pub bindings: Vec<ParamBinding>,
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

    /// Iterate over all function adapters required by checked coercions.
    pub fn iter_function_coercions(&self) -> impl Iterator<Item = (&ExprId, &FunctionCoercion)> {
        self.function_coercions.iter()
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
        param_types: Vec::new(),
        call_plans: FxHashMap::default(),
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
                    let sig = baml_compiler2_ppir::elaborated_function_signature(db, func_loc);

                    // Compute the generic params for this function scope.
                    // If this is a method inside a class, also include the class's generic params.
                    let mut generic_params = sig.user_generic_params.clone();
                    generic_params.extend(sig.synthetic_effect_params.iter().cloned());
                    if let Some(parent_idx) = scope.parent {
                        let parent = &index.scopes[parent_idx.index() as usize];
                        if matches!(parent.kind, ScopeKind::Class) {
                            if let Some(class_name) = &parent.name {
                                for class_data in item_tree.classes.values() {
                                    if class_data.name == *class_name {
                                        // Check for method-level type params that shadow class-level ones.
                                        for mp in &sig.user_generic_params {
                                            if class_data.generic_params.iter().any(|cp| cp == mp) {
                                                builder.report_at_span(
                                                    crate::infer_context::TirTypeError::TypeParamShadowed {
                                                        param_name: mp.clone(),
                                                        class_name: class_name.clone(),
                                                    },
                                                    func_data.span,
                                                );
                                            }
                                        }
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
                    if let Some(sm) = baml_compiler2_ppir::function_body_source_map(db, func_loc) {
                        builder.set_body_source_map(sm);
                    }
                    builder.set_auto_derived(matches!(
                        func_data.origin,
                        ast::FunctionOrigin::AutoDerive
                    ));

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
                                baml_compiler2_ppir::elaborated_function_signature_source_map(
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
                        let sig_sm = baml_compiler2_ppir::elaborated_function_signature_source_map(
                            db, func_loc,
                        );
                        for (i, param) in sig.params.iter().enumerate() {
                            let param_ty = if param.name.as_str() == "self"
                                && matches!(param.ty, baml_compiler2_ast::TypeExpr::Unknown { .. })
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
                                    &param.ty,
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

                                    // Seed builder with lambda params.
                                    // Combine the enclosing function's generic params with the
                                    // lambda's own generic params so that `T` from
                                    // `function foo<T>() { ... || { reflect.type_of<T>() } }` is
                                    // visible inside the lambda body.
                                    //
                                    // Also include class-level generic params if the enclosing
                                    // function is a class method.  For a method on `class Box<T>`,
                                    // `func_data.generic_params` is empty (no function-level
                                    // generics), but a closure body inside `describe(self)` must
                                    // still resolve `T` from `reflect.type_of<T>()`.
                                    let mut generic_params: Vec<Name> = {
                                        let mut gp = func_data.generic_params.clone();
                                        if let Some(parent_fsi) = ancestor_scope.parent {
                                            let parent_scope =
                                                &index.scopes[parent_fsi.index() as usize];
                                            if matches!(parent_scope.kind, ScopeKind::Class) {
                                                if let Some(class_name) = &parent_scope.name {
                                                    for class_data in item_tree.classes.values() {
                                                        if class_data.name == *class_name {
                                                            let mut merged =
                                                                class_data.generic_params.clone();
                                                            merged.extend(gp);
                                                            gp = merged;
                                                            break;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        gp
                                    };
                                    for p in &func_def.generic_params {
                                        if !generic_params.contains(p) {
                                            generic_params.push(p.clone());
                                        }
                                    }
                                    builder.set_generic_params(generic_params.clone());
                                    for (i, param) in func_def.params.iter().enumerate() {
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
                                            .or_else(|| {
                                                // Fall back to contextual type from parent inference
                                                contextual_param_tys
                                                    .as_ref()
                                                    .and_then(|pts| pts.get(i))
                                                    .map(|param| param.ty.clone())
                                            })
                                            .unwrap_or(Ty::Unknown {
                                                attr: TyAttr::default(),
                                            });
                                        builder.add_local(param.name.clone(), param_ty.clone());
                                        builder.param_types.push((param.name.clone(), param_ty));
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

                                    // Seed builder with lambda params.
                                    // Mirror the Function-branch merge: a lambda assigned via
                                    // `let f = || { ... reflect.type_of<T>() }` inside a generic
                                    // method/function must still see the enclosing function's
                                    // generic params (and any class-level params if the
                                    // enclosing function is a method).  The `Let` ancestor
                                    // hides those; walk up to the nearest `Function` scope to
                                    // recover them.
                                    let mut generic_params: Vec<Name> = {
                                        let mut gp: Vec<Name> = Vec::new();
                                        let mut current = ancestor_scope.parent;
                                        while let Some(fsi) = current {
                                            let scope = &index.scopes[fsi.index() as usize];
                                            match &scope.kind {
                                                ScopeKind::Function => {
                                                    for fd in item_tree.functions.values() {
                                                        if fd.span == scope.range
                                                            && scope.name.as_ref() == Some(&fd.name)
                                                        {
                                                            gp.clone_from(&fd.generic_params);
                                                            if let Some(grandparent_fsi) =
                                                                scope.parent
                                                            {
                                                                let grandparent_scope = &index
                                                                    .scopes
                                                                    [grandparent_fsi.index()
                                                                        as usize];
                                                                if matches!(
                                                                    grandparent_scope.kind,
                                                                    ScopeKind::Class
                                                                ) {
                                                                    if let Some(class_name) =
                                                                        &grandparent_scope.name
                                                                    {
                                                                        for class_data in item_tree
                                                                            .classes
                                                                            .values()
                                                                        {
                                                                            if class_data.name
                                                                                == *class_name
                                                                            {
                                                                                let mut merged =
                                                                                    class_data
                                                                                        .generic_params
                                                                                        .clone();
                                                                                merged.extend(gp);
                                                                                gp = merged;
                                                                                break;
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            break;
                                                        }
                                                    }
                                                    break;
                                                }
                                                ScopeKind::Let => {
                                                    current = scope.parent;
                                                }
                                                _ => break,
                                            }
                                        }
                                        gp
                                    };
                                    for p in &func_def.generic_params {
                                        if !generic_params.contains(p) {
                                            generic_params.push(p.clone());
                                        }
                                    }
                                    builder.set_generic_params(generic_params.clone());
                                    for (i, param) in func_def.params.iter().enumerate() {
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
                                            .or_else(|| {
                                                contextual_param_tys
                                                    .as_ref()
                                                    .and_then(|pts| pts.get(i))
                                                    .map(|param| param.ty.clone())
                                            })
                                            .unwrap_or(Ty::Unknown {
                                                attr: TyAttr::default(),
                                            });
                                        builder.add_local(param.name.clone(), param_ty.clone());
                                        builder.param_types.push((param.name.clone(), param_ty));
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
        function_coercions,
        nested_lambda_types,
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
        param_types,
        call_plans,
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
