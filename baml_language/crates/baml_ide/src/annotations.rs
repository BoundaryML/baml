//! Inline type / parameter-name annotations for BAML files (inlay hints).
//!
//! Provides `file_annotations(db, file) -> &Vec<InlineAnnotation>` — a Salsa
//! tracked query that walks expression-body functions in a file
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
//! Types are resolved through the source span's ancestor scopes so a
//! binding/expression living in a nested block or lambda resolves without
//! accidentally matching an arena-local id from a different body.
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
    ast::{AstSourceMap, ExprBody, FunctionOrigin},
};
use baml_compiler2_hir::{body::FunctionBody, scope::FileScopeId};
use baml_compiler2_hir_ty::ide::infer_for_scope;
use baml_type::Ty;
use text_size::TextSize;

use crate::render::display_ty_for_file;

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
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
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
/// Salsa tracked query: walks every function body against type
/// inference (measured 40–150ms on real projects), which is too slow to
/// recompute per request while the file is unchanged. Editors re-request
/// inlay hints on every scroll, so this is the hottest read path.
///
/// Named `file_annotations` (like `file_outline`) because the tracked-query
/// machinery claims the bare name in the type namespace, which would collide
/// with this module.
#[salsa::tracked(returns(ref))]
pub fn file_annotations(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
) -> Vec<InlineAnnotation> {
    let index = baml_compiler2_ppir::file_semantic_index(db, file);

    let mut out: Vec<InlineAnnotation> = Vec::new();

    for &func_loc in baml_compiler2_ppir::item_data::file_functions(db, file) {
        let func_data = baml_compiler2_ppir::item_data::function_data(db, func_loc);

        // Process user-written functions and methods, plus the compiler's own
        // test-registration functions: `Internal` is the origin the lowering
        // stamps on the synthesized `$init_test*` functions, whose lambda
        // arguments carry the user-authored `test` / `testset` bodies — recurse
        // so those get hints. Synthesized companions and auto-derived methods
        // have no user-authored bodies. Skip LLM declarative functions: we must
        // never surface their synthetic `client` / `function_name` / `args`
        // calls, and since we don't recurse into skipped functions, their
        // internals stay hidden.
        match func_data.metadata.origin {
            FunctionOrigin::UserDefined | FunctionOrigin::Internal => {}
            FunctionOrigin::Companion | FunctionOrigin::AutoDerive => continue,
        }
        if baml_compiler2_ppir::item_data::function_llm_meta(db, func_loc).is_some() {
            continue;
        }

        let body = baml_compiler2_ppir::function_body(db, func_loc);
        let FunctionBody::Expr(expr_body) = body.as_ref() else {
            continue;
        };
        let Some(source_map) = baml_compiler2_ppir::function_body_source_map(db, func_loc) else {
            continue;
        };

        let owner_scope = baml_compiler2_ppir::item_data::function_scope(db, func_loc)
            .map(|scope| scope.file_scope_id(db))
            .unwrap_or_else(|| {
                let func_span =
                    baml_compiler2_ppir::item_data::function_source_map(db, func_loc).span;
                index.scope_at_offset(func_span.start(), Some(&func_data.name))
            });
        process_body(
            db,
            file,
            index,
            owner_scope,
            expr_body,
            &source_map,
            &mut out,
        );
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
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    index: &SemanticIndex<'_>,
    owner_scope: FileScopeId,
    body: &ExprBody,
    source_map: &AstSourceMap,
    out: &mut Vec<InlineAnnotation>,
) {
    // ── Type hints for let bindings without annotations ───────────────────────
    for (stmt_id, stmt) in body.stmts.iter() {
        let Stmt::Let { pattern, .. } = stmt else {
            continue;
        };

        // Skip compiler-synthesized bindings — e.g. the accumulator a `${…}`
        // interpolation lowers to (`let " __m3_concat" = ""`). Their spans point
        // inside the backtick template, so a `: T` hint there is noise the user
        // never wrote. Marked at lowering time (see `AstSourceMap::synthetic_stmts`).
        if source_map.is_synthetic_stmt(stmt_id) {
            continue;
        }

        // `let x: T` (Bind with sub-pattern) or a bare type pattern already
        // carries an explicit annotation — skip.
        let pat = &body.patterns[*pattern];
        if matches!(
            pat,
            baml_compiler2_ast::Pattern::Bind {
                subpat: Some(_),
                ..
            } | baml_compiler2_ast::Pattern::Type(_)
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

        // Resolve through the binding's real source scope chain. PatIds are
        // arena-local, so scanning every file scope can hit a foreign body that
        // happens to reuse the same numeric id.
        let mut ty_str: Option<String> = None;
        let use_scope = scope_at_offset_within_body(index, pat_span.start(), owner_scope);
        for file_scope_id in ancestor_scopes_within_body(index, use_scope, owner_scope) {
            let scope_id = index.scope_ids[file_scope_id.index() as usize];
            let Some(inference) = infer_for_scope(db, scope_id) else {
                continue;
            };
            if let Some(ty) = inference.type_of_pat.get(pattern) {
                let ty = ty.to_plain();
                if !should_suppress_type(&ty) {
                    ty_str = Some(display_ty_for_file(db, file, &ty));
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

    // ── Parameter-name hints on calls ─────────────────────────────────────────
    // Lambda bodies share this arena, so this one pass covers them too.
    for (expr_id, expr) in body.exprs.iter() {
        if let Expr::Call { callee, args, .. } = expr {
            // Skip synthesized test/testset registration calls — their
            // `name` / `body` / `collector` / `runner` arguments are codegen,
            // not user-facing. We still recurse into their lambda arguments
            // (the actual test bodies), which live in the same arena.
            if is_synthetic_registration(body, *callee) {
                continue;
            }
            // Skip compiler-synthesized wrapping calls — e.g. the
            // `string.from(${expr})` that `${…}` interpolation lowers to.
            // Marked at lowering time (see `AstSourceMap::synthetic_exprs`),
            // so without this every interpolation would get a spurious
            // `value:` parameter hint.
            if source_map.is_synthetic_expr(expr_id) {
                continue;
            }
            let callee_span = source_map.expr_span(*callee);
            if callee_span.is_empty() {
                continue;
            }

            // Find a scope where the callee resolves to a function type.
            // ExprIds are arena-local (per body), so restrict lookup to the
            // callee's source scope chain instead of scanning every scope in
            // the file for the first matching numeric id.
            let use_scope = scope_at_offset_within_body(index, callee_span.start(), owner_scope);
            for file_scope_id in ancestor_scopes_within_body(index, use_scope, owner_scope) {
                let scope_id = index.scope_ids[file_scope_id.index() as usize];
                let Some(inference) = infer_for_scope(db, scope_id) else {
                    continue;
                };
                let Some(callee_ty) = inference
                    .type_of_expr
                    .get(callee)
                    .map(baml_type::interned::Ty::to_plain)
                else {
                    continue;
                };
                let Ty::Function { ref params, .. } = callee_ty else {
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
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Returns `true` for types that would produce noisy or unhelpful hints.
///
/// We suppress:
/// - `Ty::Error` — type-check error, nothing useful to show
/// - `Ty::Unknown` — no useful info
/// - `Ty::Never` — unreachable / error types
fn should_suppress_type(ty: &Ty) -> bool {
    baml_type_runtime::contains_error_recovery(ty)
        || matches!(ty, Ty::Unknown { .. } | Ty::Never { .. })
}

/// True if `callee` names a test/testset registration method
/// (`register_test` / `register_test_set`, or the `*_at` forms the
/// synthesized `$init_test` body calls — the same name set the lowering's own
/// `count_register_calls` classifier matches). These calls are emitted by
/// test/testset desugaring; their `name` / `body` / `collector` / `runner`
/// arguments are codegen and shouldn't get parameter-name hints.
///
/// This is a name heuristic: the desugaring allocates these calls without
/// marking them in `AstSourceMap::synthetic_exprs`, so there is no
/// source-map/firewall signal to key on. A user-authored call to a function
/// with one of these names is also suppressed — remove this once the lowering
/// marks registration calls synthetic.
fn is_synthetic_registration(body: &ExprBody, callee: ExprId) -> bool {
    let name = match &body.exprs[callee] {
        Expr::MemberAccess { member, .. } => member.as_str(),
        Expr::Path(segments) => match segments.last() {
            Some(n) => n.as_str(),
            None => return false,
        },
        _ => return false,
    };
    matches!(
        name,
        "register_test" | "register_test_set" | "register_test_at" | "register_test_set_at"
    )
}

fn scope_at_offset_within_body(
    index: &SemanticIndex<'_>,
    offset: TextSize,
    owner_scope: FileScopeId,
) -> FileScopeId {
    let scope_id = index.scope_at_offset(offset, None);
    if scope_is_descendant_or_self(index, scope_id, owner_scope) {
        scope_id
    } else {
        owner_scope
    }
}

fn ancestor_scopes_within_body(
    index: &SemanticIndex<'_>,
    start_scope: FileScopeId,
    owner_scope: FileScopeId,
) -> Vec<FileScopeId> {
    let mut scopes = Vec::new();
    let mut current = Some(start_scope);
    while let Some(scope_id) = current {
        scopes.push(scope_id);
        if scope_id == owner_scope {
            return scopes;
        }
        current = index.scopes[scope_id.index() as usize].parent;
    }
    vec![owner_scope]
}

fn scope_is_descendant_or_self(
    index: &SemanticIndex<'_>,
    scope_id: FileScopeId,
    owner_scope: FileScopeId,
) -> bool {
    scope_id == owner_scope
        || index.scopes[owner_scope.index() as usize]
            .descendants
            .contains(&scope_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ProjectTest;

    #[test]
    fn lambda_type_hints_use_later_constraints_and_hide_unresolved_types() {
        const SOURCE: &str = r#"
function resolved() -> int {
    let resolved_lambda = (b) -> { 1 }
    let constrained: (int) -> int throws never = resolved_lambda
    constrained(0)
}

function unresolved() -> int {
    let unresolved_lambda = (b) -> { 1 }
    0
}
"#;

        let mut builder = ProjectTest::builder();
        builder.source("main.baml", SOURCE);
        let project = builder.build();
        let hints = file_annotations(&project.db, project.files[0]);

        let binding_offset = |name: &str| {
            let start = SOURCE.find(name).expect("binding should exist");
            TextSize::try_from(start + name.len()).expect("binding offset should fit in TextSize")
        };
        let type_hints_at = |offset| {
            hints
                .iter()
                .filter(|hint| hint.kind == AnnotationKind::Type && hint.offset == offset)
                .map(|hint| hint.label.as_str())
                .collect::<Vec<_>>()
        };

        assert_eq!(
            type_hints_at(binding_offset("resolved_lambda")),
            vec![": (b: int) -> int throws never"]
        );
        assert!(type_hints_at(binding_offset("unresolved_lambda")).is_empty());
        assert!(hints.iter().all(|hint| !hint.label.contains("!error")));
    }

    #[test]
    fn annotations_skip_declarative_llm_synthetic_call_hints() {
        let mut builder = ProjectTest::builder();
        builder.source(
            "main.baml",
            r##"
function summarize(input: string) -> string {
    client: GPT4
    prompt: `summarize ${input}`
}

function echo(x: string) -> string {
    x
}

function use_echo() -> string {
    echo("hi")
}
"##,
        );
        let project = builder.build();

        let hints = file_annotations(&project.db, project.files[0]);
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
    fn annotations_skip_tagged_template_synthetic_hints() {
        // A custom `//baml:tagged_string` tag used in an expression function
        // desugars to a closure body full of compiler-generated nodes
        // (`__tt_parts`/`__tt_values`/`__tt_cur` accumulators and `.push(...)`
        // calls). None of them should produce inlay hints — only the user's
        // `let q` binding does. Marked synthetic at lowering (see
        // `AstSourceMap::synthetic_stmts`/`synthetic_exprs`), so this holds even
        // if those nodes later gain real spans / lose their type annotations.
        let mut builder = ProjectTest::builder();
        builder.source(
            "main.baml",
            r##"
//baml:tagged_string
function sql(body: (x: int) -> baml.TaggedString) -> string {
    "ok"
}

function demo(items: int[]) -> string {
    let q = sql`SELECT ${1} ${for (let x in items)}c_${x}, ${endfor}done`
    q
}
"##,
        );
        let project = builder.build();
        let hints = file_annotations(&project.db, project.files[0]);

        // The synthesized `.push(...)` calls must not surface `value:`-style hints.
        assert!(
            hints.iter().all(|h| h.kind != AnnotationKind::Parameter),
            "tagged-template desugaring must not emit parameter hints, got {:?}",
            hints.iter().map(|h| h.label.as_str()).collect::<Vec<_>>()
        );
        // The only type hint is the user's `let q: string`; the `__tt_*`
        // accumulators must not contribute their own.
        let type_hints: Vec<_> = hints
            .iter()
            .filter(|h| h.kind == AnnotationKind::Type)
            .map(|h| h.label.as_str())
            .collect();
        assert_eq!(
            type_hints,
            vec![": string"],
            "only the user's `let q` should get a type hint"
        );
    }

    #[test]
    fn annotations_skip_string_interpolation_synthetic_hints() {
        let mut builder = ProjectTest::builder();
        builder.source(
            "main.baml",
            r##"
function greet(name: string, items: int[]) -> string {
    let greeting = `Hi ${name}! you have ${items.length()} items`
    let counted = `count: ${ let n = items.length() }${n} done`
    greeting + counted
}
"##,
        );
        let project = builder.build();

        let hints = file_annotations(&project.db, project.files[0]);
        let labels: Vec<_> = hints.iter().map(|hint| hint.label.as_str()).collect();

        // `${expr}` lowers to `string.from(expr)`; that synthetic wrapper call
        // must not produce a `value:` parameter hint on every interpolation.
        assert!(
            !labels.contains(&"value: "),
            "synthesized string.from() interpolation calls should not get parameter hints, got {labels:?}"
        );
        // The concat-scope accumulator (`let " __m3_concat" = ""`, a
        // compiler-synthesized binding) must not produce a type hint either;
        // real `let` bindings (`greeting`, `counted`) still do, so the
        // suppression is targeted.
        assert!(
            labels.iter().any(|label| label.starts_with(": ")),
            "real let bindings should still get type hints, got {labels:?}"
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

        let hints = file_annotations(&project.db, project.files[0]);
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

        let hints = file_annotations(&project.db, project.files[0]);
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

        let hints = file_annotations(&project.db, project.files[0]);
        let labels: Vec<_> = hints.iter().map(|hint| hint.label.as_str()).collect();

        assert!(
            labels
                .iter()
                .all(|label| !matches!(*label, "name: " | "body: " | "collector: " | "runner: ")),
            "synthesized test/testset registration hints should be suppressed, got {labels:?}"
        );
    }

    #[test]
    fn call_parameter_hints_use_the_calls_own_scope() {
        let mut builder = ProjectTest::builder();
        let source = r##"
function left(alpha: string) -> string {
    alpha
}

function right(beta: string) -> string {
    beta
}

function earlier() -> string {
    left("x")
}

function later() -> string {
    right("y")
}
"##;
        builder.source("main.baml", source);
        let project = builder.build();

        let hints = file_annotations(&project.db, project.files[0]);
        let y_offset = TextSize::from(
            u32::try_from(source.find("\"y\"").expect("test arg")).expect("offset fits"),
        );
        let labels_at_y: Vec<_> = hints
            .iter()
            .filter(|hint| hint.offset == y_offset)
            .map(|hint| hint.label.as_str())
            .collect();

        assert_eq!(
            labels_at_y,
            vec!["beta: "],
            "expected later's call to use right's parameter, got {labels_at_y:?}"
        );
    }
}
