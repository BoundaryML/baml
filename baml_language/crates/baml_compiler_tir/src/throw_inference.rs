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

use std::collections::{BTreeSet, HashSet};

use baml_base::Name;
use baml_compiler_analysis::{AnalysisGraph, AnalysisResult};
use baml_compiler_hir::{Expr, ExprBody, Literal, Pattern, Stmt};

use crate::divergence::call_target_from_callee_expr;

/// A throw fact: the string name of a type that may be thrown.
pub type ThrowFact = String;

/// Input row for throw analysis.
pub struct ThrowAnalysisInput<'a> {
    pub name: Name,
    pub body: Option<&'a ExprBody>,
    /// Expanded throw facts from a `throws` declaration, if present.
    /// When `Some`, replaces body-derived facts as the caller-visible throw set.
    pub declared_throws: Option<BTreeSet<ThrowFact>>,
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
/// throw fact for each. Then filters out facts that match catch binding
/// variable names — `throw e` inside `catch (e) { ... }` is a re-throw
/// whose types propagate through call edges, not direct facts.
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

    let catch_bindings = collect_catch_binding_names(body);
    if !catch_bindings.is_empty() {
        facts.retain(|fact| !catch_bindings.contains(fact.as_str()));
    }

    facts
}

/// Collect all catch binding variable names from a function body.
///
/// These names are used to filter garbage facts from `collect_direct_throws`.
/// A `throw e` where `e` is a catch binding produces the variable name as a
/// fact, which is meaningless — the actual throw types propagate through
/// call edges from the catch base's callee.
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
        // When a function has a `throws` declaration, use that as its
        // caller-visible throw set. The contract check (in TIR inference)
        // ensures the body is consistent with the declaration.
        let direct_throws = if let Some(ref declared) = function.declared_throws {
            declared.clone()
        } else {
            function
                .body
                .map_or_else(BTreeSet::new, collect_direct_throws)
        };
        graph.add_node(function.name.clone(), direct_throws);
    }

    // Call edges still come from the body — they determine transitive propagation
    // for functions *without* a `throws` declaration.
    // Functions *with* a declaration act as "firewalls": their declared facts
    // are the complete set, so callee edges beyond them don't propagate further.
    for function in functions {
        // Only add call edges for functions without `throws` declarations.
        // With a declaration, the declared set *is* the complete throw surface.
        if function.declared_throws.is_none() {
            if let Some(b) = function.body {
                for target in collect_call_targets(b) {
                    graph.add_edge(function.name.clone(), target);
                }
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
                declared_throws: None,
            },
            ThrowAnalysisInput {
                name: Name::new("B"),
                body: Some(&body_b),
                declared_throws: None,
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
                declared_throws: None,
            },
            ThrowAnalysisInput {
                name: Name::new("B"),
                body: Some(&body_b),
                declared_throws: None,
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

    #[test]
    fn analyze_throws_uses_declared_contract() {
        // A has `throws string | int` declared; body only throws string.
        // The declared set should be used as the caller-visible throw set.
        let body_a = make_throw_body(Literal::String("boom".to_string()));
        let body_b = make_call_body("A");

        let inputs = vec![
            ThrowAnalysisInput {
                name: Name::new("A"),
                body: Some(&body_a),
                declared_throws: Some(BTreeSet::from(["string".to_string(), "int".to_string()])),
            },
            ThrowAnalysisInput {
                name: Name::new("B"),
                body: Some(&body_b),
                declared_throws: None,
            },
        ];

        let result = analyze_throws(&inputs);
        // A's transitive set is its declared contract
        let a_set = result.transitive(&Name::new("A")).unwrap();
        assert!(a_set.contains("string"), "A should contain string");
        assert!(
            a_set.contains("int"),
            "A should contain int (from declaration)"
        );

        // B calls A, so B sees A's declared contract
        let b_set = result.transitive(&Name::new("B")).unwrap();
        assert!(b_set.contains("string"), "B should see string from A");
        assert!(
            b_set.contains("int"),
            "B should see int from A's declaration"
        );
    }

    #[test]
    fn collect_direct_throws_filters_catch_binding_rethrows() {
        use baml_compiler_hir::{CatchArm, CatchClause, CatchClauseKind, Pattern};

        // Build: catch (e) { _ => throw e }
        // Also has: throw "real_error"
        // The "e" fact should be filtered; "string" should survive.
        let mut exprs = Arena::new();
        let mut patterns = Arena::new();
        let mut catch_arms_arena = Arena::new();

        // `throw "real_error"` at top level
        let real_value = exprs.alloc(Expr::Literal(Literal::String("real_error".into())));
        let _real_throw = exprs.alloc(Expr::Throw { value: real_value });

        // catch base: some call
        let callee = exprs.alloc(Expr::Path(vec![Name::new("getProfile")]));
        let call = exprs.alloc(Expr::Call {
            callee,
            args: Vec::new(),
        });

        // catch arm body: `throw e`
        let e_ref = exprs.alloc(Expr::Path(vec![Name::new("e")]));
        let rethrow = exprs.alloc(Expr::Throw { value: e_ref });

        // catch binding pattern
        let binding_pat = patterns.alloc(Pattern::Binding(Name::new("e")));
        // arm pattern (wildcard)
        let arm_pat = patterns.alloc(Pattern::Binding(Name::new("_")));

        let arm = catch_arms_arena.alloc(CatchArm {
            pattern: arm_pat,
            body: rethrow,
        });

        let _catch_expr = exprs.alloc(Expr::Catch {
            base: call,
            clauses: vec![CatchClause {
                kind: CatchClauseKind::Catch,
                binding: binding_pat,
                arms: vec![arm],
            }],
        });

        let body = ExprBody {
            exprs,
            stmts: Arena::new(),
            patterns,
            match_arms: Arena::new(),
            catch_arms: catch_arms_arena,
            types: Arena::new(),
            root_expr: None,
            diagnostics: Vec::new(),
        };

        let throws = collect_direct_throws(&body);
        assert!(
            throws.contains("string"),
            "should keep 'string' from throw \"real_error\", got: {throws:?}"
        );
        assert!(
            !throws.contains("e"),
            "should filter 'e' (catch binding re-throw), got: {throws:?}"
        );
    }

    #[test]
    fn analyze_throws_declared_acts_as_firewall() {
        // A throws "string" (body-derived).
        // B declares `throws int`, body calls A.
        // C calls B.
        // C should only see "int" (B's declaration), NOT "string" from A.
        let body_a = make_throw_body(Literal::String("boom".to_string()));
        let body_b = make_call_body("A");
        let body_c = make_call_body("B");

        let inputs = vec![
            ThrowAnalysisInput {
                name: Name::new("A"),
                body: Some(&body_a),
                declared_throws: None,
            },
            ThrowAnalysisInput {
                name: Name::new("B"),
                body: Some(&body_b),
                declared_throws: Some(BTreeSet::from(["int".to_string()])),
            },
            ThrowAnalysisInput {
                name: Name::new("C"),
                body: Some(&body_c),
                declared_throws: None,
            },
        ];

        let result = analyze_throws(&inputs);
        let c_set = result.transitive(&Name::new("C")).unwrap();
        assert!(
            c_set.contains("int"),
            "C should see int from B's declaration"
        );
        assert!(
            !c_set.contains("string"),
            "C should NOT see string — B's declaration is a firewall"
        );
    }
}
