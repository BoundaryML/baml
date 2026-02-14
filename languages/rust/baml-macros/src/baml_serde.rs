use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Data, DataEnum, DataStruct, DeriveInput, Ident, Result};

use crate::shared::{ContainerAttrs, FieldAttrs, VariantAttrs};

pub(crate) fn derive_serde(input: &DeriveInput) -> Result<TokenStream> {
    let container_attrs = ContainerAttrs::from_attrs(&input.attrs)?;

    match &input.data {
        Data::Struct(data_struct) => derive_serde_struct(data_struct, &input.ident),
        Data::Enum(..) if container_attrs.union => Err(syn::Error::new(
            Span::call_site(),
            "BAML unions should use the inner-tagged default serde implementation.",
        )),
        Data::Enum(data_enum) => derive_serde_enum(data_enum, &input.ident),
        Data::Union(..) => Err(syn::Error::new(
            Span::call_site(),
            "Rust unions are not supported for this derive macro.",
        )),
    }
}

fn derive_serde_struct(data: &DataStruct, ident: &Ident) -> Result<TokenStream> {
    let mut dynamic_field = None;
    let mut field_idents = Vec::new();
    let mut field_names = Vec::new();
    let mut ser_fields = Vec::new();
    let mut de_field_match_arms = Vec::new();
    for field in &data.fields {
        let field_attrs = FieldAttrs::from_attrs(&field.attrs)?;
        if field_attrs.skip {
            continue;
        }
        if field_attrs.dynamic_fields {
            if dynamic_field.is_some() {
                return Err(syn::Error::new(
                    Span::call_site(),
                    "Only one dynamic field is allowed per struct.",
                ));
            }
            dynamic_field = Some(field);
            continue;
        }
        let field_ident = field
            .ident
            .as_ref()
            .expect("Struct fields must have a name.");
        field_idents.push(field_ident);
        let field_name = field_attrs.name.unwrap_or_else(|| field_ident.to_string());
        ser_fields.push(quote! {
            map.serialize_entry(#field_name, &self.#field_ident)?;
        });
        de_field_match_arms.push(quote! {
            #field_name => {
                #field_ident = Some(map.next_value()?);
            }
        });
        field_names.push(field_name);
    }
    let dynamic_ident = dynamic_field.as_ref().map(|field| {
        field
            .ident
            .as_ref()
            .expect("Struct fields must have a name.")
    });
    let ser_fields = ser_fields.as_slice();

    let ser_dynamic_field = dynamic_ident.map(|dynamic_ident| {
        quote! {
            for (k, v) in self.#dynamic_ident.iter() {
                map.serialize_entry(k, v)?;
            }
        }
    });
    let de_dynamic_hashmap = dynamic_ident.map(|dynamic_ident| {
        quote! {
            let mut #dynamic_ident = ::std::collections::HashMap::new();
        }
    });
    let de_catchall_match_arm = match dynamic_ident {
        Some(dynamic_ident) => quote! {
            other => {
                #dynamic_ident.insert(other.to_string(), map.next_value()?);
            }
        },
        None => quote! {
            other => {
                return Err(::baml::__internal::serde::de::Error::invalid_value(::baml::__internal::serde::de::Unexpected::Other(&other), &self));
            }
        },
    };
    let de_dynamic_hashmap_field = dynamic_ident.map(|dynamic_ident| {
        quote! { #dynamic_ident, }
    });

    let impls = quote! {
        impl ::baml::__internal::serde::Serialize for #ident {
            fn serialize<S: ::baml::__internal::serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                use ::baml::__internal::serde::ser::SerializeMap;
                let mut map = serializer.serialize_map(::std::option::Option::None)?;
                #(#ser_fields)*
                #ser_dynamic_field
                map.end()
            }
        }
        impl<'de> ::baml::__internal::serde::Deserialize<'de> for #ident {
            fn deserialize<D: ::baml::__internal::serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct Visitor;
                impl<'v> ::baml::__internal::serde::de::Visitor<'v> for Visitor {
                    type Value = #ident;
                    fn expecting(&self, formatter: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                        formatter.write_str("BAML class")
                    }
                    fn visit_map<A: ::baml::__internal::serde::de::MapAccess<'v>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                        #(let mut #field_idents = None;)*
                        #de_dynamic_hashmap

                        while let Some(key) = map.next_key()? {
                            match key {
                                #(#de_field_match_arms)*
                                #de_catchall_match_arm
                            }
                        }

                        Ok(#ident {
                            #(#field_idents: #field_idents.ok_or_else(|| ::baml::__internal::serde::de::Error::missing_field(#field_names))?,)*
                            #de_dynamic_hashmap_field
                        })
                    }
                }
                deserializer.deserialize_map(Visitor)
            }
        }
    };
    Ok(impls)
}

fn derive_serde_enum(data: &DataEnum, ident: &Ident) -> Result<TokenStream> {
    let name = ident.to_string();

    let mut ser_variant_arms = Vec::new();
    let mut de_variant_arms = Vec::new();
    let mut dynamic_variant = None;
    for variant in &data.variants {
        let variant_attrs = VariantAttrs::from_attrs(&variant.attrs)?;
        if variant_attrs.dynamic_variant {
            if dynamic_variant.is_some() {
                return Err(syn::Error::new(
                    Span::call_site(),
                    "Only one dynamic variant is allowed per enum.",
                ));
            }
            dynamic_variant = Some(variant);
            continue;
        }
        if !variant.fields.is_empty() {
            return Err(syn::Error::new(
                Span::call_site(),
                "Enum variants must be unit.",
            ));
        }
        let variant_ident = &variant.ident;
        let variant_name = variant_attrs
            .name
            .unwrap_or_else(|| variant_ident.to_string());

        ser_variant_arms.push(quote! {
            Self::#variant_ident => serializer.serialize_str(#variant_name),
        });
        de_variant_arms.push(quote! {
            #variant_name => Ok(Self::Value::#variant_ident),
        });
    }
    if let Some(dynamic_variant) = dynamic_variant {
        let dynamic_variant_ident = &dynamic_variant.ident;
        ser_variant_arms.push(quote! {
            Self::#dynamic_variant_ident(v) => serializer.serialize_str(v),
        });
        de_variant_arms.push(quote! {
            other => Ok(Self::Value::#dynamic_variant_ident(other.to_string())),
        });
    } else {
        de_variant_arms.push(quote! {
            other => Err(::baml::__internal::serde::de::Error::invalid_value(::baml::__internal::serde::de::Unexpected::Other(&other), &self)),
        });
    }

    let impls = quote! {
        impl ::baml::__internal::serde::Serialize for #ident {
            fn serialize<S: ::baml::__internal::serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                match self {
                    #(#ser_variant_arms)*
                }
            }
        }
        impl<'de> ::baml::__internal::serde::Deserialize<'de> for #ident {
            fn deserialize<D: ::baml::__internal::serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct Visitor;
                impl<'v> ::baml::__internal::serde::de::Visitor<'v> for Visitor {
                    type Value = #ident;
                    fn expecting(&self, formatter: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                        format!("BAML enum {} variant (string)", #name)
                    }
                    fn visit_str<E: ::baml::__internal::serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                        match v {
                            #(#de_variant_arms)*
                            _ => Err(::baml::__internal::serde::de::Error::invalid_value(::baml::__internal::serde::de::Unexpected::Str(v), &self)),
                        }
                    }
                };
            }
        }
    };

    Ok(impls)
}
