//! Enum emission: a fieldless Rust enum plus the value conversions.
//! Variant identifiers come from the BAML variant names; the wire carries
//! each variant's declared *value*, so both conversion directions match
//! on the value string, never the Rust identifier.
//!
//! The impl shape is pinned by the hand-written executable spec in
//! `bridge_rust/tests/conversions.rs`.

use baml_codegen_types::{Enum, Name};
use proc_macro2::TokenStream;
use quote::quote;

use super::function::doc_attrs;
use crate::idents;

pub(crate) fn emit(name: &Name, enum_: &Enum) -> TokenStream {
    let fqn = name.to_string();
    let ident = idents::ident(name.name().as_str());
    let docs = doc_attrs(enum_.docstring.as_deref());

    let mut variant_defs = Vec::new();
    let mut encode_arms = Vec::new();
    let mut decode_arms = Vec::new();
    for variant in &enum_.variants {
        let variant_ident = idents::ident(variant.name.as_str());
        let value = variant.value.as_str();
        let variant_docs = doc_attrs(variant.docstring.as_deref());
        variant_defs.push(quote! {
            #(#variant_docs)*
            #variant_ident
        });
        encode_arms.push(quote! { Self::#variant_ident => #value });
        decode_arms.push(quote! { #value => ::std::result::Result::Ok(Self::#variant_ident) });
    }

    quote! {
        #(#docs)*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum #ident {
            #(#variant_defs,)*
        }

        impl ::baml_bridge::baml_value::internal::__BamlValuePrivate for #ident {
            fn to_baml(&self) -> ::baml_bridge::wire::InboundValue {
                ::baml_bridge::encode::enum_value(#fqn, match self {
                    #(#encode_arms,)*
                })
            }

            fn from_baml(
                v: ::baml_bridge::wire::BamlOutboundValue,
            ) -> ::std::result::Result<Self, ::baml_bridge::DecodeError> {
                match ::baml_bridge::decode::enum_variant(v, #fqn)?.as_str() {
                    #(#decode_arms,)*
                    other => ::std::result::Result::Err(
                        ::baml_bridge::DecodeError::UnknownEnumVariant {
                            enum_fqn: #fqn,
                            got: other.to_string(),
                        },
                    ),
                }
            }

            fn baml_ty() -> ::baml_bridge::wire::BamlTy {
                ::baml_bridge::baml_value::internal::enum_ty(#fqn)
            }
        }
    }
}
