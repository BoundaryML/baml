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
    package::{PackageId, PackageItems, package_items},
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

#[salsa::tracked(returns(ref), cycle_initial=function_throw_sets_initial)]
pub fn function_throw_sets<'db>(
    db: &'db dyn crate::Db,
    package_id: PackageId<'db>,
) -> FunctionThrowSets {
    let pkg_items = package_items(db, package_id);
    let res_ctx = crate::package_interface::package_resolution_context(db, package_id);
    let mut aliases = collect_type_aliases(db, pkg_items);
    // Merge dependency type aliases for cross-package field type resolution
    for dep in &res_ctx.deps {
        for types_in_ns in dep.interface.types.values() {
            for exported in types_in_ns.values() {
                if let crate::package_interface::ExportedType::TypeAlias { qtn, resolved } =
                    exported
                {
                    aliases.insert(qtn.clone(), resolved.clone());
                }
            }
        }
    }
    let deps = &res_ctx.deps;

    let mut graph: crate::analysis::AnalysisGraph<Name, ThrowFact> =
        crate::analysis::AnalysisGraph::new();

    let mut call_edges: BTreeMap<Name, BTreeSet<Name>> = BTreeMap::new();
    let mut has_declared_contract: BTreeMap<Name, bool> = BTreeMap::new();
    // Track direct facts separately so we can merge cross-package facts before adding to graph
    let mut direct_facts: BTreeMap<Name, BTreeSet<ThrowFact>> = BTreeMap::new();

    for ns in pkg_items.namespaces.values() {
        for def in ns.values.values() {
            let Definition::Function(func_loc) = def else {
                continue;
            };

            let key = callable_throw_key(db, *func_loc);
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
                let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(db, file, method_id);
                let key = callable_throw_key(db, func_loc);

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
            if let Some(dep_throws) = lookup_dep_throw_set(db, deps, to) {
                // Cross-package: merge dependency's transitive throw facts into caller's direct facts
                direct_facts
                    .entry(from.clone())
                    .or_default()
                    .extend(dep_throws);
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
            if lookup_dep_throw_set(db, deps, to).is_none() {
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

fn function_throw_sets_initial<'db>(
    _db: &'db dyn crate::Db,
    _id: salsa::Id,
    _package_id: PackageId<'db>,
) -> FunctionThrowSets {
    FunctionThrowSets {
        direct: BTreeMap::new(),
        transitive: BTreeMap::new(),
    }
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

pub fn callable_throw_key<'db>(
    db: &'db dyn crate::Db,
    func: baml_compiler2_hir::loc::FunctionLoc<'db>,
) -> Name {
    let file = func.file(db);
    let item_tree = baml_compiler2_hir::file_item_tree(db, file);
    let func_data = &item_tree[func.id(db)];
    let pkg = baml_compiler2_hir::file_package::file_package(db, file);
    let short_name = item_tree
        .classes
        .values()
        .find_map(|class_data| {
            class_data
                .methods
                .contains(&func.id(db))
                .then(|| Name::new(format!("{}.{}", class_data.name, func_data.name)))
        })
        .unwrap_or_else(|| func_data.name.clone());
    throw_set_key(&pkg.namespace_path, &short_name)
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
    MemberFieldCallCollector::new(
        db,
        res_ctx,
        ns_context,
        generic_params,
        sig,
        body,
        aliases,
        class_context,
    )
    .collect()
}

/// Conservative pre-inference recovery pass used only for outward-throws
/// collection.
///
/// This intentionally duplicates a narrow slice of expression typing because
/// `function_throw_sets` runs before full scope inference and must remain
/// cycle-safe. Supported shapes should be kept explicit and conservative; full
/// TIR inference remains the source of truth for general expression semantics.
struct MemberFieldCallCollector<'a, 'db> {
    db: &'db dyn crate::Db,
    res_ctx: &'a crate::package_interface::PackageResolutionContext<'db>,
    ns_context: &'a [Name],
    generic_params: &'a [Name],
    body: &'a ExprBody,
    aliases: &'a HashMap<crate::ty::QualifiedTypeName, Ty>,
    class_context: Option<(&'a Name, &'a [Name])>,
    direct_facts: BTreeSet<ThrowFact>,
    extra_edges: BTreeSet<Name>,
    initial_locals: HashMap<Name, Ty>,
}

impl<'a, 'db> MemberFieldCallCollector<'a, 'db> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        db: &'db dyn crate::Db,
        res_ctx: &'a crate::package_interface::PackageResolutionContext<'db>,
        ns_context: &'a [Name],
        generic_params: &'a [Name],
        sig: &'a baml_compiler2_hir::signature::FunctionSignature,
        body: &'a ExprBody,
        aliases: &'a HashMap<crate::ty::QualifiedTypeName, Ty>,
        class_context: Option<(&'a Name, &'a [Name])>,
    ) -> Self {
        let pkg_items = res_ctx.own_items;
        let initial_locals = sig
            .params
            .iter()
            .map(|(name, te)| {
                let mut diags = Vec::new();
                let ty = lower_type_expr_in_ns(
                    db,
                    te,
                    pkg_items,
                    ns_context,
                    generic_params,
                    &mut diags,
                );
                (name.clone(), ty)
            })
            .collect();
        Self {
            db,
            res_ctx,
            ns_context,
            generic_params,
            body,
            aliases,
            class_context,
            direct_facts: BTreeSet::new(),
            extra_edges: BTreeSet::new(),
            initial_locals,
        }
    }

    fn collect(mut self) -> (BTreeSet<ThrowFact>, BTreeSet<Name>) {
        if let Some(root_expr) = self.body.root_expr {
            let mut env = self.initial_locals.clone();
            self.visit_expr(root_expr, &mut env);
        }
        (self.direct_facts, self.extra_edges)
    }

    fn visit_expr(&mut self, expr_id: baml_compiler2_ast::ExprId, env: &mut HashMap<Name, Ty>) {
        match &self.body.exprs[expr_id] {
            Expr::Literal(_)
            | Expr::ByteStringLiteral(_)
            | Expr::Null
            | Expr::Path(_)
            | Expr::Lambda(_)
            | Expr::Missing => {}
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.visit_expr(*condition, env);
                let mut then_env = env.clone();
                self.visit_expr(*then_branch, &mut then_env);
                if let Some(else_expr) = else_branch {
                    let mut else_env = env.clone();
                    self.visit_expr(*else_expr, &mut else_env);
                }
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.visit_expr(*scrutinee, env);
                for arm_id in arms {
                    let arm = &self.body.match_arms[*arm_id];
                    let mut arm_env = env.clone();
                    if let Some(guard) = arm.guard {
                        self.visit_expr(guard, &mut arm_env);
                    }
                    self.visit_expr(arm.body, &mut arm_env);
                }
            }
            Expr::Catch { base, clauses } => {
                self.visit_expr(*base, env);
                for clause in clauses {
                    for arm_id in &clause.arms {
                        let arm = &self.body.catch_arms[*arm_id];
                        let mut arm_env = env.clone();
                        self.visit_expr(arm.body, &mut arm_env);
                    }
                }
            }
            Expr::Throw { value } => self.visit_expr(*value, env),
            Expr::Binary { lhs, rhs, .. } => {
                self.visit_expr(*lhs, env);
                self.visit_expr(*rhs, env);
            }
            Expr::Unary { expr, .. } | Expr::OptionalChain { expr } => {
                self.visit_expr(*expr, env);
            }
            Expr::Call { callee, args } | Expr::OptionalCall { callee, args } => {
                self.collect_member_call(*callee, env);
                self.visit_expr(*callee, env);
                for arg in args {
                    self.visit_expr(*arg, env);
                }
            }
            Expr::Object {
                fields, spreads, ..
            } => {
                for (_, value) in fields {
                    self.visit_expr(*value, env);
                }
                for spread in spreads {
                    self.visit_expr(spread.expr, env);
                }
            }
            Expr::Array { elements } => {
                for elem in elements {
                    self.visit_expr(*elem, env);
                }
            }
            Expr::Map { entries } => {
                for (key, value) in entries {
                    self.visit_expr(*key, env);
                    self.visit_expr(*value, env);
                }
            }
            Expr::Block { stmts, tail_expr } => {
                let mut block_env = env.clone();
                for stmt_id in stmts {
                    self.visit_stmt(*stmt_id, &mut block_env);
                }
                if let Some(tail_expr) = tail_expr {
                    self.visit_expr(*tail_expr, &mut block_env);
                }
            }
            Expr::FieldAccess { base, .. } | Expr::OptionalFieldAccess { base, .. } => {
                self.visit_expr(*base, env);
            }
            Expr::Index { base, index } | Expr::OptionalIndex { base, index } => {
                self.visit_expr(*base, env);
                self.visit_expr(*index, env);
            }
        }
    }

    fn visit_stmt(&mut self, stmt_id: baml_compiler2_ast::StmtId, env: &mut HashMap<Name, Ty>) {
        match &self.body.stmts[stmt_id] {
            baml_compiler2_ast::Stmt::Expr(expr_id) => self.visit_expr(*expr_id, env),
            baml_compiler2_ast::Stmt::Let {
                pattern,
                type_annotation,
                initializer,
                ..
            } => {
                let inferred_ty = initializer.and_then(|expr_id| {
                    self.visit_expr(expr_id, env);
                    self.recover_expr_ty(expr_id, env)
                });
                if let Some((binding_name, binding_ty)) =
                    self.resolve_binding_ty(*pattern, *type_annotation, inferred_ty)
                {
                    env.insert(binding_name, binding_ty);
                }
            }
            baml_compiler2_ast::Stmt::While {
                condition,
                body,
                after,
                ..
            } => {
                self.visit_expr(*condition, env);
                let mut body_env = env.clone();
                self.visit_expr(*body, &mut body_env);
                if let Some(after_stmt) = after {
                    self.visit_stmt(*after_stmt, env);
                }
            }
            baml_compiler2_ast::Stmt::For {
                binding,
                collection,
                body,
            } => {
                self.visit_expr(*collection, env);
                let mut body_env = env.clone();
                if let Some(binding_name) = self.binding_name(*binding)
                    && let Some(elem_ty) = self.resolve_collection_element_ty(*collection, env)
                {
                    body_env.insert(binding_name, elem_ty);
                }
                self.visit_expr(*body, &mut body_env);
            }
            baml_compiler2_ast::Stmt::Return(expr) => {
                if let Some(expr_id) = expr {
                    self.visit_expr(*expr_id, env);
                }
            }
            baml_compiler2_ast::Stmt::Assign { target, value }
            | baml_compiler2_ast::Stmt::AssignOp { target, value, .. } => {
                self.visit_expr(*target, env);
                self.visit_expr(*value, env);
                if let Some((binding_name, binding_ty)) =
                    self.resolve_assignment_ty(*target, *value, env)
                {
                    env.insert(binding_name, binding_ty);
                }
            }
            baml_compiler2_ast::Stmt::Throw { value } => self.visit_expr(*value, env),
            baml_compiler2_ast::Stmt::Break
            | baml_compiler2_ast::Stmt::Continue
            | baml_compiler2_ast::Stmt::Missing
            | baml_compiler2_ast::Stmt::HeaderComment { .. } => {}
        }
    }

    fn collect_member_call(
        &mut self,
        callee_id: baml_compiler2_ast::ExprId,
        env: &HashMap<Name, Ty>,
    ) {
        let (base_id, field) = match &self.body.exprs[callee_id] {
            Expr::FieldAccess { base, field } | Expr::OptionalFieldAccess { base, field } => {
                (*base, field)
            }
            _ => return,
        };

        let Some(base_ty) = self.recover_expr_ty(base_id, env) else {
            return;
        };
        let resolved_base = crate::throws_semantics::resolve_alias_chain(&base_ty, self.aliases);
        let Ty::Class(class_name, _) = &resolved_base else {
            return;
        };

        let fields = self.res_ctx.lookup_class_fields(self.db, class_name);
        if let Some((_, field_ty)) = fields.iter().find(|(name, _)| name == field) {
            if let Some(facts) = function_throws_facts(field_ty, self.aliases) {
                self.direct_facts.extend(facts.into_iter().filter(|fact| {
                    !matches!(fact, Ty::TypeVar(_, _) | Ty::Never { .. } | Ty::Void { .. })
                }));
            }
            return;
        }

        if let Some(_resolved_method) = self.res_ctx.lookup_class_method(self.db, class_name, field)
        {
            let class_ns = class_name.namespace();
            let method_short = Name::new(format!("{}.{}", class_name.name(), field));
            let method_key = throw_set_key(class_ns, &method_short);
            self.extra_edges.insert(method_key);
        }
    }

    fn recover_expr_ty(
        &self,
        expr_id: baml_compiler2_ast::ExprId,
        env: &HashMap<Name, Ty>,
    ) -> Option<Ty> {
        match &self.body.exprs[expr_id] {
            Expr::Path(segments) => self.recover_path_ty(segments, env),
            Expr::FieldAccess { base, field } | Expr::OptionalFieldAccess { base, field } => {
                let base_ty = self.recover_expr_ty(*base, env)?;
                self.resolve_member_ty(&base_ty, field)
            }
            Expr::Call { callee, .. } | Expr::OptionalCall { callee, .. } => {
                let callee_ty = self.recover_expr_ty(*callee, env)?;
                match callee_ty {
                    Ty::Function { ret, .. } => Some(*ret),
                    Ty::Optional(inner, _) => match *inner {
                        Ty::Function { ret, .. } => Some(*ret),
                        _ => None,
                    },
                    _ => None,
                }
            }
            Expr::Object {
                type_name: Some(name),
                type_args,
                ..
            } => self.recover_typed_object_ty(name, type_args),
            Expr::Array { elements } => self.recover_array_ty(elements, env),
            Expr::OptionalChain { expr } => self.recover_expr_ty(*expr, env),
            _ => None,
        }
    }

    fn recover_path_ty(&self, segments: &[Name], env: &HashMap<Name, Ty>) -> Option<Ty> {
        if segments.len() == 1 {
            let name = &segments[0];
            if name.as_str() == "self" {
                self.self_ty()
            } else {
                env.get(name)
                    .cloned()
                    .or_else(|| self.named_callable_ty(segments))
            }
        } else {
            self.named_callable_ty(segments)
        }
    }

    fn recover_typed_object_ty(&self, name: &Name, type_args: &[TypeExpr]) -> Option<Ty> {
        let def = self.res_ctx.own_items.lookup_type(self.ns_context, name)?;
        match def {
            Definition::Class(_) => {
                let qtn = qualify_def(self.db, def, name);
                let mut diags = Vec::new();
                let lowered_type_args = type_args
                    .iter()
                    .map(|te| {
                        lower_type_expr_in_ns(
                            self.db,
                            te,
                            self.res_ctx.own_items,
                            self.ns_context,
                            self.generic_params,
                            &mut diags,
                        )
                    })
                    .collect();
                Some(Ty::Class(
                    crate::ty::NominalTypeRef::new_with_type_args(qtn, lowered_type_args),
                    TyAttr::default(),
                ))
            }
            Definition::Enum(_) => {
                let qtn = qualify_def(self.db, def, name);
                Some(Ty::Enum(
                    crate::ty::NominalTypeRef::new_with_type_args(qtn, Vec::new()),
                    TyAttr::default(),
                ))
            }
            _ => None,
        }
    }

    fn recover_array_ty(
        &self,
        elements: &[baml_compiler2_ast::ExprId],
        env: &HashMap<Name, Ty>,
    ) -> Option<Ty> {
        let element_tys: Vec<Ty> = elements
            .iter()
            .map(|expr_id| self.recover_expr_ty(*expr_id, env))
            .collect::<Option<Vec<_>>>()?;
        let element_ty = collapse_types(element_tys)?;
        Some(Ty::List(Box::new(element_ty), TyAttr::default()))
    }

    fn resolve_member_ty(&self, base_ty: &Ty, field: &Name) -> Option<Ty> {
        let resolved_base = crate::throws_semantics::resolve_alias_chain(base_ty, self.aliases);
        let Ty::Class(class_name, _) = &resolved_base else {
            return None;
        };

        let fields = self.res_ctx.lookup_class_fields(self.db, class_name);
        if let Some((_, field_ty)) = fields.iter().find(|(name, _)| name == field) {
            return Some(field_ty.clone());
        }

        let method = self
            .res_ctx
            .lookup_class_method(self.db, class_name, field)?;
        Some(method.function.as_ty())
    }

    fn named_callable_ty(&self, path: &[Name]) -> Option<Ty> {
        let (_source, function) = self
            .res_ctx
            .lookup_function(self.db, path, self.ns_context)?;
        Some(function.as_ty())
    }

    fn resolve_binding_ty(
        &self,
        pattern: baml_compiler2_ast::PatId,
        type_annotation: Option<baml_compiler2_ast::TypeAnnotId>,
        inferred_ty: Option<Ty>,
    ) -> Option<(Name, Ty)> {
        let binding_name = self.binding_name(pattern)?;
        let explicit_ty = type_annotation
            .map(|annot_id| self.lower_local_type_expr(&self.body.type_annotations[annot_id]))
            .or_else(|| match &self.body.patterns[pattern] {
                Pattern::TypedBinding { ty, .. } => Some(self.lower_local_type_expr(ty)),
                _ => None,
            });
        Some((binding_name, explicit_ty.or(inferred_ty)?))
    }

    fn resolve_assignment_ty(
        &self,
        target: baml_compiler2_ast::ExprId,
        value: baml_compiler2_ast::ExprId,
        env: &HashMap<Name, Ty>,
    ) -> Option<(Name, Ty)> {
        match &self.body.exprs[target] {
            Expr::Path(segments) if segments.len() == 1 => {
                Some((segments[0].clone(), self.recover_expr_ty(value, env)?))
            }
            _ => None,
        }
    }

    fn resolve_collection_element_ty(
        &self,
        collection: baml_compiler2_ast::ExprId,
        env: &HashMap<Name, Ty>,
    ) -> Option<Ty> {
        match self.recover_expr_ty(collection, env)? {
            Ty::List(inner, _) => Some(*inner),
            _ => None,
        }
    }

    fn binding_name(&self, pattern: baml_compiler2_ast::PatId) -> Option<Name> {
        match &self.body.patterns[pattern] {
            Pattern::Binding(name) | Pattern::TypedBinding { name, .. } => Some(name.clone()),
            _ => None,
        }
    }

    fn lower_local_type_expr(&self, te: &TypeExpr) -> Ty {
        let mut diags = Vec::new();
        lower_type_expr_in_ns(
            self.db,
            te,
            self.res_ctx.own_items,
            self.ns_context,
            self.generic_params,
            &mut diags,
        )
    }

    fn self_ty(&self) -> Option<Ty> {
        self.class_context
            .and_then(|(class_name, class_generic_params)| {
                let def = self
                    .res_ctx
                    .own_items
                    .lookup_type(self.ns_context, class_name)?;
                match def {
                    Definition::Class(_) => {
                        let qtn = qualify_def(self.db, def, class_name);
                        let type_args: Vec<Ty> = class_generic_params
                            .iter()
                            .map(|name| Ty::TypeVar(name.clone(), TyAttr::default()))
                            .collect();
                        Some(Ty::Class(
                            crate::ty::NominalTypeRef::new_with_type_args(qtn, type_args),
                            TyAttr::default(),
                        ))
                    }
                    _ => None,
                }
            })
    }
}

fn collapse_types(types: Vec<Ty>) -> Option<Ty> {
    let mut unique = BTreeSet::new();
    for ty in types {
        unique.insert(ty);
    }
    match unique.len() {
        0 => None,
        1 => unique.into_iter().next(),
        _ => Some(Ty::Union(unique.into_iter().collect(), TyAttr::default())),
    }
}

/// Look up a function's transitive throw set from dependency interfaces.
fn lookup_dep_throw_set<'db>(
    db: &'db dyn crate::Db,
    deps: &[crate::package_interface::ResolvedDependency<'db>],
    target_name: &Name,
) -> Option<BTreeSet<ThrowFact>> {
    for dep in deps {
        if !dep.interface.callable_keys.contains(target_name) {
            continue;
        }
        if let Some(throws) = function_throw_sets(db, dep.package_id).transitive_for(target_name) {
            return Some(throws.clone());
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
