use std::collections::BTreeSet;

use baml_base::Name;
use baml_compiler2_ast::ExprBody;
use baml_compiler2_hir::{
    body::FunctionBody, file_package, loc::FunctionLoc, package::PackageId, scope::ScopeKind,
};
use rustc_hash::FxHashMap;

use crate::{
    inference::{CallPlan, MemberResolution, ScopeInference, infer_scope_types},
    lower_type_expr::lower_type_expr_in_ns,
    package_interface::package_resolution_context,
    throw_inference::{function_throw_sets, throw_set_key},
    throws_analysis::ThrowsAnalysisContext,
    ty::Ty,
};

fn join_throw_facts(facts: &BTreeSet<Ty>) -> Ty {
    if facts.is_empty() {
        return Ty::Never;
    }
    let tys: Vec<Ty> = facts.iter().cloned().collect();
    match tys.as_slice() {
        [single] => single.clone(),
        _ => Ty::Union(tys),
    }
}

/// Shared signature-lowering context for the declared/cycle-initial throws paths.
struct SignatureLoweringCtx<'db> {
    db: &'db dyn crate::Db,
    sig: std::sync::Arc<baml_compiler2_hir::signature::ElaboratedFunctionSignature>,
    pkg_info: baml_compiler2_hir::file_package::PackageInfo,
    pkg_items: &'db baml_compiler2_hir::package::PackageItems<'db>,
    generic_params: Vec<Name>,
}

impl<'db> SignatureLoweringCtx<'db> {
    fn new(db: &'db dyn crate::Db, function: FunctionLoc<'db>) -> Self {
        let file = function.file(db);
        let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
        let sig = baml_compiler2_ppir::elaborated_function_signature(db, function);
        let pkg_info = file_package::file_package(db, file);
        let pkg_id = PackageId::new(db, pkg_info.package.clone());
        let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);

        let mut generic_params = enclosing_class_generic_params(&item_tree, function.id(db));
        generic_params.extend(sig.user_generic_params.iter().cloned());
        generic_params.extend(sig.synthetic_effect_params.iter().cloned());

        Self {
            db,
            sig,
            pkg_info,
            pkg_items,
            generic_params,
        }
    }

    fn lower(&self, type_expr: &baml_compiler2_ast::TypeExpr) -> Ty {
        let mut diags = Vec::new();
        lower_type_expr_in_ns(
            self.db,
            type_expr,
            self.pkg_items,
            &self.pkg_info.namespace_path,
            &self.generic_params,
            &mut diags,
        )
    }
}

fn lowered_declared_callable_throws<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> Option<Ty> {
    let ctx = SignatureLoweringCtx::new(db, function);
    ctx.sig
        .throws
        .as_ref()
        .map(|declared_throws| ctx.lower(declared_throws))
}

fn signature_cycle_initial_callable_throws<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> Ty {
    let ctx = SignatureLoweringCtx::new(db, function);

    let mut facts = BTreeSet::new();
    for param in &ctx.sig.params {
        if let Ty::Function { throws, .. } = ctx.lower(&param.ty) {
            facts.extend(crate::throw_inference::flatten_ty_to_facts(&throws));
        }
    }

    join_throw_facts(&facts)
}

fn enclosing_class(
    item_tree: &baml_compiler2_hir::item_tree::ItemTree,
    function_id: baml_compiler2_hir::ids::LocalItemId<baml_compiler2_hir::ids::FunctionMarker>,
) -> Option<&baml_compiler2_hir::item_tree::Class> {
    item_tree
        .classes
        .values()
        .find(|class_data| class_data.methods.contains(&function_id))
}

fn enclosing_class_generic_params(
    item_tree: &baml_compiler2_hir::item_tree::ItemTree,
    function_id: baml_compiler2_hir::ids::LocalItemId<baml_compiler2_hir::ids::FunctionMarker>,
) -> Vec<Name> {
    enclosing_class(item_tree, function_id)
        .map(|class_data| class_data.generic_params.clone())
        .unwrap_or_default()
}

fn callable_short_name<'db>(db: &'db dyn crate::Db, function: FunctionLoc<'db>) -> Name {
    let file = function.file(db);
    let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
    let func_data = &item_tree[function.id(db)];

    if let Some(class_data) = enclosing_class(&item_tree, function.id(db)) {
        Name::new(format!("{}.{}", class_data.name, func_data.name))
    } else {
        func_data.name.clone()
    }
}

fn callable_key<'db>(db: &'db dyn crate::Db, function: FunctionLoc<'db>) -> Name {
    let namespace = file_package::file_package(db, function.file(db)).namespace_path;
    let short_name = callable_short_name(db, function);
    throw_set_key(&namespace, &short_name)
}

fn function_scope_id<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> Option<baml_compiler2_hir::scope::ScopeId<'db>> {
    let file = function.file(db);
    let index = baml_compiler2_ppir::file_semantic_index(db, file);
    let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
    let func_data = &item_tree[function.id(db)];

    index.scope_ids.iter().copied().find(|scope_id| {
        let scope = &index.scopes[scope_id.file_scope_id(db).index() as usize];
        matches!(scope.kind, ScopeKind::Function)
            && scope.range == func_data.span
            && scope.name.as_ref() == Some(&func_data.name)
    })
}

fn named_callee_key<'db>(
    db: &'db dyn crate::Db,
    pkg_id: PackageId<'db>,
    ns_context: &[Name],
    inference: &ScopeInference<'db>,
    callee_expr_id: baml_compiler2_ast::ExprId,
    body: &ExprBody,
) -> Option<Name> {
    let direct_func_loc = match inference.resolution(callee_expr_id) {
        Some(
            MemberResolution::Free { func_loc }
            | MemberResolution::BoundMethod { func_loc, .. }
            | MemberResolution::UnboundMethod { func_loc, .. },
        ) => Some(func_loc),
        _ => None,
    };
    let func_loc = direct_func_loc.or_else(|| {
        match inference
            .path_member_resolution(callee_expr_id)
            .and_then(|resolutions| resolutions.last())
        {
            Some(
                MemberResolution::BoundMethod { func_loc, .. }
                | MemberResolution::UnboundMethod { func_loc, .. },
            ) => Some(func_loc),
            _ => None,
        }
    });
    if let Some(func_loc) = func_loc {
        return Some(callable_key(db, *func_loc));
    }

    let segments = crate::throws_analysis::expr_to_path_segments(callee_expr_id, body)?;
    let res_ctx = package_resolution_context(db, pkg_id);
    let (_, definition) = res_ctx.resolve_value(db, &segments, ns_context)?;
    let baml_compiler2_hir::contributions::Definition::Function(func_loc) = definition else {
        return None;
    };
    Some(callable_key(db, func_loc))
}

/// Look up the transitive throw set for `key`, checking the package's own
/// function throw sets first, then each dependency interface's throw sets.
///
/// Shared by `callable.rs` and `builder.rs`; `dep_interfaces` is passed in so
/// callers can use whichever resolution context they already hold.
pub(crate) fn lookup_named_throw_summary<'db>(
    db: &'db dyn crate::Db,
    pkg_id: PackageId<'db>,
    dep_interfaces: &[(Name, crate::package_interface::PackageInterface)],
    key: &Name,
) -> Option<BTreeSet<Ty>> {
    let own = function_throw_sets(db, pkg_id);
    if let Some(throws) = own.transitive_for(key) {
        return Some(throws.clone());
    }

    for (_dep_name, dep_iface) in dep_interfaces {
        if let Some(throws) = dep_iface.throw_sets.transitive_for(key) {
            return Some(throws.clone());
        }
    }

    None
}

struct CallableThrowsAnalysis<'a, 'db> {
    db: &'db dyn crate::Db,
    pkg_id: PackageId<'db>,
    ns_context: &'a [Name],
    inference: &'a ScopeInference<'db>,
}

impl ThrowsAnalysisContext for CallableThrowsAnalysis<'_, '_> {
    fn expression_type(&self, expr_id: baml_compiler2_ast::ExprId) -> Option<Ty> {
        self.inference.expression_type(expr_id).cloned()
    }

    fn catch_residual_throws(&self, expr_id: baml_compiler2_ast::ExprId) -> Option<BTreeSet<Ty>> {
        self.inference
            .catch_residual_throws(expr_id)
            .map(|residual| residual.iter().cloned().collect())
    }

    fn instantiated_callee_throws(
        &self,
        callee_expr_id: baml_compiler2_ast::ExprId,
        args: &[baml_compiler2_ast::ExprId],
        unwrap_optional_callee: bool,
    ) -> Option<Ty> {
        let inference = self.inference;
        let call_plan = inference.call_plan_for_provided_args(args);

        let callee_ty = inference.expression_type(callee_expr_id)?;
        let typed_callee = if unwrap_optional_callee {
            crate::narrowing::remove_null(callee_ty)
        } else {
            callee_ty.clone()
        };

        let Ty::Function { params, throws, .. } = typed_callee else {
            return None;
        };

        let uses_method_call_convention = crate::inference::uses_method_call_convention(
            inference.resolution(callee_expr_id),
            inference
                .path_member_resolution(callee_expr_id)
                .and_then(<[_]>::last),
        );
        let effective_params = if uses_method_call_convention {
            crate::generics::skip_self_param(&params)
        } else {
            params.as_slice()
        };

        Some(substitute_throws_with_inferred_bindings(
            effective_params,
            &throws,
            args,
            call_plan,
            |arg_expr_id| {
                inference
                    .expression_type(arg_expr_id)
                    .cloned()
                    .unwrap_or(Ty::Unknown)
            },
        ))
    }

    fn named_callee_summary(
        &self,
        callee_expr_id: baml_compiler2_ast::ExprId,
        body: &ExprBody,
    ) -> Option<BTreeSet<Ty>> {
        let key = named_callee_key(
            self.db,
            self.pkg_id,
            self.ns_context,
            self.inference,
            callee_expr_id,
            body,
        )?;
        let res_ctx = package_resolution_context(self.db, self.pkg_id);
        lookup_named_throw_summary(self.db, self.pkg_id, &res_ctx.dep_interfaces, &key)
    }
}

/// Infer generic bindings from `(param, arg)` pairs (honoring an explicit
/// `call_plan` when present, else positional zip) and substitute them into
/// `throws`. `arg_ty` resolves each argument `ExprId` to its inferred type.
///
/// Shared by both `instantiated_callee_throws` implementations (here and in
/// `builder.rs`) so the binding/substitution tail lives in one place.
pub(crate) fn substitute_throws_with_inferred_bindings(
    effective_params: &[crate::ty::FunctionParamTy],
    throws: &Ty,
    args: &[baml_compiler2_ast::ExprId],
    call_plan: Option<&CallPlan>,
    mut arg_ty: impl FnMut(baml_compiler2_ast::ExprId) -> Ty,
) -> Ty {
    let mut bindings: FxHashMap<Name, Ty> = FxHashMap::default();
    if let Some(call_plan) = call_plan {
        for (param_index, arg_expr_id) in call_plan.provided_param_args() {
            let Some(param) = effective_params.get(param_index) else {
                continue;
            };
            crate::generics::infer_bindings_allow_typevars(
                &param.ty,
                &arg_ty(arg_expr_id),
                &mut bindings,
            );
        }
    } else {
        for (param, arg_expr_id) in effective_params.iter().zip(args.iter()) {
            crate::generics::infer_bindings_allow_typevars(
                &param.ty,
                &arg_ty(*arg_expr_id),
                &mut bindings,
            );
        }
    }
    crate::generics::substitute_ty(throws, &bindings)
}

fn callable_throws_cycle_initial<'db>(
    db: &'db dyn crate::Db,
    _id: salsa::Id,
    function: FunctionLoc<'db>,
) -> Ty {
    lowered_declared_callable_throws(db, function)
        .unwrap_or_else(|| signature_cycle_initial_callable_throws(db, function))
}

#[salsa::tracked(returns(ref), cycle_initial=callable_throws_cycle_initial)]
pub fn callable_throws<'db>(db: &'db dyn crate::Db, function: FunctionLoc<'db>) -> Ty {
    if let Some(declared_throws) = lowered_declared_callable_throws(db, function) {
        return declared_throws;
    }

    let file = function.file(db);
    let pkg_info = file_package::file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package.clone());

    match baml_compiler2_ppir::function_body(db, function).as_ref() {
        FunctionBody::Expr(body) => {
            let Some(scope_id) = function_scope_id(db, function) else {
                return Ty::Unknown;
            };
            let inference = infer_scope_types(db, scope_id);
            let facts = crate::throws_analysis::collect_escaping_throws(
                &CallableThrowsAnalysis {
                    db,
                    pkg_id,
                    ns_context: &pkg_info.namespace_path,
                    inference,
                },
                body,
            );
            join_throw_facts(&facts)
        }
        FunctionBody::Builtin(_) => Ty::Never,
        FunctionBody::Missing => Ty::Unknown,
    }
}
