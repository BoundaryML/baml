//! HIR-level throw inference for compiler2 (BEP-007).
//!
//! This runs before type inference and computes a per-function transitive throw
//! set over the call graph. Functions with declared `throws` clauses act as
//! firewalls: their declared set becomes caller-visible, replacing body-derived
//! facts for propagation.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use baml_base::Name;
use baml_compiler2_ast::{Expr, ExprBody, Literal, Pattern, TypeExpr};
use baml_compiler2_hir::{
    contributions::Definition,
    package::{PackageId, PackageItems, package_dependencies, package_items},
};

use crate::{
    callable_boundary::{directly_invoked_callback_params, lower_callable_boundary},
    inference::collect_type_aliases,
    lower_type_expr::{lower_type_expr_in_ns, qualify_def},
    throws_semantics::function_throws_facts,
    ty::{PrimitiveType, Ty, TyAttr},
};

/// A throw fact is now a proper `Ty` — no more lossy string round-trips.
pub type ThrowFact = Ty;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionThrowSets {
    pub direct: BTreeMap<Name, BTreeSet<ThrowFact>>,
    pub transitive: BTreeMap<Name, BTreeSet<ThrowFact>>,
}

// Safety: comparison-based replacement for Salsa early cutoff.
#[allow(unsafe_code)]
unsafe impl salsa::Update for FunctionThrowSets {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        // SAFETY: pointer is Salsa-owned and valid for replacement.
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

impl FunctionThrowSets {
    pub fn direct_for(&self, name: &Name) -> Option<&BTreeSet<ThrowFact>> {
        self.direct.get(name)
    }

    pub fn transitive_for(&self, name: &Name) -> Option<&BTreeSet<ThrowFact>> {
        self.transitive.get(name)
    }
}

#[salsa::tracked(returns(ref))]
pub fn function_throw_sets<'db>(
    db: &'db dyn crate::Db,
    package_id: PackageId<'db>,
) -> FunctionThrowSets {
    let pkg_items = package_items(db, package_id);
    let res_ctx = crate::package_interface::package_resolution_context(db, package_id);
    let mut aliases = collect_type_aliases(db, pkg_items);
    // Merge dependency type aliases for cross-package field type resolution
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
    // Load dependency interfaces for cross-package throw lookup
    let dep_interfaces: Vec<(Name, &crate::package_interface::PackageInterface)> =
        package_dependencies(db, package_id)
            .iter()
            .map(|dep_id| {
                let name = dep_id.name(db);
                let iface = crate::package_interface::package_interface(db, *dep_id);
                (name, iface)
            })
            .collect();

    let mut graph: crate::analysis::AnalysisGraph<Name, ThrowFact> =
        crate::analysis::AnalysisGraph::new();

    let mut call_edges: BTreeMap<Name, BTreeSet<Name>> = BTreeMap::new();
    let mut has_declared_contract: BTreeMap<Name, bool> = BTreeMap::new();
    // Track direct facts separately so we can merge cross-package facts before adding to graph
    let mut direct_facts: BTreeMap<Name, BTreeSet<ThrowFact>> = BTreeMap::new();

    for ns in pkg_items.namespaces.values() {
        for (short_name, def) in &ns.values {
            let Definition::Function(func_loc) = def else {
                continue;
            };

            let key = function_key(db, *func_loc, short_name);
            let sig = baml_compiler2_hir::signature::function_signature(db, *func_loc);
            let body = baml_compiler2_hir::body::function_body(db, *func_loc);
            let item_tree = baml_compiler2_hir::file_item_tree(db, func_loc.file(db));
            let func_data = &item_tree[func_loc.id(db)];
            let func_ns = baml_compiler2_hir::file_package::file_package(db, func_loc.file(db))
                .namespace_path;

            let declared_throws = sig.throws.as_ref().map(|te| {
                let mut diags = Vec::new();
                let lowered = lower_type_expr_in_ns(
                    db,
                    te,
                    pkg_items,
                    &func_ns,
                    &func_data.generic_params,
                    &mut diags,
                );
                drop(diags);
                flatten_ty_to_facts(&lowered)
            });

            let direct = if let Some(declared) = declared_throws.clone() {
                declared
            } else if let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() {
                let mut direct = collect_direct_throws(db, pkg_items, &func_ns, expr_body);
                direct.extend(collect_direct_param_call_throws(
                    db,
                    pkg_items,
                    &func_ns,
                    &func_data.generic_params,
                    sig.as_ref(),
                    expr_body,
                    &aliases,
                ));
                let (member_facts, _) = collect_member_field_call_throws(
                    db,
                    res_ctx,
                    &func_ns,
                    &func_data.generic_params,
                    sig.as_ref(),
                    expr_body,
                    &aliases,
                    None,
                );
                direct.extend(member_facts);
                direct
            } else {
                BTreeSet::new()
            };

            direct_facts.insert(key.clone(), direct);
            has_declared_contract.insert(key.clone(), declared_throws.is_some());

            if let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() {
                // Build combined target set: syntactic call targets + member call edges
                let mut targets = collect_call_targets(expr_body);
                let (_, member_edges) = collect_member_field_call_throws(
                    db,
                    res_ctx,
                    &func_ns,
                    &func_data.generic_params,
                    sig.as_ref(),
                    expr_body,
                    &aliases,
                    None,
                );
                targets.extend(member_edges);
                call_edges.insert(key, targets);
            }
        }

        // Also process class methods, which are not in ns.values.
        for (class_name, def) in &ns.types {
            let Definition::Class(class_loc) = def else {
                continue;
            };
            let file = class_loc.file(db);
            let item_tree = baml_compiler2_hir::file_item_tree(db, file);
            let class_data = &item_tree[class_loc.id(db)];

            for &method_id in &class_data.methods {
                let method_data = &item_tree[method_id];
                let method_name = &method_data.name;
                let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(db, file, method_id);
                // Key as "ClassName.method_name" (with namespace prefix if any).
                let method_short = Name::new(format!("{class_name}.{method_name}"));
                let key = function_key(db, func_loc, &method_short);

                let sig = baml_compiler2_hir::signature::function_signature(db, func_loc);
                let body = baml_compiler2_hir::body::function_body(db, func_loc);

                let method_ns =
                    baml_compiler2_hir::file_package::file_package(db, file).namespace_path;
                let mut method_generic_params = class_data.generic_params.clone();
                method_generic_params.extend(method_data.generic_params.iter().cloned());
                let declared_throws = sig.throws.as_ref().map(|te| {
                    let mut diags = Vec::new();
                    let lowered = lower_type_expr_in_ns(
                        db,
                        te,
                        pkg_items,
                        &method_ns,
                        &method_generic_params,
                        &mut diags,
                    );
                    drop(diags);
                    flatten_ty_to_facts(&lowered)
                });

                let direct = if let Some(declared) = declared_throws.clone() {
                    declared
                } else if let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) =
                    body.as_ref()
                {
                    let mut direct = collect_direct_throws(db, pkg_items, &method_ns, expr_body);
                    direct.extend(collect_direct_param_call_throws(
                        db,
                        pkg_items,
                        &method_ns,
                        &method_generic_params,
                        sig.as_ref(),
                        expr_body,
                        &aliases,
                    ));
                    let (member_facts, _) = collect_member_field_call_throws(
                        db,
                        res_ctx,
                        &method_ns,
                        &method_generic_params,
                        sig.as_ref(),
                        expr_body,
                        &aliases,
                        Some((class_name, class_data.generic_params.as_slice())),
                    );
                    direct.extend(member_facts);
                    direct
                } else {
                    BTreeSet::new()
                };

                direct_facts.insert(key.clone(), direct);
                has_declared_contract.insert(key.clone(), declared_throws.is_some());

                if let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() {
                    let raw_targets = collect_call_targets(expr_body);
                    let (_, member_edges) = collect_member_field_call_throws(
                        db,
                        res_ctx,
                        &method_ns,
                        &method_generic_params,
                        sig.as_ref(),
                        expr_body,
                        &aliases,
                        Some((class_name, class_data.generic_params.as_slice())),
                    );
                    // Merge syntactic targets + member edges, then rewrite self references
                    let mut combined = raw_targets;
                    combined.extend(member_edges);
                    let rewritten: BTreeSet<Name> = combined
                        .into_iter()
                        .map(|t| rewrite_self_target(&t, class_name))
                        .collect();
                    call_edges.insert(key, rewritten);
                }
            }
        }
    }

    // Process call edges: for cross-package targets, merge their throw facts
    // into the caller's direct facts; for same-package targets, add edges.
    for (from, targets) in &call_edges {
        if has_declared_contract.get(from).copied().unwrap_or(false) {
            continue;
        }
        for to in targets {
            if let Some(dep_throws) = lookup_dep_throw_set(&dep_interfaces, to) {
                // Cross-package: merge dependency's transitive throw facts into caller's direct facts
                direct_facts
                    .entry(from.clone())
                    .or_default()
                    .extend(dep_throws.iter().cloned());
            } else {
                // Same-package: will add edge after nodes are added
                // (edges added below)
            }
        }
    }

    // Add all nodes with their (possibly enriched) direct facts
    for (key, facts) in &direct_facts {
        graph.add_node(key.clone(), facts.clone());
    }

    // Add same-package call edges
    for (from, targets) in &call_edges {
        if has_declared_contract.get(from).copied().unwrap_or(false) {
            continue;
        }
        for to in targets {
            if lookup_dep_throw_set(&dep_interfaces, to).is_none() {
                graph.add_edge(from.clone(), to.clone());
            }
        }
    }

    let analysis = graph.analyze();

    let mut direct = BTreeMap::new();
    let mut transitive = BTreeMap::new();
    for (name, facts) in analysis.iter_direct() {
        direct.insert(name.clone(), facts.clone());
    }
    for (name, facts) in analysis.iter_transitive() {
        transitive.insert(name.clone(), facts.clone());
    }

    FunctionThrowSets { direct, transitive }
}

/// Build the throw-set lookup key for a function given its namespace path and short name.
///
/// For top-level functions the key is just the short name; for namespaced
/// functions it is `"ns1.ns2.name"`.
pub fn throw_set_key(namespace_path: &[Name], short_name: &Name) -> Name {
    if namespace_path.is_empty() {
        short_name.clone()
    } else {
        let mut parts: Vec<String> = namespace_path
            .iter()
            .map(|n| n.as_str().to_string())
            .collect();
        parts.push(short_name.as_str().to_string());
        Name::new(parts.join("."))
    }
}

fn function_key<'db>(
    db: &'db dyn crate::Db,
    func: baml_compiler2_hir::loc::FunctionLoc<'db>,
    short_name: &Name,
) -> Name {
    let file = func.file(db);
    let pkg = baml_compiler2_hir::file_package::file_package(db, file);
    throw_set_key(&pkg.namespace_path, short_name)
}

pub fn collect_direct_throws<'db>(
    db: &'db dyn crate::Db,
    pkg_items: &PackageItems<'db>,
    ns_context: &[Name],
    body: &ExprBody,
) -> BTreeSet<ThrowFact> {
    let mut facts = BTreeSet::new();

    for (_, expr) in body.exprs.iter() {
        if let Expr::Throw { value } = expr {
            facts.insert(throw_fact_from_expr(
                db, pkg_items, ns_context, *value, body,
            ));
        }
    }
    for (_, stmt) in body.stmts.iter() {
        if let baml_compiler2_ast::Stmt::Throw { value } = stmt {
            facts.insert(throw_fact_from_expr(
                db, pkg_items, ns_context, *value, body,
            ));
        }
    }

    // Remove facts that correspond to catch binding variable names.
    // This is a heuristic: if a binding name happens to shadow a type name,
    // the corresponding fact is suppressed.
    let catch_bindings = collect_catch_binding_names(body);
    if !catch_bindings.is_empty() {
        facts.retain(|fact| {
            let name = fact_display_name(fact);
            !catch_bindings.contains(name.as_str())
        });
    }

    facts
}

/// Get a display name for a throw fact, used for the catch binding name filter.
fn fact_display_name(fact: &Ty) -> String {
    match fact {
        Ty::Primitive(p, _) => p.to_string(),
        Ty::Class(qn, _) | Ty::Enum(qn, _) | Ty::TypeAlias(qn, _) => qn.to_string(),
        Ty::EnumVariant(qn, variant, _) => format!("{qn}.{variant}"),
        Ty::Unknown { .. } => "unknown".to_string(),
        _ => format!("{fact}"),
    }
}

pub fn collect_call_targets(body: &ExprBody) -> BTreeSet<Name> {
    let mut targets = BTreeSet::new();
    for (_, expr) in body.exprs.iter() {
        if let Expr::Call { callee, .. } = expr {
            if let Some(path) = expr_to_path(*callee, body) {
                let joined = path.iter().map(Name::as_str).collect::<Vec<_>>().join(".");
                targets.insert(Name::new(joined));
            }
        }
    }
    targets
}

/// Convert a thrown expression to a `Ty` directly, using `pkg_items` to resolve
/// paths to their actual types (enum variants, classes, etc).
fn throw_fact_from_expr<'db>(
    db: &'db dyn crate::Db,
    pkg_items: &PackageItems<'db>,
    ns_context: &[Name],
    expr_id: baml_compiler2_ast::ExprId,
    body: &ExprBody,
) -> Ty {
    match &body.exprs[expr_id] {
        Expr::Literal(Literal::String(_)) => {
            Ty::Primitive(PrimitiveType::String, TyAttr::default())
        }
        Expr::Literal(Literal::Int(_)) => Ty::Primitive(PrimitiveType::Int, TyAttr::default()),
        Expr::Literal(Literal::Float(_)) => Ty::Primitive(PrimitiveType::Float, TyAttr::default()),
        Expr::Literal(Literal::Bool(_)) => Ty::Primitive(PrimitiveType::Bool, TyAttr::default()),
        Expr::Null => Ty::Primitive(PrimitiveType::Null, TyAttr::default()),
        Expr::Path(segments) if !segments.is_empty() => {
            resolve_path_to_ty(db, pkg_items, ns_context, segments)
        }
        Expr::FieldAccess { .. } => expr_to_path(expr_id, body)
            .map(|segments| resolve_path_to_ty(db, pkg_items, ns_context, &segments))
            .unwrap_or(Ty::Unknown {
                attr: TyAttr::default(),
            }),
        Expr::Object {
            type_name: Some(name),
            ..
        } => {
            if let Some(def) = pkg_items.lookup_type(ns_context, name) {
                match def {
                    Definition::Class(_) => {
                        Ty::Class(qualify_def(db, def, name).into(), TyAttr::default())
                    }
                    Definition::Enum(_) => {
                        Ty::Enum(qualify_def(db, def, name).into(), TyAttr::default())
                    }
                    _ => Ty::Unknown {
                        attr: TyAttr::default(),
                    },
                }
            } else {
                Ty::Unknown {
                    attr: TyAttr::default(),
                }
            }
        }
        _ => Ty::Unknown {
            attr: TyAttr::default(),
        },
    }
}

/// Rewrite a call target name from `self.X` to `ClassName.X`.
/// Other targets are returned unchanged.
fn rewrite_self_target(target: &Name, class_name: &Name) -> Name {
    let s = target.as_str();
    if let Some(rest) = s.strip_prefix("self.") {
        Name::new(format!("{class_name}.{rest}"))
    } else {
        target.clone()
    }
}

/// Resolve a path like `["Status", "HttpError"]` or `["ns", "Status", "HttpError"]`
/// or `["Status"]` to a `Ty`.
fn resolve_path_to_ty<'db>(
    db: &'db dyn crate::Db,
    pkg_items: &PackageItems<'db>,
    ns_context: &[Name],
    segments: &[Name],
) -> Ty {
    // Try treating the last segment as an enum variant and the prefix as
    // the enum path. For bare `["Status", "HttpError"]` from a namespaced file,
    // try namespace-qualified first, then unqualified.
    if segments.len() >= 2 {
        let enum_path = &segments[..segments.len() - 1];
        let variant = &segments[segments.len() - 1];
        let enum_name = enum_path.last().expect("enum_path is non-empty");
        let enum_ns = &enum_path[..enum_path.len() - 1];
        // Try with namespace context for bare enum names
        let def = if !ns_context.is_empty() && enum_ns.is_empty() {
            pkg_items
                .lookup_type(ns_context, enum_name)
                .or_else(|| pkg_items.lookup_type(enum_ns, enum_name))
        } else {
            pkg_items.lookup_type(enum_ns, enum_name)
        };
        if let Some(def) = def {
            if let Definition::Enum(_) = def {
                let qtn = qualify_def(db, def, enum_name);
                return Ty::EnumVariant(qtn, variant.clone(), TyAttr::default());
            }
        }
    }

    // Try the full path as a type lookup. For single-segment bare names,
    // try namespace-qualified first, then unqualified.
    let name = segments.last().expect("segments is non-empty");
    let seg_ns = &segments[..segments.len() - 1];
    let def = if !ns_context.is_empty() && seg_ns.is_empty() {
        let ns: Vec<Name> = ns_context.iter().chain(seg_ns.iter()).cloned().collect();
        pkg_items
            .lookup_type(&ns, name)
            .or_else(|| pkg_items.lookup_type(seg_ns, name))
    } else {
        pkg_items.lookup_type(seg_ns, name)
    };
    if let Some(def) = def {
        return match def {
            Definition::Class(_) => Ty::Class(qualify_def(db, def, name).into(), TyAttr::default()),
            Definition::Enum(_) => Ty::Enum(qualify_def(db, def, name).into(), TyAttr::default()),
            Definition::TypeAlias(_) => {
                Ty::TypeAlias(qualify_def(db, def, name).into(), TyAttr::default())
            }
            _ => Ty::Unknown {
                attr: TyAttr::default(),
            },
        };
    }

    Ty::Unknown {
        attr: TyAttr::default(),
    }
}

fn collect_catch_binding_names(body: &ExprBody) -> HashSet<&str> {
    let mut names = HashSet::new();
    for (_, expr) in body.exprs.iter() {
        if let Expr::Catch { clauses, .. } = expr {
            for clause in clauses {
                match &body.patterns[clause.binding] {
                    Pattern::Binding(name) | Pattern::TypedBinding { name, .. } => {
                        names.insert(name.as_str());
                    }
                    _ => {}
                }
            }
        }
    }
    names
}

fn expr_to_path(expr_id: baml_compiler2_ast::ExprId, body: &ExprBody) -> Option<Vec<Name>> {
    match &body.exprs[expr_id] {
        Expr::Path(segments) if !segments.is_empty() => Some(segments.clone()),
        Expr::FieldAccess { base, field } => {
            let mut base_path = expr_to_path(*base, body)?;
            base_path.push(field.clone());
            Some(base_path)
        }
        _ => None,
    }
}

pub fn flatten_ty_to_facts(ty: &Ty) -> BTreeSet<ThrowFact> {
    crate::throws_semantics::flatten_ty_to_facts(ty)
}

fn collect_direct_param_call_throws<'db>(
    db: &'db dyn crate::Db,
    pkg_items: &PackageItems<'db>,
    ns_context: &[Name],
    generic_params: &[Name],
    sig: &baml_compiler2_hir::signature::FunctionSignature,
    body: &ExprBody,
    aliases: &HashMap<crate::ty::QualifiedTypeName, Ty>,
) -> BTreeSet<ThrowFact> {
    let boundary = lower_callable_boundary(db, pkg_items, ns_context, generic_params, sig, None);
    let directly_invoked = directly_invoked_callback_params(body);

    if directly_invoked.is_empty() {
        return BTreeSet::new();
    }

    let mut out = BTreeSet::new();
    for ((param_name, _), (_, param_ty)) in sig.params.iter().zip(boundary.params.iter()) {
        if !directly_invoked.contains(param_name) {
            continue;
        }
        let Some(facts) = function_throws_facts(param_ty, aliases) else {
            continue;
        };
        out.extend(facts.into_iter().filter(|fact| {
            !matches!(fact, Ty::TypeVar(_, _) | Ty::Never { .. } | Ty::Void { .. })
        }));
    }

    out
}

/// Extract direct throw facts from member field calls on typed parameters/self.
///
/// Handles two sub-cases:
/// 1. Function-typed field calls (e.g., `h.run()` where `run` is a `Class::fields` entry):
///    resolve the base type from the function signature, look up the field via
///    `PackageResolutionContext::lookup_class_fields` (with generic substitution applied),
///    and extract throws as direct facts if the field is `Ty::Function`.
/// 2. Named method calls (e.g., `h.do_thing()` where `do_thing` is a `Class::methods` entry):
///    rewrite the call target to namespace-qualified `"ns.ClassName.method"` form and add
///    as a call-graph edge.
#[allow(clippy::too_many_arguments)]
fn collect_member_field_call_throws<'db>(
    db: &'db dyn crate::Db,
    res_ctx: &crate::package_interface::PackageResolutionContext<'db>,
    ns_context: &[Name],
    generic_params: &[Name],
    sig: &baml_compiler2_hir::signature::FunctionSignature,
    body: &ExprBody,
    aliases: &HashMap<crate::ty::QualifiedTypeName, Ty>,
    // (class_name, class_generic_params) — needed to build accurate self type
    // with TypeVar type_args, e.g. Handler<E> not bare Handler
    class_context: Option<(&Name, &[Name])>,
) -> (BTreeSet<ThrowFact>, BTreeSet<Name>) {
    let mut direct_facts = BTreeSet::new();
    let mut extra_edges = BTreeSet::new();

    let pkg_items = res_ctx.own_items;
    // Build param name → Ty map from the function signature.
    let param_types: HashMap<Name, Ty> = sig
        .params
        .iter()
        .map(|(name, te)| {
            let mut diags = Vec::new();
            let ty =
                lower_type_expr_in_ns(db, te, pkg_items, ns_context, generic_params, &mut diags);
            (name.clone(), ty)
        })
        .collect();

    // Extend with for-loop variable types.
    // For `for (let elem in collection)` where `collection` is a known param of type `T[]`,
    // map `elem` → `T`. This handles patterns like `for task in tasks` where
    // `tasks: Task<string | int>[]`.
    let mut local_types: HashMap<Name, Ty> = param_types.clone();
    for (_, stmt) in body.stmts.iter() {
        if let baml_compiler2_ast::Stmt::For {
            binding,
            collection,
            ..
        } = stmt
        {
            // Get the binding name
            let binding_name = match &body.patterns[*binding] {
                baml_compiler2_ast::Pattern::Binding(name)
                | baml_compiler2_ast::Pattern::TypedBinding { name, .. } => name.clone(),
                _ => continue,
            };
            // Get the collection's type by resolving its path to a param
            let collection_ty = match &body.exprs[*collection] {
                Expr::Path(segments) if segments.len() == 1 => {
                    param_types.get(&segments[0]).cloned()
                }
                _ => None,
            };
            if let Some(coll_ty) = collection_ty {
                // Peel List type to get element type
                let elem_ty = match coll_ty {
                    Ty::List(inner, _) => Some(*inner),
                    _ => None,
                };
                if let Some(elem) = elem_ty {
                    local_types.insert(binding_name, elem);
                }
            }
        }
    }

    for (_, expr) in body.exprs.iter() {
        let callee_id = match expr {
            Expr::Call { callee, .. } | Expr::OptionalCall { callee, .. } => *callee,
            _ => continue,
        };

        // Match FieldAccess { base, field } pattern
        let (base_id, field) = match &body.exprs[callee_id] {
            Expr::FieldAccess { base, field } => (*base, field),
            _ => continue,
        };

        // Resolve base to a type: direct parameter, for-loop variable, or `self`
        let base_ty = match &body.exprs[base_id] {
            Expr::Path(segments) if segments.len() == 1 => {
                let base_name = &segments[0];
                if base_name.as_str() == "self" {
                    // self-based member calls — resolve self type from class_context.
                    // Build accurate self type with TypeVar type_args so that
                    // lookup_class_fields applies correct generic substitution.
                    class_context.and_then(|(class_name, class_generic_params)| {
                        let def = pkg_items.lookup_type(ns_context, class_name)?;
                        match def {
                            Definition::Class(_) => {
                                let qtn = qualify_def(db, def, class_name);
                                let type_args: Vec<Ty> = class_generic_params
                                    .iter()
                                    .map(|name| Ty::TypeVar(name.clone(), TyAttr::default()))
                                    .collect();
                                let nominal =
                                    crate::ty::NominalTypeRef::new_with_type_args(qtn, type_args);
                                Some(Ty::Class(nominal, TyAttr::default()))
                            }
                            _ => None,
                        }
                    })
                } else {
                    local_types.get(base_name).cloned()
                }
            }
            _ => None,
        };

        let Some(base_ty) = base_ty else {
            continue;
        };

        // Resolve the member on the base type
        let resolved_base = crate::throws_semantics::resolve_alias_chain(&base_ty, aliases);
        let Ty::Class(class_name, _) = &resolved_base else {
            continue;
        };

        // Look up the field via PackageResolutionContext (with generic substitution)
        let fields = res_ctx.lookup_class_fields(db, class_name);
        if let Some((_, field_ty)) = fields.iter().find(|(n, _)| n == field) {
            // Function-typed field → extract throws as direct facts
            if let Some(facts) = function_throws_facts(field_ty, aliases) {
                direct_facts.extend(facts.into_iter().filter(|fact| {
                    !matches!(fact, Ty::TypeVar(_, _) | Ty::Never { .. } | Ty::Void { .. })
                }));
            }
            continue;
        }

        // Check if it's a named method → validate via res_ctx and add as call-graph edge.
        // We MUST validate the method exists before adding an edge, because
        // AnalysisGraph::add_edge (analysis.rs:54-58) auto-creates missing target
        // nodes with empty facts. A bogus edge would inject a phantom node and
        // alter graph topology, potentially masking real throw propagation.
        // We also must use the class declaration namespace for the key, NOT the caller's
        // ns_context, so the edge matches the declaration-side key built by function_key().
        if let Some(_resolved_method) = res_ctx.lookup_class_method(db, class_name, field) {
            let class_ns = class_name.namespace();
            let method_short = Name::new(format!("{}.{}", class_name.name(), field));
            let method_key = throw_set_key(class_ns, &method_short);
            extra_edges.insert(method_key);
        }
    }

    (direct_facts, extra_edges)
}

/// Look up a function's transitive throw set from dependency interfaces.
fn lookup_dep_throw_set<'a>(
    dep_interfaces: &'a [(Name, &crate::package_interface::PackageInterface)],
    target_name: &Name,
) -> Option<&'a BTreeSet<ThrowFact>> {
    for (_dep_name, dep_iface) in dep_interfaces {
        if let Some(throws) = dep_iface.throw_sets.transitive_for(target_name) {
            return Some(throws);
        }
    }
    None
}

pub fn is_banned_catch_binding_type(ty: &TypeExpr) -> Option<&'static str> {
    match ty {
        TypeExpr::BuiltinUnknown { .. } => Some("unknown"),
        TypeExpr::Path { segments, .. } if segments.len() == 1 && segments[0].as_str() == "any" => {
            Some("any")
        }
        _ => None,
    }
}
