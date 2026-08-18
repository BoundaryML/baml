//! Synthesized union-enum emission: one variant per (null-stripped) arm,
//! `From` per concrete payload arm so call sites can pass bare arm values
//! through the generated `impl Into<_>` parameters, and trial-order decode.
//!
//! A union with a `TypeVar` arm (`T | string`) becomes a *generic* enum
//! (`TOrString<T>`). The `TypeVar` arm gets no `From` impl: a blanket
//! `impl<T> From<T>` would overlap every concrete arm's `From` (they
//! coincide when `T` is instantiated to that arm's type), which Rust's
//! coherence rejects — so those arms are constructed by variant.
//!
//! The impl shape is pinned by the hand-written executable spec in
//! `bridge_rust/tests/conversions.rs`.

use baml_codegen_types::Ty;
use proc_macro2::TokenStream;
use quote::quote;

use crate::{
    SkipKind, SkipWarning, idents,
    translate_ty::{self, TyCtx},
    unions::{UnionArmKind, UnionEnum},
};

/// Emit the enum + `From` impls + conversions for one synthesized union.
///
/// A translation failure here means the registry and the translator
/// disagree on the supported subset — a generator bug; the caller
/// escalates rather than skipping.
pub(crate) fn emit(union_enum: &UnionEnum, ctx: &TyCtx<'_>) -> Result<TokenStream, SkipWarning> {
    let ident = idents::ident(&union_enum.rust_name);

    // `<T: BamlValue, …>` on the enum + impl headers, `<T, …>` on the
    // referenced type. Empty for a fully concrete union.
    let param_idents: Vec<_> = union_enum
        .generic_params
        .iter()
        .map(|p| idents::ident(p))
        .collect();
    let (bounded_generics, type_generics) = if param_idents.is_empty() {
        (TokenStream::new(), TokenStream::new())
    } else {
        let bounded = param_idents
            .iter()
            .map(|id| quote! { #id: ::baml_bridge::BamlValue });
        (quote! { <#(#bounded),*> }, quote! { <#(#param_idents),*> })
    };

    let mut variant_defs = Vec::new();
    let mut from_impls = Vec::new();
    let mut encode_arms = Vec::new();
    let mut trial_decodes = Vec::new();
    let mut indexed_decodes = Vec::new();
    let mut baml_ty_options = Vec::new();
    let name_str = union_enum.rust_name.as_str();
    for arm in &union_enum.arms {
        let variant = idents::ident(&arm.variant);
        match &arm.kind {
            UnionArmKind::Payload(ty) => {
                let payload = translate_ty::translate(ty, ctx).map_err(|u| SkipWarning {
                    kind: SkipKind::Type,
                    fqn: union_enum.rust_name.clone(),
                    reason: format!(
                        "generator bug: registered union arm failed to translate: {}",
                        u.reason
                    ),
                })?;
                baml_ty_options.push(quote! {
                    <#payload as ::baml_bridge::baml_value::internal::__BamlValuePrivate>::baml_ty()
                });
                variant_defs.push(quote! { #variant(#payload) });
                // A `TypeVar` arm gets no `From` (coherence — see the module
                // docs); it is constructed by naming the variant.
                if !matches!(ty, Ty::TypeVar(..)) {
                    from_impls.push(quote! {
                        impl #bounded_generics ::std::convert::From<#payload>
                            for #ident #type_generics
                        {
                            fn from(value: #payload) -> Self {
                                Self::#variant(value)
                            }
                        }
                    });
                }
                encode_arms.push(quote! {
                    Self::#variant(value) => {
                        ::baml_bridge::baml_value::internal::annotate_selected_type(
                            ::baml_bridge::baml_value::internal::__BamlValuePrivate::to_baml(value),
                            <#payload as ::baml_bridge::baml_value::internal::__BamlValuePrivate>::baml_ty(),
                        )
                    }
                });
                indexed_decodes.push(quote! {
                    if ::baml_bridge::decode::selected_type_matches(
                        &<#payload as ::baml_bridge::baml_value::internal::__BamlValuePrivate>::baml_ty(),
                        &selected_type,
                    ) {
                        return <#payload as ::baml_bridge::baml_value::internal::__BamlValuePrivate>::from_baml(v)
                            .map(Self::#variant);
                    }
                });
                trial_decodes.push(quote! {
                    if let ::std::result::Result::Ok(value) =
                        <#payload as ::baml_bridge::baml_value::internal::__BamlValuePrivate>::from_baml(
                            v.clone(),
                        )
                    {
                        return ::std::result::Result::Ok(Self::#variant(value));
                    }
                });
            }
            UnionArmKind::StringLiteral(value) => {
                let value = value.as_str();
                baml_ty_options.push(quote! {
                    ::baml_bridge::baml_value::internal::literal_string_ty(#value)
                });
                variant_defs.push(quote! { #variant });
                encode_arms.push(quote! {
                    Self::#variant => {
                        ::baml_bridge::baml_value::internal::annotate_selected_type(
                            ::baml_bridge::baml_value::internal::__BamlValuePrivate::to_baml(
                                &<::std::string::String as ::std::convert::From<&str>>::from(#value),
                            ),
                            ::baml_bridge::baml_value::internal::literal_string_ty(#value),
                        )
                    }
                });
                indexed_decodes.push(quote! {
                    if ::baml_bridge::decode::selected_type_matches(
                        &::baml_bridge::baml_value::internal::literal_string_ty(#value),
                        &selected_type,
                    ) {
                        let value = <::std::string::String as ::baml_bridge::baml_value::internal::__BamlValuePrivate>::from_baml(v)?;
                        if value == #value {
                            return ::std::result::Result::Ok(Self::#variant);
                        }
                        return ::std::result::Result::Err(
                            ::baml_bridge::decode::no_union_arm(#name_str, &::baml_bridge::wire::BamlOutboundValue {
                                value: ::std::option::Option::Some(
                                    ::baml_bridge::wire::baml_outbound_value::Value::StringValue(value),
                                ),
                            }),
                        );
                    }
                });
                trial_decodes.push(quote! {
                    if let ::std::result::Result::Ok(value) =
                        <::std::string::String as ::baml_bridge::baml_value::internal::__BamlValuePrivate>::from_baml(
                            v.clone(),
                        )
                        && value == #value
                    {
                        return ::std::result::Result::Ok(Self::#variant);
                    }
                });
            }
        }
    }

    Ok(quote! {
        #[derive(Debug, Clone, PartialEq)]
        pub enum #ident #bounded_generics {
            #(#variant_defs,)*
        }

        #(#from_impls)*

        impl #bounded_generics ::baml_bridge::baml_value::internal::__BamlValuePrivate
            for #ident #type_generics
        {
            fn to_baml(&self) -> ::baml_bridge::wire::InboundValue {
                match self {
                    #(#encode_arms,)*
                }
            }

            fn from_baml(
                v: ::baml_bridge::wire::BamlOutboundValue,
            ) -> ::std::result::Result<Self, ::baml_bridge::DecodeError> {
                let (selected_type, v) = ::baml_bridge::decode::union_selected(v)?;
                if let ::std::option::Option::Some(selected_type) = selected_type {
                    #(#indexed_decodes)*
                    return ::std::result::Result::Err(
                        ::baml_bridge::decode::no_union_arm(#name_str, &v),
                    );
                }
                let v = ::baml_bridge::decode::unwrap(v);
                #(#trial_decodes)*
                ::std::result::Result::Err(::baml_bridge::decode::no_union_arm(#name_str, &v))
            }

            fn baml_ty() -> ::baml_bridge::wire::BamlTy {
                ::baml_bridge::baml_value::internal::union_ty(::std::vec![
                    #(#baml_ty_options,)*
                ])
            }
        }
    })
}
