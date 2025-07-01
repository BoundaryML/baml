// lib.rs or baml_hash_derive/src/lib.rs
extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, parse_quote, DeriveInput, Field, GenericParam, Type};

fn is_type(ty: &Type, target: &str) -> bool {
    match ty {
        Type::Path(tp) => tp
            .path
            .segments
            .last()
            .is_some_and(|seg| seg.ident == target),
        _ => false,
    }
}

fn is_option_of_float(ty: &Type) -> Option<&'static str> {
    if let Type::Path(type_path) = ty {
        if let Some(seg) = type_path.path.segments.last() {
            if seg.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                    if let Some(syn::GenericArgument::Type(Type::Path(inner))) = args.args.first() {
                        if let Some(inner_seg) = inner.path.segments.last() {
                            let inner_ident = inner_seg.ident.to_string();
                            if inner_ident == "f32" || inner_ident == "f64" {
                                return Some(Box::leak(inner_ident.into_boxed_str()));
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn has_baml_safe_attr(field: &Field) -> bool {
    field
        .attrs
        .iter()
        .any(|attr| attr.path().is_ident("baml_safe_hash"))
}

#[proc_macro_derive(BamlHash, attributes(baml_safe_hash))]
pub fn derive_baml_hash(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let mut generics = input.generics.clone();

    // Add Hash bounds to generic parameters
    for param in generics.params.iter_mut() {
        if let GenericParam::Type(tp) = param {
            tp.bounds.push(parse_quote!(::std::hash::Hash));
        }
    }

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let hash_impl = match &input.data {
        syn::Data::Struct(data_struct) => {
            let hash_stmts = data_struct.fields.iter().map(|f| {
                let ident = f.ident.as_ref().unwrap();
                let ty = &f.ty;
                let is_safe = has_baml_safe_attr(f);

                if is_type(ty, "f32") {
                    if is_safe {
                        quote! {
                            state.write_u32(self.#ident.to_bits());
                        }
                    } else {
                        quote! {
                            compile_error!("f32 must be marked with #[baml_safe_hash]");
                        }
                    }
                } else if is_type(ty, "f64") {
                    if is_safe {
                        quote! {
                            state.write_u64(self.#ident.to_bits());
                        }
                    } else {
                        quote! {
                            compile_error!("f64 must be marked with #[baml_safe_hash]");
                        }
                    }
                } else if let Some(inner_ty) = is_option_of_float(ty) {
                    if is_safe {
                        if inner_ty == "f32" {
                            quote! {
                                if let Some(val) = self.#ident {
                                    state.write_u32(val.to_bits());
                                } else {
                                    state.write_u8(0);
                                }
                            }
                        } else {
                            quote! {
                                if let Some(val) = self.#ident {
                                    state.write_u64(val.to_bits());
                                } else {
                                    state.write_u8(0);
                                }
                            }
                        }
                    } else {
                        quote! {
                            compile_error!("Option<f32/f64> must be marked with #[baml_safe_hash]");
                        }
                    }
                } else if is_safe && is_type(ty, "IndexMap") {
                    quote! {
                        for (k, v) in &self.#ident {
                            k.hash(state);
                            v.hash(state);
                        }
                    }
                } else {
                    quote! {
                        self.#ident.hash(state);
                    }
                }
            });

            quote! {
                fn hash<H: ::std::hash::Hasher>(&self, state: &mut H) {
                    #(#hash_stmts)*
                }
            }
        }
        syn::Data::Enum(data_enum) => {
            let arms = data_enum.variants.iter().enumerate().map(|(idx, variant)| {
                let variant_ident = &variant.ident;
                let idx = idx as u8;

                match &variant.fields {
                    syn::Fields::Unit => quote! {
                        Self::#variant_ident => {
                            state.write_u8(#idx);
                        }
                    },
                    syn::Fields::Unnamed(fields) => {
                        let binders: Vec<_> = (0..fields.unnamed.len())
                            .map(|i| syn::Ident::new(&format!("v{i}"), variant.ident.span()))
                            .collect();

                        let hash_stmts = binders.iter().zip(fields.unnamed.iter()).map(|(ident, field)| {
                            let ty = &field.ty;
                            let is_safe = has_baml_safe_attr(field);

                            if is_type(ty, "f32") {
                                if is_safe {
                                    quote! { state.write_u32(#ident.to_bits()); }
                                } else {
                                    quote! { compile_error!("f32 must be marked with #[baml_safe_hash]"); }
                                }
                            } else if is_type(ty, "f64") {
                                if is_safe {
                                    quote! { state.write_u64(#ident.to_bits()); }
                                } else {
                                    quote! { compile_error!("f64 must be marked with #[baml_safe_hash]"); }
                                }
                            } else if let Some(inner_ty) = is_option_of_float(ty) {
                                if is_safe {
                                    if inner_ty == "f32" {
                                        quote! {
                                            if let Some(val) = #ident {
                                                state.write_u32(val.to_bits());
                                            } else {
                                                state.write_u8(0);
                                            }
                                        }
                                    } else {
                                        quote! {
                                            if let Some(val) = #ident {
                                                state.write_u64(val.to_bits());
                                            } else {
                                                state.write_u8(0);
                                            }
                                        }
                                    }
                                } else {
                                    quote! { compile_error!("Option<f32/f64> must be marked with #[baml_safe_hash]"); }
                                }
                            } else if is_safe && is_type(ty, "IndexMap") {
                                quote! {
                                    for (k, v) in #ident {
                                        k.hash(state);
                                        v.hash(state);
                                    }
                                }
                            } else {
                                quote! { #ident.hash(state); }
                            }
                        });

                        quote! {
                            Self::#variant_ident( #(#binders),* ) => {
                                state.write_u8(#idx);
                                #(#hash_stmts)*
                            }
                        }
                    }
                    syn::Fields::Named(fields) => {
                        let binders: Vec<_> = fields.named.iter()
                            .map(|f| f.ident.as_ref().unwrap())
                            .collect();

                        let hash_stmts = binders.iter().zip(fields.named.iter()).map(|(ident, field)| {
                            let ty = &field.ty;
                            let is_safe = has_baml_safe_attr(field);

                            if is_type(ty, "f32") {
                                if is_safe {
                                    quote! { state.write_u32(#ident.to_bits()); }
                                } else {
                                    quote! { compile_error!("f32 must be marked with #[baml_safe_hash]"); }
                                }
                            } else if is_type(ty, "f64") {
                                if is_safe {
                                    quote! { state.write_u64(#ident.to_bits()); }
                                } else {
                                    quote! { compile_error!("f64 must be marked with #[baml_safe_hash]"); }
                                }
                            } else if let Some(inner_ty) = is_option_of_float(ty) {
                                if is_safe {
                                    if inner_ty == "f32" {
                                        quote! {
                                            if let Some(val) = #ident {
                                                state.write_u32(val.to_bits());
                                            } else {
                                                state.write_u8(0);
                                            }
                                        }
                                    } else {
                                        quote! {
                                            if let Some(val) = #ident {
                                                state.write_u64(val.to_bits());
                                            } else {
                                                state.write_u8(0);
                                            }
                                        }
                                    }
                                } else {
                                    quote! { compile_error!("Option<f32/f64> must be marked with #[baml_safe_hash]"); }
                                }
                            } else if is_safe && is_type(ty, "IndexMap") {
                                quote! {
                                    for (k, v) in #ident {
                                        k.hash(state);
                                        v.hash(state);
                                    }
                                }
                            } else {
                                quote! { #ident.hash(state); }
                            }
                        });

                        quote! {
                            Self::#variant_ident { #(#binders),* } => {
                                state.write_u8(#idx);
                                #(#hash_stmts)*
                            }
                        }
                    }
                }
            });

            quote! {
                fn hash<H: ::std::hash::Hasher>(&self, state: &mut H) {
                    match self {
                        #(#arms),*
                    }
                }
            }
        }
        _ => panic!("Unsupported input for BamlHash"),
    };

    let expanded = quote! {
        impl #impl_generics ::std::hash::Hash for #name #ty_generics #where_clause {
            #hash_impl
        }
    };

    TokenStream::from(expanded)
}
