//! HIR-level throw inference for compiler2 (BEP-007).
//!
//! This runs before type inference and computes a per-function transitive throw
//! set over the call graph. Functions with declared `throws` clauses act as
//! firewalls: their declared set becomes caller-visible, replacing body-derived
//! facts for propagation.

use std::collections::{BTreeMap, BTreeSet};

use baml_base::Name;
use baml_compiler2_ast::{AstSourceMap, Expr, ExprBody, Literal};
use baml_compiler2_hir::{
    contributions::Definition,
    package::{PackageId, PackageItems, package_dependencies},
};

use crate::{
    lower_type_expr::{lower_type_expr_in_ns, qualify_def},
    ty::{Ty, TyAttr},
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

            // `(flattened named facts, has_open_hole)`. A `_` in the clause makes
            // the declaration open: the named facts seed the set, but the body's
            // transitive throws are still merged on top (the barrier is disabled).
            let declared_throws_info = sig.throws.as_ref().map(|te| {
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
                let has_hole = throws_ty_has_infer_hole(&lowered);
                (flatten_ty_to_facts(&lowered), has_hole)
            });
            let declared_throws = declared_throws_info
                .as_ref()
                .map(|(facts, _)| facts.clone());
            let declared_has_hole = declared_throws_info
                .as_ref()
                .is_some_and(|(_, has_hole)| *has_hole);

            let direct = if let Some(declared) =
                declared_throws.clone().filter(|_| !declared_has_hole)
            {
                declared
            } else if let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() {
                let item_tree = baml_compiler2_ppir::file_item_tree(db, func_loc.file(db));
                let func_data = &item_tree[func_loc.id(db)];
                let param_types = lower_param_types(
                    db,
                    pkg_items,
                    &func_ns,
                    &func_data.generic_params,
                    &func_data.params,
                );
                // For an open (`| _`) declaration, seed with the named throws and
                // let the call-graph merge add the body's transitive throws.
                let mut facts = collect_direct_throws(
                    db,
                    pkg_items,
                    &func_ns,
                    *func_loc,
                    expr_body,
                    &param_types,
                );
                if let Some(named) = declared_throws.clone() {
                    facts.extend(named);
                }
                facts
            } else {
                declared_throws.clone().unwrap_or_default()
            };

            direct_facts.insert(key.clone(), direct);
            // An open (`| _`) declaration is NOT a contract barrier: callee
            // throws must still propagate so the hole is filled.
            has_declared_contract
                .insert(key.clone(), declared_throws.is_some() && !declared_has_hole);

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
                let declared_throws_info = sig.throws.as_ref().map(|te| {
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
                    let has_hole = throws_ty_has_infer_hole(&lowered);
                    (flatten_ty_to_facts(&lowered), has_hole)
                });
                let declared_throws = declared_throws_info
                    .as_ref()
                    .map(|(facts, _)| facts.clone());
                let declared_has_hole = declared_throws_info
                    .as_ref()
                    .is_some_and(|(_, has_hole)| *has_hole);

                let direct = if let Some(declared) =
                    declared_throws.clone().filter(|_| !declared_has_hole)
                {
                    declared
                } else if let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) =
                    body.as_ref()
                {
                    let param_types = lower_param_types(
                        db,
                        pkg_items,
                        &method_ns,
                        &method_data.generic_params,
                        &method_data.params,
                    );
                    let mut facts = collect_direct_throws(
                        db,
                        pkg_items,
                        &method_ns,
                        func_loc,
                        expr_body,
                        &param_types,
                    );
                    if let Some(named) = declared_throws.clone() {
                        facts.extend(named);
                    }
                    facts
                } else {
                    declared_throws.clone().unwrap_or_default()
                };

                direct_facts.insert(key.clone(), direct);
                has_declared_contract
                    .insert(key.clone(), declared_throws.is_some() && !declared_has_hole);

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
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
    body: &ExprBody,
    param_types: &[(Name, Ty)],
) -> BTreeSet<ThrowFact> {
    let mut facts = BTreeSet::new();
    let catch_arm_bodies = collect_catch_arm_bodies(body);
    // Rethrow detection is scoped by source span (a `throw e` rethrows only
    // inside the `catch (e)` arm that binds it), so fetch the span-bearing
    // source map — but only when the body actually has a `catch`. A `catch`-free
    // body needs no spans and stays insensitive to whitespace-only edits.
    let source_map = (!catch_arm_bodies.is_empty())
        .then(|| baml_compiler2_ppir::function_body_source_map(db, func_loc))
        .flatten();

    for (_, expr) in body.exprs.iter() {
        if let Expr::Throw { value } = expr
            && !is_catch_rethrow(*value, body, source_map.as_ref(), &catch_arm_bodies)
        {
            facts.insert(throw_fact_from_expr(
                db,
                pkg_items,
                ns_context,
                param_types,
                *value,
                body,
            ));
        }
    }
    for (_, stmt) in body.stmts.iter() {
        if let baml_compiler2_ast::Stmt::Throw { value } = stmt
            && !is_catch_rethrow(*value, body, source_map.as_ref(), &catch_arm_bodies)
        {
            facts.insert(throw_fact_from_expr(
                db,
                pkg_items,
                ns_context,
                param_types,
                *value,
                body,
            ));
        }
    }

    facts
}

/// Lower a function's parameter declarations to `(name, Ty)` pairs.
///
/// Used so a `throw <param>` expression can be typed from the declaration site
/// without invoking body inference (which would cycle back through throw-set
/// computation). Parameters without a written type (e.g. `self`) are skipped.
fn lower_param_types<'db>(
    db: &'db dyn crate::Db,
    pkg_items: &PackageItems<'db>,
    ns_context: &[Name],
    generic_params: &[Name],
    params: &[baml_compiler2_hir::item_tree::FunctionParam],
) -> Vec<(Name, Ty)> {
    params
        .iter()
        .filter_map(|param| {
            let type_expr = param.type_expr.as_ref()?;
            let mut diags = Vec::new();
            let ty = lower_type_expr_in_ns(
                db,
                type_expr,
                pkg_items,
                ns_context,
                generic_params,
                &mut diags,
            );
            Some((param.name.clone(), ty))
        })
        .collect()
}

/// Whether `value` is the operand of a *rethrow* — a `throw e` whose `e` names
/// a `catch` clause binding in scope, as in `catch (e) { _ => throw e }`.
///
/// A rethrow re-raises a value already accounted for (the caught expression's
/// throws are collected independently), so it contributes no new fact; the bare
/// binding can't be resolved to a nameable type anyway. The check is scoped by
/// span containment: `throw e` is a rethrow only when it lies *inside* a
/// `catch (e)` arm body. That is what distinguishes it from `throw e` where `e`
/// is a same-named parameter or local *outside* the catch — which is a real
/// throw of `e`, and treating it as a rethrow would drop its type from the set.
fn is_catch_rethrow(
    value: baml_compiler2_ast::ExprId,
    body: &ExprBody,
    source_map: Option<&AstSourceMap>,
    catch_arm_bodies: &[(&str, baml_compiler2_ast::ExprId)],
) -> bool {
    let Some(source_map) = source_map else {
        return false;
    };
    let Expr::Path(segments) = &body.exprs[value] else {
        return false;
    };
    let [name] = segments.as_slice() else {
        return false;
    };
    let value_span = source_map.expr_span(value);
    catch_arm_bodies.iter().any(|(binding, arm_body)| {
        *binding == name.as_str() && source_map.expr_span(*arm_body).contains_range(value_span)
    })
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
    param_types: &[(Name, Ty)],
    expr_id: baml_compiler2_ast::ExprId,
    body: &ExprBody,
) -> Ty {
    let fact = match &body.exprs[expr_id] {
        Expr::Literal(Literal::String(_)) => Ty::String {
            attr: TyAttr::default(),
        },
        Expr::Literal(Literal::Int(_)) => Ty::Int {
            attr: TyAttr::default(),
        },
        Expr::Literal(Literal::Float(_)) => Ty::Float {
            attr: TyAttr::default(),
        },
        Expr::Literal(Literal::Bool(_)) => Ty::Bool {
            attr: TyAttr::default(),
        },
        Expr::Null => Ty::Null {
            attr: TyAttr::default(),
        },
        Expr::Path(segments) if !segments.is_empty() => {
            // A thrown bare identifier naming a parameter (`throw s`) carries
            // that parameter's declared type — it is a value, not a type path.
            if let [name] = segments.as_slice()
                && let Some((_, ty)) = param_types.iter().find(|(param, _)| param == name)
            {
                ty.clone()
            } else {
                resolve_path_to_ty(db, pkg_items, ns_context, segments)
            }
        }
        Expr::MemberAccess { .. } => expr_to_path(expr_id, body)
            .map(|segments| resolve_path_to_ty(db, pkg_items, ns_context, &segments))
            .unwrap_or(Ty::Unknown {
                attr: TyAttr::default(),
            }),
        Expr::Object {
            type_name: path, ..
        } => resolve_path_to_ty(db, pkg_items, ns_context, path.segments()),
        _ => Ty::Unknown {
            attr: TyAttr::default(),
        },
    };
    // This lightweight, cycle-avoiding pass can't statically name every thrown
    // value: a call/binary/array/conditional result, or an unresolved path,
    // falls through to `Ty::Unknown`. `Unknown` is an inference-only sentinel
    // with no runtime representation — emitting it as a throws fact would trip
    // the `RuntimeTy` conversion boundary at codegen. Over-approximate to the
    // top type `unknown` (`BuiltinUnknown`) instead: it is sound (a `catch` must
    // handle the top type) and has a runtime representation. TIR's full
    // inference types these precisely for diagnostics; this set only feeds
    // runtime throws metadata, where a conservative bound is correct.
    if matches!(fact, Ty::Unknown { .. }) {
        Ty::BuiltinUnknown {
            attr: TyAttr::default(),
        }
    } else {
        fact
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
    // the enum path (e.g. `Status.HttpError` or `root.Status.Failed`).
    if segments.len() >= 2 {
        let enum_path = &segments[..segments.len() - 1];
        let variant = &segments[segments.len() - 1];
        let enum_name = enum_path.last().expect("enum_path is non-empty");
        let enum_ns = &enum_path[..enum_path.len() - 1];
        if let Some(def @ Definition::Enum(_)) =
            lookup_type_in_scope(db, pkg_items, ns_context, enum_ns, enum_name)
        {
            let qtn = qualify_def(db, def, enum_name);
            return Ty::EnumVariant(qtn, variant.clone(), TyAttr::default());
        }
    }

    // Otherwise resolve the full path as a type.
    let name = segments.last().expect("segments is non-empty");
    let seg_ns = &segments[..segments.len() - 1];
    if let Some(def) = lookup_type_in_scope(db, pkg_items, ns_context, seg_ns, name) {
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

/// Resolve `type_name` within namespace `ns`, applying the same fallbacks as
/// `lower_type_expr_in_ns` so throw-set recovery resolves a path identically
/// whether it appears as an enum-variant prefix or a plain type:
///
/// - a bare name (`ns` empty) is tried in `ns_context` first, then at the
///   package root;
/// - a `root.`-prefixed `ns` is retried against the current package with the
///   prefix stripped (own-package alias);
/// - any other leading segment is treated as a sibling package name.
fn lookup_type_in_scope<'db>(
    db: &'db dyn crate::Db,
    pkg_items: &PackageItems<'db>,
    ns_context: &[Name],
    ns: &[Name],
    type_name: &Name,
) -> Option<Definition<'db>> {
    let direct = if ns.is_empty() && !ns_context.is_empty() {
        pkg_items
            .lookup_type(ns_context, type_name)
            .or_else(|| pkg_items.lookup_type(ns, type_name))
    } else {
        pkg_items.lookup_type(ns, type_name)
    };
    direct.or_else(|| {
        let (first, rest) = ns.split_first()?;
        if first.as_str() == "root" {
            pkg_items.lookup_type(rest, type_name)
        } else {
            let pkg_id = PackageId::new(db, first.clone());
            let pkg = baml_compiler2_ppir::package_items(db, pkg_id);
            pkg.lookup_type(rest, type_name)
        }
    })
}

/// Collect, for each `catch` clause binding, the `(binding-name, arm-body-expr)`
/// pairs whose arm-body span scopes the rethrows of that binding.
fn collect_catch_arm_bodies(body: &ExprBody) -> Vec<(&str, baml_compiler2_ast::ExprId)> {
    let mut arms = Vec::new();
    for (_, expr) in body.exprs.iter() {
        if let Expr::Catch { clauses, .. } = expr {
            for clause in clauses {
                if let Some(name) = body.patterns[clause.binding].binding_name(&body.patterns) {
                    for &arm_id in &clause.arms {
                        arms.push((name.as_str(), body.catch_arms[arm_id].body));
                    }
                }
            }
        }
    }
    arms
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
/// Unions (including a nullable `T | null`) are decomposed; leaf types are
/// kept as-is.
pub fn flatten_ty_to_facts(ty: &Ty) -> BTreeSet<ThrowFact> {
    let mut out = BTreeSet::new();
    collect_leaf_types(ty, &mut out);
    out
}

/// Does a declared `throws` type contain a `_` inference hole (`Ty::Infer`),
/// either as the whole clause (`throws _`) or as a union member
/// (`throws AppError | _`)? Such a clause is an *open* contract: the function's
/// exposed throw set is the named members PLUS whatever its body transitively
/// throws, so the contract firewall must not narrow callers to the named set.
pub fn throws_ty_has_infer_hole(ty: &Ty) -> bool {
    match ty {
        Ty::Infer { .. } => true,
        Ty::Union(members, _) => members.iter().any(throws_ty_has_infer_hole),
        _ => false,
    }
}

fn collect_leaf_types(ty: &Ty, out: &mut BTreeSet<Ty>) {
    match ty {
        // Compound types: decompose
        Ty::Union(members, _) => {
            for member in members {
                collect_leaf_types(member, out);
            }
        }
        // Literal types: widen to primitive for throw fact purposes
        Ty::Literal(lit, _, _) => {
            let attr = TyAttr::default();
            out.insert(match lit {
                Literal::Int(_) => Ty::Int { attr },
                Literal::Bigint(_) => Ty::Bigint { attr },
                Literal::Float(_) => Ty::Float { attr },
                Literal::String(_) => Ty::String { attr },
                Literal::Bool(_) => Ty::Bool { attr },
            });
        }
        // Bottom/void: no facts. An inference hole (`_`) is likewise not a
        // concrete throw fact — it is an open-slot marker handled separately
        // (`throws_ty_has_infer_hole`), so the flattened set holds only the
        // named throws (`throws AppError | _` flattens to `{AppError}`).
        Ty::Never { .. } | Ty::Void { .. } | Ty::Infer { .. } => {}
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
