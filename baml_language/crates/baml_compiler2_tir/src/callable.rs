use std::collections::BTreeSet;

use baml_base::Name;
use baml_compiler2_ast::{Expr, ExprBody, Stmt};
use baml_compiler2_hir::{
    body::FunctionBody, file_package, loc::FunctionLoc, package::PackageId, scope::ScopeKind,
};
use rustc_hash::FxHashMap;

use crate::{
    inference::{MemberResolution, ScopeInference, infer_scope_types},
    lower_type_expr::lower_type_expr_in_ns,
    package_interface::package_resolution_context,
    throw_inference::{flatten_ty_to_facts, function_throw_sets, throw_set_key},
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

    let segments = expr_to_path_segments(callee_expr_id, body)?;
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

fn collect_value_throw_facts<'db>(
    inference: &ScopeInference<'db>,
    value_expr_id: baml_compiler2_ast::ExprId,
    out: &mut BTreeSet<Ty>,
) {
    let thrown_ty = inference
        .expression_type(value_expr_id)
        .cloned()
        .unwrap_or(Ty::Unknown {
            attr: TyAttr::default(),
        });
    out.extend(flatten_ty_to_facts(&thrown_ty));
}

fn callee_uses_method_call_convention<'db>(
    inference: &ScopeInference<'db>,
    callee_expr_id: baml_compiler2_ast::ExprId,
) -> bool {
    matches!(
        inference.resolution(callee_expr_id),
        Some(MemberResolution::BoundMethod { .. } | MemberResolution::UnboundMethod { .. })
    ) || matches!(
        inference
            .path_member_resolution(callee_expr_id)
            .and_then(|resolutions| resolutions.last()),
        Some(MemberResolution::BoundMethod { .. } | MemberResolution::UnboundMethod { .. })
    )
}

pub(crate) fn instantiated_callee_throws<'db>(
    inference: &ScopeInference<'db>,
    callee_expr_id: baml_compiler2_ast::ExprId,
    args: &[baml_compiler2_ast::ExprId],
    unwrap_optional_callee: bool,
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
    for ((_, param_ty), arg_expr_id) in effective_params.iter().zip(args.iter()) {
        let arg_ty = inference
            .expression_type(*arg_expr_id)
            .cloned()
            .unwrap_or(Ty::Unknown {
                attr: TyAttr::default(),
            });
        crate::generics::infer_bindings_allow_typevars(param_ty, &arg_ty, &mut bindings);
    }

    Some(crate::generics::substitute_ty(&throws, &bindings))
}

fn collect_callee_escaping_throws<'db>(
    db: &'db dyn crate::Db,
    pkg_id: PackageId<'db>,
    ns_context: &[Name],
    inference: &ScopeInference<'db>,
    callee_expr_id: baml_compiler2_ast::ExprId,
    args: &[baml_compiler2_ast::ExprId],
    body: &ExprBody,
    unwrap_optional_callee: bool,
    out: &mut BTreeSet<Ty>,
) {
    let mut accounted = false;

    if let Some(throws) =
        instantiated_callee_throws(inference, callee_expr_id, args, unwrap_optional_callee)
    {
        out.extend(flatten_ty_to_facts(&throws));
        accounted = true;
    }

    if !accounted
        && let Some(key) = named_callee_key(db, pkg_id, ns_context, inference, callee_expr_id, body)
    {
        if let Some(summary) = lookup_named_throw_summary(db, pkg_id, &key) {
            out.extend(summary);
            accounted = true;
        }
    }

    if !accounted {
        out.insert(Ty::Unknown {
            attr: TyAttr::default(),
        });
    }
}

fn collect_callable_throws_from_stmt<'db>(
    db: &'db dyn crate::Db,
    pkg_id: PackageId<'db>,
    ns_context: &[Name],
    inference: &ScopeInference<'db>,
    stmt_id: baml_compiler2_ast::StmtId,
    body: &ExprBody,
    out: &mut BTreeSet<Ty>,
) {
    match &body.stmts[stmt_id] {
        Stmt::Expr(expr_id) => collect_callable_throws_from_expr(
            db, pkg_id, ns_context, inference, *expr_id, body, out,
        ),
        Stmt::Let { initializer, .. } => {
            if let Some(init) = initializer {
                collect_callable_throws_from_expr(
                    db, pkg_id, ns_context, inference, *init, body, out,
                );
            }
        }
        Stmt::While {
            condition,
            body: while_body,
            after,
            ..
        } => {
            collect_callable_throws_from_expr(
                db, pkg_id, ns_context, inference, *condition, body, out,
            );
            collect_callable_throws_from_expr(
                db,
                pkg_id,
                ns_context,
                inference,
                *while_body,
                body,
                out,
            );
            if let Some(after_stmt) = after {
                collect_callable_throws_from_stmt(
                    db,
                    pkg_id,
                    ns_context,
                    inference,
                    *after_stmt,
                    body,
                    out,
                );
            }
        }
        Stmt::For {
            collection,
            body: for_body,
            ..
        } => {
            collect_callable_throws_from_expr(
                db,
                pkg_id,
                ns_context,
                inference,
                *collection,
                body,
                out,
            );
            collect_callable_throws_from_expr(
                db, pkg_id, ns_context, inference, *for_body, body, out,
            );
        }
        Stmt::Return(expr) => {
            if let Some(expr) = expr {
                collect_callable_throws_from_expr(
                    db, pkg_id, ns_context, inference, *expr, body, out,
                );
            }
        }
        Stmt::Assign { target, value } | Stmt::AssignOp { target, value, .. } => {
            collect_callable_throws_from_expr(
                db, pkg_id, ns_context, inference, *target, body, out,
            );
            collect_callable_throws_from_expr(db, pkg_id, ns_context, inference, *value, body, out);
        }
        Stmt::Throw { value } => {
            collect_callable_throws_from_expr(db, pkg_id, ns_context, inference, *value, body, out);
            collect_value_throw_facts(inference, *value, out);
        }
        Stmt::Break | Stmt::Continue | Stmt::Missing | Stmt::HeaderComment { .. } => {}
    }
}

fn collect_callable_throws_from_expr<'db>(
    db: &'db dyn crate::Db,
    pkg_id: PackageId<'db>,
    ns_context: &[Name],
    inference: &ScopeInference<'db>,
    expr_id: baml_compiler2_ast::ExprId,
    body: &ExprBody,
    out: &mut BTreeSet<Ty>,
) {
    match &body.exprs[expr_id] {
        Expr::Throw { value } => {
            collect_callable_throws_from_expr(db, pkg_id, ns_context, inference, *value, body, out);
            collect_value_throw_facts(inference, *value, out);
        }
        Expr::Call { callee, args } => {
            collect_callable_throws_from_expr(
                db, pkg_id, ns_context, inference, *callee, body, out,
            );
            for arg in args {
                collect_callable_throws_from_expr(
                    db, pkg_id, ns_context, inference, *arg, body, out,
                );
            }
            collect_callee_escaping_throws(
                db, pkg_id, ns_context, inference, *callee, args, body, false, out,
            );
        }
        Expr::OptionalCall { callee, args } => {
            collect_callable_throws_from_expr(
                db, pkg_id, ns_context, inference, *callee, body, out,
            );
            for arg in args {
                collect_callable_throws_from_expr(
                    db, pkg_id, ns_context, inference, *arg, body, out,
                );
            }
            collect_callee_escaping_throws(
                db, pkg_id, ns_context, inference, *callee, args, body, true, out,
            );
        }

        Expr::Catch { base: _, clauses } => {
            if let Some(residual) = inference.catch_residual_throws(expr_id) {
                out.extend(residual.iter().cloned());
            }
            for clause in clauses {
                for arm_id in &clause.arms {
                    let arm = &body.catch_arms[*arm_id];
                    collect_callable_throws_from_expr(
                        db, pkg_id, ns_context, inference, arm.body, body, out,
                    );
                }
            }
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_callable_throws_from_expr(
                db, pkg_id, ns_context, inference, *condition, body, out,
            );
            collect_callable_throws_from_expr(
                db,
                pkg_id,
                ns_context,
                inference,
                *then_branch,
                body,
                out,
            );
            if let Some(else_expr) = else_branch {
                collect_callable_throws_from_expr(
                    db, pkg_id, ns_context, inference, *else_expr, body, out,
                );
            }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            collect_callable_throws_from_expr(
                db, pkg_id, ns_context, inference, *scrutinee, body, out,
            );
            for arm_id in arms {
                let arm = &body.match_arms[*arm_id];
                if let Some(guard) = arm.guard {
                    collect_callable_throws_from_expr(
                        db, pkg_id, ns_context, inference, guard, body, out,
                    );
                }
                collect_callable_throws_from_expr(
                    db, pkg_id, ns_context, inference, arm.body, body, out,
                );
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_callable_throws_from_expr(db, pkg_id, ns_context, inference, *lhs, body, out);
            collect_callable_throws_from_expr(db, pkg_id, ns_context, inference, *rhs, body, out);
        }
        Expr::Unary { expr, .. } | Expr::OptionalChain { expr } => {
            collect_callable_throws_from_expr(db, pkg_id, ns_context, inference, *expr, body, out);
        }
        Expr::Object {
            fields, spreads, ..
        } => {
            for (_, value) in fields {
                collect_callable_throws_from_expr(
                    db, pkg_id, ns_context, inference, *value, body, out,
                );
            }
            for spread in spreads {
                collect_callable_throws_from_expr(
                    db,
                    pkg_id,
                    ns_context,
                    inference,
                    spread.expr,
                    body,
                    out,
                );
            }
        }
        Expr::Array { elements } => {
            for elem in elements {
                collect_callable_throws_from_expr(
                    db, pkg_id, ns_context, inference, *elem, body, out,
                );
            }
        }
        Expr::Map { entries } => {
            for (key, value) in entries {
                collect_callable_throws_from_expr(
                    db, pkg_id, ns_context, inference, *key, body, out,
                );
                collect_callable_throws_from_expr(
                    db, pkg_id, ns_context, inference, *value, body, out,
                );
            }
        }
        Expr::Block { stmts, tail_expr } => {
            for stmt_id in stmts {
                collect_callable_throws_from_stmt(
                    db, pkg_id, ns_context, inference, *stmt_id, body, out,
                );
            }
            if let Some(tail) = tail_expr {
                collect_callable_throws_from_expr(
                    db, pkg_id, ns_context, inference, *tail, body, out,
                );
            }
        }
        Expr::MemberAccess { base, .. } | Expr::OptionalMemberAccess { base, .. } => {
            collect_callable_throws_from_expr(db, pkg_id, ns_context, inference, *base, body, out);
        }
        Expr::Index { base, index } | Expr::OptionalIndex { base, index } => {
            collect_callable_throws_from_expr(db, pkg_id, ns_context, inference, *base, body, out);
            collect_callable_throws_from_expr(db, pkg_id, ns_context, inference, *index, body, out);
        }
        Expr::Lambda(_)
        | Expr::Literal(_)
        | Expr::ByteStringLiteral(_)
        | Expr::Null
        | Expr::Path(_)
        | Expr::Missing => {}
    }
}

fn expr_to_path_segments(
    expr_id: baml_compiler2_ast::ExprId,
    body: &ExprBody,
) -> Option<Vec<Name>> {
    match &body.exprs[expr_id] {
        Expr::Path(segments) if !segments.is_empty() => Some(segments.clone()),
        Expr::MemberAccess { base, member } => {
            let mut base_segments = expr_to_path_segments(*base, body)?;
            base_segments.push(member.clone());
            Some(base_segments)
        }
        _ => None,
    }
}

fn callable_throws_cycle_initial<'db>(
    db: &'db dyn crate::Db,
    _id: salsa::Id,
    function: FunctionLoc<'db>,
) -> Ty {
    lowered_declared_callable_throws(db, function).unwrap_or(Ty::Never {
        attr: TyAttr::default(),
    })
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
            let mut facts = BTreeSet::new();
            if let Some(root_expr) = body.root_expr {
                collect_callable_throws_from_expr(
                    db,
                    pkg_id,
                    &pkg_info.namespace_path,
                    &inference,
                    root_expr,
                    body,
                    &mut facts,
                );
            }
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
