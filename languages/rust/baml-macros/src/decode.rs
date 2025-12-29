//! Implementation of `#[derive(BamlDecode)]`

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Result, Type};

use crate::shared::{baml_crate_path, ContainerAttrs, FieldAttrs, VariantAttrs};

pub fn derive_decode(input: DeriveInput) -> Result<TokenStream> {
    let container_attrs = ContainerAttrs::from_attrs(&input.attrs)?;
    let type_name = &input.ident;
    let baml_name = container_attrs
        .name
        .unwrap_or_else(|| type_name.to_string());

    let baml_crate = baml_crate_path();

    match &input.data {
        Data::Struct(data) => {
            derive_struct_decode(type_name, &baml_name, &data.fields, &baml_crate)
        }
        Data::Enum(data) => {
            derive_enum_decode(type_name, &baml_name, data, &baml_crate, container_attrs.union)
        }
        Data::Union(_) => Err(syn::Error::new_spanned(
            &input,
            "BamlDecode cannot be derived for unions",
        )),
    }
}

fn derive_struct_decode(
    type_name: &syn::Ident,
    baml_name: &str,
    fields: &Fields,
    baml_crate: &TokenStream,
) -> Result<TokenStream> {
    let Fields::Named(named_fields) = fields else {
        return Err(syn::Error::new_spanned(
            fields,
            "BamlDecode can only be derived for structs with named fields",
        ));
    };

    let mut field_decodings = Vec::new();

    for field in &named_fields.named {
        let field_attrs = FieldAttrs::from_attrs(&field.attrs)?;
        let field_name = field.ident.as_ref().unwrap();
        let baml_field_name = field_attrs
            .name
            .unwrap_or_else(|| field_name.to_string());

        // Check if this is an Option type
        let is_optional = is_option_type(&field.ty);

        let decode_expr = if is_optional {
            quote! {
                #baml_crate::decode_optional_field(&class.fields, #baml_field_name)?
            }
        } else {
            quote! {
                #baml_crate::decode_field(&class.fields, #baml_field_name)?
            }
        };

        field_decodings.push(quote! {
            #field_name: #decode_expr
        });
    }

    Ok(quote! {
        impl #baml_crate::BamlClass for #type_name {
            const TYPE_NAME: &'static str = #baml_name;

            fn from_class_value(
                class: &#baml_crate::__internal::CffiValueClass
            ) -> ::core::result::Result<Self, #baml_crate::BamlError> {
                Ok(Self {
                    #(#field_decodings),*
                })
            }
        }

        impl #baml_crate::BamlDecode for #type_name {
            fn baml_decode(
                holder: &#baml_crate::__internal::CffiValueHolder
            ) -> ::core::result::Result<Self, #baml_crate::BamlError> {
                #baml_crate::__internal::decode_class::<Self>(holder)
            }
        }
    })
}

fn derive_enum_decode(
    type_name: &syn::Ident,
    baml_name: &str,
    data: &syn::DataEnum,
    baml_crate: &TokenStream,
    is_union: bool,
) -> Result<TokenStream> {
    if is_union {
        derive_union_decode(type_name, data, baml_crate)
    } else {
        derive_baml_enum_decode(type_name, baml_name, data, baml_crate)
    }
}

/// Generate BamlEnum impl for regular BAML enums (unit variants only)
fn derive_baml_enum_decode(
    type_name: &syn::Ident,
    baml_name: &str,
    data: &syn::DataEnum,
    baml_crate: &TokenStream,
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
                    #baml_variant_name => Ok(Self::#variant_name)
                });
            }
            _ => {
                return Err(syn::Error::new_spanned(
                    variant,
                    "BamlDecode for enums only supports unit variants (use #[baml(union)] for union types)",
                ));
            }
        }
    }

    Ok(quote! {
        impl #baml_crate::BamlEnum for #type_name {
            const ENUM_NAME: &'static str = #baml_name;

            fn from_variant_name(name: &str) -> ::core::result::Result<Self, #baml_crate::BamlError> {
                match name {
                    #(#variant_arms,)*
                    other => Err(#baml_crate::BamlError::internal(
                        format!("unknown variant '{}' for enum {}", other, #baml_name)
                    ))
                }
            }
        }

        impl #baml_crate::BamlDecode for #type_name {
            fn baml_decode(
                holder: &#baml_crate::__internal::CffiValueHolder
            ) -> ::core::result::Result<Self, #baml_crate::BamlError> {
                #baml_crate::decode_enum::<Self>(holder)
            }
        }
    })
}

/// Generate BamlDecode impl for BAML union types (single-field tuple variants)
fn derive_union_decode(
    type_name: &syn::Ident,
    data: &syn::DataEnum,
    baml_crate: &TokenStream,
) -> Result<TokenStream> {
    let mut decode_attempts = Vec::new();

    for variant in &data.variants {
        let variant_name = &variant.ident;

        match &variant.fields {
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                let field_type = &fields.unnamed.first().unwrap().ty;
                decode_attempts.push(quote! {
                    if let Ok(v) = <#field_type as #baml_crate::BamlDecode>::baml_decode(inner) {
                        return Ok(Self::#variant_name(v));
                    }
                });
            }
            _ => {
                return Err(syn::Error::new_spanned(
                    variant,
                    "BamlDecode for union types only supports single-field tuple variants",
                ));
            }
        }
    }

    let type_name_str = type_name.to_string();

    Ok(quote! {
        impl #baml_crate::BamlDecode for #type_name {
            fn baml_decode(
                holder: &#baml_crate::__internal::CffiValueHolder
            ) -> ::core::result::Result<Self, #baml_crate::BamlError> {
                // Extract the inner value from UnionVariantValue
                let inner = #baml_crate::__internal::extract_union_variant(holder)?;

                // Try each variant type in order
                #(#decode_attempts)*

                Err(#baml_crate::BamlError::internal(
                    format!("failed to decode any variant of union {}", #type_name_str)
                ))
            }
        }
    })
}

/// Check if a type is `Option<T>`
fn is_option_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Option";
        }
    }
    false
}
