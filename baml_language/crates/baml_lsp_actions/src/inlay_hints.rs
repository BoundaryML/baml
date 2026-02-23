//! Inlay hints for BAML files.

use std::sync::Arc;

use baml_db::{
    SourceFile,
    baml_compiler_hir::{
        ExprBody, FunctionBody, HirSourceMap, ItemId, Stmt, SymbolTable, file_items, function_body,
        function_signature, symbol_table,
    },
    baml_compiler_tir::{self, InferenceResult, ResolvedValue},
    baml_workspace::Project,
};
use baml_project::ProjectDatabase;
use text_size::TextSize;

/// An inlay hint to display inline in the editor.
pub struct InlayHint {
    /// Byte offset where the hint is displayed.
    pub offset: TextSize,
    /// Text shown, e.g. `"name:"` or `": string"`.
    pub label: String,
    /// Insert a thin space between the hint and the token to its left.
    pub padding_left: bool,
    /// Insert a thin space between the hint and the token to its right.
    pub padding_right: bool,
}

/// Shared data passed to every [`HintCollector`] for a single function body.
pub struct HintContext<'a> {
    pub body: &'a ExprBody,
    pub inference: &'a Arc<InferenceResult>,
    pub source_map: &'a HirSourceMap,
    pub sym_table: &'a SymbolTable<'a>,
    pub db: &'a ProjectDatabase,
}

/// Shared data passed to every [`ItemHintCollector`] for a single top-level item.
pub struct ItemHintContext<'a> {
    pub item_id: &'a ItemId<'a>,
    pub sym_table: &'a SymbolTable<'a>,
    pub db: &'a ProjectDatabase,
}

/// A hint producer that runs once per **function body**.
///
/// Implement this to add hints that depend on inferred types or
/// the expression/statement structure inside a function.
pub trait HintCollector {
    fn collect(&self, ctx: &HintContext<'_>, hints: &mut Vec<InlayHint>);
}

/// A hint producer that runs once per **top-level item** in the file.
///
/// Implement this to add hints at the item level (e.g. closing brace labels)
/// without needing type-inference data.
pub trait ItemHintCollector {
    fn collect(&self, ctx: &ItemHintContext<'_>, hints: &mut Vec<InlayHint>);
}

/// Returns the display type for a hint, or `None` if the type should be suppressed.
///
/// Filters out `unknown`/`error` noise and widens literal types to their base
/// type (e.g. the integer literal `1` → `int`).
fn display_ty(ty: &baml_db::baml_compiler_tir::Ty) -> Option<baml_db::baml_compiler_tir::Ty> {
    use baml_db::baml_compiler_tir::{LiteralValue, Ty};
    match ty {
        Ty::Unknown | Ty::Error | Ty::BuiltinUnknown => None,
        Ty::Literal(LiteralValue::Int(_)) => Some(Ty::Int),
        Ty::Literal(LiteralValue::Float(_)) => Some(Ty::Float),
        Ty::Literal(LiteralValue::String(_)) => Some(Ty::String),
        Ty::Literal(LiteralValue::Bool(_)) => Some(Ty::Bool),
        other => Some(other.clone()),
    }
}

/// Emits `param_name:` labels before positional call arguments.
pub struct CallArgNames;

impl HintCollector for CallArgNames {
    fn collect(&self, ctx: &HintContext<'_>, hints: &mut Vec<InlayHint>) {
        use baml_db::baml_compiler_hir::{Definition, Expr};

        for (_, expr) in ctx.body.exprs.iter() {
            let Expr::Call { callee, args } = expr else {
                continue;
            };

            if args.is_empty() {
                continue;
            }

            let Some(ResolvedValue::Function(fqn)) = ctx.inference.expr_resolutions.get(callee)
            else {
                continue;
            };

            let Some(Definition::Function(callee_func_loc)) =
                ctx.sym_table.lookup_value(ctx.db, fqn)
            else {
                continue;
            };

            let sig = function_signature(ctx.db, callee_func_loc);

            for (i, arg_id) in args.iter().enumerate() {
                let Some(param) = sig.params.get(i) else {
                    break;
                };
                let Some(arg_span) = ctx.source_map.expr_span(*arg_id) else {
                    continue;
                };

                hints.push(InlayHint {
                    offset: arg_span.range.start(),
                    label: format!("{}:", param.name),
                    padding_left: false,
                    padding_right: true,
                });
            }
        }
    }
}

/// Emits `: Type` labels after the variable name in unannotated `let` bindings.
///
/// The hint is suppressed when the binding already carries an explicit type
/// annotation, or when the inferred type is `unknown` / `error`.
pub struct LetTypeAnnotations;

impl HintCollector for LetTypeAnnotations {
    fn collect(&self, ctx: &HintContext<'_>, hints: &mut Vec<InlayHint>) {
        for (stmt_id, stmt) in ctx.body.stmts.iter() {
            let Stmt::Let {
                pattern,
                type_annotation,
                initializer,
                ..
            } = stmt
            else {
                continue;
            };

            // Skip if the user already wrote an explicit type annotation.
            if type_annotation.is_some() {
                continue;
            }

            let Some(init_id) = initializer else {
                continue;
            };

            let Some(raw_ty) = ctx.inference.expr_types.get(init_id) else {
                continue;
            };

            let Some(ty) = display_ty(raw_ty) else {
                continue;
            };

            // Place the hint at the end of the bound pattern.
            let Some(pat_span) = ctx.source_map.pattern_span(*pattern) else {
                // Fall back to the statement span.
                let Some(stmt_span) = ctx.source_map.stmt_span(stmt_id) else {
                    continue;
                };
                hints.push(InlayHint {
                    offset: stmt_span.range.start(),
                    label: format!(": {ty} "),
                    padding_left: false,
                    padding_right: false,
                });
                continue;
            };

            hints.push(InlayHint {
                offset: pat_span.range.end(),
                label: format!(": {ty}"),
                padding_left: false,
                padding_right: true,
            });
        }
    }
}

/// Emits `: Type` labels after the scrutinee in unannotated `match` expressions.
///
/// Only emitted when the user has not already written an explicit type annotation
/// on the scrutinee (e.g. `match (x: int) { ... }` is left alone).
/// Suppressed for `unknown` / `error` types.
///
/// Example output:
/// ```baml
/// match (x: int) { ... }
/// ```
pub struct MatchScrutineeTypes;

impl HintCollector for MatchScrutineeTypes {
    fn collect(&self, ctx: &HintContext<'_>, hints: &mut Vec<InlayHint>) {
        use baml_db::baml_compiler_hir::Expr;

        for (_, expr) in ctx.body.exprs.iter() {
            let Expr::Match {
                scrutinee,
                scrutinee_type,
                ..
            } = expr
            else {
                continue;
            };

            // Skip if the user already wrote an explicit type annotation.
            if scrutinee_type.is_some() {
                continue;
            }

            let Some(raw_ty) = ctx.inference.expr_types.get(scrutinee) else {
                continue;
            };

            let Some(ty) = display_ty(raw_ty) else {
                continue;
            };

            let Some(scrutinee_span) = ctx.source_map.expr_span(*scrutinee) else {
                continue;
            };

            hints.push(InlayHint {
                offset: scrutinee_span.range.end(),
                label: format!(": {ty}"),
                padding_left: false,
                padding_right: false,
            });
        }
    }
}

/// Emits a label after the closing `}` of each top-level item showing what it
/// closes, e.g. `} function foo` or `} class Foo`.
///
/// Only emitted for multi-line items where the hint adds navigational value.
pub struct ClosingBraceLabels;

impl ItemHintCollector for ClosingBraceLabels {
    fn collect(&self, ctx: &ItemHintContext<'_>, hints: &mut Vec<InlayHint>) {
        match ctx.item_id {
            ItemId::Function(func_loc) => {
                let sig = function_signature(ctx.db, *func_loc);
                let body = function_body(ctx.db, *func_loc);
                let FunctionBody::Expr(expr_body, source_map) = &*body else {
                    return;
                };
                let Some(root_id) = expr_body.root_expr else {
                    return;
                };
                let Some(block_span) = source_map.expr_span(root_id) else {
                    return;
                };
                hints.push(InlayHint {
                    offset: block_span.range.end(),
                    label: format!("function {}", sig.name),
                    padding_left: true,
                    padding_right: false,
                });
            }
            _ => {}
        }
    }
}

/// Compute all inlay hints for the given file.
///
/// To add new hint categories:
/// - Body-level hints: implement [`HintCollector`] and add to `body_collectors`
/// - Item-level hints: implement [`ItemHintCollector`] and add to `item_collectors`
pub fn inlay_hints(db: &ProjectDatabase, file: SourceFile, _project: Project) -> Vec<InlayHint> {
    let body_collectors: &[&dyn HintCollector] =
        &[&CallArgNames, &LetTypeAnnotations, &MatchScrutineeTypes];
    let item_collectors: &[&dyn ItemHintCollector] = &[&ClosingBraceLabels];

    let mut hints = Vec::new();

    let Some(project) = db.get_project() else {
        return hints;
    };

    let file_items = file_items(db, file);
    let sym_table = symbol_table(db, project);

    for item_id in file_items.items(db) {
        // Item-level hints run for every item kind.
        let item_ctx = ItemHintContext {
            item_id,
            sym_table: &sym_table,
            db,
        };
        for collector in item_collectors {
            collector.collect(&item_ctx, &mut hints);
        }

        // Body-level hints only apply to functions with an expression body.
        let ItemId::Function(func_loc) = item_id else {
            continue;
        };

        let body = function_body(db, *func_loc);
        let FunctionBody::Expr(expr_body, source_map) = &*body else {
            continue;
        };

        let inference = baml_compiler_tir::function_type_inference(db, *func_loc);

        let ctx = HintContext {
            body: expr_body,
            inference: &inference,
            source_map,
            sym_table: &sym_table,
            db,
        };

        for collector in body_collectors {
            collector.collect(&ctx, &mut hints);
        }
    }

    hints
}
