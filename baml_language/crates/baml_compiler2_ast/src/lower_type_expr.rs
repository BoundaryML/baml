//! CST `TypeExpr` node → `ast::SpannedTypeExpr` recursive lowering.
//!
//! Produces a recursive `SpannedTypeExpr` where every sub-expression carries
//! its own `TextRange`. A span-free `TypeExpr` can be obtained via
//! `SpannedTypeExpr::to_type_expr()`.

use baml_base::Name;
use baml_compiler_syntax::{FunctionTypeParam, ast::TypeExpr as CstTypeExpr};
use rowan::ast::AstNode;
use text_size::{TextRange, TextSize};

use crate::ast::{SpannedFunctionTypeParam, SpannedTypeExpr, SpannedTypeExprKind};

/// Convert a CST `TypeExpr` node to a recursive `SpannedTypeExpr`.
pub(crate) fn lower_type_expr_node(type_expr: &CstTypeExpr) -> SpannedTypeExpr {
    let span = type_expr.trimmed_text_range();

    if type_expr.is_optional() {
        let inner = lower_without_optional(type_expr);
        return SpannedTypeExpr {
            kind: SpannedTypeExprKind::Optional(Box::new(inner)),
            span,
        };
    }

    lower_without_optional(type_expr)
}

fn lower_without_optional(type_expr: &CstTypeExpr) -> SpannedTypeExpr {
    let span = type_expr.trimmed_text_range();

    if type_expr.is_union() {
        let member_parts = type_expr.union_member_parts();
        let members: Vec<SpannedTypeExpr> = member_parts.iter().map(lower_union_member).collect();
        return SpannedTypeExpr {
            kind: SpannedTypeExprKind::Union(members),
            span,
        };
    }

    if type_expr.is_array() {
        let element = lower_array_element(type_expr);
        let wrapper_span =
            TextRange::new(element.span.end(), element.span.end() + TextSize::from(1));
        return SpannedTypeExpr {
            kind: SpannedTypeExprKind::List(Box::new(element)),
            span: wrapper_span,
        };
    }

    lower_base(type_expr)
}

fn lower_array_element(type_expr: &CstTypeExpr) -> SpannedTypeExpr {
    if let Some(inner) = type_expr.inner_type_expr() {
        return lower_type_expr_node(&inner);
    }

    let depth = type_expr.array_depth();
    let base = lower_base_type(type_expr);
    let full_span = type_expr.trimmed_text_range();

    let mut result = base;
    let n_wrappers = depth.saturating_sub(1);
    for i in 0..n_wrappers {
        let is_outermost = i == n_wrappers - 1;
        let span = if is_outermost {
            full_span
        } else {
            TextRange::new(result.span.end(), result.span.end() + TextSize::from(1))
        };
        result = SpannedTypeExpr {
            kind: SpannedTypeExprKind::List(Box::new(result)),
            span,
        };
    }
    result
}

fn lower_base(type_expr: &CstTypeExpr) -> SpannedTypeExpr {
    let span = type_expr.trimmed_text_range();

    if type_expr.is_function_type() {
        let params = type_expr
            .function_type_params()
            .iter()
            .map(|p| {
                let name = p.name().map(|s| Name::new(&s));
                let ty =
                    p.ty()
                        .map(|t| lower_type_expr_node(&t))
                        .unwrap_or_else(|| SpannedTypeExpr {
                            kind: SpannedTypeExprKind::Unknown,
                            span,
                        });
                SpannedFunctionTypeParam { name, ty }
            })
            .collect();
        let ret = type_expr
            .function_return_type()
            .map(|t| lower_type_expr_node(&t))
            .unwrap_or_else(|| SpannedTypeExpr {
                kind: SpannedTypeExprKind::Unknown,
                span,
            });
        return SpannedTypeExpr {
            kind: SpannedTypeExprKind::Function {
                params,
                ret: Box::new(ret),
            },
            span,
        };
    }

    if let Some(inner) = type_expr.inner_type_expr() {
        return lower_type_expr_node(&inner);
    }

    if type_expr.is_parenthesized() && !type_expr.is_function_type() {
        let params = type_expr.function_type_params();
        if params.len() > 1 {
            let members: Vec<SpannedTypeExpr> = params
                .iter()
                .filter_map(FunctionTypeParam::ty)
                .map(|t| lower_type_expr_node(&t))
                .collect();
            if !members.is_empty() {
                return SpannedTypeExpr {
                    kind: SpannedTypeExprKind::Union(members),
                    span,
                };
            }
        }
    }

    lower_base_type(type_expr)
}

fn lower_base_type(type_expr: &CstTypeExpr) -> SpannedTypeExpr {
    let span = type_expr.trimmed_text_range();

    if let Some(s) = type_expr.string_literal() {
        return SpannedTypeExpr {
            kind: SpannedTypeExprKind::Literal(baml_base::Literal::String(s)),
            span,
        };
    }

    if let Some(i) = type_expr.integer_literal() {
        return SpannedTypeExpr {
            kind: SpannedTypeExprKind::Literal(baml_base::Literal::Int(i)),
            span,
        };
    }

    if let Some(f) = type_expr.float_literal() {
        return SpannedTypeExpr {
            kind: SpannedTypeExprKind::Literal(baml_base::Literal::Float(f)),
            span,
        };
    }

    if let Some(b) = type_expr.bool_literal() {
        return SpannedTypeExpr {
            kind: SpannedTypeExprKind::Literal(baml_base::Literal::Bool(b)),
            span,
        };
    }

    if let Some(name) = type_expr.dotted_name() {
        if name == "map" {
            let args = type_expr.type_arg_exprs();
            if args.len() == 2 {
                let key = lower_type_expr_node(&args[0]);
                let value = lower_type_expr_node(&args[1]);
                return SpannedTypeExpr {
                    kind: SpannedTypeExprKind::Map {
                        key: Box::new(key),
                        value: Box::new(value),
                    },
                    span,
                };
            }
        }

        return lower_from_type_name(&name, span);
    }

    SpannedTypeExpr {
        kind: SpannedTypeExprKind::Unknown,
        span,
    }
}

fn lower_union_member(parts: &baml_compiler_syntax::ast::UnionMemberParts) -> SpannedTypeExpr {
    let span = parts.text_range();

    if let Some(type_expr) = parts.type_expr() {
        let inner = lower_type_expr_node(&type_expr);
        return apply_modifiers_from_parts(inner, parts, span);
    }

    if let Some(func_param) = parts.function_type_param() {
        if let Some(inner_type_expr) = func_param
            .children()
            .find(|n| n.kind() == baml_compiler_syntax::SyntaxKind::TYPE_EXPR)
        {
            if let Some(type_expr) = baml_compiler_syntax::ast::TypeExpr::cast(inner_type_expr) {
                let inner = lower_type_expr_node(&type_expr);
                return apply_modifiers_from_parts(inner, parts, span);
            }
        }
    }

    if let Some(s) = parts.string_literal() {
        let base = SpannedTypeExpr {
            kind: SpannedTypeExprKind::Literal(baml_base::Literal::String(s)),
            span,
        };
        return apply_modifiers_from_parts(base, parts, span);
    }

    if let Some(i) = parts.integer_literal() {
        let base = SpannedTypeExpr {
            kind: SpannedTypeExprKind::Literal(baml_base::Literal::Int(i)),
            span,
        };
        return apply_modifiers_from_parts(base, parts, span);
    }

    if let Some(f) = parts.float_literal() {
        let base = SpannedTypeExpr {
            kind: SpannedTypeExprKind::Literal(baml_base::Literal::Float(f)),
            span,
        };
        return apply_modifiers_from_parts(base, parts, span);
    }

    if let Some(name) = parts.dotted_name() {
        if name == "map" {
            if let Some(type_args_node) = parts.type_args() {
                let type_arg_exprs: Vec<_> = type_args_node
                    .children()
                    .filter(|n| n.kind() == baml_compiler_syntax::SyntaxKind::TYPE_EXPR)
                    .map(|n| baml_compiler_syntax::ast::TypeExpr::cast(n).unwrap())
                    .collect();

                if type_arg_exprs.len() == 2 {
                    let key = lower_type_expr_node(&type_arg_exprs[0]);
                    let value = lower_type_expr_node(&type_arg_exprs[1]);
                    let base = SpannedTypeExpr {
                        kind: SpannedTypeExprKind::Map {
                            key: Box::new(key),
                            value: Box::new(value),
                        },
                        span,
                    };
                    return apply_modifiers_from_parts(base, parts, span);
                }
            }
        }

        let base = match name.as_str() {
            "true" => SpannedTypeExpr {
                kind: SpannedTypeExprKind::Literal(baml_base::Literal::Bool(true)),
                span,
            },
            "false" => SpannedTypeExpr {
                kind: SpannedTypeExprKind::Literal(baml_base::Literal::Bool(false)),
                span,
            },
            _ => lower_from_type_name(&name, span),
        };
        return apply_modifiers_from_parts(base, parts, span);
    }

    SpannedTypeExpr {
        kind: SpannedTypeExprKind::Unknown,
        span,
    }
}

fn apply_modifiers_from_parts(
    base: SpannedTypeExpr,
    parts: &baml_compiler_syntax::ast::UnionMemberParts,
    _outer_span: TextRange,
) -> SpannedTypeExpr {
    let mut result = base;
    for modifier in parts.postfix_modifiers() {
        let span = TextRange::new(result.span.end(), result.span.end() + TextSize::from(1));
        result = match modifier {
            baml_compiler_syntax::ast::TypePostFixModifier::Optional => SpannedTypeExpr {
                kind: SpannedTypeExprKind::Optional(Box::new(result)),
                span,
            },
            baml_compiler_syntax::ast::TypePostFixModifier::Array => SpannedTypeExpr {
                kind: SpannedTypeExprKind::List(Box::new(result)),
                span,
            },
        };
    }
    result
}

fn lower_from_type_name(name: &str, span: TextRange) -> SpannedTypeExpr {
    let kind = match name {
        "int" => SpannedTypeExprKind::Int,
        "float" => SpannedTypeExprKind::Float,
        "string" => SpannedTypeExprKind::String,
        "bool" => SpannedTypeExprKind::Bool,
        "null" => SpannedTypeExprKind::Null,
        "never" => SpannedTypeExprKind::Never,
        "unknown" => SpannedTypeExprKind::BuiltinUnknown,
        "type" => SpannedTypeExprKind::Type,
        "$rust_type" => SpannedTypeExprKind::Rust,
        "image" => SpannedTypeExprKind::Media(baml_base::MediaKind::Image),
        "audio" => SpannedTypeExprKind::Media(baml_base::MediaKind::Audio),
        "video" => SpannedTypeExprKind::Media(baml_base::MediaKind::Video),
        "pdf" => SpannedTypeExprKind::Media(baml_base::MediaKind::Pdf),
        _ => {
            if name.contains('.') {
                let segments: Vec<Name> = name.split('.').map(Name::new).collect();
                SpannedTypeExprKind::Path(segments)
            } else {
                SpannedTypeExprKind::Path(vec![Name::new(name)])
            }
        }
    };
    SpannedTypeExpr { kind, span }
}
