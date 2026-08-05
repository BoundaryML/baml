use std::collections::{BTreeSet, HashMap};

use baml_base::Name;
use baml_compiler2_ast::{BuiltinKind, ExprBody};
use baml_compiler2_hir::{body::FunctionBody, file_package, loc::FunctionLoc, package::PackageId};
use rustc_hash::FxHashMap;

use crate::{
    inference::{CallPlan, MemberResolution, ScopeInference, infer_scope_types},
    package_interface::package_resolution_context,
    throw_inference::{function_throw_sets, throw_set_key},
    throws_analysis::ThrowsAnalysisContext,
    ty::{FunctionParamMode, FunctionParamTy, ParamTy, Ty, TyAttr},
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
    let sig = baml_compiler2_ppir::item_data::elaborated_function_data(db, function);
    let pkg_info = file_package::file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package.clone());
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);

    let generic_params = crate::generic_env::function_generic_env(db, function)
        .source_params()
        .to_vec();

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
    let sig = baml_compiler2_ppir::item_data::elaborated_function_data(db, function);
    let pkg_info = file_package::file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package.clone());
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);

    let generic_params = crate::generic_env::function_generic_env(db, function)
        .source_params()
        .to_vec();

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

// ── Declaration-site resolved signature ────────────────────────────────────

/// The declaration-site resolved signature of a function or method: every
/// annotation on the declaration lowered to a TIR [`Ty`], with `Self` bound to
/// the enclosing class's receiver type when there is one.
///
/// This is the *declared* surface only. The inferred throws contract is a
/// separate fact with its own convergence rules — pair this with
/// [`callable_throws`] (as [`crate::package_interface::ExportedFunction`]
/// does) when the effective contract is needed.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionSignatureTy {
    /// Parameters in declaration order, `self` included for instance methods
    /// (its type is the receiver type). A default-valued parameter is
    /// [`FunctionParamMode::Optional`].
    pub params: Vec<FunctionParamTy>,
    /// The declared return type; `Ty::Error` when no annotation was written —
    /// a declaration-site fact is either written or a malformed declaration,
    /// never an inference hole.
    pub return_type: Ty,
    /// The declared `throws` clause, lowered; `None` when omitted.
    pub declared_throws: Option<Ty>,
    /// The function's own generic parameters — user-declared plus the
    /// synthetic callback effect parameters introduced by bounded signature
    /// elaboration. Excludes the enclosing type's parameters.
    pub generic_params: Vec<ParamTy>,
    /// `Some` when the body is a `$rust_function`/`$rust_io_function`/
    /// `$compiler_intrinsic` marker rather than BAML code.
    pub builtin_kind: Option<BuiltinKind>,
}

impl FunctionSignatureTy {
    /// The signature as a `Ty::Function`, with the throws slot supplied by the
    /// caller (normally [`callable_throws`]; `Ty::Function` carries no generic
    /// parameters, so [`Self::generic_params`] survive as free `Ty::TypeVar`s
    /// for call-site inference to bind).
    pub fn to_function_ty(&self, throws: Ty) -> Ty {
        Ty::Function {
            params: self.params.clone(),
            ret: Box::new(self.return_type.clone()),
            throws: Box::new(throws),
            attr: TyAttr::default(),
        }
    }
}

/// Resolve the declaration-site signature of `function`.
///
/// The single lowering path for a declared signature; `package_interface`'s
/// exports are assembled from it. Owner handling:
///
/// - free function — lowered in the file's namespace scope;
/// - class method (including in-body/merged `implements` methods) — `Self`
///   is bound to the class receiver, and a bare `self` parameter adopts it;
/// - interface default method / free-impl method — lowered without a `Self`
///   binding, so `Self`/`Self.Assoc` mentions resolve to `Ty::Error` and a
///   bare `self` stays `Ty::Unknown`. Call-site resolution substitutes
///   correctly (`impl_rules::realize_with_symbolic_self`); binding `Self`
///   symbolically *here* is the eventual fix, tracked separately.
///
/// Lowering diagnostics are dropped, matching the export path: the checked
/// diagnostics for a signature are produced by inference, not by this query.
#[salsa::tracked(returns(ref))]
pub fn function_signature_ty<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> FunctionSignatureTy {
    use baml_compiler2_ppir::item_data::MethodOwner;

    let sig = baml_compiler2_ppir::item_data::elaborated_function_data(db, function);
    let body = baml_compiler2_ppir::function_body(db, function);
    let generic_env = crate::generic_env::function_generic_env(db, function);
    let owner = baml_compiler2_ppir::item_data::method_owner(db, function);

    // Class methods lower in the *class's* scope (same file by construction —
    // in-body methods and merged `implements` blocks never cross files).
    let pkg_info = file_package::file_package(db, function.file(db));
    let pkg_id = PackageId::new(db, pkg_info.package.clone());
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);

    let self_ty = match owner {
        Some(MethodOwner::Class(class_loc)) => {
            let class_data = baml_compiler2_ppir::item_data::class_data(db, class_loc);
            Some(crate::lower_type_expr::self_type_for_class_data(
                class_data,
                generic_env
                    .parent()
                    .expect("class method generic environment has a parent")
                    .params(),
                &pkg_info.namespace_path,
                pkg_info.package.clone(),
            ))
        }
        Some(MethodOwner::Interface(_) | MethodOwner::FreeImpl(_)) | None => None,
    };

    let ctx = crate::lower_type_expr::ScopeCtx {
        db,
        package_items: pkg_items,
        ns_context: &pkg_info.namespace_path,
        generic_params: generic_env.source_params(),
        bounds: crate::lower_type_expr::function_in_scope_generic_param_bounds(db, function),
        self_ty: self_ty.clone(),
    };
    let mut diags = Vec::new();
    let lower = |id: baml_compiler2_hir::type_ref::TypeRefId,
                 diags: &mut Vec<crate::infer_context::TirTypeError>| {
        crate::lower_type_expr::lower_type_ref(&sig.type_refs, id, &ctx, diags)
    };

    let mut params = Vec::new();
    for param in &sig.params {
        // A bare `self` has no written annotation (its elaborated ref is
        // `Unknown`); it adopts the receiver type when one is bound.
        let param_ty = match (&self_ty, param.name.as_str()) {
            (Some(self_ty), "self")
                if matches!(
                    sig.type_refs[param.type_ref].kind,
                    baml_compiler2_hir::type_ref::TypeRefKind::Unknown
                ) =>
            {
                self_ty.clone()
            }
            _ => lower(param.type_ref, &mut diags),
        };
        params.push(FunctionParamTy {
            name: Some(param.name.clone()),
            ty: param_ty,
            mode: if param.has_default {
                FunctionParamMode::Optional
            } else {
                FunctionParamMode::Required
            },
        });
    }

    let return_type = sig.return_type.map_or(
        Ty::Error {
            attr: TyAttr::default(),
        },
        |id| lower(id, &mut diags),
    );

    let declared_throws = sig.throws.map(|id| lower(id, &mut diags));

    let generic_params = sig
        .user_generic_params
        .iter()
        .chain(sig.synthetic_effect_params.iter())
        .map(|name| {
            generic_env
                .resolve_param(name)
                .expect("function generic parameter is in its environment")
                .clone()
        })
        .collect();

    FunctionSignatureTy {
        params,
        return_type,
        declared_throws,
        generic_params,
        builtin_kind: match body.as_ref() {
            FunctionBody::Builtin(kind) => Some(*kind),
            _ => None,
        },
    }
}

fn callable_short_name<'db>(db: &'db dyn crate::Db, function: FunctionLoc<'db>) -> Name {
    let func_data = baml_compiler2_ppir::item_data::function_data(db, function);

    // Only a class owner qualifies the name — interface default methods and
    // free-impl methods keep their bare name, preserving the throw-set key
    // format the scan produced.
    if let Some(baml_compiler2_ppir::item_data::MethodOwner::Class(class_loc)) =
        baml_compiler2_ppir::item_data::method_owner(db, function)
    {
        let class_data = baml_compiler2_ppir::item_data::class_data(db, class_loc);
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
        call_expr_id: baml_compiler2_ast::ExprId,
        callee_expr_id: baml_compiler2_ast::ExprId,
        args: &[baml_compiler2_ast::ExprId],
        unwrap_optional_callee: bool,
    ) -> Option<Ty> {
        let call_plan = self.inference.call_plan(call_expr_id);
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
    if let Some(throws) = call_plan.and_then(|plan| plan.instantiated_throws.clone()) {
        return Some(throws);
    }
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
        let mut bindings: FxHashMap<ParamTy, Ty> = FxHashMap::default();
        for (param, arg_expr_id) in pairs {
            let arg_ty = inference
                .expression_type(arg_expr_id)
                .cloned()
                .unwrap_or(Ty::Unknown {
                    attr: TyAttr::default(),
                });
            crate::generics::infer_bindings_allow_typevars(&param.ty, &arg_ty, &mut bindings);
        }
        let substituted = crate::generics::substitute_ty(throws, &bindings);
        if crate::generics::contains_typevar(&substituted)
            && let Some(instantiated) = call_plan.and_then(|plan| plan.instantiated_throws.as_ref())
        {
            return instantiated.clone();
        }
        substituted
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
            // Salsa-cached per package — previously rebuilt for every callable.
            let aliases = crate::inference::package_resolved_aliases(db, pkg_id);
            let facts = crate::throws_analysis::collect_escaping_throws(
                &CallableThrowsAnalysis {
                    db,
                    pkg_id,
                    ns_context: &pkg_info.namespace_path,
                    inference,
                    aliases,
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
