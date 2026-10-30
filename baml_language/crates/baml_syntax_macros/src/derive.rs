//! `#[derive(Node)]`, `#[derive(Walk)]`, `#[derive(AnyNode)]`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, GenericParam, Type, spanned::Spanned as _};

/// Type constructors a node field may not use: nodes hold ids, tokens and
/// pool lists, never their own allocations. Matched by name anywhere in the
/// field's type (`Option<Vec<T>>` is rejected too); the `Walk` bound at
/// `define_tree!` is the real enforcement, this is the readable error.
const FORBIDDEN: [&str; 5] = ["Vec", "Box", "String", "Rc", "Arc"];

/// The first forbidden constructor in `ty`, generic arguments included.
fn forbidden_in(ty: &Type) -> Option<&proc_macro2::Ident> {
    let Type::Path(path) = ty else {
        return None;
    };
    path.path.segments.iter().find_map(|segment| {
        if FORBIDDEN.contains(&segment.ident.to_string().as_str()) {
            return Some(&segment.ident);
        }
        let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
            return None;
        };
        args.args.iter().find_map(|arg| match arg {
            syn::GenericArgument::Type(inner) => forbidden_in(inner),
            _ => None,
        })
    })
}

fn check_field_types(fields: &Fields) -> syn::Result<()> {
    for field in fields {
        if let Some(forbidden) = forbidden_in(&field.ty) {
            return Err(syn::Error::new(
                field.ty.span(),
                format!(
                    "node fields must not allocate: `{forbidden}` is not allowed here. Use `Id<T>` for a \
                     child node, `Token<K>` for a token, or `List<T>` for a sequence."
                ),
            ));
        }
    }
    Ok(())
}

fn named_fields<'a>(input: &'a DeriveInput, what: &str) -> syn::Result<&'a Fields> {
    match &input.data {
        Data::Struct(data) => match &data.fields {
            fields @ Fields::Named(_) => Ok(fields),
            Fields::Unit => Ok(&data.fields),
            Fields::Unnamed(_) => Err(syn::Error::new(
                input.span(),
                format!("`{what}` requires named fields: each field is a grammar position"),
            )),
        },
        Data::Enum(_) | Data::Union(_) => Err(syn::Error::new(
            input.span(),
            format!("`{what}` applies to structs; use `AnyNode` for a union of alternatives"),
        )),
    }
}

/// The arena type parameter the `Walk` impl is generic over. Picked to never
/// clash with the struct's own generics.
fn arena_param() -> proc_macro2::Ident {
    format_ident!("__Arena")
}

/// `impl<__Arena, …> Trait<__Arena> for Ty<…> where <every field type>: Walk<__Arena>`.
fn walk_impl_header(
    input: &DeriveInput,
    fields: &Fields,
    trait_path: &TokenStream,
) -> (TokenStream, TokenStream, TokenStream) {
    let arena = arena_param();
    let name = &input.ident;
    let (_, ty_generics, where_clause) = input.generics.split_for_impl();
    let own_params = input.generics.params.iter();
    let impl_generics = quote! { <#arena, #(#own_params),*> };
    let mut bounds: Vec<TokenStream> = fields
        .iter()
        .map(|field| {
            let ty = &field.ty;
            quote! { #ty: ::baml_syntax::tree::Walk<#arena> }
        })
        .collect();
    if let Some(where_clause) = where_clause {
        bounds.extend(
            where_clause
                .predicates
                .iter()
                .map(|predicate| quote! { #predicate }),
        );
    }
    // Lifetime/const parameters never carry Walk bounds; type parameters do.
    for param in &input.generics.params {
        if let GenericParam::Type(ty) = param {
            let ident = &ty.ident;
            bounds.push(quote! { #ident: ::baml_syntax::tree::Walk<#arena> });
        }
    }
    (
        quote! { impl #impl_generics #trait_path<#arena> for #name #ty_generics where #(#bounds,)* },
        quote! { #arena },
        quote! { #name },
    )
}

fn walk_fields_body(fields: &Fields, arena: &TokenStream) -> TokenStream {
    let calls = fields.iter().map(|field| {
        let ident = field
            .ident
            .as_ref()
            .unwrap_or_else(|| unreachable!("named fields were checked"));
        let name = ident.to_string();
        quote! {
            ::baml_syntax::tree::Walk::<#arena>::walk(&self.#ident, arena, #name, visitor);
        }
    });
    quote! { #(#calls)* }
}

pub(crate) fn node(input: &DeriveInput) -> syn::Result<TokenStream> {
    let fields = named_fields(input, "Node")?;
    check_field_types(fields)?;
    let (header, arena, name) =
        walk_impl_header(input, fields, &quote! { ::baml_syntax::tree::NodeFields });
    let body = walk_fields_body(fields, &arena);
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics ::baml_syntax::tree::sealed::Sealed for #name #ty_generics #where_clause {}
        impl #impl_generics ::baml_syntax::tree::NodeMarker for #name #ty_generics #where_clause {}
        #header {
            fn walk_fields(&self, arena: &#arena, visitor: &mut dyn ::baml_syntax::tree::Visitor) {
                #body
            }
        }
    })
}

pub(crate) fn walk(input: &DeriveInput) -> syn::Result<TokenStream> {
    let fields = named_fields(input, "Walk")?;
    check_field_types(fields)?;
    let (header, arena, _) = walk_impl_header(input, fields, &quote! { ::baml_syntax::tree::Walk });
    let body = walk_fields_body(fields, &arena);
    Ok(quote! {
        #header {
            fn walk(
                &self,
                arena: &#arena,
                _field: &'static str,
                visitor: &mut dyn ::baml_syntax::tree::Visitor,
            ) {
                #body
            }
        }
    })
}

pub(crate) fn any_node(input: &DeriveInput) -> syn::Result<TokenStream> {
    let Data::Enum(data) = &input.data else {
        return Err(syn::Error::new(
            input.span(),
            "`AnyNode` applies to an enum of alternatives",
        ));
    };
    if !data.variants.iter().any(|v| v.ident == "Bogus") {
        return Err(syn::Error::new(
            input.span(),
            "an `AnyNode` union must have a `Bogus` variant, so recovered garbage in this \
             position is representable without loss",
        ));
    }
    let arena = arena_param();
    let name = &input.ident;
    let mut bounds = Vec::new();
    let mut arms = Vec::new();
    let mut id_arms = Vec::new();
    for variant in &data.variants {
        let ident = &variant.ident;
        let Fields::Unnamed(unnamed) = &variant.fields else {
            return Err(syn::Error::new(
                variant.span(),
                "each `AnyNode` variant holds exactly one payload: `Variant(Id<T>)` or `Variant(Token<K>)`",
            ));
        };
        if unnamed.unnamed.len() != 1 {
            return Err(syn::Error::new(
                variant.span(),
                "each `AnyNode` variant holds exactly one payload",
            ));
        }
        let ty = &unnamed.unnamed[0].ty;
        bounds.push(quote! { #ty: ::baml_syntax::tree::Walk<#arena> });
        arms.push(quote! {
            Self::#ident(inner) => ::baml_syntax::tree::Walk::<#arena>::walk(inner, arena, field, visitor),
        });
        id_arms.push(quote! {
            Self::#ident(inner) => ::baml_syntax::tree::AnyNodeIdOf::any_node_id(inner),
        });
    }
    Ok(quote! {
        impl<#arena> ::baml_syntax::tree::Walk<#arena> for #name where #(#bounds,)* {
            fn walk(
                &self,
                arena: &#arena,
                field: &'static str,
                visitor: &mut dyn ::baml_syntax::tree::Visitor,
            ) {
                match self { #(#arms)* }
            }
        }
        impl ::baml_syntax::tree::AnyNode for #name {
            fn any_node_id(&self) -> ::core::option::Option<::baml_syntax::tree::AnyNodeId> {
                match self { #(#id_arms)* }
            }
        }
    })
}
