//! HIR-level throw inference (BEP-007).
//!
//! # Two-phase throw analysis
//!
//! Throw-fact collection happens in two phases, at two compiler layers:
//!
//! 1. **HIR-level (this module)** — syntax-only, pre-type-inference.
//!    Scans raw HIR expression/statement trees to extract throw type names
//!    from `Expr::Throw` / `Stmt::Throw` nodes, builds a call graph, and
//!    uses `AnalysisGraph` (Tarjan SCC + topological propagation) to compute
//!    transitive throw sets across functions.
//!
//! 2. **TIR-level** (`collect_throw_facts_from_value` in `lib.rs`) — uses
//!    fully inferred `Ty` from the type context. Provides precise facts for
//!    local catch-base analysis during type inference.
//!
//! ## Why two phases?
//!
//! Type inference for function A needs callee throw facts (for catch
//! exhaustiveness), but computing precise throw facts for callees requires
//! type-checking them — creating a potential cycle with mutual recursion.
//! This HIR pre-pass breaks the cycle: it runs before type inference and
//! supplies conservative cross-function facts via the `function_throw_sets`
//! salsa query.
//!
//! ## Limitations
//!
//! Operating on syntax alone, this module can resolve type names for:
//! - Literals (`throw "err"` → `"string"`)
//! - Paths (`throw Errors.NotFound` → `"Errors.NotFound"`)
//! - Typed object constructors (`throw AuthError {}` → `"AuthError"`)
//!
//! Anything requiring type resolution (variables, function call results)
//! falls back to `"unknown"`. The TIR-level pass fills in the precision
//! for local analysis.

use std::collections::BTreeSet;

use baml_base::Name;
use baml_compiler_analysis::{AnalysisGraph, AnalysisResult};
use baml_compiler_hir::{Expr, ExprBody, Literal, Stmt};

use crate::divergence::call_target_from_callee_expr;

/// A throw fact: the string name of a type that may be thrown.
pub type ThrowFact = String;

/// Input row for throw analysis.
pub struct ThrowAnalysisInput<'a> {
    pub name: Name,
    pub body: Option<&'a ExprBody>,
}

/// Extract a throw fact from a thrown expression's HIR representation.
///
/// Total function: always returns a fact. Expression forms that carry an
/// obvious type name produce that name; everything else yields `"unknown"`.
fn throw_fact_from_expr(expr: &Expr) -> ThrowFact {
    match expr {
        Expr::Literal(Literal::String(_)) => "string".into(),
        Expr::Literal(Literal::Int(_)) => "int".into(),
        Expr::Literal(Literal::Float(_)) => "float".into(),
        Expr::Literal(Literal::Bool(_)) => "bool".into(),
        Expr::Literal(Literal::Null) => "null".into(),
        Expr::Path(segments) if !segments.is_empty() => segments
            .iter()
            .map(Name::as_str)
            .collect::<Vec<_>>()
            .join("."),
        Expr::Object {
            type_name: Some(name),
            ..
        } => name.as_str().into(),
        _ => "unknown".into(),
    }
}

/// Collect direct throw types from a function body's HIR.
///
/// Flat-scans all expressions and statements for `Throw` nodes, recording a
/// throw fact for each.
pub fn collect_direct_throws(body: &ExprBody) -> BTreeSet<ThrowFact> {
    let mut facts = BTreeSet::new();

    for (_, expr) in body.exprs.iter() {
        if let Expr::Throw { value } = expr {
            facts.insert(throw_fact_from_expr(&body.exprs[*value]));
        }
    }
    for (_, stmt) in body.stmts.iter() {
        if let Stmt::Throw { value } = stmt {
            facts.insert(throw_fact_from_expr(&body.exprs[*value]));
        }
    }

    facts
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

#[cfg(test)]
mod tests {
    use la_arena::Arena;

    use super::*;

    #[test]
    fn throw_fact_from_expr_paths() {
        let single = Expr::Path(vec![Name::new("Status")]);
        assert_eq!(throw_fact_from_expr(&single), "Status");

        let dotted = Expr::Path(vec![Name::new("Status"), Name::new("HttpError")]);
        assert_eq!(throw_fact_from_expr(&dotted), "Status.HttpError");

        let deep = Expr::Path(vec![
            Name::new("pkg"),
            Name::new("Status"),
            Name::new("HttpError"),
        ]);
        assert_eq!(throw_fact_from_expr(&deep), "pkg.Status.HttpError");
    }

    #[test]
    fn throw_fact_from_expr_object_constructor() {
        let with_name = Expr::Object {
            type_name: Some(Name::new("AuthenticationError")),
            fields: Vec::new(),
            spreads: Vec::new(),
        };
        assert_eq!(throw_fact_from_expr(&with_name), "AuthenticationError");

        let without_name = Expr::Object {
            type_name: None,
            fields: Vec::new(),
            spreads: Vec::new(),
        };
        assert_eq!(throw_fact_from_expr(&without_name), "unknown");
    }

    #[test]
    fn throw_fact_from_expr_literals() {
        assert_eq!(
            throw_fact_from_expr(&Expr::Literal(Literal::String("x".into()))),
            "string"
        );
        assert_eq!(
            throw_fact_from_expr(&Expr::Literal(Literal::Int(42))),
            "int"
        );
        assert_eq!(
            throw_fact_from_expr(&Expr::Literal(Literal::Float("1.0".into()))),
            "float"
        );
        assert_eq!(
            throw_fact_from_expr(&Expr::Literal(Literal::Bool(true))),
            "bool"
        );
        assert_eq!(throw_fact_from_expr(&Expr::Literal(Literal::Null)), "null");
    }

    #[test]
    fn throw_fact_from_expr_unknown_fallback() {
        assert_eq!(throw_fact_from_expr(&Expr::Missing), "unknown");
        assert_eq!(throw_fact_from_expr(&Expr::Path(vec![])), "unknown");
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
    fn collect_direct_throws_object_constructor() {
        let mut exprs = Arena::new();
        let obj = exprs.alloc(Expr::Object {
            type_name: Some(Name::new("AuthenticationError")),
            fields: Vec::new(),
            spreads: Vec::new(),
        });
        let throw_expr = exprs.alloc(Expr::Throw { value: obj });
        let body = ExprBody {
            exprs,
            stmts: Arena::new(),
            patterns: Arena::new(),
            match_arms: Arena::new(),
            catch_arms: Arena::new(),
            types: Arena::new(),
            root_expr: Some(throw_expr),
            diagnostics: Vec::new(),
        };
        let throws = collect_direct_throws(&body);
        assert!(
            throws.contains("AuthenticationError"),
            "throw of object constructor should use type name, got: {throws:?}",
        );
        assert!(
            !throws.contains("unknown"),
            "throw of typed object constructor should NOT be 'unknown', got: {throws:?}",
        );
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
