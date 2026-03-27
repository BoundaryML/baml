//! CST `TypeExpr` node → `ast::TypeExpr` recursive enum.
//!
//! Adapts the logic from `TypeRef::from_ast()` in `baml_compiler_hir/src/type_ref.rs`.
//! The output is the same recursive structure but as `ast::TypeExpr` instead of `TypeRef`.
//!
//! Each `TypeExpr` variant carries `attrs: Vec<RawAttribute>` populated from
//! ATTRIBUTE children of the corresponding CST `TYPE_EXPR` node.

use baml_base::Name;
use baml_compiler_syntax::{FunctionTypeParam, ast::TypeExpr as CstTypeExpr};
use rowan::ast::AstNode;

use crate::{
    ast::{FunctionTypeParam as AstFunctionTypeParam, RawAttribute, TypeExpr},
    lower_cst::lower_attribute,
};

/// Collect ATTRIBUTE children from a CST `TypeExpr` node.
fn collect_type_attrs(type_expr: &CstTypeExpr) -> Vec<RawAttribute> {
    type_expr
        .syntax()
        .children()
        .filter_map(baml_compiler_syntax::ast::Attribute::cast)
        .filter_map(|attr| lower_attribute(&attr))
        .collect()
}

/// Convert a CST `TypeExpr` node to our `ast::TypeExpr` recursive enum.
///
/// 1. Lower the base type (no attrs).
/// 2. Apply postfix modifiers (`[]`, `?`).
/// 3. Attach attrs to the outermost node.
pub(crate) fn lower_type_expr_node(type_expr: &CstTypeExpr) -> TypeExpr {
    let attrs = collect_type_attrs(type_expr);
    let base = lower_base(type_expr);
    let mut result = apply_modifiers(base, &type_expr.postfix_modifiers());
    *result.attrs_mut() = attrs;
    result
}

/// Apply postfix modifiers (`[]`, `?`) to a base type, wrapping it in
/// `List` / `Optional` layers.
fn apply_modifiers(
    base: TypeExpr,
    modifiers: &[baml_compiler_syntax::ast::TypePostFixModifier],
) -> TypeExpr {
    let mut result = base;
    for modifier in modifiers {
        match modifier {
            baml_compiler_syntax::ast::TypePostFixModifier::Optional => {
                result = TypeExpr::Optional {
                    inner: Box::new(result),
                    attrs: vec![],
                };
            }
            baml_compiler_syntax::ast::TypePostFixModifier::Array => {
                result = TypeExpr::List {
                    inner: Box::new(result),
                    attrs: vec![],
                };
            }
        }
    }
    result
}

/// Extract the base type (unions, function types, parens, terminals).
/// No modifier or attr handling.
fn lower_base(type_expr: &CstTypeExpr) -> TypeExpr {
    // Handle union FIRST (top-level PIPE separators)
    // For `int[] | string[]`, this is a union of arrays, not an array of unions
    if type_expr.is_union() {
        let member_parts = type_expr.union_member_parts();
        let variants: Vec<TypeExpr> = member_parts.iter().map(lower_union_member).collect();
        return TypeExpr::Union {
            variants,
            attrs: vec![],
        };
    }

    lower_base_terminal(type_expr)
}

/// Parse the base type (no modifiers, not a union).
fn lower_base_terminal(type_expr: &CstTypeExpr) -> TypeExpr {
    // Handle function types like `(x: int, y: int) -> bool`
    if type_expr.is_function_type() {
        let params = type_expr
            .function_type_params()
            .iter()
            .map(|p| {
                let name = p.name().map(|s| Name::new(&s));
                let ty = p
                    .ty()
                    .map(|t| lower_type_expr_node(&t))
                    .unwrap_or(TypeExpr::Unknown { attrs: vec![] });
                AstFunctionTypeParam { name, ty }
            })
            .collect();
        let ret = type_expr
            .function_return_type()
            .map(|t| lower_type_expr_node(&t))
            .unwrap_or(TypeExpr::Unknown { attrs: vec![] });
        return TypeExpr::Function {
            params,
            ret: Box::new(ret),
            attrs: vec![],
        };
    }

    // Handle parenthesized types like `(int | string)`
    if let Some(inner) = type_expr.inner_type_expr() {
        // For parenthesized types, attrs go on the inner type via recursive lowering.
        // If the outer node had attrs collected, we'd need to merge, but in practice
        // the parser puts attrs at the outermost level.
        return lower_type_expr_node(&inner);
    }

    // Handle parenthesized unions: `(A | B)` where the union is inside parens
    if type_expr.is_parenthesized() && !type_expr.is_function_type() {
        let params = type_expr.function_type_params();
        if params.len() > 1 {
            let members: Vec<TypeExpr> = params
                .iter()
                .filter_map(FunctionTypeParam::ty)
                .map(|t| lower_type_expr_node(&t))
                .collect();
            if !members.is_empty() {
                return TypeExpr::Union {
                    variants: members,
                    attrs: vec![],
                };
            }
        }
    }

    lower_base_type(type_expr)
}

fn lower_base_type(type_expr: &CstTypeExpr) -> TypeExpr {
    if let Some(s) = type_expr.string_literal() {
        return TypeExpr::Literal {
            value: baml_base::Literal::String(s),
            attrs: vec![],
        };
    }

    if let Some(i) = type_expr.integer_literal() {
        return TypeExpr::Literal {
            value: baml_base::Literal::Int(i),
            attrs: vec![],
        };
    }

    if let Some(f) = type_expr.float_literal() {
        return TypeExpr::Literal {
            value: baml_base::Literal::Float(f),
            attrs: vec![],
        };
    }

    if let Some(b) = type_expr.bool_literal() {
        return TypeExpr::Literal {
            value: baml_base::Literal::Bool(b),
            attrs: vec![],
        };
    }

    // Check for map type with type args
    if let Some(name) = type_expr.dotted_name() {
        if name == "map" {
            let args = type_expr.type_arg_exprs();
            if args.len() == 2 {
                let key = lower_type_expr_node(&args[0]);
                let value = lower_type_expr_node(&args[1]);
                return TypeExpr::Map {
                    key: Box::new(key),
                    value: Box::new(value),
                    attrs: vec![],
                };
            }
        }

        // Named type (primitive or user-defined)
        return lower_from_type_name(&name);
    }

    TypeExpr::Unknown { attrs: vec![] }
}

/// Parse a union member from its structured parts.
fn lower_union_member(parts: &baml_compiler_syntax::ast::UnionMemberParts) -> TypeExpr {
    // Collect attributes from the union member's CST subtree
    let attrs: Vec<RawAttribute> = parts
        .attributes()
        .filter_map(|attr| lower_attribute(&attr))
        .collect();

    let base = lower_union_member_base(parts);
    let mut result = apply_modifiers(base, &parts.postfix_modifiers());
    *result.attrs_mut() = attrs;
    result
}

/// Extract the base type from union member parts (no modifiers or attrs).
fn lower_union_member_base(parts: &baml_compiler_syntax::ast::UnionMemberParts) -> TypeExpr {
    // Check for parenthesized type first (e.g., `(int | string)` in `A | (int | string)`)
    if let Some(type_expr) = parts.type_expr() {
        return lower_type_expr_node(&type_expr);
    }

    // Check for FUNCTION_TYPE_PARAM child (new parser structure for parenthesized types)
    if let Some(func_param) = parts.function_type_param() {
        if let Some(inner_type_expr) = func_param
            .children()
            .find(|n| n.kind() == baml_compiler_syntax::SyntaxKind::TYPE_EXPR)
        {
            if let Some(type_expr) = baml_compiler_syntax::ast::TypeExpr::cast(inner_type_expr) {
                return lower_type_expr_node(&type_expr);
            }
        }
    }

    if let Some(s) = parts.string_literal() {
        return TypeExpr::Literal {
            value: baml_base::Literal::String(s),
            attrs: vec![],
        };
    }

    if let Some(i) = parts.integer_literal() {
        return TypeExpr::Literal {
            value: baml_base::Literal::Int(i),
            attrs: vec![],
        };
    }

    if let Some(f) = parts.float_literal() {
        return TypeExpr::Literal {
            value: baml_base::Literal::Float(f),
            attrs: vec![],
        };
    }

    // Check for named/primitive type or map type
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
                    return TypeExpr::Map {
                        key: Box::new(key),
                        value: Box::new(value),
                        attrs: vec![],
                    };
                }
            }
        }

        return match name.as_str() {
            "true" => TypeExpr::Literal {
                value: baml_base::Literal::Bool(true),
                attrs: vec![],
            },
            "false" => TypeExpr::Literal {
                value: baml_base::Literal::Bool(false),
                attrs: vec![],
            },
            _ => lower_from_type_name(&name),
        };
    }

    TypeExpr::Unknown { attrs: vec![] }
}

/// Create a `TypeExpr` from a type name string (primitive or user-defined).
fn lower_from_type_name(name: &str) -> TypeExpr {
    match name {
        "int" => TypeExpr::Int { attrs: vec![] },
        "float" => TypeExpr::Float { attrs: vec![] },
        "string" => TypeExpr::String { attrs: vec![] },
        "bool" => TypeExpr::Bool { attrs: vec![] },
        "null" => TypeExpr::Null { attrs: vec![] },
        "never" => TypeExpr::Never { attrs: vec![] },
        "unknown" => TypeExpr::BuiltinUnknown { attrs: vec![] },
        "type" => TypeExpr::Type { attrs: vec![] },
        "$rust_type" => TypeExpr::Rust { attrs: vec![] },
        "image" => TypeExpr::Media {
            kind: baml_base::MediaKind::Image,
            attrs: vec![],
        },
        "audio" => TypeExpr::Media {
            kind: baml_base::MediaKind::Audio,
            attrs: vec![],
        },
        "video" => TypeExpr::Media {
            kind: baml_base::MediaKind::Video,
            attrs: vec![],
        },
        "pdf" => TypeExpr::Media {
            kind: baml_base::MediaKind::Pdf,
            attrs: vec![],
        },
        _ => {
            if name.contains('.') {
                let segments: Vec<Name> = name.split('.').map(Name::new).collect();
                TypeExpr::Path {
                    segments,
                    attrs: vec![],
                }
            } else {
                TypeExpr::Path {
                    segments: vec![Name::new(name)],
                    attrs: vec![],
                }
            }
        }
    }
}
