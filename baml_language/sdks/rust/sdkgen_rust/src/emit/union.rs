//! Synthesized union-enum emission: one variant per (null-stripped) arm,
//! `From` per payload arm so call sites can pass bare arm values through
//! the generated `impl Into<_>` parameters, and trial-order decode.
//!
//! The impl shape is pinned by the hand-written executable spec in
//! `bridge_rust/tests/conversions.rs`.

use proc_macro2::TokenStream;
use quote::quote;

use crate::{
    SkipWarning, idents,
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

    let mut variant_defs = Vec::new();
    let mut from_impls = Vec::new();
    let mut encode_arms = Vec::new();
    let mut trial_decodes = Vec::new();
    for arm in &union_enum.arms {
        let variant = idents::ident(&arm.variant);
        match &arm.kind {
            UnionArmKind::Payload(ty) => {
                let payload = translate_ty::translate(ty, ctx).map_err(|u| SkipWarning {
                    fqn: union_enum.rust_name.clone(),
                    reason: format!(
                        "generator bug: registered union arm failed to translate: {}",
                        u.reason
                    ),
                })?;
                variant_defs.push(quote! { #variant(#payload) });
                from_impls.push(quote! {
                    impl ::std::convert::From<#payload> for #ident {
                        fn from(value: #payload) -> Self {
                            Self::#variant(value)
                        }
                    }
                });
                encode_arms.push(quote! {
                    Self::#variant(value) => {
                        ::baml_bridge::baml_value::internal::__BamlValuePrivate::to_baml(value)
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
                variant_defs.push(quote! { #variant });
                encode_arms.push(quote! {
                    Self::#variant => {
                        ::baml_bridge::baml_value::internal::__BamlValuePrivate::to_baml(
                            &<::std::string::String as ::std::convert::From<&str>>::from(#value),
                        )
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

    let name_str = union_enum.rust_name.as_str();
    Ok(quote! {
        #[derive(Debug, Clone, PartialEq)]
        pub enum #ident {
            #(#variant_defs,)*
        }

        #(#from_impls)*

        impl ::baml_bridge::baml_value::internal::__BamlValuePrivate for #ident {
            fn to_baml(&self) -> ::baml_bridge::wire::InboundValue {
                match self {
                    #(#encode_arms,)*
                }
            }

            fn from_baml(
                v: ::baml_bridge::wire::BamlOutboundValue,
            ) -> ::std::result::Result<Self, ::baml_bridge::DecodeError> {
                let v = ::baml_bridge::decode::unwrap(v);
                #(#trial_decodes)*
                ::std::result::Result::Err(::baml_bridge::decode::no_union_arm(#name_str, &v))
            }
        }
    })
}
