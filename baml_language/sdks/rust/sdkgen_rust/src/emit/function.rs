//! Free-function emission: one sync + one `_async` binding per BAML
//! function, both thin wrappers over `baml_rs::runtime`.

use baml_codegen_types::{Function, Name};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::{SkipWarning, idents, translate_ty};

/// Emit the sync + async bindings for a free function, or a skip warning
/// when any type in its signature (arguments, return, throws contract)
/// is not yet representable.
pub(crate) fn emit(name: &Name, function: &Function) -> Result<TokenStream, SkipWarning> {
    let fqn = name.to_string();
    let skip = |reason: String| SkipWarning {
        fqn: fqn.clone(),
        reason,
    };

    if name.name.as_str().contains('$') {
        return Err(skip(
            "companion functions ($stream, $build_request, …) are not emitted yet".to_string(),
        ));
    }
    if !function.generic_params.is_empty() {
        return Err(skip("generic functions are not emitted yet".to_string()));
    }

    let ret = translate_ty::translate(&function.return_type)
        .map_err(|u| skip(format!("return: {}", u.reason)))?;
    let throws = match &function.throws {
        None => quote! { ::core::convert::Infallible },
        Some(ty) => translate_ty::translate(ty)
            .map_err(|u| skip(format!("throws contract: {}", u.reason)))?,
    };

    let mut params = Vec::new();
    let mut converts = Vec::new();
    let mut kwarg_entries = Vec::new();
    for arg in &function.arguments {
        let arg_name = arg.name.as_str();
        let param = idents::ident(arg_name);
        let ty = translate_ty::translate(&arg.ty)
            .map_err(|u| skip(format!("argument `{arg_name}`: {}", u.reason)))?;
        if arg.default.is_some() {
            // A defaulted BAML parameter: the wrapper accepts anything
            // convertible into `OptionalArg` so call sites pass a plain
            // value, `None` for explicit null, or `Unset` to let the
            // engine evaluate the default.
            params.push(quote! {
                #param: impl ::std::convert::Into<::baml_rs::OptionalArg<#ty>>
            });
            converts.push(quote! {
                let #param = ::std::convert::Into::<::baml_rs::OptionalArg<#ty>>::into(#param);
            });
            kwarg_entries.push(quote! { (#arg_name, #param.to_baml_opt()) });
        } else {
            params.push(quote! { #param: #ty });
            kwarg_entries.push(quote! {
                (
                    #arg_name,
                    ::std::option::Option::Some(
                        ::baml_rs::baml_value::internal::__BamlValuePrivate::to_baml(&#param),
                    ),
                )
            });
        }
    }

    let doc_attrs = doc_attrs(function.docstring.as_deref());
    let sync_name = idents::ident(name.name.as_str());
    let async_name = format_ident!("{}_async", idents::dir_segment(name.name.as_str()));
    let result_ty = quote! { ::std::result::Result<#ret, ::baml_rs::Error<#throws>> };

    Ok(quote! {
        #(#doc_attrs)*
        pub fn #sync_name(#(#params),*) -> #result_ty {
            crate::_runtime::ensure_init().map_err(::baml_rs::Error::Sdk)?;
            #(#converts)*
            ::baml_rs::runtime::invoke_sync(
                #fqn,
                ::baml_rs::encode::kwargs(::std::vec![#(#kwarg_entries),*]),
            )
        }

        #(#doc_attrs)*
        pub async fn #async_name(#(#params),*) -> #result_ty {
            crate::_runtime::ensure_init().map_err(::baml_rs::Error::Sdk)?;
            #(#converts)*
            ::baml_rs::runtime::invoke(
                #fqn,
                ::baml_rs::encode::kwargs(::std::vec![#(#kwarg_entries),*]),
            )
            .await
        }
    })
}

/// BAML docstring → `#[doc = "…"]` attributes (rendered as `///` by the
/// pretty-printer), one per line.
pub(crate) fn doc_attrs(docstring: Option<&str>) -> Vec<TokenStream> {
    let Some(doc) = docstring else {
        return Vec::new();
    };
    doc.lines()
        .map(|line| {
            let line = format!(" {}", line.trim_end());
            quote! { #[doc = #line] }
        })
        .collect()
}
