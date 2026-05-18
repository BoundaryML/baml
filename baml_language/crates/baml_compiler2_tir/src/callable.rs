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
    ty::{Ty, TyAttr},
};

fn join_throw_facts(facts: &BTreeSet<Ty>) -> Ty {
    if facts.is_empty() {
        return Ty::Never {
            attr: TyAttr::default(),
        };
    }
    let tys: Vec<Ty> = facts.iter().cloned().collect();
    match tys.as_slice() {
        [single] => single.clone(),
        _ => Ty::Union(tys, TyAttr::default()),
    }
}

fn lowered_declared_callable_throws<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> Option<Ty> {
    let file = function.file(db);
    let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
    let sig = baml_compiler2_ppir::elaborated_function_signature(db, function);
    let pkg_info = file_package::file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package.clone());
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);

    let mut generic_params = enclosing_class_generic_params(&item_tree, function.id(db));
    generic_params.extend(sig.user_generic_params.iter().cloned());
    generic_params.extend(sig.synthetic_effect_params.iter().cloned());

    sig.throws.as_ref().map(|declared_throws| {
        let mut diags = Vec::new();
        lower_type_expr_in_ns(
            db,
            declared_throws,
            pkg_items,
            &pkg_info.namespace_path,
            &generic_params,
            &mut diags,
        )
    })
}

fn signature_cycle_initial_callable_throws<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> Ty {
    let file = function.file(db);
    let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
    let sig = baml_compiler2_ppir::elaborated_function_signature(db, function);
    let pkg_info = file_package::file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package.clone());
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);

    let mut generic_params = enclosing_class_generic_params(&item_tree, function.id(db));
    generic_params.extend(sig.user_generic_params.iter().cloned());
    generic_params.extend(sig.synthetic_effect_params.iter().cloned());

    let mut facts = BTreeSet::new();
    for param in &sig.params {
        let mut diags = Vec::new();
        let lowered = lower_type_expr_in_ns(
            db,
            &param.ty,
            pkg_items,
            &pkg_info.namespace_path,
            &generic_params,
            &mut diags,
        );
        if let Ty::Function { throws, .. } = lowered {
            facts.extend(crate::throw_inference::flatten_ty_to_facts(&throws));
        }
    }

    join_throw_facts(&facts)
}

fn enclosing_class_generic_params(
    item_tree: &baml_compiler2_hir::item_tree::ItemTree,
    function_id: baml_compiler2_hir::ids::LocalItemId<baml_compiler2_hir::ids::FunctionMarker>,
) -> Vec<Name> {
    item_tree
        .classes
        .values()
        .find(|class_data| class_data.methods.contains(&function_id))
        .map(|class_data| class_data.generic_params.clone())
        .unwrap_or_default()
}

fn callable_short_name<'db>(db: &'db dyn crate::Db, function: FunctionLoc<'db>) -> Name {
    let file = function.file(db);
    let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
    let func_data = &item_tree[function.id(db)];

    if let Some(class_data) = item_tree
        .classes
        .values()
        .find(|class_data| class_data.methods.contains(&function.id(db)))
    {
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
    if let Some(
        MemberResolution::Free { func_loc }
        | MemberResolution::BoundMethod { func_loc, .. }
        | MemberResolution::UnboundMethod { func_loc, .. },
    ) = inference.resolution(callee_expr_id)
    {
        return Some(callable_key(db, *func_loc));
    }

    if let Some(
        MemberResolution::BoundMethod { func_loc, .. }
        | MemberResolution::UnboundMethod { func_loc, .. },
    ) = inference
        .path_member_resolution(callee_expr_id)
        .and_then(|resolutions| resolutions.last())
    {
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

fn lookup_named_throw_summary<'db>(
    db: &'db dyn crate::Db,
    pkg_id: PackageId<'db>,
    key: &Name,
) -> Option<BTreeSet<Ty>> {
    let own = function_throw_sets(db, pkg_id);
    if let Some(throws) = own.transitive_for(key) {
        return Some(throws.clone());
    }

    let res_ctx = package_resolution_context(db, pkg_id);
    for (_dep_name, dep_iface) in &res_ctx.dep_interfaces {
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
        let call_plan = self.inference.call_plan_for_provided_args(args);
        instantiated_callee_throws(
            self.inference,
            callee_expr_id,
            args,
            unwrap_optional_callee,
            call_plan,
        )
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
        lookup_named_throw_summary(self.db, self.pkg_id, &key)
    }
}

fn callee_uses_method_call_convention(
    inference: &ScopeInference<'_>,
    callee_expr_id: baml_compiler2_ast::ExprId,
) -> bool {
    matches!(
        inference.resolution(callee_expr_id),
        Some(MemberResolution::BoundMethod { .. })
    ) || matches!(
        inference
            .path_member_resolution(callee_expr_id)
            .and_then(|resolutions| resolutions.last()),
        Some(MemberResolution::BoundMethod { .. })
    )
}

pub(crate) fn instantiated_callee_throws(
    inference: &ScopeInference<'_>,
    callee_expr_id: baml_compiler2_ast::ExprId,
    args: &[baml_compiler2_ast::ExprId],
    unwrap_optional_callee: bool,
    call_plan: Option<&CallPlan>,
) -> Option<Ty> {
    let callee_ty = inference.expression_type(callee_expr_id)?;
    let typed_callee = if unwrap_optional_callee {
        crate::narrowing::remove_null(callee_ty)
    } else {
        callee_ty.clone()
    };

    let Ty::Function { params, throws, .. } = typed_callee else {
        return None;
    };

    let effective_params = if callee_uses_method_call_convention(inference, callee_expr_id) {
        crate::generics::skip_self_param(&params)
    } else {
        params.as_slice()
    };

    let mut bindings: FxHashMap<Name, Ty> = FxHashMap::default();
    if let Some(call_plan) = call_plan {
        for (param_index, arg_expr_id) in call_plan.provided_param_args() {
            let Some(param) = effective_params.get(param_index) else {
                continue;
            };
            let arg_ty = inference
                .expression_type(arg_expr_id)
                .cloned()
                .unwrap_or(Ty::Unknown {
                    attr: TyAttr::default(),
                });
            crate::generics::infer_bindings_allow_typevars(&param.ty, &arg_ty, &mut bindings);
        }
    } else {
        for (param, arg_expr_id) in effective_params.iter().zip(args.iter()) {
            let arg_ty = inference
                .expression_type(*arg_expr_id)
                .cloned()
                .unwrap_or(Ty::Unknown {
                    attr: TyAttr::default(),
                });
            crate::generics::infer_bindings_allow_typevars(&param.ty, &arg_ty, &mut bindings);
        }
    }

    Some(crate::generics::substitute_ty(&throws, &bindings))
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
                return Ty::Unknown {
                    attr: TyAttr::default(),
                };
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
        FunctionBody::Builtin(_) => Ty::Never {
            attr: TyAttr::default(),
        },
        FunctionBody::Missing => Ty::Unknown {
            attr: TyAttr::default(),
        },
    }
}
