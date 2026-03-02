//! Inlay hints for BAML files.

use std::sync::Arc;

use baml_db::{
    SourceFile,
    baml_compiler_hir::{
        ExprBody, FunctionBody, HirSourceMap, ItemId, LetOrigin, Stmt, SymbolTable, file_item_tree,
        file_items, function_body, symbol_table,
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

/// A text edit applied when the user double-clicks an inlay hint.
pub struct InlayHintTextEdit {
    /// Byte offset where the edit is inserted.
    pub offset: TextSize,
    /// The text to insert.
    pub new_text: String,
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
    /// Text edits applied when the user double-clicks the hint.
    pub text_edits: Vec<InlayHintTextEdit>,
}

/// Shared data passed to every hint collector for a single function body.
pub struct HintContext<'a> {
    pub body: &'a ExprBody,
    pub inference: &'a Arc<InferenceResult>,
    pub source_map: &'a HirSourceMap,
    pub sym_table: &'a SymbolTable<'a>,
    pub db: &'a ProjectDatabase,
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

                if let Some(name) = &param.0 {
                    parts.push(InlayHintLabelPart {
                        value: format!("{name}: "),
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
fn collect_call_arg_names(ctx: &HintContext<'_>, hints: &mut Vec<InlayHint>) {
    use baml_db::baml_compiler_hir::Expr;

    for (_, expr) in ctx.body.exprs.iter() {
        let Expr::Call { callee, args } = expr else {
            continue;
        };

        if args.is_empty() {
            continue;
        }

        // Get parameter names from the callee's inferred function type.
        let Some(Ty::Function { params, .. }) = ctx.inference.expr_types.get(callee) else {
            continue;
        };

        for (i, arg_id) in args.iter().enumerate() {
            let Some((Some(name), _)) = params.get(i) else {
                continue;
            };

            // Skip hint when the argument's final path segment matches or
            // contains the parameter name (e.g. `foo(bar)` or `foo(obj.bar)`
            // where the param is `bar`, or `foo(my_bar)` containing `bar`).
            if let Expr::Path(segments) = &ctx.body.exprs[*arg_id] {
                if let Some(last) = segments.last() {
                    let last = last.as_str();
                    let param = name.as_str();
                    if last.contains(param) {
                        continue;
                    }
                }
            }

            let Some(arg_span) = ctx.source_map.expr_span(*arg_id) else {
                continue;
            };

            hints.push(InlayHint {
                offset: arg_span.range.start(),
                label: plain_label(format!("{name}:")),
                kind: Some(InlayHintKind::Parameter),
                padding_left: false,
                padding_right: true,
                text_edits: vec![],
            });
        }
    }
}

/// Emits `: Type` labels after the variable name in unannotated `let` bindings.
///
/// The hint is suppressed when the binding already carries an explicit type
/// annotation, or when the inferred type is `unknown` / `error`.
/// This works for let statements in both types of for loops as well, with extra
/// logic to prevent suggesting inserting type annotations in for-in loops.
fn collect_let_type_annotations(ctx: &HintContext<'_>, hints: &mut Vec<InlayHint>) {
    for (stmt_id, stmt) in ctx.body.stmts.iter() {
        let Stmt::Let {
            pattern,
            type_annotation,
            initializer,
            origin,
            ..
        } = stmt
        else {
            continue;
        };

        // Skip if the user already wrote an explicit type annotation.
        if type_annotation.is_some() {
            continue;
        }

        // Skip if there is no initializer.
        let Some(init_id) = initializer else {
            continue;
        };

        // Get the inferred type of the initializer.
        let Some(raw_ty) = ctx.inference.expr_types.get(init_id) else {
            continue;
        };

        // Pretty print the type with goto definition support.
        let Some(ty) = display_ty(raw_ty) else {
            continue;
        };

        // Build label parts: ": " (plain) + type (with links).
        let mut label = plain_label(": ");
        label.extend(ty_to_label_parts(ctx.db, &ty));

        // Place the hint at the end of the bound pattern, falling back to
        // the statement start when no pattern span is available.
        let offset = if let Some(pat_span) = ctx.source_map.pattern_span(*pattern) {
            pat_span.range.end()
        } else if let Some(stmt_span) = ctx.source_map.stmt_span(stmt_id) {
            stmt_span.range.start()
        } else {
            continue;
        };

        // Concatenate label parts into the text to insert on double-click.
        // Only allow this if the let statement was written in the source code.
        // This is to prevent suggesting inserting type annotations in for-in loops.
        let insert_text: Option<String> = if *origin == LetOrigin::Source {
            Some(label.iter().map(|p| p.value.as_str()).collect())
        } else {
            None
        };

        hints.push(InlayHint {
            offset,
            label,
            kind: Some(InlayHintKind::Type),
            padding_left: false,
            padding_right: false,
            text_edits: insert_text.map_or(Vec::new(), |text| {
                vec![InlayHintTextEdit {
                    offset,
                    new_text: text,
                }]
            }),
        });
    }
}

/// Compute all inlay hints for the given file.
///
/// To add new hint categories, add a new collector function and call it in this function.
pub fn inlay_hints(db: &ProjectDatabase, file: SourceFile, project: Project) -> Vec<InlayHint> {
    let mut hints = Vec::new();

    // Collect all function bodies in the file.
    let file_items = file_items(db, file);
    let sym_table = symbol_table(db, project);
    for item_id in file_items.items(db) {
        // Only collect hints for function bodies. (for now, we may expand this)
        let ItemId::Function(func_loc) = item_id else {
            continue;
        };

        // Skip compiler-generated functions (e.g. client resolve, LLM helpers).
        let file = func_loc.file(db);
        let item_tree = file_item_tree(db, file);
        let func = &item_tree[func_loc.id(db)];
        if func.compiler_generated.is_some() {
            continue;
        }

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

        // Run all hint collectors.
        collect_call_arg_names(&ctx, &mut hints);
        collect_let_type_annotations(&ctx, &mut hints);
    }

    hints
}
