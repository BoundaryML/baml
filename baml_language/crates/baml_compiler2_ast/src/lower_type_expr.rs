//! CST `TypeExpr` node → `ast::TypeExpr` recursive enum.
//!
//! Adapts the logic from `TypeRef::from_ast()` in `baml_compiler_hir/src/type_ref.rs`.
//! The output is the same recursive structure but as `ast::TypeExpr` instead of `TypeRef`.
//!
//! Each `TypeExpr` variant carries `attrs: Vec<RawAttribute>` populated from
//! ATTRIBUTE children of the corresponding CST `TYPE_EXPR` node.

use baml_base::Name;
use baml_compiler_syntax::{FunctionTypeParam, SyntaxKind, ast::TypeExpr as CstTypeExpr};
use rowan::ast::AstNode;
use text_size::TextRange;

use crate::{
    LoweringDiagnostic,
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
/// Called by `lower_cst.rs` for field/alias/param/return-type positions.
/// Hoists trailing union attrs at the outermost level.
pub(crate) fn lower_type_expr_node(type_expr: &CstTypeExpr) -> TypeExpr {
    lower_type_expr_inner(type_expr, true)
}

/// Inner recursive lowering. `hoist_trailing` controls whether trailing attrs
/// after the last PIPE in a union are moved to the Union node (true only at
/// the outermost level).
fn lower_type_expr_inner(type_expr: &CstTypeExpr, hoist_trailing: bool) -> TypeExpr {
    // For unions, use the specialized path that distributes attrs per-member
    if type_expr.is_union() {
        return lower_union_with_attrs(type_expr, hoist_trailing);
    }

    let attrs = collect_type_attrs(type_expr);
    let base = lower_base(type_expr);
    let mut result = apply_modifiers(base, &type_expr.postfix_modifiers());
    *result.attrs_mut() = attrs;
    result
}

/// Lower a union `TYPE_EXPR`, distributing attrs to the correct members
/// based on positional ordering of CST children.
///
/// `hoist_trailing` is `true` when lowering a class field definition, so that
/// when parsing `field: string | int @foo`, the `@foo` is hoisted to the union;
/// but in recursive calls, it is false, so `(string | int @foo | float)` sees `@foo`
/// applied to the `int` variant of the union.
fn lower_union_with_attrs(type_expr: &CstTypeExpr, hoist_trailing: bool) -> TypeExpr {
    let member_parts = type_expr.union_member_parts();

    // Lower each member — this puts per-member attrs on each variant
    let mut variants: Vec<TypeExpr> = member_parts.iter().map(lower_union_member).collect();

    let mut union_attrs = Vec::new();

    if hoist_trailing {
        // Identify "trailing" attrs: ATTRIBUTE nodes that appear after the
        // last PIPE in the CST. These were grouped into the last member by
        // `union_member_parts()`, but they should go on the outer Union.
        //
        // Walk CST children to find the position of the last PIPE, then collect
        // ATTRIBUTE spans that come after it.
        let mut trailing_attr_spans: Vec<text_size::TextRange> = Vec::new();

        // First, count PIPEs to find the last one
        let children: Vec<_> = type_expr.syntax().children_with_tokens().collect();
        let last_pipe_idx = children.iter().rposition(
            |c| matches!(c, rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::PIPE),
        );

        if let Some(last_pipe_idx) = last_pipe_idx {
            for child in &children[(last_pipe_idx + 1)..] {
                if let rowan::NodeOrToken::Node(node) = child {
                    if node.kind() == SyntaxKind::ATTRIBUTE {
                        trailing_attr_spans.push(node.text_range());
                    }
                }
            }
        }

        // Move trailing attrs from the last variant to the Union's attrs
        if !trailing_attr_spans.is_empty() {
            if let Some(last_variant) = variants.last_mut() {
                let all_attrs = std::mem::take(last_variant.attrs_mut());
                let (trailing, member): (Vec<_>, Vec<_>) = all_attrs
                    .into_iter()
                    .partition(|a| trailing_attr_spans.contains(&a.span));
                *last_variant.attrs_mut() = member;
                union_attrs = trailing;
            }
        }
    }

    // Apply postfix modifiers (e.g., `(A | B)[]`)
    apply_modifiers(
        TypeExpr::Union {
            variants,
            attrs: union_attrs,
        },
        &type_expr.postfix_modifiers(),
    )
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

/// Extract the base type (function types, parens, terminals).
/// No modifier or attr handling. Unions are handled by `lower_union_with_attrs`.
fn lower_base(type_expr: &CstTypeExpr) -> TypeExpr {
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
                let optional = p.is_optional();
                let ty = p
                    .ty()
                    .map(|t| lower_type_expr_inner(&t, false))
                    .unwrap_or(TypeExpr::Unknown { attrs: vec![] });
                AstFunctionTypeParam { name, optional, ty }
            })
            .collect();
        let ret = type_expr
            .function_return_type()
            .map(|t| lower_type_expr_inner(&t, false))
            .unwrap_or(TypeExpr::Unknown { attrs: vec![] });
        let throws = type_expr
            .function_throws_type()
            .map(|t| Box::new(lower_type_expr_inner(&t, false)));
        return TypeExpr::Function {
            params,
            ret: Box::new(ret),
            throws,
            attrs: vec![],
        };
    }

    // Handle parenthesized types like `(int | string)`
    if let Some(inner) = type_expr.inner_type_expr() {
        // For parenthesized types, attrs go on the inner type via recursive lowering.
        // If the outer node had attrs collected, we'd need to merge, but in practice
        // the parser puts attrs at the outermost level.
        return lower_type_expr_inner(&inner, false);
    }

    // Handle parenthesized unions: `(A | B)` where the union is inside parens
    if type_expr.is_parenthesized() && !type_expr.is_function_type() {
        let params = type_expr.function_type_params();
        if params.len() > 1 {
            let members: Vec<TypeExpr> = params
                .iter()
                .filter_map(FunctionTypeParam::ty)
                .map(|t| lower_type_expr_inner(&t, false))
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
        let args = type_expr.type_arg_exprs();
        if name == "map" && args.len() == 2 {
            let key = lower_type_expr_inner(&args[0], false);
            let value = lower_type_expr_inner(&args[1], false);
            return TypeExpr::Map {
                key: Box::new(key),
                value: Box::new(value),
                attrs: vec![],
            };
        }

        // Named type (primitive or user-defined), preserving generic args
        let generic_args: Vec<TypeExpr> = args
            .iter()
            .map(|arg| lower_type_expr_inner(arg, false))
            .collect();
        return lower_from_type_name_with_generic_args(&name, generic_args);
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
    result.attrs_mut().extend(attrs);
    result
}

/// Extract the base type from union member parts (no modifiers or attrs).
fn lower_union_member_base(parts: &baml_compiler_syntax::ast::UnionMemberParts) -> TypeExpr {
    // Check for parenthesized type first (e.g., `(int | string)` in `A | (int | string)`)
    if let Some(type_expr) = parts.type_expr() {
        return lower_type_expr_inner(&type_expr, false);
    }

    // Check for FUNCTION_TYPE_PARAM child (new parser structure for parenthesized types)
    if let Some(func_param) = parts.function_type_param() {
        if let Some(inner_type_expr) = func_param
            .children()
            .find(|n| n.kind() == baml_compiler_syntax::SyntaxKind::TYPE_EXPR)
        {
            if let Some(type_expr) = baml_compiler_syntax::ast::TypeExpr::cast(inner_type_expr) {
                return lower_type_expr_inner(&type_expr, false);
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
        let type_arg_exprs: Vec<_> = parts
            .type_args()
            .map(|type_args_node| {
                type_args_node
                    .children()
                    .filter(|n| n.kind() == baml_compiler_syntax::SyntaxKind::TYPE_EXPR)
                    .map(|n| baml_compiler_syntax::ast::TypeExpr::cast(n).unwrap())
                    .collect()
            })
            .unwrap_or_default();

        if name == "map" {
            if type_arg_exprs.len() == 2 {
                let key = lower_type_expr_inner(&type_arg_exprs[0], false);
                let value = lower_type_expr_inner(&type_arg_exprs[1], false);
                return TypeExpr::Map {
                    key: Box::new(key),
                    value: Box::new(value),
                    attrs: vec![],
                };
            }
        }

        let generic_args: Vec<TypeExpr> = type_arg_exprs
            .iter()
            .map(|arg| lower_type_expr_inner(arg, false))
            .collect();

        return match name.as_str() {
            "true" => TypeExpr::Literal {
                value: baml_base::Literal::Bool(true),
                attrs: vec![],
            },
            "false" => TypeExpr::Literal {
                value: baml_base::Literal::Bool(false),
                attrs: vec![],
            },
            _ => lower_from_type_name_with_generic_args(&name, generic_args),
        };
    }

    TypeExpr::Unknown { attrs: vec![] }
}

/// Create a `TypeExpr` from a type name string with optional generic arguments.
///
/// Generic args are preserved on `Path` types (e.g., `Stream<T>`). For primitive
/// types, generic args are silently dropped (primitives can't be generic).
fn lower_from_type_name_with_generic_args(name: &str, generic_args: Vec<TypeExpr>) -> TypeExpr {
    match name {
        "int" => TypeExpr::Int { attrs: vec![] },
        "float" => TypeExpr::Float { attrs: vec![] },
        "string" => TypeExpr::String { attrs: vec![] },
        "bool" => TypeExpr::Bool { attrs: vec![] },
        "null" => TypeExpr::Null { attrs: vec![] },
        "never" => TypeExpr::Never { attrs: vec![] },
        "void" => TypeExpr::Void { attrs: vec![] },
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
        "uint8array" => TypeExpr::Uint8Array { attrs: vec![] },
        "json" => TypeExpr::Path {
            segments: vec![Name::new("baml"), Name::new("json"), Name::new("json")],
            generic_args: vec![],
            attrs: vec![],
        },
        _ => {
            if name.contains('.') {
                let segments: Vec<Name> = name.split('.').map(Name::new).collect();
                TypeExpr::Path {
                    segments,
                    generic_args,
                    attrs: vec![],
                }
            } else {
                TypeExpr::Path {
                    segments: vec![Name::new(name)],
                    generic_args,
                    attrs: vec![],
                }
            }
        }
    }
}

/// Recursively check that `void` does not appear in a non-return-type position.
///
/// `void` is only valid as the *bare* return type of a function. It must not
/// appear in parameter types, field types, union members, list/optional
/// wrappers, or function-type parameter positions. When `void` IS used as the
/// return type of a `TypeExpr::Function`, it is exempt.
///
/// Set `allow_root_void = true` when calling on a function return-type annotation
/// to permit a bare `-> void` while still rejecting `-> void?` or `-> void[]`.
///
/// Emits `VoidInNonReturnPosition` for every invalid occurrence.
pub(crate) fn check_void_type(
    type_expr: &TypeExpr,
    context: String,
    span: TextRange,
    allow_root_void: bool,
    diags: &mut Vec<LoweringDiagnostic>,
) {
    match type_expr {
        TypeExpr::Void { .. } => {
            if !allow_root_void {
                diags.push(LoweringDiagnostic::VoidInNonReturnPosition { context, span });
            }
        }
        TypeExpr::Optional { inner, .. } => {
            // Once inside a wrapper, void is never allowed (even in return position).
            check_void_type(
                inner,
                "an optional type (`void?`)".to_string(),
                span,
                false,
                diags,
            );
        }
        TypeExpr::List { inner, .. } => {
            check_void_type(
                inner,
                "a list type (`void[]`)".to_string(),
                span,
                false,
                diags,
            );
        }
        TypeExpr::Map { key, value, .. } => {
            check_void_type(key, "a map key type".to_string(), span, false, diags);
            check_void_type(value, "a map value type".to_string(), span, false, diags);
        }
        TypeExpr::Union { variants, .. } => {
            for v in variants {
                check_void_type(v, "a union member".to_string(), span, false, diags);
            }
        }
        TypeExpr::Function {
            params,
            ret: _,
            throws,
            ..
        } => {
            // ret is exempt — void IS allowed as function-type return type.
            // But void in param types is not allowed.
            for p in params {
                check_void_type(&p.ty, context.clone(), span, false, diags);
            }
            if let Some(throws) = throws {
                check_void_type(throws, "a throws type".to_string(), span, false, diags);
            }
        }
        // All other variants (primitives, path, etc.) cannot contain void.
        _ => {}
    }
}
