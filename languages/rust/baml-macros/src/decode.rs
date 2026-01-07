//! Implementation of `#[derive(BamlDecode)]`

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Result};

use crate::shared::{baml_crate_path, ContainerAttrs, FieldAttrs, VariantAttrs};

pub(crate) fn derive_decode(input: &DeriveInput) -> Result<TokenStream> {
    let container_attrs = ContainerAttrs::from_attrs(&input.attrs)?;
    let type_name = &input.ident;
    let baml_name = container_attrs
        .name
        .unwrap_or_else(|| type_name.to_string());

    let baml_crate = baml_crate_path();

    match &input.data {
        Data::Struct(data) => derive_struct_decode(
            type_name,
            &baml_name,
            &data.fields,
            &baml_crate,
            container_attrs.dynamic,
        ),
        Data::Enum(data) => derive_enum_decode(
            type_name,
            &baml_name,
            data,
            &baml_crate,
            container_attrs.union,
            container_attrs.dynamic,
        ),
        Data::Union(_) => Err(syn::Error::new_spanned(
            input,
            "BamlDecode cannot be derived for unions",
        )),
    }
}

fn derive_struct_decode(
    type_name: &syn::Ident,
    baml_name: &str,
    fields: &Fields,
    baml_crate: &TokenStream,
    is_dynamic: bool,
) -> Result<TokenStream> {
    let Fields::Named(named_fields) = fields else {
        return Err(syn::Error::new_spanned(
            fields,
            "BamlDecode can only be derived for structs with named fields",
        ));
    };

    let mut field_decodings = Vec::new();
    let mut known_field_names = Vec::new();
    let mut dynamic_field_name: Option<syn::Ident> = None;

    for field in &named_fields.named {
        let field_attrs = FieldAttrs::from_attrs(&field.attrs)?;
        let field_name = field.ident.as_ref().unwrap();

        if field_attrs.dynamic_fields {
            // This is the __dynamic field - we'll populate it separately
            dynamic_field_name = Some(field_name.clone());
            continue;
        }

        let baml_field_name = field_attrs.name.unwrap_or_else(|| field_name.to_string());
        known_field_names.push(baml_field_name.clone());

        let decode_expr = quote! {
            #baml_crate::decode_field(&class.fields, #baml_field_name)?
        };

        field_decodings.push(quote! {
            #field_name: #decode_expr
        });
    }

    // Generate the dynamic field initialization if present
    if let Some(ref dyn_field) = dynamic_field_name {
        if !is_dynamic {
            return Err(syn::Error::new_spanned(
                dyn_field,
                "#[baml(dynamic_fields)] requires #[baml(dynamic)] on the struct",
            ));
        }

        let known_fields_array = known_field_names.iter().map(|n| quote! { #n });
        let dynamic_init = quote! {
            #dyn_field: {
                let known_fields: std::collections::HashSet<&str> = [#(#known_fields_array),*].into_iter().collect();
                let mut dynamic = std::collections::HashMap::new();
                for entry in &class.fields {
                    if !known_fields.contains(entry.key.as_str()) {
                        if let Some(value) = &entry.value {
                            dynamic.insert(
                                entry.key.clone(),
                                #baml_crate::BamlDecode::baml_decode(value)?
                            );
                        }
                    }
                }
                dynamic
            }
        };
        field_decodings.push(dynamic_init);
    } else if is_dynamic {
        return Err(syn::Error::new_spanned(
            type_name,
            "#[baml(dynamic)] struct requires a field with #[baml(dynamic_fields)]",
        ));
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
    is_dynamic: bool,
) -> Result<TokenStream> {
    if is_union {
        derive_union_decode(type_name, data, baml_crate)
    } else {
        derive_baml_enum_decode(type_name, baml_name, data, baml_crate, is_dynamic)
    }
}

/// Generate `BamlEnum` impl for regular BAML enums (unit variants only)
fn derive_baml_enum_decode(
    type_name: &syn::Ident,
    baml_name: &str,
    data: &syn::DataEnum,
    baml_crate: &TokenStream,
    is_dynamic: bool,
) -> Result<TokenStream> {
    let mut variant_arms = Vec::new();
    let mut dynamic_variant_name: Option<syn::Ident> = None;

    for variant in &data.variants {
        let variant_attrs = VariantAttrs::from_attrs(&variant.attrs)?;
        let variant_name = &variant.ident;

        if variant_attrs.dynamic_variant {
            // This is the _Dynamic(String) catch-all variant
            match &variant.fields {
                Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                    dynamic_variant_name = Some(variant_name.clone());
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        variant,
                        "#[baml(dynamic_variant)] must be a single-field tuple variant like _Dynamic(String)",
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

    // Generate the catch-all arm
    let catch_all = if let Some(ref dyn_var) = dynamic_variant_name {
        if !is_dynamic {
            return Err(syn::Error::new_spanned(
                dyn_var,
                "#[baml(dynamic_variant)] requires #[baml(dynamic)] on the enum",
            ));
        }
        quote! {
            other => Ok(Self::#dyn_var(other.to_string()))
        }
    } else if is_dynamic {
        return Err(syn::Error::new_spanned(
            type_name,
            "#[baml(dynamic)] enum requires a variant with #[baml(dynamic_variant)]",
        ));
    } else {
        quote! {
            other => Err(#baml_crate::BamlError::internal(
                format!("unknown variant '{}' for enum {}", other, #baml_name)
            ))
        }
    };

    Ok(quote! {
        impl #baml_crate::BamlEnum for #type_name {
            const ENUM_NAME: &'static str = #baml_name;

            fn from_variant_name(name: &str) -> ::core::result::Result<Self, #baml_crate::BamlError> {
                match name {
                    #(#variant_arms,)*
                    #catch_all
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

/// Generate `BamlDecode` impl for BAML union types (single-field tuple
/// variants)
fn derive_union_decode(
    type_name: &syn::Ident,
    data: &syn::DataEnum,
    baml_crate: &TokenStream,
) -> Result<TokenStream> {
    let mut variant_arms = Vec::new();

    for variant in &data.variants {
        let variant_attrs = VariantAttrs::from_attrs(&variant.attrs)?;
        let variant_name = &variant.ident;

        // Get the BAML union variant name from #[baml(name = "...")] attribute
        // This is required for unions - the code generator must provide it
        let is_literal = variant_attrs.is_literal();
        let baml_variant_name = variant_attrs.name.ok_or_else(|| {
            syn::Error::new_spanned(
                variant,
                "union variants require #[baml(name = \"...\")] to specify the BAML variant name",
            )
        })?;

        match &variant.fields {
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                let field_type = &fields.unnamed.first().unwrap().ty;
                variant_arms.push(quote! {
                    #baml_variant_name => {
                        let v = <#field_type as #baml_crate::BamlDecode>::baml_decode(inner)?;
                        Ok(Self::#variant_name(v))
                    }
                });
            }
            Fields::Unit if is_literal => {
                variant_arms.push(quote! {
                    #baml_variant_name => Ok(Self::#variant_name)
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
                // Extract the variant name and inner value from UnionVariantValue
                let (variant_name, inner) = #baml_crate::__internal::extract_union_variant_with_name(#type_name_str, holder)?;

                // Match on the variant name to decode the correct type
                match variant_name {
                    #(#variant_arms,)*
                    other => Err(#baml_crate::BamlError::internal(
                        format!("unknown variant '{}' for union {}", other, #type_name_str)
                    ))
                }
            }
        }
    })
}
