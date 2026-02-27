//! BEP-007: Throw inference and contract checking.
//!
//! Scans HIR function bodies for `throw` expressions to compute direct throw
//! facts, builds a call graph, and uses `AnalysisGraph` to propagate throw
//! types transitively. Checks declared `throws` contracts against inferred sets.

use std::collections::BTreeSet;

use baml_base::Name;
use baml_compiler_analysis::{AnalysisGraph, AnalysisResult};
use baml_compiler_hir::{Expr, ExprBody, FunctionSignature, Literal, Stmt, TypeRef};

use crate::divergence::call_target_from_callee_expr;

/// A throw fact: the string name of a type that may be thrown.
pub type ThrowFact = String;

/// Input row for throw analysis.
pub struct ThrowAnalysisInput<'a> {
    pub name: Name,
    pub body: Option<&'a ExprBody>,
}

/// Result of throw inference for a single function.
#[derive(Debug, Clone)]
pub struct FunctionThrowInfo {
    /// Types directly thrown by this function.
    pub direct: BTreeSet<ThrowFact>,
    /// Types transitively thrown (direct + propagated from callees).
    pub transitive: BTreeSet<ThrowFact>,
}

/// Collect direct throw types from a function body's HIR.
///
/// Walks all expressions and statements looking for `Expr::Throw` and
/// `Stmt::Throw`, recording the inferred type name of the thrown value.
pub fn collect_direct_throws(body: &ExprBody) -> BTreeSet<ThrowFact> {
    let mut throws = BTreeSet::new();

    for (_id, expr) in body.exprs.iter() {
        if let Expr::Throw { value } = expr {
            if let Some(type_name) = infer_throw_type_name(&body.exprs[*value]) {
                throws.insert(type_name);
            } else {
                throws.insert("unknown".to_string());
            }
        }
    }

    for (_id, stmt) in body.stmts.iter() {
        if let Stmt::Throw { value } = stmt {
            if let Some(type_name) = infer_throw_type_name(&body.exprs[*value]) {
                throws.insert(type_name);
            } else {
                throws.insert("unknown".to_string());
            }
        }
    }

    throws
}

/// Collect function call targets from a function body's HIR.
///
/// Returns the set of function names that this body calls.
pub fn collect_call_targets(body: &ExprBody) -> BTreeSet<Name> {
    let mut targets: BTreeSet<Name> = BTreeSet::new();

    for (_id, expr) in body.exprs.iter() {
        if let Expr::Call { callee, .. } = expr {
            if let Some(target) = call_target_from_callee_expr(*callee, body) {
                targets.insert(target);
            }
        }
    }

    targets
}

/// Try to infer the type name of a thrown expression from its HIR representation.
fn infer_throw_type_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Literal(Literal::String(_)) => Some("string".to_string()),
        Expr::Literal(Literal::Int(_)) => Some("int".to_string()),
        Expr::Literal(Literal::Float(_)) => Some("float".to_string()),
        Expr::Literal(Literal::Bool(_)) => Some("bool".to_string()),
        Expr::Literal(Literal::Null) => Some("null".to_string()),
        // Preserve path precision for thrown values:
        // - `Status.HttpError` -> `Status.HttpError`
        // - `pkg.Status.HttpError` -> `pkg.Status.HttpError`
        // - `Status` -> `Status`
        Expr::Path(segments) if !segments.is_empty() => {
            if segments.len() == 1 {
                Some(segments[0].as_str().to_string())
            } else {
                Some(
                    segments
                        .iter()
                        .map(Name::as_str)
                        .collect::<Vec<_>>()
                        .join("."),
                )
            }
        }
        _ => None,
    }
}

/// Build a throw analysis graph from a set of function signatures and bodies.
///
/// Returns the analysis result with per-function direct and transitive throw sets.
pub fn analyze_throws(functions: &[ThrowAnalysisInput<'_>]) -> AnalysisResult<Name, ThrowFact> {
    let mut graph: AnalysisGraph<Name, ThrowFact> = AnalysisGraph::new();

    for function in functions {
        let direct_throws = function
            .body
            .map_or_else(BTreeSet::new, collect_direct_throws);
        graph.add_node(function.name.clone(), direct_throws);
    }

    for function in functions {
        if let Some(b) = function.body {
            for target in collect_call_targets(b) {
                graph.add_edge(function.name.clone(), target);
            }
        }
    }

    graph.analyze()
}

/// Check a function's throws contract against its inferred throw set.
///
/// Returns `None` if the contract is satisfied or there is no contract.
/// Returns `Some(violation)` with a description of the violation.
pub fn check_throws_contract(
    sig: &FunctionSignature,
    inferred: &BTreeSet<ThrowFact>,
) -> Option<ThrowsViolation> {
    let declared = sig.throws.as_ref()?;
    let declared_types = extract_type_names(declared);

    // Check if all inferred types are covered by the declared set
    let uncovered: BTreeSet<&String> = inferred
        .iter()
        .filter(|t| !declared_types.contains(t.as_str()))
        .collect();

    if uncovered.is_empty() {
        return None;
    }

    Some(ThrowsViolation {
        function_name: sig.name.to_string(),
        declared_types,
        uncovered_types: uncovered.into_iter().cloned().collect(),
    })
}

/// A throws contract violation.
#[derive(Debug, Clone)]
pub struct ThrowsViolation {
    pub function_name: String,
    pub declared_types: BTreeSet<String>,
    pub uncovered_types: BTreeSet<String>,
}

impl std::fmt::Display for ThrowsViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Function '{}' declares `throws {}` but may also throw: {}",
            self.function_name,
            self.declared_types
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(" | "),
            self.uncovered_types
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
        )
    }
}

/// Extract the set of type names from a `TypeRef` (handling unions).
fn extract_type_names(ty: &TypeRef) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    match ty {
        TypeRef::Union(members) => {
            for member in members {
                names.extend(extract_type_names(member));
            }
        }
        TypeRef::Int => {
            names.insert("int".to_string());
        }
        TypeRef::Float => {
            names.insert("float".to_string());
        }
        TypeRef::String => {
            names.insert("string".to_string());
        }
        TypeRef::Bool => {
            names.insert("bool".to_string());
        }
        TypeRef::Null => {
            names.insert("null".to_string());
        }
        TypeRef::Path(path) => {
            let full_name = path
                .segments
                .iter()
                .map(Name::as_str)
                .collect::<Vec<_>>()
                .join(".");
            names.insert(full_name);
        }
        _ => {}
    }
    names
}

#[cfg(test)]
mod tests {
    use la_arena::Arena;

    use super::*;

    #[test]
    fn extract_type_names_primitives() {
        assert_eq!(
            extract_type_names(&TypeRef::Int),
            ["int".to_string()].into()
        );
        assert_eq!(
            extract_type_names(&TypeRef::String),
            ["string".to_string()].into()
        );
    }

    #[test]
    fn extract_type_names_union() {
        let union_ty = TypeRef::Union(vec![TypeRef::Int, TypeRef::String]);
        let names = extract_type_names(&union_ty);
        assert!(names.contains("int"));
        assert!(names.contains("string"));
        assert_eq!(names.len(), 2);
    }

    #[test]
    fn infer_throw_type_name_preserves_variant_path_for_variants() {
        let single = Expr::Path(vec![Name::new("Status")]);
        assert_eq!(infer_throw_type_name(&single), Some("Status".to_string()));

        let two = Expr::Path(vec![Name::new("Status"), Name::new("HttpError")]);
        assert_eq!(
            infer_throw_type_name(&two),
            Some("Status.HttpError".to_string())
        );

        let three = Expr::Path(vec![
            Name::new("pkg"),
            Name::new("Status"),
            Name::new("HttpError"),
        ]);
        assert_eq!(
            infer_throw_type_name(&three),
            Some("pkg.Status.HttpError".to_string())
        );
    }

    fn make_throw_body(value: Literal) -> ExprBody {
        let mut exprs = Arena::new();
        let value_id = exprs.alloc(Expr::Literal(value));
        let throw_id = exprs.alloc(Expr::Throw { value: value_id });
        ExprBody {
            exprs,
            stmts: Arena::new(),
            patterns: Arena::new(),
            match_arms: Arena::new(),
            catch_arms: Arena::new(),
            types: Arena::new(),
            root_expr: Some(throw_id),
            diagnostics: Vec::new(),
        }
    }

    fn make_call_body(target: &str) -> ExprBody {
        let mut exprs = Arena::new();
        let callee = exprs.alloc(Expr::Path(vec![Name::new(target)]));
        let call = exprs.alloc(Expr::Call {
            callee,
            args: Vec::new(),
        });
        ExprBody {
            exprs,
            stmts: Arena::new(),
            patterns: Arena::new(),
            match_arms: Arena::new(),
            catch_arms: Arena::new(),
            types: Arena::new(),
            root_expr: Some(call),
            diagnostics: Vec::new(),
        }
    }

    fn make_throw_and_call_body(value: Literal, target: &str) -> ExprBody {
        let mut exprs = Arena::new();
        let value_id = exprs.alloc(Expr::Literal(value));
        let throw_id = exprs.alloc(Expr::Throw { value: value_id });
        let callee = exprs.alloc(Expr::Path(vec![Name::new(target)]));
        let _call = exprs.alloc(Expr::Call {
            callee,
            args: Vec::new(),
        });
        ExprBody {
            exprs,
            stmts: Arena::new(),
            patterns: Arena::new(),
            match_arms: Arena::new(),
            catch_arms: Arena::new(),
            types: Arena::new(),
            root_expr: Some(throw_id),
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn analyze_throws_propagates_transitively() {
        let body_a = make_throw_body(Literal::String("boom".to_string()));
        let body_b = make_call_body("A");

        let inputs = vec![
            ThrowAnalysisInput {
                name: Name::new("A"),
                body: Some(&body_a),
            },
            ThrowAnalysisInput {
                name: Name::new("B"),
                body: Some(&body_b),
            },
        ];

        let result = analyze_throws(&inputs);
        assert!(
            result
                .transitive(&Name::new("A"))
                .is_some_and(|s| s.contains("string"))
        );
        assert!(
            result
                .transitive(&Name::new("B"))
                .is_some_and(|s| s.contains("string"))
        );
    }

    #[test]
    fn analyze_throws_handles_recursive_scc() {
        let body_a = make_throw_and_call_body(Literal::Int(1), "B");
        let body_b = make_call_body("A");

        let inputs = vec![
            ThrowAnalysisInput {
                name: Name::new("A"),
                body: Some(&body_a),
            },
            ThrowAnalysisInput {
                name: Name::new("B"),
                body: Some(&body_b),
            },
        ];

        let result = analyze_throws(&inputs);
        assert!(
            result
                .transitive(&Name::new("A"))
                .is_some_and(|s| s.contains("int"))
        );
        assert!(
            result
                .transitive(&Name::new("B"))
                .is_some_and(|s| s.contains("int"))
        );
    }
}
