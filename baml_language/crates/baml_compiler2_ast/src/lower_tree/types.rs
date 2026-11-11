//! Type expressions: `AnyType` → `ast::TypeExpr`.

use baml_base::Literal;
use baml_syntax::{
    nodes::{
        AnyLiteralTypeValue, AnyType, AnyTypeArg, AssocTypeBinding, FunctionType, LiteralType,
        PathType, QualifiedPath, UnionType,
    },
    token::TokenKind,
    tree::{AnyNodeId, Id, Missing},
};

use super::View;
use crate::{
    LoweringDiagnostic,
    ast::{AssociatedTypeBinding, FunctionTypeParam, TypeExpr, TypeExprKind},
    lower_type_expr::lower_from_type_name_with_generic_args,
};

/// A type at a position where one is required; `Missing` where the text has
/// none, at the point it would have been.
pub fn lower_type_or_missing(
    cx: View<'_>,
    diags: &mut Vec<LoweringDiagnostic>,
    ty: Result<AnyType, Missing>,
) -> TypeExpr {
    match ty {
        Ok(ty) => lower_type(cx, diags, ty),
        Err(missing) => TypeExprKind::Missing.at(View::missing_range(missing)),
    }
}

pub fn lower_type(cx: View<'_>, diags: &mut Vec<LoweringDiagnostic>, ty: AnyType) -> TypeExpr {
    match ty {
        AnyType::Path(id) => lower_path(cx, diags, id),
        AnyType::Literal(id) => lower_literal(cx, diags, id),
        // Grouping only: the parens leave no trace.
        AnyType::Paren(id) => lower_type_or_missing(cx, diags, cx.node(id).inner),
        AnyType::Function(id) => lower_function(cx, diags, id),
        AnyType::Optional(id) => {
            let span = cx.range_of(id);
            let inner = lower_type(cx, diags, cx.node(id).inner);
            TypeExprKind::Optional {
                inner: Box::new(inner),
            }
            .at(span)
        }
        AnyType::Array(id) => {
            let span = cx.range_of(id);
            let inner = lower_type(cx, diags, cx.node(id).inner);
            TypeExprKind::List {
                inner: Box::new(inner),
            }
            .at(span)
        }
        AnyType::Union(id) => lower_union(cx, diags, id),
        AnyType::Qualified(id) => lower_qualified(cx, diags, id),
        // The parser reported it; a removed `unreflect(…)` is one too.
        AnyType::Bogus(id) => TypeExprKind::Error.at(cx.range_of(id)),
    }
}

/// `a.b.c<Args>`: the dotted name resolves to a primitive, a compiler alias or
/// a path, with its generic arguments and associated-type bindings.
fn lower_path(cx: View<'_>, diags: &mut Vec<LoweringDiagnostic>, id: Id<PathType>) -> TypeExpr {
    let node = *cx.node(id);
    let span = cx.range_of(id);
    let name = dotted_text(cx, node.head, node.segments);
    let mut generic_args = Vec::new();
    let mut bindings = Vec::new();
    if let Some(args) = node.args {
        for item in cx.tree.list(cx.node(args).args) {
            match item.item {
                AnyTypeArg::Type(ty) => generic_args.push(lower_type(cx, diags, ty)),
                AnyTypeArg::Binding(binding) => bindings.push(lower_binding(cx, diags, binding)),
                // Reported by the parser; not an argument.
                AnyTypeArg::Bogus(_) => {}
            }
        }
    }
    lower_from_type_name_with_generic_args(&name, generic_args, bindings, span, diags)
}

/// `Item = T` inside type arguments.
fn lower_binding(
    cx: View<'_>,
    diags: &mut Vec<LoweringDiagnostic>,
    id: Id<AssocTypeBinding>,
) -> AssociatedTypeBinding {
    let node = *cx.node(id);
    let ty = lower_type_or_missing(cx, diags, node.ty);
    AssociatedTypeBinding {
        name: cx.name(node.name),
        ty: Box::new(ty),
    }
}

/// `a.b.c` as text; a dangling `.` (reported) ends the name.
pub(crate) fn dotted_text(
    cx: View<'_>,
    head: Id<baml_syntax::nodes::Name>,
    segments: baml_syntax::tree::List<baml_syntax::nodes::PathSegment>,
) -> String {
    let mut text = cx.name(head).as_str().to_string();
    for segment in cx.tree.list(segments) {
        let Ok(name) = segment.name else { break };
        text.push('.');
        text.push_str(cx.name(name).as_str());
    }
    text
}

fn lower_literal(
    cx: View<'_>,
    diags: &mut Vec<LoweringDiagnostic>,
    id: Id<LiteralType>,
) -> TypeExpr {
    let node = *cx.node(id);
    let span = cx.range_of(id);
    let negated = node.minus.is_some();
    let value = match node.value {
        AnyLiteralTypeValue::Scalar(token) => {
            let text = cx.text(token.idx);
            let range = cx.token_range(token.idx);
            match cx.tree.tokens().kind(token.idx) {
                TokenKind::Integer => {
                    let value = crate::lower_int_literal(text, range, diags);
                    Literal::Int(if negated { -value } else { value })
                }
                TokenKind::Bigint => {
                    let value = crate::lower_bigint_literal(text, range, diags);
                    Literal::Bigint(if negated { -value } else { value })
                }
                TokenKind::Float => {
                    let text = baml_base::num_lit::normalize_float_literal(text);
                    Literal::Float(if negated { format!("-{text}") } else { text })
                }
                TokenKind::True => Literal::Bool(true),
                TokenKind::False => Literal::Bool(false),
                _ => Literal::String(string_token_value(text)),
            }
        }
        // A template as a literal type: its text between the backticks.
        AnyLiteralTypeValue::Backtick(id) => {
            Literal::String(backtick_interior(cx, AnyNodeId::of(id)).to_string())
        }
        AnyLiteralTypeValue::Bogus(id) => return TypeExprKind::Error.at(cx.range_of(id)),
    };
    TypeExprKind::Literal { value }.at(span)
}

/// The value of a string token: the text inside a closed pair of `"` with
/// escapes decoded; otherwise the text after the opening quotes.
pub(crate) fn string_token_value(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        baml_base::escape::unescape_string_literal(&trimmed[1..trimmed.len() - 1])
    } else {
        trimmed.trim_start_matches('"').to_string()
    }
}

/// The text of a template between its opener and closer.
fn backtick_interior(cx: View<'_>, id: AnyNodeId) -> &str {
    let text = cx.text_of(id);
    text.trim_start_matches('`').trim_end_matches('`')
}

fn lower_function(
    cx: View<'_>,
    diags: &mut Vec<LoweringDiagnostic>,
    id: Id<FunctionType>,
) -> TypeExpr {
    let node = *cx.node(id);
    let span = cx.range_of(id);
    let params: Vec<_> = cx
        .tree
        .list(node.params)
        .iter()
        .map(|item| item.item)
        .collect();
    let params = params
        .into_iter()
        .map(|param| {
            let param = *cx.node(param);
            let name = param.name.map(|named| cx.name(named.name));
            let optional = param.name.is_some_and(|named| named.question.is_some());
            let ty = lower_type_or_missing(cx, diags, param.ty);
            FunctionTypeParam { name, optional, ty }
        })
        .collect();
    let ret = lower_type_or_missing(cx, diags, node.ret);
    let throws = node
        .throws
        .and_then(|clause| cx.node(clause).ty.ok())
        .map(|ty| Box::new(lower_type(cx, diags, ty)));
    TypeExprKind::Function {
        params,
        ret: Box::new(ret),
        throws,
    }
    .at(span)
}

fn lower_union(cx: View<'_>, diags: &mut Vec<LoweringDiagnostic>, id: Id<UnionType>) -> TypeExpr {
    let node = *cx.node(id);
    let span = cx.range_of(id);
    let mut variants = vec![lower_type(cx, diags, node.first)];
    let arms: Vec<_> = cx.tree.list(node.arms.as_list()).to_vec();
    for arm in arms {
        // A `|` with nothing after it was reported; it is not a member.
        if let Ok(ty) = arm.ty {
            variants.push(lower_type(cx, diags, ty));
        }
    }
    TypeExprKind::Union { variants }.at(span)
}

/// `(Base as Interface).Member`.
fn lower_qualified(
    cx: View<'_>,
    diags: &mut Vec<LoweringDiagnostic>,
    id: Id<QualifiedPath>,
) -> TypeExpr {
    let node = *cx.node(id);
    let span = cx.range_of(id);
    let Ok(member) = node.member else {
        // With no member there is no projection; the parser reported it.
        return TypeExprKind::Missing.at(span);
    };
    let base = lower_type(cx, diags, node.base);
    let interface = lower_type_or_missing(cx, diags, node.iface);
    TypeExprKind::AssociatedTypeProjection {
        base: Box::new(base),
        interface: Some(Box::new(interface)),
        member: cx.name(member),
    }
    .at(span)
}
