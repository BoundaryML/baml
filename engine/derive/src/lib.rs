use proc_macro::TokenStream;
use quote::quote;
use syn::Data;

#[proc_macro_derive(FfiWrapper)]
pub fn hello_macro_derive(input: TokenStream) -> TokenStream {
    // Construct a representation of Rust code as a syntax tree
    // that we can manipulate
    let ast = syn::parse(input).unwrap();

    // Build the trait implementation
    impl_hello_macro3(&ast).into()
}

fn impl_hello_macro3(ast: &syn::DeriveInput) -> impl Into<TokenStream> {
    let name = &ast.ident;

    let syn::Data::Struct(data) = &ast.data else {
        panic!("HelloMacro derive only supports structs");
    };

    if let syn::Fields::Named(fields) = &data.fields {
        for field in fields.named.iter() {
            if let Some(ident) = &field.ident {
                if ident.to_string() == "inner" {
                    let ty = &field.ty;

                    if let Some(ty) = as_unary_generic(ty, &["Arc", "Mutex"]) {
                        return quote! {
                            impl From<#ty> for #name {
                                fn from(inner: #ty) -> Self {
                                    Self { inner: Arc::new(Mutex::new(inner)) }
                                }
                            }
                        };
                    }

                    if let Some(ty) = as_unary_generic(ty, &["Arc"]) {
                        return quote! {
                            impl From<#ty> for #name {
                                fn from(inner: #ty) -> Self {
                                    Self { inner: Arc::new(inner) }
                                }
                            }
                        };
                    }

                    return quote! {
                        impl From<#ty> for #name {
                            fn from(inner: #ty) -> Self {
                                Self { inner }
                            }
                        }
                    };
                }
            }
        }
    };

    panic!("HelloMacro derive only supports structs with a field named 'inner'");
}

fn as_unary_generic<'s>(ty: &'s syn::Type, idents: &[&'static str]) -> Option<&'s syn::Type> {
    let Some((ident, idents)) = idents.split_first() else {
        return None;
    };

    if let syn::Type::Path(path) = ty {
        if let Some(last) = path.path.segments.last() {
            if last.ident.to_string() == *ident {
                if let syn::PathArguments::AngleBracketed(inner_ty) = &last.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = inner_ty.args.first() {
                        if idents.is_empty() {
                            return Some(inner_ty);
                        } else {
                            return as_unary_generic(inner_ty, idents);
                        }
                    }
                }
            }
        }
    }

    None
}
