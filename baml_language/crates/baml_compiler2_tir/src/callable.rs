use std::collections::{BTreeSet, HashMap};

use baml_base::Name;
use baml_compiler2_ast::ExprBody;
use baml_compiler2_hir::{body::FunctionBody, file_package, loc::FunctionLoc, package::PackageId};
use rustc_hash::FxHashMap;

use crate::{
    inference::{CallPlan, MemberResolution, ScopeInference, infer_scope_types},
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
    let sig = baml_compiler2_ppir::item_data::elaborated_function_data(db, function);
    let pkg_info = file_package::file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package.clone());
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);

    let mut generic_params = item_tree
        .enclosing_type_generic_params(function.id(db))
        .to_vec();
    generic_params.extend(sig.user_generic_params.iter().cloned());
    generic_params.extend(sig.synthetic_effect_params.iter().cloned());

    sig.throws.map(|declared_throws| {
        let mut diags = Vec::new();
        crate::lower_type_expr::lower_type_ref(
            &sig.type_refs,
            declared_throws,
            &crate::lower_type_expr::ScopeCtx {
                db,
                package_items: pkg_items,
                ns_context: &pkg_info.namespace_path,
                generic_params: &generic_params,
                bounds: crate::lower_type_expr::function_in_scope_generic_param_bounds(
                    db, function,
                ),
                self_ty: None,
            },
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
    let sig = baml_compiler2_ppir::item_data::elaborated_function_data(db, function);
    let pkg_info = file_package::file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package.clone());
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);

    let mut generic_params = item_tree
        .enclosing_type_generic_params(function.id(db))
        .to_vec();
    generic_params.extend(sig.user_generic_params.iter().cloned());
    generic_params.extend(sig.synthetic_effect_params.iter().cloned());

    let param_scope = crate::lower_type_expr::ScopeCtx {
        db,
        package_items: pkg_items,
        ns_context: &pkg_info.namespace_path,
        generic_params: &generic_params,
        bounds: crate::lower_type_expr::function_in_scope_generic_param_bounds(db, function),
        self_ty: None,
    };
    let mut facts = BTreeSet::new();
    for param in &sig.params {
        let mut diags = Vec::new();
        let lowered = crate::lower_type_expr::lower_type_ref(
            &sig.type_refs,
            param.type_ref,
            &param_scope,
            &mut diags,
        );
        if let Ty::Function { throws, .. } = lowered {
            facts.extend(crate::throw_inference::flatten_ty_to_facts(&throws));
        }
    }

    join_throw_facts(&facts)
}

fn callable_short_name<'db>(db: &'db dyn crate::Db, function: FunctionLoc<'db>) -> Name {
    let file = function.file(db);
    let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
    let func_data = &item_tree[function.id(db)];

    // Only a class owner qualifies the name — interface default methods and
    // free-impl methods keep their bare name, preserving the throw-set key
    // format the scan produced.
    if let Some(baml_compiler2_hir::item_tree::MethodOwner::Class(class_id)) =
        item_tree.method_owners.get(&function.id(db))
    {
        Name::new(format!("{}.{}", item_tree[*class_id].name, func_data.name))
    } else {
        func_data.name.clone()
    }
}

fn callable_key<'db>(db: &'db dyn crate::Db, function: FunctionLoc<'db>) -> Name {
    let namespace = file_package::file_package(db, function.file(db)).namespace_path;
    let short_name = callable_short_name(db, function);
    throw_set_key(&namespace, &short_name)
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
    aliases: &'a HashMap<crate::ty::QualifiedTypeName, Ty>,
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
            self.aliases,
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

    fn runtime_id_set_throws(&self) -> Option<BTreeSet<Ty>> {
        // Namespace-relative key within the `baml` package — see the
        // builder impl.
        lookup_named_throw_summary(self.db, self.pkg_id, &Name::new("id.set"))
    }

    fn to_json_fallback_throws(&self) -> Option<BTreeSet<Ty>> {
        // `recv.to_json()` lowers to `baml.json.from(recv)` — see the builder impl.
        lookup_named_throw_summary(self.db, self.pkg_id, &Name::new("json.from"))
    }

    #[allow(clippy::wrong_self_convention)]
    fn from_json_fallback_throws(&self) -> Option<BTreeSet<Ty>> {
        // `Type.from_json(j)` lowers to `baml.json.to<Type>(j)` — see the builder impl.
        lookup_named_throw_summary(self.db, self.pkg_id, &Name::new("json.to"))
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
    aliases: &HashMap<crate::ty::QualifiedTypeName, Ty>,
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
    // A callee whose type is a function-type alias (e.g. a `TestSetBody`
    // parameter) must be resolved to its underlying `Ty::Function` before its
    // `throws` can be read; otherwise the match below falls through and the
    // caller fabricates an `Unknown` throw fact.
    let typed_callee = crate::inference::expand_alias_chains(typed_callee, aliases);

    let uses_method_convention = callee_uses_method_call_convention(inference, callee_expr_id);

    // Instantiate a single function callee's declared `throws`: bind its type
    // parameters from the argument types, then substitute them in.
    let function_throws = |params: &[_], throws: &Ty| -> Ty {
        let effective_params: &[_] = if uses_method_convention {
            crate::generics::skip_self_param(params)
        } else {
            params
        };
        let pairs: Vec<_> = if let Some(call_plan) = call_plan {
            call_plan
                .provided_param_args()
                .filter_map(|(param_index, arg)| {
                    effective_params.get(param_index).map(|param| (param, arg))
                })
                .collect()
        } else {
            effective_params.iter().zip(args.iter().copied()).collect()
        };
        let mut bindings: FxHashMap<Name, Ty> = FxHashMap::default();
        for (param, arg_expr_id) in pairs {
            let arg_ty = inference
                .expression_type(arg_expr_id)
                .cloned()
                .unwrap_or(Ty::Unknown {
                    attr: TyAttr::default(),
                });
            crate::generics::infer_bindings_allow_typevars(&param.ty, &arg_ty, &mut bindings);
        }
        crate::generics::substitute_ty(throws, &bindings)
    };

    match &typed_callee {
        Ty::Function { params, throws, .. } => Some(function_throws(params, throws)),
        // A method call dispatched over a union receiver (`(A | B).method()`)
        // resolves the callee to a union of the members' method types. The call
        // throws whatever the dispatched member throws, so join the members'
        // instantiated `throws` — falling back to a conservative `Ty::Unknown`
        // here would leave the function's `throws` un-resolvable at runtime.
        Ty::Union(members, attr) if members.iter().all(|m| matches!(m, Ty::Function { .. })) => {
            let member_throws: Vec<Ty> = members
                .iter()
                .filter_map(|member| match member {
                    Ty::Function { params, throws, .. } => Some(function_throws(params, throws)),
                    _ => None,
                })
                .collect();
            Some(Ty::Union(member_throws, attr.clone()))
        }
        _ => None,
    }
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
    // A seeded value from a previous compile short-circuits body
    // inference for a clean function, returning exactly the `Ty` this query
    // produced last time. `seeds.by_path(db)` is a *tracked* read of the
    // `SeededCallableThrows` input (present-from-construction, empty until
    // seeded), so a later seed reliably invalidates this memo. The seed is
    // keyed by (source path, item-tree `LocalItemId`) — process-independent for
    // byte-identical files. Only functions the reuse plan proved clean are
    // seeded, so a converged fixpoint value is returned without re-entering the
    // callee body; a dirty function is never in the map and infers below.
    if let Some(seeds) = db.seeded_callable_throws() {
        // `by_path(db)` is the tracked read (kept unconditional so a later seed
        // still invalidates this memo), but the path-display allocation and the
        // lookup are skipped whenever no seeds were injected — the LSP and every
        // cold CLI compile hold the empty map, so this guard avoids a per-eval
        // `String` allocation on the hot `callable_throws` path.
        let by_path = seeds.by_path(db);
        if !by_path.is_empty() {
            let path = function.file(db).path(db).display().to_string();
            if let Some(ty) = by_path
                .get(&path)
                .and_then(|by_id| by_id.get(&function.id(db).as_u32()))
            {
                return ty.clone();
            }
        }
    }

    if let Some(declared_throws) = lowered_declared_callable_throws(db, function) {
        return declared_throws;
    }

    let file = function.file(db);
    let pkg_info = file_package::file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package.clone());

    match baml_compiler2_ppir::function_body(db, function).as_ref() {
        FunctionBody::Expr(body) => {
            let Some(scope_id) = baml_compiler2_ppir::item_data::function_scope(db, function)
            else {
                return Ty::Unknown {
                    attr: TyAttr::default(),
                };
            };
            let inference = infer_scope_types(db, scope_id);
            let res_ctx = package_resolution_context(db, pkg_id);
            let aliases = crate::inference::package_alias_map(db, res_ctx);
            let facts = crate::throws_analysis::collect_escaping_throws(
                &CallableThrowsAnalysis {
                    db,
                    pkg_id,
                    ns_context: &pkg_info.namespace_path,
                    inference,
                    aliases: &aliases,
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
