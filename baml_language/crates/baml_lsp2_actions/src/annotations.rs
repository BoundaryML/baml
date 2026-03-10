//! Inline type / parameter-name annotations for BAML files (inlay hints).
//!
//! Provides `annotations(db, file) -> Vec<InlineAnnotation>` — a regular
//! function (not a Salsa query) that walks all expression-body functions in a
//! file and produces two kinds of hints:
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

use baml_base::SourceFile;
use baml_compiler2_ast::{Expr, Pattern, Stmt};
use baml_compiler2_hir::{body::FunctionBody, loc::FunctionLoc, scope::ScopeKind};
use baml_compiler2_tir::ty::Ty;
use text_size::TextSize;

use crate::{Db, utils};

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
    let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
    let index = baml_compiler2_ppir::file_semantic_index(db, file);

    let mut out: Vec<InlineAnnotation> = Vec::new();

    for (func_local_id, func_data) in &item_tree.functions {
        let func_loc = FunctionLoc::new(db, file, *func_local_id);

        // Only expression-body functions have type information we can display.
        let body = baml_compiler2_hir::body::function_body(db, func_loc);
        let FunctionBody::Expr(expr_body) = body.as_ref() else {
            continue;
        };

        // Source map gives ExprId/StmtId/PatId → TextRange.
        let Some(source_map) = baml_compiler2_hir::body::function_body_source_map(db, func_loc)
        else {
            continue;
        };

        // Find the function's scope in the semantic index.
        let func_scope_file_id = index
            .scopes
            .iter()
            .enumerate()
            .find(|(_, s)| s.kind == ScopeKind::Function && s.range == func_data.span)
            .map(|(i, _)| {
                #[allow(clippy::cast_possible_truncation)]
                baml_compiler2_hir::scope::FileScopeId::new(i as u32)
            });

        let Some(func_scope_file_id) = func_scope_file_id else {
            continue;
        };

        let func_scope_id = index.scope_ids[func_scope_file_id.index() as usize];
        let inference = baml_compiler2_tir::inference::infer_scope_types(db, func_scope_id);

        // ── Type hints for let bindings without annotations ───────────────────

        for (stmt_id, stmt) in expr_body.stmts.iter() {
            let Stmt::Let {
                pattern,
                type_annotation,
                ..
            } = stmt
            else {
                continue;
            };

            // Skip if the user already wrote a type annotation.
            if type_annotation.is_some() {
                continue;
            }

            // Get the binding name to suppress hints for `_`.
            let pat = &expr_body.patterns[*pattern];
            let binding_name = match pat {
                Pattern::Binding(name) => name.as_str(),
                Pattern::TypedBinding { name, .. } => name.as_str(),
                _ => continue, // Not a simple binding — skip
            };

            if binding_name == "_" {
                continue;
            }

            // Look up the inferred type.
            let ty = match inference.binding_type(*pattern) {
                Some(ty) => ty,
                None => {
                    // Try other scopes for nested blocks.
                    let ty_str = find_binding_ty_any_scope(db, &index, *pattern);
                    if let Some(ty_str) = ty_str {
                        // Emit hint: position at end of pattern span.
                        let pat_span = source_map.pattern_span(*pattern);
                        if !pat_span.is_empty() {
                            out.push(InlineAnnotation {
                                offset: pat_span.end(),
                                label: format!(": {ty_str}"),
                                kind: AnnotationKind::Type,
                                padding_left: false,
                                padding_right: true,
                            });
                        }
                    }
                    continue;
                }
            };

            // Suppress noisy / unhelpful types.
            if should_suppress_type(ty) {
                continue;
            }

            let ty_str = utils::display_ty(ty);

            // Position the hint at the end of the pattern span (after the var name).
            let pat_span = source_map.pattern_span(*pattern);
            if pat_span.is_empty() {
                // Fall back to using the statement span start if the pattern span is unknown.
                let stmt_span = source_map.stmt_span(stmt_id);
                if stmt_span.is_empty() {
                    continue;
                }
                // Emit after the identifier — we don't know the exact position, skip.
                let _ = stmt_span;
                continue;
            }

            out.push(InlineAnnotation {
                offset: pat_span.end(),
                label: format!(": {ty_str}"),
                kind: AnnotationKind::Type,
                padding_left: false,
                padding_right: true,
            });
        }

        // ── Parameter-name hints on call expressions ──────────────────────────

        for (_expr_id, expr) in expr_body.exprs.iter() {
            let Expr::Call { callee, args } = expr else {
                continue;
            };

            // Get the callee's type.
            let Some(callee_ty) = inference.expression_type(*callee) else {
                continue;
            };

            // Only process `Ty::Function` where params have names.
            let Ty::Function { params, .. } = callee_ty else {
                continue;
            };

            // Skip if arg count doesn't match param count (variadic / error cases).
            if args.len() != params.len() {
                continue;
            }

            for (arg_expr_id, (param_name, _param_ty)) in args.iter().zip(params.iter()) {
                // Only emit hints for named parameters.
                let Some(name) = param_name else {
                    continue;
                };

                let name_str = name.as_str();

                // Skip `self` parameter hints — they're implicit.
                if name_str == "self" {
                    continue;
                }

                // Position hint at the start of the argument's span.
                let arg_span = source_map.expr_span(*arg_expr_id);
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
        }
    }

    // Sort by offset to ensure document order (required by LSP).
    out.sort_by_key(|h| h.offset);
    out
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Returns `true` for types that would produce noisy or unhelpful hints.
///
/// We suppress:
/// - `Ty::Error` — type-check error, nothing useful to show
/// - `Ty::BuiltinUnknown` / `Ty::Unknown` — no useful info
/// - `Ty::Never` — unreachable / error types
fn should_suppress_type(ty: &Ty) -> bool {
    matches!(ty, Ty::Error | Ty::BuiltinUnknown | Ty::Unknown | Ty::Never)
}

/// Search all scopes in the file for the binding type of `pat_id`.
///
/// Used as a fallback when the let binding is in a nested block scope
/// (not directly in the enclosing function scope). Returns the display
/// string directly to avoid allocating a `Ty`.
fn find_binding_ty_any_scope(
    db: &dyn Db,
    index: &baml_compiler2_hir::semantic_index::FileSemanticIndex<'_>,
    pat_id: baml_compiler2_ast::PatId,
) -> Option<String> {
    for scope_id in &index.scope_ids {
        let inference = baml_compiler2_tir::inference::infer_scope_types(db, *scope_id);
        if let Some(ty) = inference.binding_type(pat_id) {
            if should_suppress_type(ty) {
                return None;
            }
            return Some(utils::display_ty(ty));
        }
    }
    None
}
