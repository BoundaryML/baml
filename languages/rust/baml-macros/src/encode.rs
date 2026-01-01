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
    let mut dynamic_field_name: Option<syn::Ident> = None;

    for field in &named_fields.named {
        let field_attrs = FieldAttrs::from_attrs(&field.attrs)?;

        if field_attrs.skip {
            continue;
        }

        if field_attrs.dynamic_fields {
            // Remember this field for flattening later
            dynamic_field_name = Some(field.ident.as_ref().unwrap().clone());
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

    // Generate the encode implementation
    let encode_body = if let Some(dyn_field) = dynamic_field_name {
        // Flatten dynamic fields into the encoding
        quote! {
            let mut fields: Vec<(&str, #baml_crate::__internal::HostValue)> = vec![#(#field_encodings),*];
            // Flatten dynamic fields
            for (key, value) in &self.#dyn_field {
                fields.push((key.as_str(), #baml_crate::BamlEncode::baml_encode(value)));
            }
            #baml_crate::encode_class_dynamic(#baml_name, fields)
        }
    } else {
        quote! {
            #baml_crate::encode_class(
                #baml_name,
                vec![#(#field_encodings),*]
            )
        }
    };

    Ok(quote! {
        impl #baml_crate::BamlEncode for #type_name {
            fn baml_encode(&self) -> #baml_crate::__internal::HostValue {
                #encode_body
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

        if variant_attrs.dynamic_variant {
            // Handle _Dynamic(String) variant
            match &variant.fields {
                Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                    variant_arms.push(quote! {
                        Self::#variant_name(s) => #baml_crate::encode_enum(#baml_name, s)
                    });
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        variant,
                        "#[baml(dynamic_variant)] must be a single-field tuple variant",
                    ));
                }
            }
            continue;
        }

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
