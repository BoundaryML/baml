//! Implementation of `#[derive(BamlEncode)]`

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Result};

use crate::shared::{baml_crate_path, ContainerAttrs, FieldAttrs, VariantAttrs};

pub fn derive_encode(input: DeriveInput) -> Result<TokenStream> {
    let container_attrs = ContainerAttrs::from_attrs(&input.attrs)?;
    let type_name = &input.ident;
    let baml_name = container_attrs
        .name
        .unwrap_or_else(|| type_name.to_string());

    let baml_crate = baml_crate_path();

    match &input.data {
        Data::Struct(data) => {
            derive_struct_encode(type_name, &baml_name, &data.fields, &baml_crate)
        }
        Data::Enum(data) => {
            derive_enum_encode(type_name, &baml_name, data, &baml_crate, container_attrs.union)
        }
        Data::Union(_) => Err(syn::Error::new_spanned(
            &input,
            "BamlEncode cannot be derived for unions",
        )),
    }
}

fn derive_struct_encode(
    type_name: &syn::Ident,
    baml_name: &str,
    fields: &Fields,
    baml_crate: &TokenStream,
) -> Result<TokenStream> {
    let Fields::Named(named_fields) = fields else {
        return Err(syn::Error::new_spanned(
            fields,
            "BamlEncode can only be derived for structs with named fields",
        ));
    };

    let mut field_encodings = Vec::new();

    for field in &named_fields.named {
        let field_attrs = FieldAttrs::from_attrs(&field.attrs)?;

        if field_attrs.skip {
            continue;
        }

        let field_name = field.ident.as_ref().unwrap();
        let baml_field_name = field_attrs
            .name
            .unwrap_or_else(|| field_name.to_string());

        field_encodings.push(quote! {
            (#baml_field_name, #baml_crate::BamlEncode::baml_encode(&self.#field_name))
        });
    }

    Ok(quote! {
        impl #baml_crate::BamlEncode for #type_name {
            fn baml_encode(&self) -> #baml_crate::__internal::HostValue {
                #baml_crate::encode_class(
                    #baml_name,
                    vec![#(#field_encodings),*]
                )
            }
        }
    })
}

fn derive_enum_encode(
    type_name: &syn::Ident,
    baml_name: &str,
    data: &syn::DataEnum,
    baml_crate: &TokenStream,
    is_union: bool,
) -> Result<TokenStream> {
    let mut variant_arms = Vec::new();

    for variant in &data.variants {
        let variant_attrs = VariantAttrs::from_attrs(&variant.attrs)?;
        let variant_name = &variant.ident;
        let baml_variant_name = variant_attrs
            .name
            .unwrap_or_else(|| variant_name.to_string());

        match &variant.fields {
            Fields::Unit => {
                variant_arms.push(quote! {
                    Self::#variant_name => #baml_crate::encode_enum(#baml_name, #baml_variant_name)
                });
            }
            Fields::Unnamed(fields) if is_union && fields.unnamed.len() == 1 => {
                // For union types, encode the inner value directly
                variant_arms.push(quote! {
                    Self::#variant_name(v) => #baml_crate::BamlEncode::baml_encode(v)
                });
            }
            _ => {
                return Err(syn::Error::new_spanned(
                    variant,
                    "BamlEncode for enums only supports unit variants (or single-field tuple variants with #[baml(union)])",
                ));
            }
        }
    }

    Ok(quote! {
        impl #baml_crate::BamlEncode for #type_name {
            fn baml_encode(&self) -> #baml_crate::__internal::HostValue {
                match self {
                    #(#variant_arms),*
                }
            }
        }
    })
}
