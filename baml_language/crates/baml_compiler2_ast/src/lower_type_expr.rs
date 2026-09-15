//! CST `TypeExpr` node → `ast::TypeExpr` recursive enum.
//!
//! Adapts the logic from `TypeRef::from_ast()` in `baml_compiler_hir/src/type_ref.rs`.
//! The output is the same recursive structure but as `ast::TypeExpr` instead of `TypeRef`.
//!
//! Source types carry no attributes (BEP-075), so every lowered node's `attrs` is empty.

use baml_base::Name;
use baml_compiler_syntax::{
    FunctionTypeParam, SyntaxKind, SyntaxNodeExt, ast::TypeExpr as CstTypeExpr,
};
use rowan::ast::AstNode;
use text_size::TextRange;

use crate::{
    LoweringDiagnostic,
    ast::{
        AssociatedTypeBinding, FunctionTypeParam as AstFunctionTypeParam, TypeExpr, TypeExprKind,
    },
    lowering_diagnostic::TypeExprOwner,
};

/// Convert a CST `TypeExpr` node to our `ast::TypeExpr` recursive enum.
///
/// Called by `lower_cst.rs` for field/alias/param/return-type positions.
pub(crate) fn lower_type_expr_node(
    type_expr: &CstTypeExpr,
    diags: &mut Vec<LoweringDiagnostic>,
    owner: TypeExprOwner,
) -> TypeExpr {
    if type_expr.is_union() {
        return lower_union(type_expr, diags, owner);
    }

    let base = lower_base(type_expr, diags, owner);
    apply_modifiers(base, &type_expr.postfix_modifiers())
}

/// Lower a union `TYPE_EXPR` member by member.
fn lower_union(
    type_expr: &CstTypeExpr,
    diags: &mut Vec<LoweringDiagnostic>,
    owner: TypeExprOwner,
) -> TypeExpr {
    let variants: Vec<TypeExpr> = type_expr
        .union_member_parts()
        .iter()
        .map(|m| lower_union_member(m, diags, owner))
        .collect();

    // Apply postfix modifiers (e.g., `(A | B)[]`)
    apply_modifiers(
        TypeExprKind::Union {
            variants,
            attrs: vec![],
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
/// No modifier handling. Unions are handled by `lower_union`.
fn lower_base(
    type_expr: &CstTypeExpr,
    diags: &mut Vec<LoweringDiagnostic>,
    owner: TypeExprOwner,
) -> TypeExpr {
    lower_base_terminal(type_expr, diags, owner)
}

/// Parse the base type (no modifiers, not a union).
fn lower_base_terminal(
    type_expr: &CstTypeExpr,
    diags: &mut Vec<LoweringDiagnostic>,
    owner: TypeExprOwner,
) -> TypeExpr {
    let span = type_expr.syntax().span_range();
    if let Some(unreflect) = type_expr
        .syntax()
        .children()
        .find(|node| node.kind() == SyntaxKind::UNREFLECT_TYPE)
    {
        // The parser admits `unreflect(expr)` as a type atom everywhere so
        // the removed inline spelling still recovers cleanly; only a body
        // `type T = unreflect(expr);` statement lowers it (and never through
        // this road). Every other position is the one diagnostic.
        diags.push(LoweringDiagnostic::UnreflectOutsideTypeBinding {
            span: unreflect.span_range(),
            owner,
        });
        return TypeExprKind::Error { attrs: vec![] }.at(unreflect.span_range());
    }
    // BUG: a qualified projection captures only a single member — `(base as I).A.B`
    // drops the trailing `.B` (`associated_type_projection` returns one member). A chained
    // explicit qualifier (`(T as Outer).Asdf.Assoc`) therefore silently loses `.Assoc`,
    // unlike the unqualified `T.Asdf.Assoc`. Fixing this is a grammar/CST change: parse the
    // full member chain after `(base as I)` and fold it into nested projections here.
    if let Some((base, interface, member)) = type_expr.associated_type_projection() {
        return TypeExprKind::AssociatedTypeProjection {
            base: Box::new(lower_type_expr_node(&base, diags, owner)),
            interface: Some(Box::new(lower_type_expr_node(&interface, diags, owner))),
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
                    .map(|t| lower_type_expr_node(&t, diags, owner))
                    .unwrap_or_else(|| TypeExprKind::Missing { attrs: vec![] }.at(span));
                AstFunctionTypeParam { name, optional, ty }
            })
            .collect();
        let ret = type_expr
            .function_return_type()
            .map(|t| lower_type_expr_node(&t, diags, owner))
            .unwrap_or_else(|| TypeExprKind::Missing { attrs: vec![] }.at(span));
        let throws = type_expr
            .function_throws_type()
            .map(|t| Box::new(lower_type_expr_node(&t, diags, owner)));
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
        return lower_type_expr_node(&inner, diags, owner);
    }

    // Handle parenthesized unions: `(A | B)` where the union is inside parens
    if type_expr.is_parenthesized() && !type_expr.is_function_type() {
        let params = type_expr.function_type_params();
        if params.len() > 1 {
            let members: Vec<TypeExpr> = params
                .iter()
                .filter_map(FunctionTypeParam::ty)
                .map(|t| lower_type_expr_node(&t, diags, owner))
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

    lower_base_type(type_expr, diags, owner)
}

fn lower_base_type(
    type_expr: &CstTypeExpr,
    diags: &mut Vec<LoweringDiagnostic>,
    owner: TypeExprOwner,
) -> TypeExpr {
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
            .filter_map(|b| lower_associated_type_binding(b, diags, owner))
            .collect();
        // Named type (primitive or user-defined), preserving generic args
        let generic_args: Vec<TypeExpr> = args
            .iter()
            .map(|arg| lower_type_expr_node(arg, diags, owner))
            .collect();
        return lower_from_type_name_with_generic_args(
            &name,
            generic_args,
            associated_type_bindings,
            span,
            diags,
        );
    }

    TypeExprKind::Missing { attrs: vec![] }.at(span)
}

/// Parse a union member from its structured parts.
fn lower_union_member(
    parts: &baml_compiler_syntax::ast::UnionMemberParts,
    diags: &mut Vec<LoweringDiagnostic>,
    owner: TypeExprOwner,
) -> TypeExpr {
    let base = lower_union_member_base(parts, diags, owner);
    apply_modifiers(base, &parts.postfix_modifiers())
}

/// Extract the base type from union member parts (no modifiers).
fn lower_union_member_base(
    parts: &baml_compiler_syntax::ast::UnionMemberParts,
    diags: &mut Vec<LoweringDiagnostic>,
    owner: TypeExprOwner,
) -> TypeExpr {
    let span = parts.span().unwrap_or_default();
    if let Some(unreflect) = parts
        .child_nodes
        .iter()
        .find(|node| node.kind() == SyntaxKind::UNREFLECT_TYPE)
    {
        diags.push(LoweringDiagnostic::UnreflectOutsideTypeBinding {
            span: unreflect.span_range(),
            owner,
        });
        return TypeExprKind::Error { attrs: vec![] }.at(unreflect.span_range());
    }
    if let Some((base, interface, member)) = parts.associated_type_projection() {
        return TypeExprKind::AssociatedTypeProjection {
            base: Box::new(lower_type_expr_node(&base, diags, owner)),
            interface: Some(Box::new(lower_type_expr_node(&interface, diags, owner))),
            member: Name::new(member.text()),
            attrs: vec![],
        }
        .at(span);
    }

    // Check for parenthesized type first (e.g., `(int | string)` in `A | (int | string)`)
    if let Some(type_expr) = parts.type_expr() {
        return lower_type_expr_node(&type_expr, diags, owner);
    }

    // Check for FUNCTION_TYPE_PARAM child (new parser structure for parenthesized types)
    if let Some(func_param) = parts.function_type_param() {
        if let Some(inner_type_expr) = func_param
            .children()
            .find(|n| n.kind() == baml_compiler_syntax::SyntaxKind::TYPE_EXPR)
        {
            if let Some(type_expr) = baml_compiler_syntax::ast::TypeExpr::cast(inner_type_expr) {
                return lower_type_expr_node(&type_expr, diags, owner);
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
                    .filter_map(|binding| lower_associated_type_binding(&binding, diags, owner))
                    .collect();
                (type_args, associated_bindings)
            })
            .unwrap_or_default();

        let generic_args: Vec<TypeExpr> = type_arg_exprs
            .iter()
            .map(|arg| lower_type_expr_node(arg, diags, owner))
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
                diags,
            ),
        };
    }

    TypeExprKind::Missing { attrs: vec![] }.at(span)
}

/// Create a `TypeExpr` from a type name string with optional generic arguments.
///
/// User-defined paths preserve their arguments. Compiler aliases check their
/// declared arity before lowering, so invalid arguments cannot disappear.
pub(crate) fn lower_associated_type_binding(
    binding: &baml_compiler_syntax::ast::AssociatedTypeDecl,
    diags: &mut Vec<LoweringDiagnostic>,
    owner: TypeExprOwner,
) -> Option<AssociatedTypeBinding> {
    let name = binding.name()?;
    let ty = binding
        .default_or_binding()
        .map(|ty| lower_type_expr_node(&ty, diags, owner))
        .unwrap_or_else(|| TypeExprKind::Missing { attrs: vec![] }.at(TextRange::default()));
    Some(AssociatedTypeBinding {
        name: Name::new(name.text()),
        ty: Box::new(ty),
    })
}

fn lower_from_type_name_with_generic_args(
    name: &str,
    mut generic_args: Vec<TypeExpr>,
    associated_type_bindings: Vec<AssociatedTypeBinding>,
    span: TextRange,
    diags: &mut Vec<LoweringDiagnostic>,
) -> TypeExpr {
    use baml_type::{
        PrimitiveType as P,
        compiler_aliases::{self, AliasTarget},
    };
    let path = |generic_args, associated_type_bindings| TypeExprKind::Path {
        segments: name.split('.').map(Name::new).collect(),
        generic_args,
        associated_type_bindings,
        attrs: vec![],
    };
    let kind = if let Some(alias) = compiler_aliases::by_spelling(name) {
        if generic_args.len() != alias.arity || !associated_type_bindings.is_empty() {
            diags.push(LoweringDiagnostic::InvalidBuiltinTypeArguments {
                name: name.to_owned(),
                expected: alias.arity,
                got: generic_args.len(),
                associated_bindings: associated_type_bindings.len(),
                span,
            });
            // The syntax parsed successfully. Recover without a second,
            // misleading "could not parse type expression" diagnostic.
            return TypeExprKind::Unknown { attrs: vec![] }.at(span);
        }
        match alias.target {
            AliasTarget::Primitive(primitive) => match primitive {
                P::Int => TypeExprKind::Int { attrs: vec![] },
                P::Bigint => TypeExprKind::Bigint { attrs: vec![] },
                P::Float => TypeExprKind::Float { attrs: vec![] },
                P::String => TypeExprKind::String { attrs: vec![] },
                P::Bool => TypeExprKind::Bool { attrs: vec![] },
                P::Null => TypeExprKind::Null { attrs: vec![] },
                P::Uint8Array => TypeExprKind::Uint8Array { attrs: vec![] },
                P::Image | P::Audio | P::Video | P::Pdf => TypeExprKind::Media {
                    kind: match primitive {
                        P::Image => baml_base::MediaKind::Image,
                        P::Audio => baml_base::MediaKind::Audio,
                        P::Video => baml_base::MediaKind::Video,
                        P::Pdf => baml_base::MediaKind::Pdf,
                        _ => unreachable!("media primitive"),
                    },
                    attrs: vec![],
                },
            },
            AliasTarget::Never => TypeExprKind::Never { attrs: vec![] },
            AliasTarget::Void => TypeExprKind::Void { attrs: vec![] },
            AliasTarget::Unknown => TypeExprKind::Unknown { attrs: vec![] },
            AliasTarget::Type => TypeExprKind::Type { attrs: vec![] },
            AliasTarget::Map => {
                let value = generic_args.pop().expect("checked map arity");
                let key = generic_args.pop().expect("checked map arity");
                TypeExprKind::Map {
                    key: Box::new(key),
                    value: Box::new(value),
                    attrs: vec![],
                }
            }
            AliasTarget::Json => {
                let definition = alias.definition.expect("json has an alias declaration");
                TypeExprKind::Path {
                    segments: definition.source_segments().map(Name::new).collect(),
                    generic_args,
                    associated_type_bindings,
                    attrs: vec![],
                }
            }
            AliasTarget::Future => path(generic_args, associated_type_bindings),
            AliasTarget::List => unreachable!("arrays use postfix syntax"),
        }
    } else {
        match name {
            "_" => TypeExprKind::Infer { attrs: vec![] },
            "$rust_type" => TypeExprKind::Rust { attrs: vec![] },
            _ => path(generic_args, associated_type_bindings),
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
