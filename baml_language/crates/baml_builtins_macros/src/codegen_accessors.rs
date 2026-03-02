//! Code generation for `generate_builtin_accessors` — owned structs and
//! typed accessor wrappers for `bex_heap`.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

use crate::{
    collect::{
        AccessorEnumDef, AccessorFieldDef, AccessorTypeDef, CollectedBuiltins, FieldTypeKind,
    },
    util::path_to_rust_ident,
};

pub(crate) fn generate(collected: &CollectedBuiltins) -> TokenStream2 {
    let owned_structs: Vec<_> = collected
        .accessor_types
        .iter()
        .map(gen_owned_struct)
        .collect();
    let owned_enums: Vec<_> = collected
        .accessor_enums
        .iter()
        .map(gen_owned_enum)
        .collect();
    let accessor_structs: Vec<_> = collected
        .accessor_types
        .iter()
        .map(gen_accessor_struct)
        .collect();

    quote! {
        pub mod builtin_types {
            use super::*;

            pub mod owned {
                use bex_external_types::{AsBexExternalValue, BexExternalValue};

                #(#owned_structs)*
                #(#owned_enums)*
            }

            #(#accessor_structs)*
        }
    }
}

// ============================================================================
// Owned struct generation
// ============================================================================

fn gen_owned_field_type(kind: &FieldTypeKind) -> TokenStream2 {
    match kind {
        FieldTypeKind::String => quote!(String),
        FieldTypeKind::Int => quote!(i64),
        FieldTypeKind::Float => quote!(f64),
        FieldTypeKind::Bool => quote!(bool),
        FieldTypeKind::ResourceHandle => quote!(bex_resource_types::ResourceHandle),
        FieldTypeKind::MapStringString => quote!(indexmap::IndexMap<String, String>),
        FieldTypeKind::MapStringUnknown => {
            quote!(indexmap::IndexMap<String, bex_external_types::BexExternalValue>)
        }
        FieldTypeKind::ArrayString => quote!(Vec<String>),
        FieldTypeKind::BuiltinEnum(path) => {
            let ident = path_to_rust_ident(path);
            quote!(#ident)
        }
        FieldTypeKind::ArrayBuiltinStruct(path) => {
            let ident = path_to_rust_ident(path);
            quote!(Vec<#ident>)
        }
        FieldTypeKind::OptionalBuiltinStruct(path) => {
            let ident = path_to_rust_ident(path);
            quote!(Option<#ident>)
        }
    }
}

/// Generate `BexExternalValue` conversion expression for a single field.
fn gen_as_bex_external_field(field: &AccessorFieldDef) -> (Option<TokenStream2>, TokenStream2) {
    let name = &field.name;
    let name_str = &field.name_str;
    match &field.kind {
        FieldTypeKind::String => (
            None,
            quote!(#name_str.to_string() => BexExternalValue::String(self.#name)),
        ),
        FieldTypeKind::Int => (
            None,
            quote!(#name_str.to_string() => BexExternalValue::Int(self.#name)),
        ),
        FieldTypeKind::Float => (
            None,
            quote!(#name_str.to_string() => BexExternalValue::Float(self.#name)),
        ),
        FieldTypeKind::Bool => (
            None,
            quote!(#name_str.to_string() => BexExternalValue::Bool(self.#name)),
        ),
        FieldTypeKind::ResourceHandle => (
            None,
            quote!(#name_str.to_string() => BexExternalValue::Resource(self.#name)),
        ),
        FieldTypeKind::MapStringString => {
            let binding = quote! {
                let #name = BexExternalValue::Map {
                    key_type: bex_external_types::Ty::String { attr: baml_type::TyAttr::default() },
                    value_type: bex_external_types::Ty::String { attr: baml_type::TyAttr::default() },
                    entries: self.#name
                        .into_iter()
                        .map(|(k, v)| (k, BexExternalValue::String(v)))
                        .collect(),
                };
            };
            let entry = quote!(#name_str.to_string() => #name);
            (Some(binding), entry)
        }
        FieldTypeKind::MapStringUnknown => {
            let binding = quote! {
                let #name = BexExternalValue::Map {
                    key_type: bex_external_types::Ty::String { attr: baml_type::TyAttr::default() },
                    value_type: bex_external_types::Ty::BuiltinUnknown { attr: baml_type::TyAttr::default() },
                    entries: self.#name,
                };
            };
            let entry = quote!(#name_str.to_string() => #name);
            (Some(binding), entry)
        }
        FieldTypeKind::ArrayString => {
            let binding = quote! {
                let #name = BexExternalValue::Array {
                    element_type: bex_external_types::Ty::String { attr: baml_type::TyAttr::default() },
                    items: self.#name
                        .into_iter()
                        .map(BexExternalValue::String)
                        .collect(),
                };
            };
            let entry = quote!(#name_str.to_string() => #name);
            (Some(binding), entry)
        }
        FieldTypeKind::BuiltinEnum(_) => {
            let binding = quote! {
                let #name = self.#name.into_bex_external_value();
            };
            let entry = quote!(#name_str.to_string() => #name);
            (Some(binding), entry)
        }
        FieldTypeKind::ArrayBuiltinStruct(_) => {
            let binding = quote! {
                let #name = BexExternalValue::Array {
                    element_type: bex_external_types::Ty::BuiltinUnknown { attr: baml_type::TyAttr::default() },
                    items: self.#name
                        .into_iter()
                        .map(AsBexExternalValue::into_bex_external_value)
                        .collect(),
                };
            };
            let entry = quote!(#name_str.to_string() => #name);
            (Some(binding), entry)
        }
        FieldTypeKind::OptionalBuiltinStruct(_) => {
            let binding = quote! {
                let #name = self.#name.into_bex_external_value();
            };
            let entry = quote!(#name_str.to_string() => #name);
            (Some(binding), entry)
        }
    }
}

fn gen_owned_struct(td: &AccessorTypeDef) -> TokenStream2 {
    let name = &td.rust_name;
    let fields: Vec<_> = td
        .fields
        .iter()
        .map(|f| {
            let fname = &f.name;
            let ftype = gen_owned_field_type(&f.kind);
            quote!(pub #fname: #ftype)
        })
        .collect();

    let mut pre_bindings = Vec::new();
    let mut entries = Vec::new();
    for f in &td.fields {
        let (pre, entry) = gen_as_bex_external_field(f);
        if let Some(p) = pre {
            pre_bindings.push(p);
        }
        entries.push(entry);
    }
    let class_name = &td.path;

    quote! {
        #[derive(Debug, Clone)]
        pub struct #name {
            #(#fields,)*
        }

        impl AsBexExternalValue for #name {
            fn into_bex_external_value(self) -> BexExternalValue {
                #(#pre_bindings)*
                BexExternalValue::Instance {
                    class_name: #class_name.to_string(),
                    fields: indexmap::indexmap! {
                        #(#entries,)*
                    },
                }
            }
        }
    }
}

// ============================================================================
// Owned enum generation
// ============================================================================

fn gen_owned_enum(ed: &AccessorEnumDef) -> TokenStream2 {
    let name = &ed.rust_name;
    let variants = &ed.variants;
    let enum_path = &ed.path;

    let variant_arms: Vec<_> = variants
        .iter()
        .map(|v| {
            let variant_str = v.to_string();
            quote!(#name::#v => BexExternalValue::Variant {
                enum_name: #enum_path.to_string(),
                variant_name: #variant_str.to_string(),
            })
        })
        .collect();

    quote! {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum #name {
            #(#variants,)*
        }

        impl AsBexExternalValue for #name {
            fn into_bex_external_value(self) -> BexExternalValue {
                match self {
                    #(#variant_arms,)*
                }
            }
        }
    }
}

// ============================================================================
// Accessor struct generation
// ============================================================================

fn gen_accessor_return_type(kind: &FieldTypeKind) -> TokenStream2 {
    match kind {
        FieldTypeKind::String => quote!(&'a String),
        FieldTypeKind::Int => quote!(i64),
        FieldTypeKind::Float => quote!(f64),
        FieldTypeKind::Bool => quote!(bool),
        FieldTypeKind::ResourceHandle => quote!(bex_resource_types::ResourceHandle),
        FieldTypeKind::MapStringString => quote!(indexmap::IndexMap<String, &'a String>),
        FieldTypeKind::MapStringUnknown => quote!(indexmap::IndexMap<String, BexValue<'a>>),
        FieldTypeKind::ArrayString => quote!(Vec<&'a String>),
        // Complex types: accessor reading from heap is not supported
        FieldTypeKind::BuiltinEnum(_)
        | FieldTypeKind::ArrayBuiltinStruct(_)
        | FieldTypeKind::OptionalBuiltinStruct(_) => quote!(()),
    }
}

fn accessor_needs_heap(kind: &FieldTypeKind) -> bool {
    !matches!(
        kind,
        FieldTypeKind::Int
            | FieldTypeKind::Float
            | FieldTypeKind::Bool
            | FieldTypeKind::BuiltinEnum(_)
            | FieldTypeKind::ArrayBuiltinStruct(_)
            | FieldTypeKind::OptionalBuiltinStruct(_)
    )
}

fn gen_accessor_body(field: &AccessorFieldDef) -> TokenStream2 {
    let name_str = &field.name_str;
    match field.kind {
        FieldTypeKind::String => quote! {
            self.cls
                .field(#name_str)
                .and_then(|value| value.as_string(heap))
        },
        FieldTypeKind::Int => quote! {
            self.cls
                .field(#name_str)
                .and_then(|value| value.as_int())
        },
        FieldTypeKind::Float => quote! {
            self.cls
                .field(#name_str)
                .and_then(|value| value.as_float())
        },
        FieldTypeKind::Bool => quote! {
            self.cls
                .field(#name_str)
                .and_then(|value| value.as_bool())
        },
        FieldTypeKind::ResourceHandle => quote! {
            self.cls
                .field(#name_str)
                .and_then(|value| value.as_resource_handle(heap))
        },
        FieldTypeKind::MapStringString => quote! {
            self.cls
                .field(#name_str)
                .and_then(|value| value.as_map(heap))
                .and_then(|map| {
                    map.into_iter()
                        .map(|(k, v)| v.as_string(heap).map(|s| (k, s)))
                        .collect::<Result<_, _>>()
                })
        },
        FieldTypeKind::MapStringUnknown => quote! {
            self.cls
                .field(#name_str)
                .and_then(|value| value.as_map(heap))
        },
        FieldTypeKind::ArrayString => quote! {
            self.cls.field(#name_str).and_then(|value| {
                value.as_array(heap).and_then(|items| {
                    items
                        .into_iter()
                        .map(|item| item.as_string(heap))
                        .collect::<Result<Vec<_>, AccessError>>()
                })
            })
        },
        // Complex types: accessor reading from heap not yet implemented
        FieldTypeKind::BuiltinEnum(_)
        | FieldTypeKind::ArrayBuiltinStruct(_)
        | FieldTypeKind::OptionalBuiltinStruct(_) => quote! {
            Err(AccessError::CannotConvertToOwned {
                reason: format!("complex field accessor for '{}' not supported", #name_str),
            })
        },
    }
}

fn gen_into_owned_field(field: &AccessorFieldDef) -> TokenStream2 {
    let name = &field.name;
    match field.kind {
        FieldTypeKind::String => quote!(#name: self.#name(heap)?.clone()),
        FieldTypeKind::Int | FieldTypeKind::Float | FieldTypeKind::Bool => {
            quote!(#name: self.#name()?)
        }
        FieldTypeKind::ResourceHandle => quote!(#name: self.#name(heap)?),
        FieldTypeKind::MapStringString => {
            quote!(#name: self.#name(heap)?
                .into_iter()
                .map(|(k, v)| (k, v.clone()))
                .collect())
        }
        FieldTypeKind::MapStringUnknown => {
            quote!(#name: self.#name(heap)?
                .into_iter()
                .map(|(k, v)| Ok((k, v.as_owned_but_very_slow(heap)?)))
                .collect::<Result<_, _>>()?)
        }
        FieldTypeKind::ArrayString => {
            quote!(#name: self.#name(heap)?.into_iter().cloned().collect())
        }
        // Complex types: into_owned from heap reads not yet implemented
        FieldTypeKind::BuiltinEnum(_)
        | FieldTypeKind::ArrayBuiltinStruct(_)
        | FieldTypeKind::OptionalBuiltinStruct(_) => {
            quote!(#name: return Err(AccessError::CannotConvertToOwned {
                reason: format!("complex field into_owned for '{}' not supported", stringify!(#name)),
            }))
        }
    }
}

fn gen_accessor_struct(td: &AccessorTypeDef) -> TokenStream2 {
    let name = &td.rust_name;
    let path = &td.path;

    let methods: Vec<_> = td
        .fields
        .iter()
        .map(|f| {
            let fname = &f.name;
            let ret = gen_accessor_return_type(&f.kind);
            let body = gen_accessor_body(f);
            if accessor_needs_heap(&f.kind) {
                quote! {
                    pub fn #fname(
                        &self,
                        heap: &'a GcProtectedHeap<'a>,
                    ) -> Result<#ret, AccessError> {
                        #body
                    }
                }
            } else {
                quote! {
                    pub fn #fname(&self) -> Result<#ret, AccessError> {
                        #body
                    }
                }
            }
        })
        .collect();

    let owned_fields: Vec<_> = td.fields.iter().map(gen_into_owned_field).collect();
    let owned_name = &td.rust_name;

    quote! {
        pub struct #name<'a> {
            cls: super::BexClass<'a>,
        }

        impl<'a> From<BexClass<'a>> for #name<'a> {
            fn from(cls: BexClass<'a>) -> Self {
                Self { cls }
            }
        }

        impl<'a> BuiltinClass<'a> for #name<'a> {
            fn name() -> &'static str {
                #path
            }
        }

        impl<'a> #name<'a> {
            #(#methods)*

            pub fn into_owned(
                self,
                heap: &'a GcProtectedHeap<'a>,
            ) -> Result<owned::#owned_name, AccessError> {
                Ok(owned::#owned_name {
                    #(#owned_fields,)*
                })
            }
        }
    }
}
