//! Free-function emission: one sync + one `_async` binding per BAML
//! function, both thin wrappers over `baml_bridge::runtime`.

use baml_codegen_types::{Function, Name};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::{
    SkipWarning, idents,
    translate_ty::{self, TyCtx},
};

/// Emit the sync + async bindings for a free function, or a skip warning
/// when any type in its signature (arguments, return, throws contract)
/// is not yet representable.
pub(crate) fn emit(
    name: &Name,
    function: &Function,
    ctx: &TyCtx<'_>,
) -> Result<TokenStream, SkipWarning> {
    let fqn = name.to_string();
    let skip = |reason: String| SkipWarning {
        fqn: fqn.clone(),
        reason,
    };

    if name.name().as_str().contains('$') {
        return Err(skip(
            "companion functions ($stream, $build_request, …) are not emitted yet".to_string(),
        ));
    }
    if !function.generic_params.is_empty() {
        return Err(skip("generic functions are not emitted yet".to_string()));
    }

    let ret = translate_ty::translate(&function.return_type, ctx)
        .map_err(|u| skip(format!("return: {}", u.reason)))?;
    let throws = match &function.throws {
        None => quote! { ::core::convert::Infallible },
        Some(ty) => translate_ty::translate(ty, ctx)
            .map_err(|u| skip(format!("throws contract: {}", u.reason)))?,
    };

    let mut params = Vec::new();
    let mut converts = Vec::new();
    let mut kwarg_entries = Vec::new();
    for arg in &function.arguments {
        let arg_name = arg.name.as_str();
        let param = idents::ident(arg_name);
        let ty = translate_ty::translate(&arg.ty, ctx)
            .map_err(|u| skip(format!("argument `{arg_name}`: {}", u.reason)))?;
        if arg.default.is_some() {
            // A defaulted BAML parameter: the wrapper accepts anything
            // convertible into `OptionalArg` so call sites pass a plain
            // value, `None` for explicit null, or `Unset` to let the
            // engine evaluate the default.
            params.push(quote! {
                #param: impl ::std::convert::Into<::baml_bridge::OptionalArg<#ty>>
            });
            converts.push(quote! {
                let #param = ::std::convert::Into::<::baml_bridge::OptionalArg<#ty>>::into(#param);
            });
            kwarg_entries.push(quote! { (#arg_name, #param.to_baml_opt()) });
        } else {
            // Union-typed parameters accept anything convertible into the
            // synthesized enum, so call sites pass bare arm values
            // through the per-arm `From` impls.
            if is_multi_arm_union(&arg.ty) {
                params.push(quote! { #param: impl ::std::convert::Into<#ty> });
                converts.push(quote! {
                    let #param: #ty = ::std::convert::Into::into(#param);
                });
            } else {
                params.push(quote! { #param: #ty });
            }
            kwarg_entries.push(quote! {
                (
                    #arg_name,
                    ::std::option::Option::Some(
                        ::baml_bridge::baml_value::internal::__BamlValuePrivate::to_baml(&#param),
                    ),
                )
            });
        }
    }

    let mut doc_attrs = doc_attrs(function.docstring.as_deref());
    if !function.arguments.is_empty() {
        append_by_value_note(&mut doc_attrs);
    }
    let sync_name = idents::ident(name.name().as_str());
    let async_name = format_ident!("{}_async", idents::dir_segment(name.name().as_str()));
    let result_ty = quote! { ::std::result::Result<#ret, ::baml_bridge::Error<#throws>> };
    let too_many_arguments_attr = if function.arguments.len() > 7 {
        quote! { #[allow(clippy::too_many_arguments)] }
    } else {
        TokenStream::new()
    };

    Ok(quote! {
        #(#doc_attrs)*
        #too_many_arguments_attr
        pub fn #sync_name(#(#params),*) -> #result_ty {
            crate::_runtime::ensure_init().map_err(::baml_bridge::Error::Sdk)?;
            #(#converts)*
            ::baml_bridge::runtime::invoke_sync(
                #fqn,
                ::baml_bridge::encode::kwargs(::std::vec![#(#kwarg_entries),*]),
            )
        }

        #(#doc_attrs)*
        #too_many_arguments_attr
        pub async fn #async_name(#(#params),*) -> #result_ty {
            crate::_runtime::ensure_init().map_err(::baml_bridge::Error::Sdk)?;
            #(#converts)*
            ::baml_bridge::runtime::invoke(
                #fqn,
                ::baml_bridge::encode::kwargs(::std::vec![#(#kwarg_entries),*]),
            )
            .await
        }
    })
}

/// Whether a parameter type is a multi-arm union after null-stripping —
/// i.e. its translation is (an `Option` of) a synthesized union enum,
/// which parameters accept via `impl Into<_>`.
fn is_multi_arm_union(ty: &baml_codegen_types::Ty) -> bool {
    match ty {
        baml_codegen_types::Ty::Union(items, _) => crate::unions::strip_null(items).0.len() >= 2,
        _ => false,
    }
}

/// Append the by-value calling-convention note to a binding's doc block,
/// separated from any preceding docstring by a blank `///` line.
///
/// BAML has reference semantics in-language — a function body may mutate
/// its parameters (the stdlib iterator protocol advances `self` this way)
/// — but the bridge deep-copies every argument onto the wire and only the
/// return value comes back, so a mutating callee silently behaves like a
/// no-op on the caller's values. Emitted on every generated binding with
/// at least one argument (a `self` receiver is the most common risk, but
/// any argument can be used out-param-style).
pub(crate) fn append_by_value_note(attrs: &mut Vec<TokenStream>) {
    if !attrs.is_empty() {
        attrs.push(quote! { #[doc = ""] });
    }
    for line in [
        " Arguments are passed to the BAML runtime by value: mutations the",
        " BAML function body makes to its parameters stay runtime-side and",
        " are never written back to the caller's values.",
    ] {
        attrs.push(quote! { #[doc = #line] });
    }
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
