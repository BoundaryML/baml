//! Inlay hints for BAML files.

use std::sync::Arc;

use baml_db::{
    SourceFile,
    baml_compiler_hir::{
        ExprBody, FunctionBody, HirSourceMap, ItemId, Stmt, SymbolTable, file_items, function_body,
        function_signature, symbol_table,
    },
    baml_compiler_tir::{self, InferenceResult, Ty},
    baml_workspace::Project,
};
use baml_project::ProjectDatabase;
use text_size::TextSize;

use crate::goto_definition::{NavigationTarget, lookup_symbol_definition};

/// The semantic kind of an inlay hint, mirroring the LSP `InlayHintKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlayHintKind {
    /// A parameter-name hint, e.g. `name:` before a call argument.
    Parameter,
    /// A type hint, e.g. `: string` after a variable name.
    Type,
}

/// A single segment of an inlay hint label.
///
/// When `target` is set, the editor renders the segment as a hyperlink
/// that navigates to the target definition on click.
pub struct InlayHintLabelPart {
    /// The text to display for this segment.
    pub value: String,
    /// Optional navigation target; when set, the segment is a clickable link.
    pub target: Option<NavigationTarget>,
}

/// An inlay hint to display inline in the editor.
pub struct InlayHint {
    /// Byte offset where the hint is displayed.
    pub offset: TextSize,
    /// Label segments. Each segment may optionally carry a navigation target.
    pub label: Vec<InlayHintLabelPart>,
    /// Semantic kind used by the editor for styling/filtering.
    /// `None` means no specific kind, will fall back to a default.
    pub kind: Option<InlayHintKind>,
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

/// Build a label with a single plain-text part (no navigation target).
fn plain_label(text: impl Into<String>) -> Vec<InlayHintLabelPart> {
    vec![InlayHintLabelPart {
        value: text.into(),
        target: None,
    }]
}

/// Convert a [`Ty`] into label parts, wrapping in parentheses if it's a
/// compound type (union or function) that would be ambiguous without them.
fn wrap_if_compound(db: &ProjectDatabase, ty: &Ty) -> Vec<InlayHintLabelPart> {
    if matches!(ty, Ty::Union(_) | Ty::Function { .. }) {
        let mut parts = vec![InlayHintLabelPart {
            value: "(".into(),
            target: None,
        }];
        parts.extend(ty_to_label_parts(db, ty));
        parts.push(InlayHintLabelPart {
            value: ")".into(),
            target: None,
        });
        parts
    } else {
        ty_to_label_parts(db, ty)
    }
}

/// Convert a [`Ty`] into label parts, resolving named types to clickable links.
fn ty_to_label_parts(db: &ProjectDatabase, ty: &Ty) -> Vec<InlayHintLabelPart> {
    match ty {
        Ty::Class(fqn) | Ty::Enum(fqn) | Ty::TypeAlias(fqn) => {
            let target = lookup_symbol_definition(db, fqn);
            vec![InlayHintLabelPart {
                value: fqn.to_string(),
                target,
            }]
        }
        Ty::Optional(inner) => {
            let mut parts = wrap_if_compound(db, inner);
            parts.push(InlayHintLabelPart {
                value: "?".into(),
                target: None,
            });

            parts
        }
        Ty::List(inner) => {
            let mut parts = wrap_if_compound(db, inner);
            parts.push(InlayHintLabelPart {
                value: "[]".into(),
                target: None,
            });

            parts
        }
        Ty::Map { key, value } => {
            let mut parts = vec![InlayHintLabelPart {
                value: "map<".into(),
                target: None,
            }];
            parts.extend(ty_to_label_parts(db, key));
            parts.push(InlayHintLabelPart {
                value: ", ".into(),
                target: None,
            });
            parts.extend(ty_to_label_parts(db, value));
            parts.push(InlayHintLabelPart {
                value: ">".into(),
                target: None,
            });

            parts
        }
        Ty::Union(types) => {
            let mut parts = Vec::new();
            for (i, t) in types.iter().enumerate() {
                if i > 0 {
                    parts.push(InlayHintLabelPart {
                        value: " | ".into(),
                        target: None,
                    });
                }
                parts.extend(ty_to_label_parts(db, t));
            }

            parts
        }
        Ty::Function { params, ret } => {
            let mut parts = vec![InlayHintLabelPart {
                value: "(".into(),
                target: None,
            }];
            for (i, param) in params.iter().enumerate() {
                if i > 0 {
                    parts.push(InlayHintLabelPart {
                        value: ", ".into(),
                        target: None,
                    });
                }
                parts.extend(ty_to_label_parts(db, &param.1));
            }
            parts.push(InlayHintLabelPart {
                value: ")".into(),
                target: None,
            });

            parts.push(InlayHintLabelPart {
                value: " -> ".into(),
                target: None,
            });
            parts.extend(ty_to_label_parts(db, ret));

            parts
        }
        // All other types: plain text, no link.
        other => plain_label(other.to_string()),
    }
}

/// Emits `param_name:` labels before positional call arguments.
pub struct CallArgNames;

impl HintCollector for CallArgNames {
    fn collect(&self, ctx: &HintContext<'_>, hints: &mut Vec<InlayHint>) {
        use baml_db::baml_compiler_hir::Expr;

        for (_, expr) in ctx.body.exprs.iter() {
            let Expr::Call { callee, args } = expr else {
                continue;
            };

            if args.is_empty() {
                continue;
            }

            // Get parameter names from the callee's inferred function type.
            // This works uniformly for named functions, local variables holding
            // functions, and any other expression with a function type.
            let Some(Ty::Function { params, .. }) = ctx.inference.expr_types.get(callee) else {
                continue;
            };

            for (i, arg_id) in args.iter().enumerate() {
                let Some((Some(name), _)) = params.get(i) else {
                    break;
                };
                let Some(arg_span) = ctx.source_map.expr_span(*arg_id) else {
                    continue;
                };

                hints.push(InlayHint {
                    offset: arg_span.range.start(),
                    label: plain_label(format!("{}:", name)),
                    kind: Some(InlayHintKind::Parameter),
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

            // Build label parts: ": " (plain) + type (with links).
            let mut label = plain_label(": ");
            label.extend(ty_to_label_parts(ctx.db, &ty));

            // Place the hint at the end of the bound pattern, falling back to
            // the statement start when no pattern span is available.
            let (offset, padding_right) =
                if let Some(pat_span) = ctx.source_map.pattern_span(*pattern) {
                    (pat_span.range.end(), true)
                } else if let Some(stmt_span) = ctx.source_map.stmt_span(stmt_id) {
                    (stmt_span.range.start(), false)
                } else {
                    continue;
                };

            hints.push(InlayHint {
                offset,
                label,
                kind: Some(InlayHintKind::Type),
                padding_left: false,
                padding_right,
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
                    label: plain_label(format!("function {}", sig.name)),
                    kind: None,
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
    let body_collectors: &[&dyn HintCollector] = &[&CallArgNames, &LetTypeAnnotations];
    let item_collectors: &[&dyn ItemHintCollector] = &[];

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
