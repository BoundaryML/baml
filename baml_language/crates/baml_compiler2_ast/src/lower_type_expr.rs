//! CST `TypeExpr` node → `ast::TypeExpr` recursive enum.
//!
//! Adapts the logic from `TypeRef::from_ast()` in `baml_compiler_hir/src/type_ref.rs`.
//! The output is the same recursive structure but as `ast::TypeExpr` instead of `TypeRef`.

use baml_base::Name;
use baml_compiler_syntax::{FunctionTypeParam, ast::TypeExpr as CstTypeExpr};
use rowan::ast::AstNode;

use crate::ast::{FunctionTypeParam as AstFunctionTypeParam, TypeExpr};

/// Convert a CST `TypeExpr` node to our `ast::TypeExpr` recursive enum.
pub(crate) fn lower_type_expr_node(type_expr: &CstTypeExpr) -> TypeExpr {
    // Handle optional modifier (outermost)
    // For `int[]?`, optional wraps the array
    if type_expr.is_optional() {
        let inner = lower_without_optional(type_expr);
        return TypeExpr::Optional(Box::new(inner));
    }

    lower_without_optional(type_expr)
}

/// Parse a `TypeExpr` assuming the optional modifier has been handled.
fn lower_without_optional(type_expr: &CstTypeExpr) -> TypeExpr {
    // Handle union FIRST (top-level PIPE separators)
    // For `int[] | string[]`, this is a union of arrays, not an array of unions
    if type_expr.is_union() {
        let member_parts = type_expr.union_member_parts();
        let members: Vec<TypeExpr> = member_parts.iter().map(lower_union_member).collect();
        return TypeExpr::Union(members);
    }

    // Handle array modifier
    if type_expr.is_array() {
        let element = lower_array_element(type_expr);
        return TypeExpr::List(Box::new(element));
    }

    lower_base(type_expr)
}

/// Get the element type for an array `TypeExpr`.
fn lower_array_element(type_expr: &CstTypeExpr) -> TypeExpr {
    // For parenthesized arrays like `(int | string)[]`, the element is the inner TypeExpr
    if let Some(inner) = type_expr.inner_type_expr() {
        return lower_type_expr_node(&inner);
    }

    // For non-parenthesized arrays like `int[]`, `string[][]`:
    // Use array_depth() to count nesting levels and lower_base_type() for the base.
    // For `int[][]`: depth=2, base=Int -> element is List(Int) i.e. `int[]`
    // For `int[]`: depth=1, base=Int -> element is Int
    let depth = type_expr.array_depth();
    let base = lower_base_type(type_expr);

    // Wrap base type in (depth-1) List layers to get the element type
    let mut result = base;
    for _ in 0..depth.saturating_sub(1) {
        result = TypeExpr::List(Box::new(result));
    }
    result
}

/// Parse the base type (no optional, array, or union modifiers).
fn lower_base(type_expr: &CstTypeExpr) -> TypeExpr {
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
                    .unwrap_or(TypeExpr::Unknown);
                AstFunctionTypeParam { name, ty }
            })
            .collect();
        let ret = type_expr
            .function_return_type()
            .map(|t| lower_type_expr_node(&t))
            .unwrap_or(TypeExpr::Unknown);
        return TypeExpr::Function {
            params,
            ret: Box::new(ret),
        };
    }

    // Handle parenthesized types like `(int | string)`
    if let Some(inner) = type_expr.inner_type_expr() {
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
                return TypeExpr::Union(members);
            }
        }
    }

    lower_base_type(type_expr)
}

/// Parse a base type (no modifiers, not a union).
fn lower_base_type(type_expr: &CstTypeExpr) -> TypeExpr {
    if let Some(s) = type_expr.string_literal() {
        return TypeExpr::Literal(baml_base::Literal::String(s));
    }

    if let Some(i) = type_expr.integer_literal() {
        return TypeExpr::Literal(baml_base::Literal::Int(i));
    }

    if let Some(f) = type_expr.float_literal() {
        return TypeExpr::Literal(baml_base::Literal::Float(f));
    }

    if let Some(b) = type_expr.bool_literal() {
        return TypeExpr::Literal(baml_base::Literal::Bool(b));
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
                };
            }
        }

        // Named type (primitive or user-defined)
        return lower_from_type_name(&name);
    }

    TypeExpr::Unknown
}

/// Parse a union member from its structured parts.
fn lower_union_member(parts: &baml_compiler_syntax::ast::UnionMemberParts) -> TypeExpr {
    // Check for parenthesized type first (e.g., `(int | string)` in `A | (int | string)`)
    if let Some(type_expr) = parts.type_expr() {
        let inner = lower_type_expr_node(&type_expr);
        return apply_modifiers_from_parts(inner, parts);
    }

    // Check for FUNCTION_TYPE_PARAM child (new parser structure for parenthesized types)
    if let Some(func_param) = parts.function_type_param() {
        if let Some(inner_type_expr) = func_param
            .children()
            .find(|n| n.kind() == baml_compiler_syntax::SyntaxKind::TYPE_EXPR)
        {
            if let Some(type_expr) = baml_compiler_syntax::ast::TypeExpr::cast(inner_type_expr) {
                let inner = lower_type_expr_node(&type_expr);
                return apply_modifiers_from_parts(inner, parts);
            }
        }
    }

    if let Some(s) = parts.string_literal() {
        let base = TypeExpr::Literal(baml_base::Literal::String(s));
        return apply_modifiers_from_parts(base, parts);
    }

    if let Some(i) = parts.integer_literal() {
        let base = TypeExpr::Literal(baml_base::Literal::Int(i));
        return apply_modifiers_from_parts(base, parts);
    }

    if let Some(f) = parts.float_literal() {
        let base = TypeExpr::Literal(baml_base::Literal::Float(f));
        return apply_modifiers_from_parts(base, parts);
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
                    let base = TypeExpr::Map {
                        key: Box::new(key),
                        value: Box::new(value),
                    };
                    return apply_modifiers_from_parts(base, parts);
                }
            }
        }

        let base = match name.as_str() {
            "true" => TypeExpr::Literal(baml_base::Literal::Bool(true)),
            "false" => TypeExpr::Literal(baml_base::Literal::Bool(false)),
            _ => lower_from_type_name(&name),
        };
        return apply_modifiers_from_parts(base, parts);
    }

    TypeExpr::Unknown
}

/// Apply array and optional modifiers from `UnionMemberParts` to a base type.
fn apply_modifiers_from_parts(
    base: TypeExpr,
    parts: &baml_compiler_syntax::ast::UnionMemberParts,
) -> TypeExpr {
    let mut result = base;
    for modifier in parts.postfix_modifiers() {
        match modifier {
            baml_compiler_syntax::ast::TypePostFixModifier::Optional => {
                result = TypeExpr::Optional(Box::new(result));
            }
            baml_compiler_syntax::ast::TypePostFixModifier::Array => {
                result = TypeExpr::List(Box::new(result));
            }
        }
    }

    result
}

/// Create a `TypeExpr` from a type name string (primitive or user-defined).
fn lower_from_type_name(name: &str) -> TypeExpr {
    match name {
        "int" => TypeExpr::Int,
        "float" => TypeExpr::Float,
        "string" => TypeExpr::String,
        "bool" => TypeExpr::Bool,
        "null" => TypeExpr::Null,
        "never" => TypeExpr::Never,
        "unknown" => TypeExpr::BuiltinUnknown,
        "type" => TypeExpr::Type,
        "$rust_type" => TypeExpr::Rust,
        "image" => TypeExpr::Media(baml_base::MediaKind::Image),
        "audio" => TypeExpr::Media(baml_base::MediaKind::Audio),
        "video" => TypeExpr::Media(baml_base::MediaKind::Video),
        "pdf" => TypeExpr::Media(baml_base::MediaKind::Pdf),
        _ => {
            if name.contains('.') {
                let segments: Vec<Name> = name.split('.').map(Name::new).collect();
                TypeExpr::Path(segments)
            } else {
                TypeExpr::Path(vec![Name::new(name)])
            }
        }
    }
}
