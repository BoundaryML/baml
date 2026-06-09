//! Inline type / parameter-name annotations for BAML files (inlay hints).
//!
//! Provides `annotations(db, file) -> Vec<InlineAnnotation>` — a regular
//! function (not a Salsa query) that walks expression-body functions in a file
//! (top-level functions, class/interface methods, and the synthesized
//! `$init_test` registration functions), recursing into lambda bodies (e.g.
//! the bodies of `test` / `testset` blocks, which lower to lambdas passed to
//! `register_test`). It produces two kinds of hints:
//!
//! ## Type hints on `let` bindings
//!
//! For each `Stmt::Let` **without** a type annotation, we display the inferred
//! type of the binding after the variable name, e.g.:
//!
//! ```baml
//! let x = 42          // → x: int
//! let items = [1, 2]  // → items: int[]
//! ```
//!
//! The hint is positioned at the end of the pattern span (just after the
//! variable name token).
//!
//! ## Parameter-name hints on call expressions
//!
//! For each `Expr::Call { callee, args }` where the callee resolves to a
//! `Ty::Function { params }`, we display the parameter name before each
//! positional argument, e.g.:
//!
//! ```baml
//! foo(42, "hello")  // → foo(x: 42, y: "hello")
//! ```
//!
//! Each hint is positioned at the start of the argument's span.
//!
//! ## Scopes
//!
//! Types are resolved across the file's scopes (any-scope lookup) so a
//! binding/expression living in a nested block or lambda still resolves.
//!
//! ## Suppression
//!
//! We suppress type hints for:
//! - Unknown / error types (noise)
//! - Bindings named `_` (discard patterns)
//!
//! We suppress parameter-name hints when:
//! - The callee type is not `Ty::Function` (no param info)
//! - The param name is `None` (positional-only parameter)
//! - The argument count != param count (variadic / error cases)
//!
//! LLM declarative functions are skipped entirely (and never recursed into), so
//! their synthetic `client` / `function_name` / `args` calls produce no hints.

use baml_base::SourceFile;
use baml_compiler2_ast::{
    Expr, ExprId, Stmt,
    ast::{AstSourceMap, DeclarativeMeta, ExprBody, FunctionBodyDef, FunctionOrigin},
};
use baml_compiler2_hir::{body::FunctionBody, loc::FunctionLoc};
use baml_compiler2_tir::{inference::infer_scope_types, ty::Ty};
use text_size::TextSize;

use crate::{Db, utils};

type SemanticIndex<'a> = baml_compiler2_hir::semantic_index::FileSemanticIndex<'a>;

// ── Public types ──────────────────────────────────────────────────────────────

/// The semantic kind of an inline annotation, mirroring the LSP `InlayHintKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationKind {
    /// A type hint after a variable name: `x: int`
    Type,
    /// A parameter-name hint before a call argument: `name:`
    Parameter,
}

/// A single inline annotation (inlay hint) to display in the editor.
#[derive(Debug, Clone)]
pub struct InlineAnnotation {
    /// Byte offset in the file where the hint is inserted.
    pub offset: TextSize,
    /// The text label to display (e.g. `": int"` or `"name: "`).
    pub label: String,
    /// Semantic kind used by the editor for styling/filtering.
    pub kind: AnnotationKind,
    /// Insert thin space to the left of the hint (between hint and preceding token).
    pub padding_left: bool,
    /// Insert thin space to the right of the hint (between hint and following token).
    pub padding_right: bool,
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Compute inline annotations (inlay hints) for a file.
///
/// Returns annotations sorted in document order (required by the LSP
/// `textDocument/inlayHint` contract).
///
/// Regular function (not a Salsa query). Internally calls Salsa-cached
/// queries (`function_body`, `function_body_source_map`,
/// `infer_scope_types`, `file_item_tree`, `file_semantic_index`).
pub fn annotations(db: &dyn Db, file: SourceFile) -> Vec<InlineAnnotation> {
    let item_tree = baml_compiler2_hir::file_item_tree(db, file);
    let index = baml_compiler2_hir::file_semantic_index(db, file);

    let mut out: Vec<InlineAnnotation> = Vec::new();

    for (func_local_id, func_data) in &item_tree.functions {
        // Process user-written functions and methods, plus the synthesized
        // `$init_test*` registration functions (so test/testset bodies — which
        // lower to lambdas — get hints). Skip LLM declarative functions: we must
        // never surface their synthetic `client` / `function_name` / `args`
        // calls, and since we don't recurse into skipped functions, their
        // internals stay hidden.
        let is_user = func_data.origin == FunctionOrigin::UserDefined;
        let is_test_init = func_data.name.as_str().starts_with("$init_test");
        if (!is_user && !is_test_init)
            || matches!(func_data.declarative_meta, Some(DeclarativeMeta::Llm(_)))
        {
            continue;
        }

        let func_loc = FunctionLoc::new(db, file, *func_local_id);

        let body = baml_compiler2_hir::body::function_body(db, func_loc);
        let FunctionBody::Expr(expr_body) = body.as_ref() else {
            continue;
        };
        let Some(source_map) = baml_compiler2_hir::body::function_body_source_map(db, func_loc)
        else {
            continue;
        };

        process_body(db, file, &index, expr_body, &source_map, &mut out);
    }

    // Sort by offset to ensure document order (required by LSP).
    out.sort_by_key(|h| h.offset);
    out
}

/// Emit hints for a single expression body — `let`-binding type hints and call
/// parameter-name hints — then recurse into any lambda bodies it contains
/// (each lambda has its own `ExprBody` arena and source map, e.g. the body of a
/// `test` block lowered to a lambda passed to `register_test`).
fn process_body(
    db: &dyn Db,
    file: SourceFile,
    index: &SemanticIndex<'_>,
    body: &ExprBody,
    source_map: &AstSourceMap,
    out: &mut Vec<InlineAnnotation>,
) {
    // ── Type hints for let bindings without annotations ───────────────────────
    for (_stmt_id, stmt) in body.stmts.iter() {
        let Stmt::Let { pattern, .. } = stmt else {
            continue;
        };

        // `let x: T` (Bind with sub-pattern) or a bare type pattern already
        // carries an explicit annotation — skip.
        let pat = &body.patterns[*pattern];
        if matches!(
            pat,
            baml_compiler2_ast::Pattern::Bind { subpat: Some(_), .. }
                | baml_compiler2_ast::Pattern::Type(_)
        ) {
            continue;
        }

        // Skip `_` / non-simple bindings.
        if pat.binding_name(&body.patterns).is_none() {
            continue;
        }

        let pat_span = source_map.pattern_span(*pattern);
        if pat_span.is_empty() {
            continue;
        }

        // Resolve the binding's type across all scopes (the binding may live in
        // a nested block / lambda scope, not the body's own scope).
        let mut ty_str: Option<String> = None;
        for scope_id in &index.scope_ids {
            let inference = infer_scope_types(db, *scope_id);
            if let Some(ty) = inference.binding_type(*pattern) {
                if !should_suppress_type(ty) {
                    ty_str = Some(utils::display_ty_for_file(db, file, ty));
                }
                break;
            }
        }
        let Some(ty_str) = ty_str else {
            continue;
        };

        out.push(InlineAnnotation {
            offset: pat_span.end(),
            label: format!(": {ty_str}"),
            kind: AnnotationKind::Type,
            padding_left: false,
            padding_right: true,
        });
    }

    // ── Parameter-name hints on calls + recurse into lambdas ──────────────────
    for (_expr_id, expr) in body.exprs.iter() {
        match expr {
            Expr::Call { callee, args, .. } => {
                // Skip synthesized test/testset registration calls — their
                // `name` / `body` / `collector` / `runner` arguments are codegen,
                // not user-facing. We still recurse into their lambda arguments
                // (the actual test bodies) via the `Expr::Lambda` arm below.
                if is_synthetic_registration(body, *callee) {
                    continue;
                }
                // Find a scope where the callee resolves to a function type.
                // ExprIds are arena-local (per body), so a foreign scope may
                // hold the same numeric id; on an arity mismatch keep searching
                // rather than giving up, so the correct same-arena scope wins.
                for scope_id in &index.scope_ids {
                    let inference = infer_scope_types(db, *scope_id);
                    let Some(Ty::Function { params, .. }) = inference.expression_type(*callee)
                    else {
                        continue;
                    };
                    if args.len() != params.len() {
                        continue;
                    }
                    for (arg, param) in args.iter().zip(params.iter()) {
                        if arg.label.is_some() {
                            continue;
                        }
                        let Some(name) = &param.name else {
                            continue;
                        };
                        let name_str = name.as_str();
                        // `self` is implicit.
                        if name_str == "self" {
                            continue;
                        }
                        let arg_span = source_map.expr_span(arg.expr);
                        if arg_span.is_empty() {
                            continue;
                        }
                        out.push(InlineAnnotation {
                            offset: arg_span.start(),
                            label: format!("{name_str}: "),
                            kind: AnnotationKind::Parameter,
                            padding_left: false,
                            padding_right: false,
                        });
                    }
                    break;
                }
            }
            // Lambdas (including desugared `test` / `testset` bodies) carry their
            // own body + source map — recurse so their lets and calls get hints.
            Expr::Lambda(func_def) => {
                if let Some(FunctionBodyDef::Expr(lbody, lsmap)) = &func_def.body {
                    process_body(db, file, index, lbody, lsmap, out);
                }
            }
            _ => {}
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Returns `true` for types that would produce noisy or unhelpful hints.
///
/// We suppress:
/// - `Ty::Error` — type-check error, nothing useful to show
/// - `Ty::BuiltinUnknown` / `Ty::Unknown` — no useful info
/// - `Ty::Never` — unreachable / error types
fn should_suppress_type(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Error { .. } | Ty::BuiltinUnknown { .. } | Ty::Unknown { .. } | Ty::Never { .. }
    )
}

/// True if `callee` names a synthesized test/testset registration method
/// (`register_test` / `register_test_set`). These calls are emitted by
/// `$init_test` desugaring; their `name` / `body` / `collector` / `runner`
/// arguments are codegen and shouldn't get parameter-name hints.
fn is_synthetic_registration(body: &ExprBody, callee: ExprId) -> bool {
    let name = match &body.exprs[callee] {
        Expr::MemberAccess { member, .. } => member.as_str(),
        Expr::Path(segments) => match segments.last() {
            Some(n) => n.as_str(),
            None => return false,
        },
        _ => return false,
    };
    matches!(name, "register_test" | "register_test_set")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::ProjectTest;

    #[test]
    fn annotations_skip_declarative_llm_synthetic_call_hints() {
        let mut builder = ProjectTest::builder();
        builder.source(
            "main.baml",
            r##"
function Summarize(input: string) -> string {
    client GPT4
    prompt #"Summarize {{ input }}"#
}

function Echo(x: string) -> string {
    x
}

function UseEcho() -> string {
    Echo("hi")
}
"##,
        );
        let project = builder.build();

        let hints = annotations(&project.db, project.files[0]);
        let labels: Vec<_> = hints.iter().map(|hint| hint.label.as_str()).collect();

        assert!(
            labels.contains(&"x: "),
            "expected regular function call parameter hints to remain, got {labels:?}"
        );
        assert!(
            labels
                .iter()
                .all(|label| !matches!(*label, "client: " | "function_name: " | "args: ")),
            "LLM synthetic call hints should be suppressed, got {labels:?}"
        );
    }

    #[test]
    fn annotations_inside_test_bodies() {
        let mut builder = ProjectTest::builder();
        builder.source(
            "main.baml",
            r##"
function greet(name: string) -> string {
    name
}

test "greets" {
    let g = greet("x")
    assert.equal(g, "x")
}
"##,
        );
        let project = builder.build();

        let hints = annotations(&project.db, project.files[0]);
        let labels: Vec<_> = hints.iter().map(|hint| hint.label.as_str()).collect();

        assert!(
            labels.contains(&"name: "),
            "expected a parameter hint inside the test body, got {labels:?}"
        );
        assert!(
            labels.iter().any(|label| label.starts_with(": ")),
            "expected a let type hint inside the test body, got {labels:?}"
        );
    }

    #[test]
    fn annotations_inside_methods() {
        let mut builder = ProjectTest::builder();
        builder.source(
            "main.baml",
            r##"
function greet(name: string) -> string {
    name
}

class Greeter {
    prefix: string,

    function run(self, name: string) -> string {
        let g = greet(name)
        g
    }
}
"##,
        );
        let project = builder.build();

        let hints = annotations(&project.db, project.files[0]);
        let labels: Vec<_> = hints.iter().map(|hint| hint.label.as_str()).collect();

        assert!(
            labels.contains(&"name: "),
            "expected a parameter hint inside the method body, got {labels:?}"
        );
        assert!(
            labels.iter().any(|label| label.starts_with(": ")),
            "expected a let type hint inside the method body, got {labels:?}"
        );
    }

    #[test]
    fn annotations_suppress_test_registration_hints() {
        let mut builder = ProjectTest::builder();
        builder.source(
            "main.baml",
            r##"
testset "math" {
    test "adds" {
        assert.equal(1 + 1, 2)
    }
    test "subtracts" {
        assert.equal(2 - 1, 1)
    }
}
"##,
        );
        let project = builder.build();

        let hints = annotations(&project.db, project.files[0]);
        let labels: Vec<_> = hints.iter().map(|hint| hint.label.as_str()).collect();

        assert!(
            labels
                .iter()
                .all(|label| !matches!(*label, "name: " | "body: " | "collector: " | "runner: ")),
            "synthesized test/testset registration hints should be suppressed, got {labels:?}"
        );
    }
}
