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
    // Generic free functions ARE emitted: their `<...>` params become Rust
    // generics bound by `BamlValue`. TypeVars that appear in positions the
    // translator can't represent still make the whole function skip, at the
    // per-type translation site.
    emit_binding(
        &fqn,
        name.name().as_str(),
        function,
        Receiver::None,
        &[],
        ctx,
    )
}

/// Emit the sync + async bindings for a static or instance method, shaped
/// for the enclosing class's `impl` block. The wire FQN is
/// `<class fqn>.<method name>` — the engine resolves it like any other
/// function, so the binding body is the free-function one plus (for
/// instance methods) the receiver kwarg and the class `TypeVar` bindings.
///
/// `class_params` are the enclosing class's `<...>` params — pass them for
/// instance methods only. A static has no receiver, so no class `TypeVar`s
/// bind through it: its frame has only the method's own params ("no
/// phantom class params"), and a class param in its signature would have
/// no binding to ride on — passing `&[]` keeps that fail-closed (such a
/// signature fails translation instead of silently not binding).
pub(crate) fn emit_method(
    class_name: &Name,
    method: &Function,
    receiver: Receiver,
    class_params: &[String],
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
    emit_binding(&fqn, method_name, method, receiver, class_params, ctx)
}

/// Shared body of [`emit`] / [`emit_method`]: translate the signature and
/// build the sync + async wrapper pair.
fn emit_binding(
    fqn: &str,
    binding_name: &str,
    function: &Function,
    receiver: Receiver,
    class_params: &[String],
    ctx: &TyCtx<'_>,
) -> Result<TokenStream, SkipWarning> {
    let skip = |reason: String| SkipWarning {
        fqn: fqn.to_string(),
        reason,
    };

    // The class's params (for instance methods) and the callee's own
    // `<...>` params come into scope for translating this binding's
    // signature: a `Ty::TypeVar` naming either resolves to that Rust
    // generic parameter (the class's come from the `impl` header, the
    // callee's own from this binding's `<...>`).
    let own_params: Vec<String> = function
        .generic_params
        .iter()
        .map(|param| param.as_str().to_string())
        .collect();
    // The compiler requires a method's own params to be distinct from the
    // enclosing class's (a shadowing name would make the named TyArg wire
    // ambiguous).
    debug_assert!(
        own_params.iter().all(|own| !class_params.contains(own)),
        "method type param shadows a class type param in {fqn}"
    );
    let scope: Vec<String> = class_params.iter().chain(&own_params).cloned().collect();
    let gctx = TyCtx {
        generic_params: scope.as_slice(),
        ..*ctx
    };

    let ret = translate_ty::translate(&function.return_type, &gctx)
        .map_err(|u| skip(format!("return: {}", u.reason)))?;
    let throws = match &function.throws {
        None => quote! { ::core::convert::Infallible },
        Some(ty) => translate_ty::translate(ty, &gctx)
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
        let ty = translate_ty::translate(&arg.ty, &gctx)
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
    // The `# Errors` section goes LAST: rustdoc folds everything after a
    // heading into that section, so the prose notes must precede it.
    append_errors_section(&mut doc_attrs, &raises_names(function.throws.as_ref()));

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

    // Each TypeVar becomes a Rust generic bound by `BamlValue` (the only
    // bound the SDK can express — interfaces/traits aren't wired up), and
    // its concrete binding is sent explicitly in `type_args`. Rust call
    // sites are always fully monomorphic, so we never lean on the engine's
    // argument-side inference: every param is bound by name here. Only the
    // callee's OWN params are declared on this binding — an instance
    // method's class params are declared by the enclosing `impl` header —
    // but the wire bindings cover the whole scope, class params first
    // (the De Bruijn order the engine expects).
    let generics_decl = if own_params.is_empty() {
        TokenStream::new()
    } else {
        let bounded = own_params.iter().map(|param| {
            let ident = idents::ident(param);
            quote! { #ident: ::baml_bridge::BamlValue }
        });
        quote! { <#(#bounded),*> }
    };
    let type_args_expr = if scope.is_empty() {
        quote! { ::std::vec![] }
    } else {
        let entries = scope.iter().map(|param| {
            let ident = idents::ident(param);
            quote! {
                (
                    #param,
                    <#ident as ::baml_bridge::baml_value::internal::__BamlValuePrivate>::baml_ty(),
                )
            }
        });
        quote! { ::baml_bridge::encode::type_args(::std::vec![#(#entries),*]) }
    };

    Ok(quote! {
        #(#doc_attrs)*
        #too_many_arguments_attr
        pub fn #sync_name #generics_decl (#self_param #(#params),*) -> #result_ty {
            crate::_runtime::ensure_init().map_err(::baml_bridge::Error::Sdk)?;
            #(#converts)*
            ::baml_bridge::runtime::invoke_sync(
                #fqn,
                ::baml_bridge::encode::kwargs(::std::vec![#(#kwarg_entries),*]),
                #type_args_expr,
            )
        }

        #(#doc_attrs)*
        #too_many_arguments_attr
        pub async fn #async_name #generics_decl (#self_param #(#params),*) -> #result_ty {
            crate::_runtime::ensure_init().map_err(::baml_bridge::Error::Sdk)?;
            #(#converts)*
            ::baml_bridge::runtime::invoke(
                #fqn,
                ::baml_bridge::encode::kwargs(::std::vec![#(#kwarg_entries),*]),
                #type_args_expr,
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

/// Collect the unqualified leaf names of the thrown types in a `throws`
/// `Ty`, in source order, de-duping exact-equal names. Class/Enum/TypeAlias
/// contribute their unqualified leaf name; a union contributes each
/// member's; anything else (primitives) contributes nothing. Mirrors the
/// python emitter's `collect_raises_names` so both SDKs document the same
/// name set.
fn raises_names(throws: Option<&baml_codegen_types::Ty>) -> Vec<String> {
    use baml_codegen_types::Ty;

    fn walk(ty: &Ty, out: &mut Vec<String>) {
        match ty {
            Ty::Class(name, _, _) | Ty::Enum(name, _) | Ty::TypeAlias(name, _) => {
                let n = name.name().as_str().to_string();
                if !out.contains(&n) {
                    out.push(n);
                }
            }
            Ty::Union(members, _) => members.iter().for_each(|m| walk(m, out)),
            _ => {}
        }
    }

    let mut out = Vec::new();
    if let Some(ty) = throws {
        walk(ty, &mut out);
    }
    out
}

/// Append the `# Errors` rustdoc section naming the BAML `throws`
/// contract's types — the section `clippy::missing_errors_doc` asks for,
/// and the Rust rendering of the same names python puts in a Google-style
/// `Raises:` docstring block. No-op when the contract names no nominal
/// types (including `throws: None`).
fn append_errors_section(attrs: &mut Vec<TokenStream>, names: &[String]) {
    if names.is_empty() {
        return;
    }
    if !attrs.is_empty() {
        attrs.push(quote! { #[doc = ""] });
    }
    attrs.push(quote! { #[doc = " # Errors"] });
    attrs.push(quote! { #[doc = ""] });
    let listed = names
        .iter()
        .map(|n| format!("`{n}`"))
        .collect::<Vec<_>>()
        .join(", ");
    let line = format!(" Throws {listed}.");
    attrs.push(quote! { #[doc = #line] });
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
