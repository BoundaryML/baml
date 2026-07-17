//! Callable emission — free functions and class methods: one sync + one
//! `_async` binding per BAML callable, both thin wrappers over
//! `baml_bridge::runtime`.

use baml_codegen_types::{Function, Name};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::{
    SkipWarning, idents,
    translate_ty::{self, TyCtx},
};

/// How a binding receives its subject: free functions and static methods
/// have no receiver; instance methods take `&self` and send it as the
/// `"self"` kwarg — the engine-side method's real first parameter, which
/// the pool builder strips from [`Function::arguments`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Receiver {
    None,
    RefSelf,
}

/// Emit the sync + async bindings for a free function, or a skip warning
/// when any type in its signature (arguments, return, throws contract)
/// is not yet representable.
pub(crate) fn emit(
    name: &Name,
    function: &Function,
    ctx: &TyCtx<'_>,
) -> Result<TokenStream, SkipWarning> {
    let fqn = name.to_string();
    if name.name().as_str().contains('$') {
        return Err(SkipWarning {
            fqn,
            reason: "companion functions ($stream, $build_request, …) are not emitted yet"
                .to_string(),
        });
    }
    if !function.generic_params.is_empty() {
        return Err(SkipWarning {
            fqn,
            reason: "generic functions are not emitted yet".to_string(),
        });
    }
    emit_binding(&fqn, name.name().as_str(), function, Receiver::None, ctx)
}

/// Emit the sync + async bindings for a static or instance method, shaped
/// for the enclosing class's `impl` block. The wire FQN is
/// `<class fqn>.<method name>` — the engine resolves it like any other
/// function, so the binding body is the free-function one plus (for
/// instance methods) the receiver kwarg.
pub(crate) fn emit_method(
    class_name: &Name,
    method: &Function,
    receiver: Receiver,
    ctx: &TyCtx<'_>,
) -> Result<TokenStream, SkipWarning> {
    let method_name = method.name.as_str();
    let fqn = format!("{class_name}.{method_name}");
    if method_name.contains('$') {
        return Err(SkipWarning {
            fqn,
            reason: "companion methods ($stream, $build_request, …) are not emitted yet"
                .to_string(),
        });
    }
    if !method.generic_params.is_empty() {
        return Err(SkipWarning {
            fqn,
            reason: "generic methods are not emitted yet".to_string(),
        });
    }
    emit_binding(&fqn, method_name, method, receiver, ctx)
}

/// Shared body of [`emit`] / [`emit_method`]: translate the signature and
/// build the sync + async wrapper pair.
fn emit_binding(
    fqn: &str,
    binding_name: &str,
    function: &Function,
    receiver: Receiver,
    ctx: &TyCtx<'_>,
) -> Result<TokenStream, SkipWarning> {
    let skip = |reason: String| SkipWarning {
        fqn: fqn.to_string(),
        reason,
    };

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
    if matches!(receiver, Receiver::RefSelf) {
        // The receiver rides the wire as an ordinary kwarg named `self`,
        // deep-copied like every other argument (no bridge special-casing;
        // the python and node SDKs send the same entry).
        kwarg_entries.push(quote! {
            (
                "self",
                ::std::option::Option::Some(
                    ::baml_bridge::baml_value::internal::__BamlValuePrivate::to_baml(self),
                ),
            )
        });
    }
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
    match receiver {
        // Instance methods always send the receiver, so the note rides
        // even a zero-argument method.
        Receiver::RefSelf => {
            append_by_value_note(&mut doc_attrs, ByValueSubject::ReceiverAndArguments);
        }
        Receiver::None if !function.arguments.is_empty() => {
            append_by_value_note(&mut doc_attrs, ByValueSubject::Arguments);
        }
        Receiver::None => {}
    }

    let self_param = match receiver {
        Receiver::None => TokenStream::new(),
        Receiver::RefSelf => quote! { &self, },
    };
    let sync_name = idents::ident(binding_name);
    let async_name = format_ident!("{}_async", idents::dir_segment(binding_name));
    let result_ty = quote! { ::std::result::Result<#ret, ::baml_bridge::Error<#throws>> };
    // The receiver counts toward clippy's tally (`self` is one of the
    // `fn_decl` inputs), so it counts here too.
    let arg_count = function.arguments.len() + usize::from(matches!(receiver, Receiver::RefSelf));
    let too_many_arguments_attr = if arg_count > 7 {
        quote! { #[allow(clippy::too_many_arguments)] }
    } else {
        TokenStream::new()
    };

    Ok(quote! {
        #(#doc_attrs)*
        #too_many_arguments_attr
        pub fn #sync_name(#self_param #(#params),*) -> #result_ty {
            crate::_runtime::ensure_init().map_err(::baml_bridge::Error::Sdk)?;
            #(#converts)*
            ::baml_bridge::runtime::invoke_sync(
                #fqn,
                ::baml_bridge::encode::kwargs(::std::vec![#(#kwarg_entries),*]),
            )
        }

        #(#doc_attrs)*
        #too_many_arguments_attr
        pub async fn #async_name(#self_param #(#params),*) -> #result_ty {
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

/// What a binding sends by value — picks the wording of the
/// calling-convention note.
#[derive(Clone, Copy)]
enum ByValueSubject {
    Arguments,
    ReceiverAndArguments,
}

/// Append the by-value calling-convention note to a binding's doc block,
/// separated from any preceding docstring by a blank `///` line.
///
/// BAML has reference semantics in-language — a function body may mutate
/// its parameters (the stdlib iterator protocol advances `self` this way)
/// — but the bridge deep-copies every argument onto the wire and only the
/// return value comes back, so a mutating callee silently behaves like a
/// no-op on the caller's values. Emitted on every generated binding that
/// sends something: any binding with at least one argument, and every
/// instance method (a `self` receiver is the most common risk, but any
/// argument can be used out-param-style).
fn append_by_value_note(attrs: &mut Vec<TokenStream>, subject: ByValueSubject) {
    if !attrs.is_empty() {
        attrs.push(quote! { #[doc = ""] });
    }
    let lines: [&str; 3] = match subject {
        ByValueSubject::Arguments => [
            " Arguments are passed to the BAML runtime by value: mutations the",
            " BAML function body makes to its parameters stay runtime-side and",
            " are never written back to the caller's values.",
        ],
        ByValueSubject::ReceiverAndArguments => [
            " The receiver and any arguments are passed to the BAML runtime by",
            " value: mutations the BAML method body makes to `self` or its",
            " parameters stay runtime-side and are never written back.",
        ],
    };
    for line in lines {
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
