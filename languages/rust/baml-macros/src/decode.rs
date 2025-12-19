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
        Data::Enum(data) => derive_enum_decode(type_name, &baml_name, data, &baml_crate),
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
    })
}

fn derive_enum_decode(
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

        // Only support unit variants for now
        match &variant.fields {
            Fields::Unit => {
                variant_arms.push(quote! {
                    #baml_variant_name => Ok(Self::#variant_name)
                });
            }
            _ => {
                return Err(syn::Error::new_spanned(
                    variant,
                    "BamlDecode for enums only supports unit variants",
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
