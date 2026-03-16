//! HIR-level throw inference for compiler2 (BEP-007).
//!
//! This runs before type inference and computes a per-function transitive throw
//! set over the call graph. Functions with declared `throws` clauses act as
//! firewalls: their declared set becomes caller-visible, replacing body-derived
//! facts for propagation.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use baml_base::Name;
use baml_compiler2_ast::{Expr, ExprBody, Literal, Pattern, TypeExpr};
use baml_compiler2_hir::{
    contributions::Definition,
    package::{PackageId, PackageItems, package_items},
};

use crate::{
    lower_type_expr::{lower_type_expr, qualify_def},
    ty::{PrimitiveType, Ty},
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
    let mut graph: baml_compiler_analysis::AnalysisGraph<Name, ThrowFact> =
        baml_compiler_analysis::AnalysisGraph::new();

    let mut call_edges: BTreeMap<Name, BTreeSet<Name>> = BTreeMap::new();
    let mut has_declared_contract: BTreeMap<Name, bool> = BTreeMap::new();

    for ns in pkg_items.namespaces.values() {
        for (short_name, def) in &ns.values {
            let Definition::Function(func_loc) = def else {
                continue;
            };

            let key = function_key(db, *func_loc, short_name);
            let sig = baml_compiler2_hir::signature::function_signature(db, *func_loc);
            let body = baml_compiler2_hir::body::function_body(db, *func_loc);

            let declared_throws = sig.throws.as_ref().map(|te| {
                let mut diags = Vec::new();
                let lowered = lower_type_expr(db, te, pkg_items, &mut diags);
                // These diagnostics are reported at the signature site by inference;
                // throw graph propagation still uses best-effort lowering.
                drop(diags);
                flatten_ty_to_facts(&lowered)
            });

            let direct = if let Some(declared) = declared_throws.clone() {
                declared
            } else if let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() {
                collect_direct_throws(db, pkg_items, expr_body)
            } else {
                BTreeSet::new()
            };

            graph.add_node(key.clone(), direct);
            has_declared_contract.insert(key.clone(), declared_throws.is_some());

            if let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() {
                call_edges.insert(key, collect_call_targets(expr_body));
            }
        }
    }

    for (from, targets) in &call_edges {
        if has_declared_contract.get(from).copied().unwrap_or(false) {
            continue;
        }
        for to in targets {
            graph.add_edge(from.clone(), to.clone());
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

fn function_key<'db>(
    db: &'db dyn crate::Db,
    func: baml_compiler2_hir::loc::FunctionLoc<'db>,
    short_name: &Name,
) -> Name {
    let file = func.file(db);
    let pkg = baml_compiler2_hir::file_package::file_package(db, file);
    if pkg.namespace_path.is_empty() {
        short_name.clone()
    } else {
        let mut parts: Vec<String> = pkg
            .namespace_path
            .iter()
            .map(|n| n.as_str().to_string())
            .collect();
        parts.push(short_name.as_str().to_string());
        Name::new(parts.join("."))
    }
}

pub fn collect_direct_throws<'db>(
    db: &'db dyn crate::Db,
    pkg_items: &PackageItems<'db>,
    body: &ExprBody,
) -> BTreeSet<ThrowFact> {
    let mut facts = BTreeSet::new();

    for (_, expr) in body.exprs.iter() {
        if let Expr::Throw { value } = expr {
            facts.insert(throw_fact_from_expr(db, pkg_items, *value, body));
        }
    }
    for (_, stmt) in body.stmts.iter() {
        if let baml_compiler2_ast::Stmt::Throw { value } = stmt {
            facts.insert(throw_fact_from_expr(db, pkg_items, *value, body));
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
        Ty::Primitive(p) => p.to_string(),
        Ty::Class(qn) | Ty::Enum(qn) | Ty::TypeAlias(qn) => qn.name.as_str().to_string(),
        Ty::EnumVariant(qn, variant) => format!("{}.{}", qn.name, variant),
        Ty::Unknown => "unknown".to_string(),
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

/// Convert a thrown expression to a `Ty` directly, using pkg_items to resolve
/// paths to their actual types (enum variants, classes, etc).
fn throw_fact_from_expr<'db>(
    db: &'db dyn crate::Db,
    pkg_items: &PackageItems<'db>,
    expr_id: baml_compiler2_ast::ExprId,
    body: &ExprBody,
) -> Ty {
    match &body.exprs[expr_id] {
        Expr::Literal(Literal::String(_)) => Ty::Primitive(PrimitiveType::String),
        Expr::Literal(Literal::Int(_)) => Ty::Primitive(PrimitiveType::Int),
        Expr::Literal(Literal::Float(_)) => Ty::Primitive(PrimitiveType::Float),
        Expr::Literal(Literal::Bool(_)) => Ty::Primitive(PrimitiveType::Bool),
        Expr::Null => Ty::Primitive(PrimitiveType::Null),
        Expr::Path(segments) if !segments.is_empty() => resolve_path_to_ty(db, pkg_items, segments),
        Expr::FieldAccess { .. } => expr_to_path(expr_id, body)
            .map(|segments| resolve_path_to_ty(db, pkg_items, &segments))
            .unwrap_or(Ty::Unknown),
        Expr::Object {
            type_name: Some(name),
            ..
        } => {
            if let Some(def) = pkg_items.lookup_type(&[name.clone()]) {
                match def {
                    Definition::Class(_) => Ty::Class(qualify_def(db, def, name)),
                    Definition::Enum(_) => Ty::Enum(qualify_def(db, def, name)),
                    _ => Ty::Unknown,
                }
            } else {
                Ty::Unknown
            }
        }
        _ => Ty::Unknown,
    }
}

/// Resolve a path like `["Status", "HttpError"]` or `["ns", "Status", "HttpError"]`
/// or `["Status"]` to a `Ty`.
fn resolve_path_to_ty<'db>(
    db: &'db dyn crate::Db,
    pkg_items: &PackageItems<'db>,
    segments: &[Name],
) -> Ty {
    // Try treating the last segment as an enum variant and the prefix as
    // the enum path. We try progressively shorter prefixes so that
    // ["ns", "Status", "HttpError"] → enum_path=["ns", "Status"], variant="HttpError"
    // works alongside ["Status", "HttpError"] → enum_path=["Status"], variant="HttpError".
    //
    // This must run BEFORE the generic lookup because the namespace system
    // registers enum variants as types in a child namespace, so
    // `lookup_type(&["Status", "HttpError"])` would incorrectly match
    // "HttpError" as a standalone enum rather than a variant of Status.
    if segments.len() >= 2 {
        let enum_path = &segments[..segments.len() - 1];
        let variant = &segments[segments.len() - 1];
        if let Some(def) = pkg_items.lookup_type(enum_path) {
            if let Definition::Enum(_) = def {
                let enum_name = &enum_path[enum_path.len() - 1];
                let qtn = qualify_def(db, def, enum_name);
                return Ty::EnumVariant(qtn, variant.clone());
            }
        }
    }

    // Try the full path as a type lookup (handles namespaced types and
    // single-segment names).
    if let Some(def) = pkg_items.lookup_type(segments) {
        let name = segments.last().unwrap();
        return match def {
            Definition::Class(_) => Ty::Class(qualify_def(db, def, name)),
            Definition::Enum(_) => Ty::Enum(qualify_def(db, def, name)),
            Definition::TypeAlias(_) => Ty::TypeAlias(qualify_def(db, def, name)),
            _ => Ty::Unknown,
        };
    }

    Ty::Unknown
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
        Ty::Optional(inner) => {
            collect_leaf_types(inner, out);
            out.insert(Ty::Primitive(PrimitiveType::Null));
        }
        Ty::Union(members) => {
            for member in members {
                collect_leaf_types(member, out);
            }
        }
        // Literal types: widen to primitive for throw fact purposes
        Ty::Literal(lit, _) => {
            out.insert(Ty::Primitive(PrimitiveType::from_literal(lit)));
        }
        // Bottom/void: no facts
        Ty::Never | Ty::Void => {}
        // Everything else: keep as-is
        _ => {
            out.insert(ty.clone());
        }
    }
}

pub fn is_banned_catch_binding_type(ty: &TypeExpr) -> Option<&'static str> {
    match ty {
        TypeExpr::BuiltinUnknown => Some("unknown"),
        TypeExpr::Path(segments) if segments.len() == 1 && segments[0].as_str() == "any" => {
            Some("any")
        }
        _ => None,
    }
}
