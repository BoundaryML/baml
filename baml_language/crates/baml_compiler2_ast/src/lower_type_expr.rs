//! CST `TypeExpr` node → `ast::TypeExpr` recursive enum.
//!
//! Adapts the logic from `TypeRef::from_ast()` in `baml_compiler_hir/src/type_ref.rs`.
//! The output is the same recursive structure but as `ast::TypeExpr` instead of `TypeRef`.
//!
//! Each `TypeExpr` variant carries `attrs: Vec<RawAttribute>` populated from
//! ATTRIBUTE children of the corresponding CST `TYPE_EXPR` node.

use baml_base::Name;
use baml_compiler_syntax::{
    FunctionTypeParam, SyntaxKind, SyntaxNodeExt, ast::TypeExpr as CstTypeExpr,
};
use rowan::ast::AstNode;
use text_size::TextRange;

use crate::{
    LoweringDiagnostic,
    ast::{
        AssociatedTypeBinding, FunctionTypeParam as AstFunctionTypeParam, RawAttribute, TypeExpr,
        TypeExprKind,
    },
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
pub(crate) fn lower_type_expr_node(
    type_expr: &CstTypeExpr,
    diags: &mut Vec<LoweringDiagnostic>,
) -> TypeExpr {
    lower_type_expr_inner(type_expr, true, diags)
}

/// Inner recursive lowering. `hoist_trailing` controls whether trailing attrs
/// after the last PIPE in a union are moved to the Union node (true only at
/// the outermost level).
fn lower_type_expr_inner(
    type_expr: &CstTypeExpr,
    hoist_trailing: bool,
    diags: &mut Vec<LoweringDiagnostic>,
) -> TypeExpr {
    // For unions, use the specialized path that distributes attrs per-member
    if type_expr.is_union() {
        return lower_union_with_attrs(type_expr, hoist_trailing, diags);
    }

    let attrs = collect_type_attrs(type_expr);
    let base = lower_base(type_expr, diags);
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
fn lower_union_with_attrs(
    type_expr: &CstTypeExpr,
    hoist_trailing: bool,
    diags: &mut Vec<LoweringDiagnostic>,
) -> TypeExpr {
    let member_parts = type_expr.union_member_parts();

    // Lower each member — this puts per-member attrs on each variant
    let mut variants: Vec<TypeExpr> = member_parts
        .iter()
        .map(|m| lower_union_member(m, diags))
        .collect();

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
                        trailing_attr_spans.push(node.span_range());
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
        TypeExprKind::Union {
            variants,
            attrs: union_attrs,
        }
        .at(type_expr.syntax().span_range()),
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
        let span = result.span;
        match modifier {
            baml_compiler_syntax::ast::TypePostFixModifier::Optional => {
                result = TypeExprKind::Optional {
                    inner: Box::new(result),
                    attrs: vec![],
                }
                .at(span);
            }
            baml_compiler_syntax::ast::TypePostFixModifier::Array => {
                result = TypeExprKind::List {
                    inner: Box::new(result),
                    attrs: vec![],
                }
                .at(span);
            }
        }
    }
    result
}

/// Extract the base type (function types, parens, terminals).
/// No modifier or attr handling. Unions are handled by `lower_union_with_attrs`.
fn lower_base(type_expr: &CstTypeExpr, diags: &mut Vec<LoweringDiagnostic>) -> TypeExpr {
    lower_base_terminal(type_expr, diags)
}

/// Parse the base type (no modifiers, not a union).
fn lower_base_terminal(type_expr: &CstTypeExpr, diags: &mut Vec<LoweringDiagnostic>) -> TypeExpr {
    let span = type_expr.syntax().span_range();
    if let Some(unreflect) = type_expr
        .syntax()
        .children()
        .find(|node| node.kind() == SyntaxKind::UNREFLECT_TYPE)
    {
        return TypeExprKind::Unreflect {
            operand: None,
            attrs: vec![],
        }
        .at(unreflect.span_range());
    }
    // BUG: a qualified projection captures only a single member — `(base as I).A.B`
    // drops the trailing `.B` (`associated_type_projection` returns one member). A chained
    // explicit qualifier (`(T as Outer).Asdf.Assoc`) therefore silently loses `.Assoc`,
    // unlike the unqualified `T.Asdf.Assoc`. Fixing this is a grammar/CST change: parse the
    // full member chain after `(base as I)` and fold it into nested projections here.
    if let Some((base, interface, member)) = type_expr.associated_type_projection() {
        return TypeExprKind::AssociatedTypeProjection {
            base: Box::new(lower_type_expr_inner(&base, false, diags)),
            interface: Some(Box::new(lower_type_expr_inner(&interface, false, diags))),
            member: Name::new(member.text()),
            attrs: vec![],
        }
        .at(span);
    }

    // Handle function types like `(x: int, y: int) -> bool`. A function type
    // cannot declare its own generic parameters (rejected by the parser); any
    // leading `<...>` is left in the CST for recovery and ignored here.
    if type_expr.is_function_type() {
        let params = type_expr
            .function_type_params()
            .iter()
            .map(|p| {
                let name = p.name().map(|s| Name::new(&s));
                let optional = p.is_optional();
                let ty = p
                    .ty()
                    .map(|t| lower_type_expr_inner(&t, false, diags))
                    .unwrap_or_else(|| TypeExprKind::Missing { attrs: vec![] }.at(span));
                AstFunctionTypeParam { name, optional, ty }
            })
            .collect();
        let ret = type_expr
            .function_return_type()
            .map(|t| lower_type_expr_inner(&t, false, diags))
            .unwrap_or_else(|| TypeExprKind::Missing { attrs: vec![] }.at(span));
        let throws = type_expr
            .function_throws_type()
            .map(|t| Box::new(lower_type_expr_inner(&t, false, diags)));
        return TypeExprKind::Function {
            params,
            ret: Box::new(ret),
            throws,
            attrs: vec![],
        }
        .at(span);
    }

    // Handle parenthesized types like `(int | string)`
    if let Some(inner) = type_expr.inner_type_expr() {
        // For parenthesized types, attrs go on the inner type via recursive lowering.
        // If the outer node had attrs collected, we'd need to merge, but in practice
        // the parser puts attrs at the outermost level.
        return lower_type_expr_inner(&inner, false, diags);
    }

    // Handle parenthesized unions: `(A | B)` where the union is inside parens
    if type_expr.is_parenthesized() && !type_expr.is_function_type() {
        let params = type_expr.function_type_params();
        if params.len() > 1 {
            let members: Vec<TypeExpr> = params
                .iter()
                .filter_map(FunctionTypeParam::ty)
                .map(|t| lower_type_expr_inner(&t, false, diags))
                .collect();
            if !members.is_empty() {
                return TypeExprKind::Union {
                    variants: members,
                    attrs: vec![],
                }
                .at(span);
            }
        }
    }

    lower_base_type(type_expr, diags)
}

fn lower_base_type(type_expr: &CstTypeExpr, diags: &mut Vec<LoweringDiagnostic>) -> TypeExpr {
    let span = type_expr.syntax().span_range();
    if let Some(s) = type_expr.string_literal() {
        return TypeExprKind::Literal {
            value: baml_base::Literal::String(s),
            attrs: vec![],
        }
        .at(span);
    }

    if let Some((negated, tok)) = type_expr.bigint_literal() {
        let value = crate::lower_bigint_literal(tok.text(), tok.text_range(), diags);
        return TypeExprKind::Literal {
            value: baml_base::Literal::Bigint(if negated { -value } else { value }),
            attrs: vec![],
        }
        .at(span);
    }

    if let Some((negated, tok)) = type_expr.integer_literal() {
        let value = crate::lower_int_literal(tok.text(), tok.text_range(), diags);
        return TypeExprKind::Literal {
            value: baml_base::Literal::Int(if negated { -value } else { value }),
            attrs: vec![],
        }
        .at(span);
    }

    if let Some((negated, tok)) = type_expr.float_literal() {
        let text = baml_base::num_lit::normalize_float_literal(tok.text());
        return TypeExprKind::Literal {
            value: baml_base::Literal::Float(if negated { format!("-{text}") } else { text }),
            attrs: vec![],
        }
        .at(span);
    }

    if let Some(b) = type_expr.bool_literal() {
        return TypeExprKind::Literal {
            value: baml_base::Literal::Bool(b),
            attrs: vec![],
        }
        .at(span);
    }

    // Check for map type with type args
    if let Some(name) = type_expr.dotted_name() {
        let args = type_expr.type_arg_exprs();
        let associated_type_bindings = type_expr
            .type_arg_associated_bindings()
            .iter()
            .filter_map(|b| lower_associated_type_binding(b, diags))
            .collect();
        if name == "map" && args.len() == 2 {
            let key = lower_type_expr_inner(&args[0], false, diags);
            let value = lower_type_expr_inner(&args[1], false, diags);
            return TypeExprKind::Map {
                key: Box::new(key),
                value: Box::new(value),
                attrs: vec![],
            }
            .at(span);
        }

        // Named type (primitive or user-defined), preserving generic args
        let generic_args: Vec<TypeExpr> = args
            .iter()
            .map(|arg| lower_type_expr_inner(arg, false, diags))
            .collect();
        return lower_from_type_name_with_generic_args(
            &name,
            generic_args,
            associated_type_bindings,
            span,
        );
    }

    TypeExprKind::Missing { attrs: vec![] }.at(span)
}

/// Parse a union member from its structured parts.
fn lower_union_member(
    parts: &baml_compiler_syntax::ast::UnionMemberParts,
    diags: &mut Vec<LoweringDiagnostic>,
) -> TypeExpr {
    // Collect attributes from the union member's CST subtree
    let attrs: Vec<RawAttribute> = parts
        .attributes()
        .filter_map(|attr| lower_attribute(&attr))
        .collect();

    let base = lower_union_member_base(parts, diags);
    let mut result = apply_modifiers(base, &parts.postfix_modifiers());
    result.attrs_mut().extend(attrs);
    result
}

/// Extract the base type from union member parts (no modifiers or attrs).
fn lower_union_member_base(
    parts: &baml_compiler_syntax::ast::UnionMemberParts,
    diags: &mut Vec<LoweringDiagnostic>,
) -> TypeExpr {
    let span = parts.span().unwrap_or_default();
    if let Some(unreflect) = parts
        .child_nodes
        .iter()
        .find(|node| node.kind() == SyntaxKind::UNREFLECT_TYPE)
    {
        return TypeExprKind::Unreflect {
            operand: None,
            attrs: vec![],
        }
        .at(unreflect.span_range());
    }
    if let Some((base, interface, member)) = parts.associated_type_projection() {
        return TypeExprKind::AssociatedTypeProjection {
            base: Box::new(lower_type_expr_inner(&base, false, diags)),
            interface: Some(Box::new(lower_type_expr_inner(&interface, false, diags))),
            member: Name::new(member.text()),
            attrs: vec![],
        }
        .at(span);
    }

    // Check for parenthesized type first (e.g., `(int | string)` in `A | (int | string)`)
    if let Some(type_expr) = parts.type_expr() {
        return lower_type_expr_inner(&type_expr, false, diags);
    }

    // Check for FUNCTION_TYPE_PARAM child (new parser structure for parenthesized types)
    if let Some(func_param) = parts.function_type_param() {
        if let Some(inner_type_expr) = func_param
            .children()
            .find(|n| n.kind() == baml_compiler_syntax::SyntaxKind::TYPE_EXPR)
        {
            if let Some(type_expr) = baml_compiler_syntax::ast::TypeExpr::cast(inner_type_expr) {
                return lower_type_expr_inner(&type_expr, false, diags);
            }
        }
    }

    if let Some(s) = parts.string_literal() {
        return TypeExprKind::Literal {
            value: baml_base::Literal::String(s),
            attrs: vec![],
        }
        .at(span);
    }

    if let Some((negated, tok)) = parts.bigint_literal() {
        let value = crate::lower_bigint_literal(tok.text(), tok.text_range(), diags);
        return TypeExprKind::Literal {
            value: baml_base::Literal::Bigint(if negated { -value } else { value }),
            attrs: vec![],
        }
        .at(span);
    }

    if let Some((negated, tok)) = parts.integer_literal() {
        let value = crate::lower_int_literal(tok.text(), tok.text_range(), diags);
        return TypeExprKind::Literal {
            value: baml_base::Literal::Int(if negated { -value } else { value }),
            attrs: vec![],
        }
        .at(span);
    }

    if let Some((negated, tok)) = parts.float_literal() {
        let text = baml_base::num_lit::normalize_float_literal(tok.text());
        return TypeExprKind::Literal {
            value: baml_base::Literal::Float(if negated { format!("-{text}") } else { text }),
            attrs: vec![],
        }
        .at(span);
    }

    // Check for named/primitive type or map type
    if let Some(name) = parts.dotted_name() {
        let (type_arg_exprs, associated_type_bindings): (Vec<_>, Vec<_>) = parts
            .type_args()
            .map(|type_args_node| {
                let type_args = type_args_node
                    .children()
                    .filter(|n| n.kind() == baml_compiler_syntax::SyntaxKind::TYPE_EXPR)
                    .map(|n| baml_compiler_syntax::ast::TypeExpr::cast(n).unwrap())
                    .collect();
                let associated_bindings = type_args_node
                    .children()
                    .filter_map(baml_compiler_syntax::ast::AssociatedTypeDecl::cast)
                    .filter_map(|binding| lower_associated_type_binding(&binding, diags))
                    .collect();
                (type_args, associated_bindings)
            })
            .unwrap_or_default();

        if name == "map" {
            if type_arg_exprs.len() == 2 {
                let key = lower_type_expr_inner(&type_arg_exprs[0], false, diags);
                let value = lower_type_expr_inner(&type_arg_exprs[1], false, diags);
                return TypeExprKind::Map {
                    key: Box::new(key),
                    value: Box::new(value),
                    attrs: vec![],
                }
                .at(span);
            }
        }

        let generic_args: Vec<TypeExpr> = type_arg_exprs
            .iter()
            .map(|arg| lower_type_expr_inner(arg, false, diags))
            .collect();

        return match name.as_str() {
            "true" => TypeExprKind::Literal {
                value: baml_base::Literal::Bool(true),
                attrs: vec![],
            }
            .at(span),
            "false" => TypeExprKind::Literal {
                value: baml_base::Literal::Bool(false),
                attrs: vec![],
            }
            .at(span),
            _ => lower_from_type_name_with_generic_args(
                &name,
                generic_args,
                associated_type_bindings,
                span,
            ),
        };
    }

    TypeExprKind::Missing { attrs: vec![] }.at(span)
}

/// Create a `TypeExpr` from a type name string with optional generic arguments.
///
/// Generic args are preserved on `Path` types (e.g., `Stream<T>`). For primitive
/// types, generic args are silently dropped (primitives can't be generic).
pub(crate) fn lower_associated_type_binding(
    binding: &baml_compiler_syntax::ast::AssociatedTypeDecl,
    diags: &mut Vec<LoweringDiagnostic>,
) -> Option<AssociatedTypeBinding> {
    let name = binding.name()?;
    let ty = binding
        .default_or_binding()
        .map(|ty| lower_type_expr_inner(&ty, false, diags))
        .unwrap_or_else(|| TypeExprKind::Missing { attrs: vec![] }.at(TextRange::default()));
    Some(AssociatedTypeBinding {
        name: Name::new(name.text()),
        ty: Box::new(ty),
    })
}

fn lower_from_type_name_with_generic_args(
    name: &str,
    generic_args: Vec<TypeExpr>,
    associated_type_bindings: Vec<AssociatedTypeBinding>,
    span: TextRange,
) -> TypeExpr {
    let kind = match name {
        "int" => TypeExprKind::Int { attrs: vec![] },
        "bigint" => TypeExprKind::Bigint { attrs: vec![] },
        "float" => TypeExprKind::Float { attrs: vec![] },
        "string" => TypeExprKind::String { attrs: vec![] },
        "bool" => TypeExprKind::Bool { attrs: vec![] },
        "null" => TypeExprKind::Null { attrs: vec![] },
        "never" => TypeExprKind::Never { attrs: vec![] },
        "void" => TypeExprKind::Void { attrs: vec![] },
        "unknown" => TypeExprKind::Unknown { attrs: vec![] },
        // The wildcard `_` is an inference hole, not a named type. Any stray
        // generic args on it (`_<...>`, nonsensical) are dropped.
        "_" => TypeExprKind::Infer { attrs: vec![] },
        // `reflect.Type` is the source spelling of the runtime metatype. Keep
        // the dedicated AST node so the VM representation remains a compiler
        // primitive rather than an ordinary class instance.
        "reflect.Type" => TypeExprKind::Type { attrs: vec![] },
        "$rust_type" => TypeExprKind::Rust { attrs: vec![] },
        "image" => TypeExprKind::Media {
            kind: baml_base::MediaKind::Image,
            attrs: vec![],
        },
        "audio" => TypeExprKind::Media {
            kind: baml_base::MediaKind::Audio,
            attrs: vec![],
        },
        "video" => TypeExprKind::Media {
            kind: baml_base::MediaKind::Video,
            attrs: vec![],
        },
        "pdf" => TypeExprKind::Media {
            kind: baml_base::MediaKind::Pdf,
            attrs: vec![],
        },
        "uint8array" => TypeExprKind::Uint8Array { attrs: vec![] },
        "json" => TypeExprKind::Path {
            segments: vec![Name::new("baml"), Name::new("json"), Name::new("json")],
            generic_args: vec![],
            associated_type_bindings: vec![],
            attrs: vec![],
        },
        _ => {
            let segments: Vec<Name> = if name.contains('.') {
                name.split('.').map(Name::new).collect()
            } else {
                vec![Name::new(name)]
            };
            TypeExprKind::Path {
                segments,
                generic_args,
                associated_type_bindings,
                attrs: vec![],
            }
        }
    };
    kind.at(span)
}

/// Recursively check that `void` does not appear in a non-return-type position.
///
/// `void` is only valid as the *bare* return type of a function. It must not
/// appear in parameter types, field types, union members, list/optional
/// wrappers, or function-type parameter positions. When `void` IS used as the
/// return type of a `TypeExprKind::Function`, it is exempt.
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
    match &type_expr.kind {
        TypeExprKind::Void { .. } if !allow_root_void => {
            diags.push(LoweringDiagnostic::VoidInNonReturnPosition { context, span });
        }
        TypeExprKind::Void { .. } => {}
        TypeExprKind::Optional { inner, .. } => {
            // Once inside a wrapper, void is never allowed (even in return position).
            check_void_type(
                inner,
                "an optional type (`void?`)".to_string(),
                span,
                false,
                diags,
            );
        }
        TypeExprKind::List { inner, .. } => {
            check_void_type(
                inner,
                "a list type (`void[]`)".to_string(),
                span,
                false,
                diags,
            );
        }
        TypeExprKind::Map { key, value, .. } => {
            check_void_type(key, "a map key type".to_string(), span, false, diags);
            check_void_type(value, "a map value type".to_string(), span, false, diags);
        }
        TypeExprKind::Union { variants, .. } => {
            for v in variants {
                check_void_type(v, "a union member".to_string(), span, false, diags);
            }
        }
        TypeExprKind::Function {
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

/// Validate `_` placement in a `throws` clause, neutralizing illegal holes.
///
/// `_` is the open-contract marker (`throws AppError | _` or bare `throws _`),
/// so it is allowed as a top-level member of the clause and is left in place
/// (the TIR firewall fills it from the inferred throw set). But a `_` *nested*
/// inside a thrown type (`throws Err<_>`) has nothing to infer from; it is
/// reported and rewritten to an error sentinel (see [`check_wildcard_type`]) so
/// it never reaches type checking as an inference hole.
pub(crate) fn check_throws_wildcard(
    type_expr: &mut TypeExpr,
    span: TextRange,
    diags: &mut Vec<LoweringDiagnostic>,
) {
    match &mut type_expr.kind {
        // Bare `throws _` — the whole error set is inferred; keep the hole.
        TypeExprKind::Infer { .. } => {}
        TypeExprKind::Union { variants, .. } => {
            for v in variants {
                // A top-level `_` member is the open slot (kept); anything
                // deeper is a non-inferable nested hole (rewritten to an error).
                if !matches!(v.kind, TypeExprKind::Infer { .. }) {
                    check_wildcard_type(v, "a `throws` clause", span, diags);
                }
            }
        }
        _ => check_wildcard_type(type_expr, "a `throws` clause", span, diags),
    }
}

/// Reject — and neutralize — the `_` wildcard type at a DECLARATION-site type
/// position, where it cannot be inferred.
///
/// `_` is an inference hole. This firewall governs only the declaration sites it
/// is applied to (a signature parameter/return, a field, an alias, a generic
/// bound): there is nothing local to infer a hole from there, so every
/// occurrence is reported AND rewritten to [`TypeExprKind::Error`]. It then
/// lowers to the error-recovery `Ty::Error` sentinel rather than an inference hole
/// — which would otherwise reach type normalization, where an inference hole has
/// no sound form (see `baml_type::normalize`).
///
/// An inference hole therefore reaches type checking only from the positions this
/// firewall does NOT cover, each of which fills-or-rejects the hole itself:
///   * a `let` binding annotation (filled from the initializer),
///   * a top-level `throws`-clause member (filled from the inferred throw set;
///     nested `throws` holes ARE firewalled, see `check_throws_wildcard`), and
///   * the expression-context type positions — a call turbofish, an object
///     construction, a generic-apply value, an upcast target — handled during
///     inference (see the expression-context `_` hole policy in
///     the retired TIR builder).
///
/// Emits `WildcardTypeNotAllowed` for every occurrence at any depth.
pub(crate) fn check_wildcard_type(
    type_expr: &mut TypeExpr,
    context: &str,
    span: TextRange,
    diags: &mut Vec<LoweringDiagnostic>,
) {
    match &mut type_expr.kind {
        TypeExprKind::Infer { .. } => {
            diags.push(LoweringDiagnostic::WildcardTypeNotAllowed {
                context: context.to_string(),
                span,
            });
            // Neutralize: a hole must never survive into TIR type checking.
            type_expr.kind = TypeExprKind::Error { attrs: vec![] };
        }
        TypeExprKind::Optional { inner, .. } | TypeExprKind::List { inner, .. } => {
            check_wildcard_type(inner, context, span, diags);
        }
        TypeExprKind::Map { key, value, .. } => {
            check_wildcard_type(key, context, span, diags);
            check_wildcard_type(value, context, span, diags);
        }
        TypeExprKind::Union { variants, .. } => {
            for v in variants {
                check_wildcard_type(v, context, span, diags);
            }
        }
        TypeExprKind::Path {
            generic_args,
            associated_type_bindings,
            ..
        } => {
            for arg in generic_args {
                check_wildcard_type(arg, context, span, diags);
            }
            for binding in associated_type_bindings {
                check_wildcard_type(&mut binding.ty, context, span, diags);
            }
        }
        TypeExprKind::Function {
            params,
            ret,
            throws,
            ..
        } => {
            for p in params {
                check_wildcard_type(&mut p.ty, context, span, diags);
            }
            check_wildcard_type(ret, context, span, diags);
            if let Some(throws) = throws {
                check_wildcard_type(throws, context, span, diags);
            }
        }
        TypeExprKind::AssociatedTypeProjection {
            base, interface, ..
        } => {
            check_wildcard_type(base, context, span, diags);
            if let Some(iface) = interface {
                check_wildcard_type(iface, context, span, diags);
            }
        }
        // Primitives and other leaves cannot contain a wildcard.
        _ => {}
    }
}
