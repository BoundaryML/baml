//! HIR-level throw inference for compiler2 (BEP-007).
//!
//! This runs before type inference and computes a per-function transitive throw
//! set over the call graph. Functions with declared `throws` clauses act as
//! firewalls: their declared set becomes caller-visible, replacing body-derived
//! facts for propagation.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use baml_base::Name;
use baml_compiler2_ast::{Expr, ExprBody, Literal, Pattern, Stmt, TypeExpr};
use baml_compiler2_hir::{contributions::Definition, package::PackageId};
use baml_compiler2_ppir::package_items;

use crate::{lower_type_expr::lower_type_expr, ty::Ty};

pub type ThrowFact = String;

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
                throw_facts_from_ty(&lowered)
            });

            let direct = if let Some(declared) = declared_throws.clone() {
                declared
            } else if let baml_compiler2_hir::body::FunctionBody::Expr(expr_body) = body.as_ref() {
                collect_direct_throws(expr_body)
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

pub fn collect_direct_throws(body: &ExprBody) -> BTreeSet<ThrowFact> {
    let mut facts = BTreeSet::new();

    for (_, expr) in body.exprs.iter() {
        if let Expr::Throw { value } = expr {
            facts.insert(throw_fact_from_expr(*value, body));
        }
    }
    for (_, stmt) in body.stmts.iter() {
        if let Stmt::Throw { value } = stmt {
            facts.insert(throw_fact_from_expr(*value, body));
        }
    }

    let catch_bindings = collect_catch_binding_names(body);
    if !catch_bindings.is_empty() {
        facts.retain(|fact| !catch_bindings.contains(fact.as_str()));
    }

    facts
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

fn throw_fact_from_expr(expr_id: baml_compiler2_ast::ExprId, body: &ExprBody) -> ThrowFact {
    match &body.exprs[expr_id] {
        Expr::Literal(Literal::String(_)) => "string".into(),
        Expr::Literal(Literal::Int(_)) => "int".into(),
        Expr::Literal(Literal::Float(_)) => "float".into(),
        Expr::Literal(Literal::Bool(_)) => "bool".into(),
        Expr::Null => "null".into(),
        Expr::Path(segments) if !segments.is_empty() => segments
            .iter()
            .map(Name::as_str)
            .collect::<Vec<_>>()
            .join("."),
        Expr::FieldAccess { .. } => expr_to_path(expr_id, body)
            .map(|segments| {
                segments
                    .iter()
                    .map(Name::as_str)
                    .collect::<Vec<_>>()
                    .join(".")
            })
            .unwrap_or_else(|| "unknown".into()),
        Expr::Object {
            type_name: Some(name),
            ..
        } => name.as_str().into(),
        _ => "unknown".into(),
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

pub fn throw_facts_from_ty(ty: &Ty) -> BTreeSet<ThrowFact> {
    let mut out = BTreeSet::new();
    collect_throw_facts_from_ty(ty, &mut out);
    out
}

fn collect_throw_facts_from_ty(ty: &Ty, out: &mut BTreeSet<ThrowFact>) {
    match ty {
        Ty::Primitive(p) => out.insert(p.to_string()),
        Ty::Literal(lit, _) => out.insert(match lit {
            baml_base::Literal::String(_) => "string".to_string(),
            baml_base::Literal::Int(_) => "int".to_string(),
            baml_base::Literal::Float(_) => "float".to_string(),
            baml_base::Literal::Bool(_) => "bool".to_string(),
        }),
        Ty::Class(qn) | Ty::Enum(qn) | Ty::TypeAlias(qn) => {
            out.insert(qn.name.as_str().to_string())
        }
        Ty::EnumVariant(qn, variant) => out.insert(format!("{}.{}", qn.name, variant)),
        Ty::Optional(inner) => {
            collect_throw_facts_from_ty(inner, out);
            out.insert("null".to_string())
        }
        Ty::Union(members) => {
            for member in members {
                collect_throw_facts_from_ty(member, out);
            }
            true
        }
        Ty::Unknown | Ty::Error | Ty::BuiltinUnknown => out.insert("unknown".to_string()),
        Ty::Never | Ty::Void => true,
        Ty::List(_)
        | Ty::Map(_, _)
        | Ty::EvolvingList(_)
        | Ty::EvolvingMap(_, _)
        | Ty::Function { .. }
        | Ty::RustType => out.insert(ty.to_string()),
    };
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
