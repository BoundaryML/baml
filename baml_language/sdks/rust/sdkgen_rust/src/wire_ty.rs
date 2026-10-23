//! `baml_sdkgen_types::Ty` → Rust expression building its wire `BamlTy`.
//!
//! The engine converts nothing: a list, map, class or callable node states
//! its exact type. A Rust type cannot spell every BAML type (a literal or
//! enum-variant type translates to its base Rust type), so the statement is
//! the declared BAML type, built here, rather than the Rust type's own
//! `baml_ty()`. A type variable has no BAML spelling of its own: the caller
//! says how each is spelled where the expression is emitted.

use baml_base::{Literal, MediaKind};
use baml_sdkgen_types::{CodegenFunctionParamMode, Ty};
use proc_macro2::TokenStream;
use quote::quote;

use crate::{
    translate_ty::{TyCtx, Unsupported},
    unions,
};

/// How a type variable's wire type is spelled where the expression is
/// emitted, or `None` for a variable not in scope there.
pub(crate) type TypeVarSpelling<'a> = &'a dyn Fn(&str) -> Option<TokenStream>;

/// Each type variable in `scope` is a Rust generic parameter of the emitted
/// item, spelled as the BAML type of the Rust type the item is instantiated
/// with.
pub(crate) fn own_type_var(scope: &[String]) -> impl Fn(&str) -> Option<TokenStream> + '_ {
    move |name| {
        scope.iter().any(|param| param == name).then(|| {
            let ident = crate::idents::ident(name);
            quote! { <#ident as ::baml_bridge::baml_value::internal::__BamlValuePrivate>::baml_ty() }
        })
    }
}

/// The expression building the wire `BamlTy` of declared type `ty`.
pub(crate) fn wire_ty(
    ty: &Ty,
    ctx: &TyCtx<'_>,
    type_var: TypeVarSpelling<'_>,
) -> Result<TokenStream, Unsupported> {
    let internal = quote! { ::baml_bridge::baml_value::internal };
    let primitive = |kind: TokenStream| {
        quote! { #internal::primitive_ty(::baml_bridge::wire::BamlTyPrimitiveKind::#kind) }
    };
    let leaf = |variant: TokenStream| {
        quote! { #internal::leaf_ty(::baml_bridge::wire::baml_ty::Ty::#variant) }
    };
    let each = |tys: &[Ty]| {
        tys.iter()
            .map(|ty| wire_ty(ty, ctx, type_var))
            .collect::<Result<Vec<_>, _>>()
    };
    Ok(match ty {
        Ty::Int => primitive(quote! { BamlTyPrimitiveInt }),
        Ty::Bigint => primitive(quote! { BamlTyPrimitiveBigint }),
        Ty::Float => primitive(quote! { BamlTyPrimitiveFloat }),
        Ty::String => primitive(quote! { BamlTyPrimitiveString }),
        Ty::Bool => primitive(quote! { BamlTyPrimitiveBool }),
        Ty::Null => primitive(quote! { BamlTyPrimitiveNull }),
        Ty::Uint8Array => primitive(quote! { BamlTyPrimitiveBytes }),
        Ty::Literal(literal, _) => {
            let literal = match literal {
                Literal::Int(value) => quote! { IntValue(#value) },
                Literal::Bool(value) => quote! { BoolValue(#value) },
                Literal::String(value) => quote! { StringValue(#value.to_string()) },
                // The wire spells bigint and float literals as their decimal
                // text.
                Literal::Bigint(value) => {
                    let value = value.to_string();
                    quote! { BigintValue(#value.to_string()) }
                }
                Literal::Float(value) => quote! { FloatValue(#value.to_string()) },
            };
            quote! { #internal::literal_ty(::baml_bridge::wire::baml_ty_literal::Literal::#literal) }
        }
        Ty::Media(kind) => {
            let kind = match kind {
                MediaKind::Image => quote! { Image },
                MediaKind::Audio => quote! { Audio },
                MediaKind::Video => quote! { Video },
                MediaKind::Pdf => quote! { Pdf },
                MediaKind::Generic => quote! { Generic },
            };
            quote! { #internal::media_ty(::baml_bridge::wire::BamlTyMediaKind::#kind) }
        }
        Ty::Class(name, args) => {
            let fqn = name.to_string();
            let args = each(args)?;
            quote! { #internal::class_ty(#fqn, ::std::vec![#(#args),*]) }
        }
        Ty::Interface(name, args, bindings) => {
            let fqn = name.to_string();
            let args = each(args)?;
            let bindings = bindings
                .iter()
                .map(|(binding, ty)| {
                    let binding = binding.as_str();
                    let ty = wire_ty(ty, ctx, type_var)?;
                    Ok(quote! { (#binding, #ty) })
                })
                .collect::<Result<Vec<_>, Unsupported>>()?;
            quote! {
                #internal::interface_ty(#fqn, ::std::vec![#(#args),*], ::std::vec![#(#bindings),*])
            }
        }
        Ty::Enum(name) => {
            let fqn = name.to_string();
            quote! { #internal::enum_ty(#fqn) }
        }
        Ty::EnumVariant(name, variant) => {
            let fqn = name.to_string();
            let variant = variant.as_str();
            quote! { #internal::enum_variant_ty(#fqn, #variant) }
        }
        Ty::List(item) => {
            let item = wire_ty(item, ctx, type_var)?;
            quote! { #internal::list_ty(#item) }
        }
        Ty::Map { key, value } => {
            let key = wire_ty(key, ctx, type_var)?;
            let value = wire_ty(value, ctx, type_var)?;
            quote! { #internal::map_ty(#key, #value) }
        }
        // Spelled as the engine spells a union: a member and `null` as the
        // member's optional, any other union by all its members in order.
        Ty::Union(members) => {
            let (non_null, nullable) = unions::strip_null(members);
            match (non_null.as_slice(), nullable) {
                ([], true) => primitive(quote! { BamlTyPrimitiveNull }),
                ([], false) => leaf(quote! { Never(::baml_bridge::wire::BamlTyNever {}) }),
                ([only], false) => wire_ty(only, ctx, type_var)?,
                ([only], true) => {
                    let inner = wire_ty(only, ctx, type_var)?;
                    quote! { #internal::optional_ty(#inner) }
                }
                _ => {
                    let members = each(members)?;
                    quote! { #internal::union_ty(::std::vec![#(#members),*]) }
                }
            }
        }
        Ty::Function {
            params,
            ret,
            throws,
        } => {
            let params = params
                .iter()
                .map(|param| {
                    let ty = wire_ty(&param.ty, ctx, type_var)?;
                    let name = match param.mode {
                        CodegenFunctionParamMode::Required => {
                            quote! { ::std::option::Option::None }
                        }
                        CodegenFunctionParamMode::Optional => {
                            let Some(name) = param.name.as_ref() else {
                                return Err(Unsupported {
                                    reason:
                                        "an optional callable parameter has no name to pass it by"
                                            .to_string(),
                                });
                            };
                            let name = name.as_str();
                            quote! { ::std::option::Option::Some(#name) }
                        }
                    };
                    Ok(quote! { #internal::function_param(#name, #ty) })
                })
                .collect::<Result<Vec<_>, Unsupported>>()?;
            let ret = wire_ty(ret, ctx, type_var)?;
            let throws = wire_ty(throws, ctx, type_var)?;
            quote! { #internal::function_ty(::std::vec![#(#params),*], #ret, #throws) }
        }
        Ty::Future(value, error) => {
            let value = wire_ty(value, ctx, type_var)?;
            let error = wire_ty(error, ctx, type_var)?;
            quote! { #internal::future_ty(#value, #error) }
        }
        Ty::RustType => leaf(quote! { RustType(::baml_bridge::wire::BamlTyRustType {}) }),
        Ty::Type => leaf(quote! { MetaType(::baml_bridge::wire::BamlTyMetaType {}) }),
        Ty::Resource => leaf(quote! { Resource(::baml_bridge::wire::BamlTyResource {}) }),
        Ty::PromptAst => leaf(quote! { PromptAst(::baml_bridge::wire::BamlTyPromptAst {}) }),
        Ty::Unknown => leaf(quote! { Unknown(::baml_bridge::wire::BamlTyUnknown {}) }),
        Ty::Never => leaf(quote! { Never(::baml_bridge::wire::BamlTyNever {}) }),
        // Only a recursive alias exists as a declaration at run time: the
        // stdlib's `json` is spelled by name, and a statement spells any other
        // emitted alias by its body.
        Ty::TypeAlias(name) if crate::translate_ty::is_canonical_json(name) => {
            let fqn = name.to_string();
            quote! { #internal::type_alias_ty(#fqn) }
        }
        Ty::TypeAlias(name) => {
            let Some(body) = ctx.analysis.alias_body(name) else {
                return Err(Unsupported {
                    reason: format!("references skipped or unknown type `{name}`"),
                });
            };
            wire_ty(body, ctx, type_var)?
        }
        Ty::TypeVar(param) => type_var(param.as_str()).ok_or_else(|| Unsupported {
            reason: format!(
                "type variable `{}` (not a function type parameter)",
                param.as_str()
            ),
        })?,
    })
}
