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
            reason: "legacy `$` companion functions are not emitted; callable companions use `@`"
                .to_string(),
        });
    }
    // Generic free functions ARE emitted: their `<...>` params become Rust
    // generics bound by `BamlValue`. TypeVars that appear in positions the
    // translator can't represent still make the whole function skip, at the
    // per-type translation site.
    let binding_name = name.name().as_str().replace('@', "_");
    emit_binding(&fqn, &binding_name, function, Receiver::None, &[], ctx)
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
            reason: "legacy `$` companion methods are not emitted; callable companions use `@`"
                .to_string(),
        });
    }
    let binding_name = method_name.replace('@', "_");
    emit_binding(&fqn, &binding_name, method, receiver, class_params, ctx)
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
    // A callback parameter with an inferred `throws` carries a synthetic
    // effect-param typevar (the type system desugars `foo(cb: () -> void)` to
    // `foo<E>(cb: () -> void throws E) -> … throws E`). It is deliberately NOT
    // in `generic_params` — kept off the language-agnostic surface so erasing
    // SDKs are unaffected — but the Rust binding realizes it as an explicit
    // generic tied to the callback's associated `Throws`, so the inferred
    // outer `throws` (which references it) resolves.
    let effect_params = collect_effect_params(function, class_params, &own_params);
    // Translation scope: class params, the callee's own params, then the
    // effect params — a `Ty::TypeVar` naming any of them resolves.
    let translation_scope: Vec<String> = class_params
        .iter()
        .chain(&own_params)
        .chain(&effect_params)
        .cloned()
        .collect();
    // Wire type-args cover only the named generics the engine expects (class +
    // own, De Bruijn order); effect params are realized from the callback at
    // dispatch, never sent.
    let type_args_scope: Vec<String> = class_params.iter().chain(&own_params).cloned().collect();
    let gctx = TyCtx {
        generic_params: translation_scope.as_slice(),
        ..*ctx
    };
    // Compiler-injected controls have defaults at the VM boundary. Rust does
    // not yet expose optional host-callable controls, so omit them instead of
    // dropping the entire authored function. User-declared arguments are never
    // marked injected and remain fail-closed.
    let host_arguments: Vec<_> = function
        .arguments
        .iter()
        .filter(|argument| !argument.injected)
        .collect();

    let ret = translate_ty::translate(&function.return_type, &gctx)
        .map_err(|u| skip(format!("return: {}", u.reason)))?;
    let throws = match &function.throws {
        None => quote! { ::core::convert::Infallible },
        Some(ty) => translate_ty::translate_throws(ty, &gctx)
            .map_err(|u| skip(format!("throws contract: {}", u.reason)))?,
    };

    let mut params = Vec::new();
    let mut converts = Vec::new();
    let mut kwarg_entries = Vec::new();
    // Callable parameters contribute two generic parameters each — the
    // closure type and its `HostCallback` marker — merged into the binding's
    // generics, and are registered as host closures rather than sent by value.
    let mut callable_generic_decls: Vec<TokenStream> = Vec::new();
    let mut callable_index = 0usize;
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
    for arg in &host_arguments {
        let arg_name = arg.name.as_str();
        let param = idents::ident(arg_name);
        // A direct callable parameter (`(…) -> R`) is registered as a host
        // closure rather than sent as a value: the binding takes it as a
        // generic bound by `HostCallback`, erases it via `callable_handle`,
        // and sends the resulting handle on the wire. Only a *direct*,
        // non-defaulted callable is handled here; one nested in a
        // union/optional or behind a default falls through to `translate_ty`
        // and skips (fail closed) — "callables only in direct parameter
        // position".
        if arg.default.is_none() && matches!(&arg.ty, baml_codegen_types::Ty::Function { .. }) {
            let CallableParts {
                args_tuple,
                ret,
                throws: cb_throws,
                host_params,
            } = translate_callable(&arg.ty, &gctx)
                .map_err(|u| skip(format!("callback `{arg_name}`: {}", u.reason)))?;
            let cb_ident = format_ident!("__BamlCb{callable_index}");
            let marker_ident = format_ident!("__BamlCbMarker{callable_index}");
            callable_index += 1;
            // The BAML callable type's `throws` is the closure's error
            // contract, realized as the callback's associated `Throws`. The
            // outer binding's `throws` names the same type (the effect param),
            // so this ties the two together — the caller's closure determines
            // both.
            callable_generic_decls.push(quote! {
                #cb_ident: ::baml_bridge::HostCallback<
                    #args_tuple, #ret, #marker_ident, Throws = #cb_throws
                >
            });
            callable_generic_decls.push(quote! { #marker_ident });
            params.push(quote! { #param: #cb_ident });
            converts.push(quote! {
                let #param = ::baml_bridge::host_value::callable_handle(
                    #param,
                    &[#(#host_params),*],
                );
            });
            kwarg_entries.push(quote! {
                (#arg_name, ::std::option::Option::Some(#param))
            });
            continue;
        }
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
        Receiver::None if !host_arguments.is_empty() => {
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
    let arg_count = host_arguments.len()
        + usize::from(matches!(receiver, Receiver::RefSelf))
        + usize::from(fqn.ends_with("@stream"));
    let too_many_arguments_attr = if arg_count > 7 {
        quote! { #[allow(clippy::too_many_arguments)] }
    } else {
        TokenStream::new()
    };
    // BAML callable names are part of the user's schema and cannot be
    // renamed merely because one matches a conventional Rust trait method
    // such as `clone`. The generated binding intentionally remains an
    // inherent method.
    let schema_method_name_attr = if matches!(receiver, Receiver::RefSelf) {
        quote! { #[allow(clippy::should_implement_trait)] }
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
    // User generics and synthetic effect params are both emitted as
    // `<T: BamlValue>`: the effect param is inferred from its callback's
    // `Throws`, but it is a first-class generic in the Rust signature.
    let mut generic_decls: Vec<TokenStream> = own_params
        .iter()
        .chain(&effect_params)
        .map(|param| {
            let ident = idents::ident(param);
            quote! { #ident: ::baml_bridge::BamlValue }
        })
        .collect();
    generic_decls.extend(callable_generic_decls);
    let generics_decl = if generic_decls.is_empty() {
        TokenStream::new()
    } else {
        quote! { <#(#generic_decls),*> }
    };
    let type_args_expr = if type_args_scope.is_empty() {
        quote! { ::std::vec![] }
    } else {
        let entries = type_args_scope.iter().map(|param| {
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

    let stream_with_bindings = if fqn.ends_with("@stream") {
        let with_name = format_ident!("{}_with", idents::dir_segment(binding_name));
        let with_async_name = format_ident!("{}_with_async", idents::dir_segment(binding_name));
        quote! {
            #(#doc_attrs)*
            #too_many_arguments_attr
            #schema_method_name_attr
            pub fn #with_name #generics_decl (
                #self_param
                #(#params,)*
                options: ::baml_bridge::CallOptions,
            ) -> #result_ty {
                crate::_runtime::ensure_init().map_err(::baml_bridge::Error::Sdk)?;
                #(#converts)*
                let mut kwargs = ::baml_bridge::encode::kwargs(
                    ::std::vec![#(#kwarg_entries),*]
                );
                options.append_to(&mut kwargs);
                ::baml_bridge::runtime::invoke_sync(#fqn, kwargs, #type_args_expr)
            }

            #(#doc_attrs)*
            #too_many_arguments_attr
            #schema_method_name_attr
            pub async fn #with_async_name #generics_decl (
                #self_param
                #(#params,)*
                options: ::baml_bridge::CallOptions,
            ) -> #result_ty {
                crate::_runtime::ensure_init().map_err(::baml_bridge::Error::Sdk)?;
                #(#converts)*
                let mut kwargs = ::baml_bridge::encode::kwargs(
                    ::std::vec![#(#kwarg_entries),*]
                );
                options.append_to(&mut kwargs);
                ::baml_bridge::runtime::invoke(#fqn, kwargs, #type_args_expr).await
            }
        }
    } else {
        TokenStream::new()
    };

    Ok(quote! {
        #(#doc_attrs)*
        #too_many_arguments_attr
        #schema_method_name_attr
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
        #schema_method_name_attr
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

        #stream_with_bindings
    })
}

/// The Rust pieces a `(…) -> R` callable parameter contributes: the
/// closure's argument-tuple and return types for the [`HostCallback`] bound
/// (`::baml_bridge::HostCallback`), plus one [`HostParam`] descriptor per
/// callable parameter that the dispatcher slots incoming BAML args against.
struct CallableParts {
    /// `(A1, A2, …)` — the closure's argument tuple (`(A,)` for one arg,
    /// `()` for none).
    args_tuple: TokenStream,
    /// The closure's return type.
    ret: TokenStream,
    /// The closure's error type, for the `HostCallback::Throws` binding
    /// (`Infallible` for a `never`-throwing callable).
    throws: TokenStream,
    /// One `HostParam { name, optional }` initializer per callable parameter,
    /// in declaration order.
    host_params: Vec<TokenStream>,
}

/// Collect the synthetic effect-param typevars a function's direct callback
/// parameters carry in their inferred `throws` (the implicit `throws E`
/// generics), in declaration order and deduped. A callback whose `throws`
/// names a class or a *user* generic is already declared, so those are
/// excluded — only the synthetic ones (absent from `generic_params`) surface
/// here.
fn collect_effect_params(
    function: &Function,
    class_params: &[String],
    own_params: &[String],
) -> Vec<String> {
    let mut effect_params: Vec<String> = Vec::new();
    for arg in &function.arguments {
        if !arg.injected
            && arg.default.is_none()
            && let Some(baml_codegen_types::Ty::Function { throws, .. }) =
                crate::effect_rename::callback_root(&arg.ty)
            && let baml_codegen_types::Ty::TypeVar(name, _) = throws.as_ref()
        {
            let name = name.as_str();
            let already = class_params.iter().any(|p| p == name)
                || own_params.iter().any(|p| p == name)
                || effect_params.iter().any(|p| p == name);
            if !already {
                effect_params.push(name.to_string());
            }
        }
    }
    effect_params
}

/// Translate a callable's declared `throws` for its `HostCallback::Throws`
/// binding: `never` (the callable throws nothing) maps to `Infallible`;
/// anything else is an ordinary type (a synthetic/user throws generic, a BAML
/// error class, or the opaque `baml.errors.HostCallable`).
fn translate_throws(
    ty: &baml_codegen_types::Ty,
    ctx: &TyCtx<'_>,
) -> Result<TokenStream, translate_ty::Unsupported> {
    match ty {
        baml_codegen_types::Ty::Never { .. } => Ok(quote! { ::core::convert::Infallible }),
        _ => translate_ty::translate(ty, ctx),
    }
}

/// Translate a `Ty::Function` in direct parameter position into the pieces a
/// host-callable binding needs. The callable's declared `throws` does not
/// appear: the `HostCallback` bound leaves the closure's error type free (a
/// marker disambiguates infallible / typed-throw / opaque-throw closures), so
/// any error family is accepted and the BAML-declared contract governs only
/// the wire at runtime.
fn translate_callable(
    func: &baml_codegen_types::Ty,
    ctx: &TyCtx<'_>,
) -> Result<CallableParts, translate_ty::Unsupported> {
    let baml_codegen_types::Ty::Function {
        params,
        ret,
        throws,
        ..
    } = func
    else {
        unreachable!("translate_callable is only called on Ty::Function");
    };
    let mut arg_types = Vec::with_capacity(params.len());
    let mut host_params = Vec::with_capacity(params.len());
    for param in params {
        let inner = translate_ty::translate(&param.ty, ctx)?;
        // An optional callable parameter is delivered to the closure as
        // `Option<T>` — a BAML-omitted optional arrives as `None`, matching
        // the dispatcher's null-fill; a required one as `T`.
        let optional = matches!(
            param.mode,
            baml_codegen_types::CodegenFunctionParamMode::Optional
        );
        arg_types.push(if optional {
            quote! { ::std::option::Option<#inner> }
        } else {
            inner
        });
        // The name is load-bearing only for optionals (matched against the
        // wire arg name); a required param's name rides only in the
        // dispatcher's "missing argument" diagnostic.
        let name = param.name.as_ref().map_or("", baml_base::Name::as_str);
        host_params.push(quote! {
            ::baml_bridge::HostParam { name: #name, optional: #optional }
        });
    }
    // A one-element tuple needs its trailing comma; the parenthesized form
    // `(A)` is just `A`, not `(A,)`.
    let args_tuple = match arg_types.as_slice() {
        [] => quote! { () },
        [one] => quote! { (#one,) },
        many => quote! { (#(#many),*) },
    };
    let ret_ty: &baml_codegen_types::Ty = ret;
    let ret = translate_ty::translate(ret_ty, ctx)?;
    let throws_ty: &baml_codegen_types::Ty = throws;
    let throws = translate_throws(throws_ty, ctx)?;
    Ok(CallableParts {
        args_tuple,
        ret,
        throws,
        host_params,
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
                let n = name.bare_name().to_string();
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
