use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Data, DataEnum, DeriveInput, Ident, Result};

use crate::shared::{ContainerAttrs, VariantAttrs};

pub(crate) fn derive_serde(input: &DeriveInput) -> Result<TokenStream> {
    let container_attrs = ContainerAttrs::from_attrs(&input.attrs)?;

    match &input.data {
        Data::Struct(..) => Err(syn::Error::new(
            Span::call_site(),
            "BamlSerde cannot be derived for structs, use `#[serde(flatten)]` on dynamic instead.",
        )),
        Data::Enum(data_enum) if container_attrs.union => {
            derive_serde_union(data_enum, &input.ident)
        }
        Data::Enum(..) => Err(syn::Error::new(
            Span::call_site(),
            "BamlSerde cannot be derived for enums, use `#[serde(untagged)]` on dynamic instead.",
        )),
        Data::Union(..) => Err(syn::Error::new(
            Span::call_site(),
            "Rust unions are not supported for this derive macro.",
        )),
    }
}

fn derive_serde_union(data: &DataEnum, ident: &Ident) -> Result<TokenStream> {
    let name = ident.to_string();

    let mut literal_serde = Vec::new();

    for variant in &data.variants {
        let variant_attrs = VariantAttrs::from_attrs(&variant.attrs)?;
        let variant_ident = &variant.ident;

        if let Some(ref literal_string) = variant_attrs.literal_string {
            literal_serde.push(quote! {
                #[allow(non_snake_case)]
                pub mod #variant_ident {
                    use ::baml::__internal::serde;
                    pub fn serialize<S: serde::Serializer>(serializer: S) -> Result<S::Ok, S::Error> {
                        serializer.serialize_str(#literal_string)
                    }
                    pub fn deserialize<'de, D: serde::Deserializer<'de>>(deserializer: D) -> ::std::result::Result<(), D::Error> {
                        let __value: String = serde::Deserialize::deserialize(deserializer)?;
                        if __value == #literal_string {
                            ::std::result::Result::Ok(())
                        } else {
                            ::std::result::Result::Err(serde::de::Error::custom("invalid literal"))
                        }
                    }
                }
            });
        } else if let Some(literal_int) = variant_attrs.literal_int {
            literal_serde.push(quote! {
                pub mod #variant_ident {
                    use ::baml::__internal::serde;
                    pub fn serialize<S: serde::Serializer>(serializer: S) -> ::std::result::Result<S::Ok, S::Error> {
                        serializer.serialize_i64(#literal_int)
                    }
                    pub fn deserialize<'de, D: serde::Deserializer<'de>>(deserializer: D) -> ::std::result::Result<(), D::Error> {
                        let __value: i64 = serde::Deserialize::deserialize(deserializer)?;
                        if __value == #literal_int {
                            ::std::result::Result::Ok(())
                        } else {
                            ::std::result::Result::Err(serde::de::Error::custom("invalid literal"))
                        }
                    }
                }
            });
        } else if let Some(literal_bool) = variant_attrs.literal_bool {
            literal_serde.push(quote! {
                pub mod #variant_ident {
                    use ::baml::__internal::serde;
                    pub fn serialize<S: serde::Serializer>(serializer: S) -> ::std::result::Result<S::Ok, S::Error> {
                        serializer.serialize_bool(#literal_bool)
                        }
                    pub fn deserialize<'de, D: serde::Deserializer<'de>>(deserializer: D) -> ::std::result::Result<(), D::Error> {
                        let __value: bool = serde::Deserialize::deserialize(deserializer)?;
                        if __value == #literal_bool {
                            ::std::result::Result::Ok(())
                        } else {
                            ::std::result::Result::Err(serde::de::Error::custom("invalid literal"))
                        }
                    }
                }
            });
        }
    }

    if literal_serde.is_empty() {
        return Ok(TokenStream::new());
    }

    let module_ident = syn::Ident::new(
        &format!("__baml_serde_union_literal_{}", name),
        ident.span(),
    );

    Ok(quote! {
        mod #module_ident {
            #(#literal_serde)*
        }
    })
}
