//! HIR-level throw inference for compiler2 (BEP-007).
//!
//! This runs before type inference and computes a per-function transitive throw
//! set over the call graph. Functions with declared `throws` clauses act as
//! firewalls: their declared set becomes caller-visible, replacing body-derived
//! facts for propagation.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use baml_base::Name;
use baml_compiler2_ast::{Expr, ExprBody, Literal};
use baml_compiler2_hir::{
    contributions::Definition,
    package::{PackageId, PackageItems, package_dependencies},
};

use crate::{
    lower_type_expr::{lower_type_expr_in_ns, qualify_def},
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
    let pkg_items = baml_compiler2_ppir::package_items(db, package_id);
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
            let sig = baml_compiler2_ppir::function_signature(db, *func_loc);
            let body = baml_compiler2_ppir::function_body(db, *func_loc);
            let func_ns = baml_compiler2_hir::file_package::file_package(db, func_loc.file(db))
                .namespace_path;

            let declared_throws = sig.throws.as_ref().map(|te| {
                let mut diags = Vec::new();
                let item_tree = baml_compiler2_ppir::file_item_tree(db, func_loc.file(db));
                let func_data = &item_tree[func_loc.id(db)];
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
                collect_direct_throws(db, pkg_items, &func_ns, expr_body)
            } else {
                BTreeSet::new()
            };

            direct_facts.insert(key.clone(), direct);
            has_declared_contract.insert(key.clone(), declared_throws.is_some());

            if let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() {
                call_edges.insert(key, collect_call_targets(expr_body));
            }
        }

        // Also process class methods, which are not in ns.values.
        for (class_name, def) in &ns.types {
            let Definition::Class(class_loc) = def else {
                continue;
            };
            let file = class_loc.file(db);
            let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
            let class_data = &item_tree[class_loc.id(db)];

            for &method_id in &class_data.methods {
                let method_data = &item_tree[method_id];
                let method_name = &method_data.name;
                let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(db, file, method_id);
                // Key as "ClassName.method_name" (with namespace prefix if any).
                let method_short = Name::new(format!("{class_name}.{method_name}"));
                let key = function_key(db, func_loc, &method_short);

                let sig = baml_compiler2_ppir::function_signature(db, func_loc);
                let body = baml_compiler2_ppir::function_body(db, func_loc);

                let method_ns =
                    baml_compiler2_hir::file_package::file_package(db, file).namespace_path;
                let declared_throws = sig.throws.as_ref().map(|te| {
                    let mut diags = Vec::new();
                    let lowered = lower_type_expr_in_ns(
                        db,
                        te,
                        pkg_items,
                        &method_ns,
                        &method_data.generic_params,
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
                    collect_direct_throws(db, pkg_items, &method_ns, expr_body)
                } else {
                    BTreeSet::new()
                };

                direct_facts.insert(key.clone(), direct);
                has_declared_contract.insert(key.clone(), declared_throws.is_some());

                if let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() {
                    // Rewrite "self.X" call targets to "ClassName.X" so edges
                    // connect to the correct graph nodes.
                    let raw_targets = collect_call_targets(expr_body);
                    let rewritten: BTreeSet<Name> = raw_targets
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
        Ty::Class(qn, _, _) | Ty::Enum(qn, _) | Ty::TypeAlias(qn, _) => qn.to_string(),
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
        Expr::MemberAccess { .. } => expr_to_path(expr_id, body)
            .map(|segments| resolve_path_to_ty(db, pkg_items, ns_context, &segments))
            .unwrap_or(Ty::Unknown {
                attr: TyAttr::default(),
            }),
        Expr::Object {
            type_name: Some(path),
            ..
        } => resolve_path_to_ty(db, pkg_items, ns_context, path.segments()),
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
    // Cross-package fallback for qualified paths whose first segment names
    // either the literal `root` package (own-package alias) or another
    // package the resolver knows about. Mirrors `lower_type_expr_in_ns` so
    // throw-set type recovery works for `root.http.Response { ... }` literals
    // and `baml.errors.DevOther` references inside throw expressions.
    let def = def.or_else(|| {
        if segments.len() < 2 {
            return None;
        }
        if segments[0].as_str() == "root" {
            pkg_items.lookup_type(&segments[1..segments.len() - 1], name)
        } else {
            let pkg_id = PackageId::new(db, segments[0].clone());
            let pkg = baml_compiler2_ppir::package_items(db, pkg_id);
            pkg.lookup_type(&segments[1..segments.len() - 1], name)
        }
    });
    if let Some(def) = def {
        return match def {
            Definition::Class(_) => {
                Ty::Class(qualify_def(db, def, name), vec![], TyAttr::default())
            }
            Definition::Enum(_) => Ty::Enum(qualify_def(db, def, name), TyAttr::default()),
            Definition::TypeAlias(_) => {
                Ty::TypeAlias(qualify_def(db, def, name), TyAttr::default())
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
                if let Some(name) = body.patterns[clause.binding].binding_name(&body.patterns) {
                    names.insert(name.as_str());
                }
            }
        }
    }
    names
}

fn expr_to_path(expr_id: baml_compiler2_ast::ExprId, body: &ExprBody) -> Option<Vec<Name>> {
    match &body.exprs[expr_id] {
        Expr::Path(segments) if !segments.is_empty() => Some(segments.clone()),
        Expr::MemberAccess { base, member } => {
            let mut base_path = expr_to_path(*base, body)?;
            base_path.push(member.clone());
            Some(base_path)
        }
        _ => None,
    }
}

/// Flatten a compound `Ty` into its leaf throw facts.
/// Unions and optionals are decomposed; leaf types are kept as-is.
pub fn flatten_ty_to_facts(ty: &Ty) -> BTreeSet<ThrowFact> {
    let mut out = BTreeSet::new();
    collect_leaf_types(ty, &mut out);
    out
}

fn collect_leaf_types(ty: &Ty, out: &mut BTreeSet<Ty>) {
    match ty {
        // Compound types: decompose
        Ty::Optional(inner, _) => {
            collect_leaf_types(inner, out);
            out.insert(Ty::Primitive(PrimitiveType::Null, TyAttr::default()));
        }
        Ty::Union(members, _) => {
            for member in members {
                collect_leaf_types(member, out);
            }
        }
        // Literal types: widen to primitive for throw fact purposes
        Ty::Literal(lit, _, _) => {
            out.insert(Ty::Primitive(
                PrimitiveType::from_literal(lit),
                TyAttr::default(),
            ));
        }
        // Bottom/void: no facts
        Ty::Never { .. } | Ty::Void { .. } => {}
        // Everything else: keep as-is
        _ => {
            out.insert(ty.clone());
        }
    }
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

/// Reject catch-everything binding types — `unknown` and unresolved `any`.
///
/// Operates on the resolved `Ty` produced by TIR's `pattern_type`. Both
/// `unknown` (an explicit `unknown` type) and `any` (an unresolved path that
/// the user typed expecting it to mean "anything") collapse to
/// `Ty::BuiltinUnknown` / `Ty::Unknown` after resolution. We can't
/// distinguish between them at this point, so the diagnostic just says
/// "unknown".
pub fn is_banned_catch_binding_type(ty: &Ty) -> Option<&'static str> {
    if matches!(ty, Ty::BuiltinUnknown { .. } | Ty::Unknown { .. }) {
        Some("unknown")
    } else {
        None
    }
}
