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
//! - Constructor expressions and bindings named like their inferred type
//!
//! We suppress parameter-name hints when:
//! - The callee type is not `Ty::Function` (no param info)
//! - The param name is `None` (positional-only parameter)
//! - The argument count != param count (variadic / error cases)
//! - Arguments already named like the parameter, and `assert.equal` calls
//!
//! LLM declarative functions are skipped entirely (and never recursed into), so
//! their synthetic `client` / `function_name` / `args` calls produce no hints.

use baml_base::SourceFile;
use baml_compiler2_ast::{
    Expr, ExprId, Stmt,
    ast::{AstSourceMap, ExprBody, FunctionOrigin},
};
use baml_compiler2_hir::{
    body::FunctionBody, contributions::Definition, item_data, package::package_items,
    scope::FileScopeId,
};
use baml_compiler2_hir_ty::ide::infer_for_scope;
use baml_type::Ty;
use text_size::{TextRange, TextSize};

use crate::{listing::ResolvedTarget, render::display_ty_for_file, resolve::SymbolTarget};

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
    /// Declaration to open when the editor clicks the label, if one exists.
    pub target: Option<(SourceFile, TextRange)>,
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
    db: &dyn baml_compiler2_hir::Db,
    file: SourceFile,
) -> Vec<InlineAnnotation> {
    let index = baml_compiler2_hir::file_semantic_index(db, file);

    let mut out: Vec<InlineAnnotation> = Vec::new();

    for &func_loc in baml_compiler2_hir::item_data::file_functions(db, file) {
        let func_data = baml_compiler2_hir::item_data::function_data(db, func_loc);

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
        if baml_compiler2_hir::item_data::function_llm_meta(db, func_loc).is_some() {
            continue;
        }

        let body = baml_compiler2_hir::body::function_body(db, func_loc);
        let FunctionBody::Expr(expr_body) = body.as_ref() else {
            continue;
        };
        let Some(source_map) = baml_compiler2_hir::body::function_body_source_map(db, func_loc)
        else {
            continue;
        };

        let owner_scope = baml_compiler2_hir::item_data::function_scope(db, func_loc)
            .map(|scope| scope.file_scope_id(db))
            .unwrap_or_else(|| {
                let func_span =
                    baml_compiler2_hir::item_data::function_source_map(db, func_loc).span;
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
    db: &dyn baml_compiler2_hir::Db,
    file: SourceFile,
    index: &SemanticIndex<'_>,
    owner_scope: FileScopeId,
    body: &ExprBody,
    source_map: &AstSourceMap,
    out: &mut Vec<InlineAnnotation>,
) {
    // ── Type hints for let bindings without annotations ───────────────────────
    for (stmt_id, stmt) in body.stmts.iter() {
        let Stmt::Let {
            pattern,
            initializer,
            ..
        } = stmt
        else {
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
        let Some(binding_name) = pat.binding_name(&body.patterns) else {
            continue;
        };

        // The constructor already spells the type immediately after `=`.
        if initializer.is_some_and(|id| matches!(body.exprs[id], Expr::Object { .. })) {
            continue;
        }

        let pat_span = source_map.pattern_span(*pattern);
        if pat_span.is_empty() {
            continue;
        }

        // Resolve through the binding's real source scope chain. PatIds are
        // arena-local, so scanning every file scope can hit a foreign body that
        // happens to reuse the same numeric id.
        let mut inferred_ty: Option<Ty> = None;
        let use_scope = scope_at_offset_within_body(index, pat_span.start(), owner_scope);
        for file_scope_id in ancestor_scopes_within_body(index, use_scope, owner_scope) {
            let scope_id = index.scope_ids[file_scope_id.index() as usize];
            let Some(inference) = infer_for_scope(db, scope_id) else {
                continue;
            };
            if let Some(ty) = inference.type_of_pat.get(pattern) {
                let ty = ty.clone();
                if !should_suppress_type(&ty) {
                    inferred_ty = Some(ty);
                }
                break;
            }
        }
        let Some(ty) = inferred_ty else {
            continue;
        };
        if type_repeats_binding_name(binding_name.as_str(), &ty) {
            continue;
        }
        let ty_str = display_ty_for_file(db, file, &ty);

        out.push(InlineAnnotation {
            offset: pat_span.end(),
            label: format!(": {ty_str}"),
            kind: AnnotationKind::Type,
            padding_left: false,
            padding_right: true,
            target: type_definition(db, &ty),
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
            if is_assert_equal(body, *callee) {
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
                let Some(callee_ty) = inference.type_of_expr.get(callee).cloned() else {
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
                    if argument_repeats_parameter(body, arg.expr, name_str) {
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
                        target: parameter_definition(db, file, callee_span, name_str),
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
    baml_type::contains_error_recovery(ty) || matches!(ty, Ty::Unknown | Ty::Never)
}

/// Compare the words a reader sees, including `files` beside `File[]` and
/// `analysis_dir` beside `analysisDir`. Only a trailing plural `s` is folded;
/// broader stemming would hide useful hints for unrelated names.
fn same_name(left: &str, right: &str) -> bool {
    let normalize = |name: &str| {
        let lower = name
            .chars()
            .filter(|c| *c != '_')
            .flat_map(char::to_lowercase)
            .collect::<String>();
        lower.strip_suffix('s').unwrap_or(&lower).to_string()
    };
    normalize(left) == normalize(right)
}

fn type_repeats_binding_name(binding: &str, ty: &Ty) -> bool {
    match ty {
        Ty::List(inner) => type_repeats_binding_name(binding, inner),
        Ty::Union(members) => {
            let mut non_null = members.iter().filter(|ty| !matches!(ty, Ty::Null));
            non_null
                .next()
                .is_some_and(|ty| type_repeats_binding_name(binding, ty))
                && non_null.next().is_none()
        }
        Ty::Class(name, _) | Ty::Enum(name) | Ty::Interface(name, _, _) | Ty::TypeAlias(name) => {
            same_name(binding, name.name().as_str())
        }
        _ => false,
    }
}

fn type_definition(db: &dyn baml_compiler2_hir::Db, ty: &Ty) -> Option<(SourceFile, TextRange)> {
    let builtin_alias = match ty {
        Ty::Int => Some("int"),
        Ty::Bigint => Some("bigint"),
        Ty::Float => Some("float"),
        Ty::String => Some("string"),
        Ty::Bool => Some("bool"),
        Ty::Uint8Array => Some("uint8array"),
        _ => None,
    };
    if let Some(alias) = builtin_alias {
        let ResolvedTarget::Item(def) = crate::listing::resolve_builtin_type_target(db, alias)?
        else {
            return None;
        };
        return crate::syntax::definition_span(db, def);
    }

    let name = match ty {
        Ty::List(inner) => return type_definition(db, inner),
        Ty::Union(members) => {
            let mut non_null = members.iter().filter(|ty| !matches!(ty, Ty::Null));
            let inner = non_null.next()?;
            return non_null
                .next()
                .is_none()
                .then(|| type_definition(db, inner))
                .flatten();
        }
        Ty::Class(name, _) | Ty::Enum(name) | Ty::Interface(name, _, _) | Ty::TypeAlias(name) => {
            name
        }
        _ => return None,
    };
    let def = package_items(db, name.root()).lookup_type(name.namespace(), name.name())?;
    crate::syntax::definition_span(db, def)
}

fn argument_repeats_parameter(body: &ExprBody, arg: ExprId, parameter: &str) -> bool {
    match &body.exprs[arg] {
        Expr::Path(parts) => parts
            .last()
            .is_some_and(|name| same_name(name.as_str(), parameter)),
        Expr::MemberAccess { member, .. } | Expr::OptionalMemberAccess { member, .. } => {
            same_name(member.as_str(), parameter)
        }
        _ => false,
    }
}

fn is_assert_equal(body: &ExprBody, callee: ExprId) -> bool {
    match &body.exprs[callee] {
        Expr::Path(parts) => {
            parts.len() >= 2
                && parts[parts.len() - 2].as_str() == "assert"
                && parts[parts.len() - 1].as_str() == "equal"
        }
        Expr::MemberAccess { base, member } if member.as_str() == "equal" => {
            matches!(&body.exprs[*base], Expr::Path(parts) if parts.last().is_some_and(|name| name.as_str() == "assert"))
        }
        _ => false,
    }
}

fn parameter_definition(
    db: &dyn baml_compiler2_hir::Db,
    file: SourceFile,
    callee_span: TextRange,
    parameter: &str,
) -> Option<(SourceFile, TextRange)> {
    let offset = callee_span.end() - TextSize::from(1);
    let function = match crate::resolve::symbol_at(db, file, offset)? {
        SymbolTarget::Item(Definition::Function(function))
        | SymbolTarget::Method { func: function } => function,
        _ => return None,
    };
    let index = item_data::function_data(db, function)
        .params
        .iter()
        .position(|param| param.name.as_str() == parameter)?;
    let span = baml_compiler2_hir::signature::function_signature_source_map(db, function)
        .param_name_spans
        .get(index)
        .copied()?;
    Some((function.file(db), span))
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
        let g = greet("hello")
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

    #[test]
    fn redundant_hints_are_suppressed_and_useful_hints_link_to_declarations() {
        let source = r#"
class File { name string }
class Conversation { text string }

function load_files() -> File[] { [File { name: "a" }] }
function load_file() -> File { File { name: "a" } }
function start_run(analysis_dir: string, count: int) -> int { count }

function demo(analysis_dir: string) -> int {
    let retained = Conversation { text: "hi" }
    let files = load_files()
    let result = load_file()
    assert.equal(1, 1)
    start_run(analysis_dir, 2)
}
"#;
        let mut builder = ProjectTest::builder();
        builder.source("main.baml", source);
        let project = builder.build();
        let hints = file_annotations(&project.db, project.files[0]);
        let type_hints = hints
            .iter()
            .filter(|hint| hint.kind == AnnotationKind::Type)
            .collect::<Vec<_>>();
        assert_eq!(type_hints.len(), 1, "unexpected type hints: {type_hints:?}");
        assert_eq!(type_hints[0].label, ": File");
        let (file, range) = type_hints[0].target.expect("class type is clickable");
        assert_eq!(file, project.files[0]);
        assert_eq!(
            &source[usize::from(range.start())..usize::from(range.end())],
            "File"
        );

        let parameter_hints = hints
            .iter()
            .filter(|hint| hint.kind == AnnotationKind::Parameter)
            .collect::<Vec<_>>();
        assert_eq!(
            parameter_hints.len(),
            1,
            "unexpected parameter hints: {parameter_hints:?}"
        );
        assert_eq!(parameter_hints[0].label, "count: ");
        let (file, range) = parameter_hints[0].target.expect("parameter is clickable");
        assert_eq!(file, project.files[0]);
        assert_eq!(
            &source[usize::from(range.start())..usize::from(range.end())],
            "count"
        );
    }
}
