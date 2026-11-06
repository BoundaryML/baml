//! Type-expression rules shared by every reader: the named-type table
//! (`lower_from_type_name_with_generic_args`) and the wildcard checks.
//!
//! Types carry no attributes (BEP-075); `@` attributes belong to declarations.

use baml_base::Name;
use text_size::TextRange;

use crate::{
    LoweringDiagnostic,
    ast::{AssociatedTypeBinding, TypeExpr, TypeExprKind},
};

pub(crate) fn lower_from_type_name_with_generic_args(
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
        path: baml_base::Path::new(name.split('.').map(Name::new).collect()),
        generic_args,
        associated_type_bindings,
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
            return TypeExprKind::Unknown.at(span);
        }
        match alias.target {
            AliasTarget::Primitive(primitive) => match primitive {
                P::Int => TypeExprKind::Int,
                P::Bigint => TypeExprKind::Bigint,
                P::Float => TypeExprKind::Float,
                P::String => TypeExprKind::String,
                P::Bool => TypeExprKind::Bool,
                P::Null => TypeExprKind::Null,
                P::Uint8Array => TypeExprKind::Uint8Array,
                P::Image | P::Audio | P::Video | P::Pdf => TypeExprKind::Media {
                    kind: match primitive {
                        P::Image => baml_base::MediaKind::Image,
                        P::Audio => baml_base::MediaKind::Audio,
                        P::Video => baml_base::MediaKind::Video,
                        P::Pdf => baml_base::MediaKind::Pdf,
                        _ => unreachable!("media primitive"),
                    },
                },
            },
            AliasTarget::Never => TypeExprKind::Never,
            // `void` is the unit type's second spelling: it denotes `null` in
            // every position, so it lowers to the very same node.
            AliasTarget::Void => TypeExprKind::Null,
            AliasTarget::Unknown => TypeExprKind::Unknown,
            AliasTarget::Type => TypeExprKind::Type,
            AliasTarget::Map => {
                let value = generic_args.pop().expect("checked map arity");
                let key = generic_args.pop().expect("checked map arity");
                TypeExprKind::Map {
                    key: Box::new(key),
                    value: Box::new(value),
                }
            }
            AliasTarget::Json => {
                let definition = alias.definition.expect("json has an alias declaration");
                TypeExprKind::Path {
                    path: definition.source_path(),
                    generic_args,
                    associated_type_bindings,
                }
            }
            AliasTarget::Future => path(generic_args, associated_type_bindings),
            AliasTarget::List => unreachable!("arrays use postfix syntax"),
        }
    } else {
        match name {
            "_" => TypeExprKind::Infer,
            "$rust_type" => TypeExprKind::Rust,
            _ => path(generic_args, associated_type_bindings),
        }
    };
    kind.at(span)
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
        TypeExprKind::Infer => {}
        TypeExprKind::Union { variants, .. } => {
            for v in variants {
                // A top-level `_` member is the open slot (kept); anything
                // deeper is a non-inferable nested hole (rewritten to an error).
                if !matches!(v.kind, TypeExprKind::Infer) {
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
        TypeExprKind::Infer => {
            diags.push(LoweringDiagnostic::WildcardTypeNotAllowed {
                context: context.to_string(),
                span,
            });
            // Neutralize: a hole must never survive into TIR type checking.
            type_expr.kind = TypeExprKind::Error;
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
