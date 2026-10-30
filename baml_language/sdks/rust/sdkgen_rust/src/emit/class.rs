//! Class emission: a plain struct with public fields plus the value
//! conversions — an owned tree, so a Rust host can never build the
//! self-referencing values the engine's boundary cannot carry — or, for a
//! class crossing by reference, an opaque wrapper around the engine's
//! instance.
//!
//! The impl shapes are pinned by the hand-written executable spec in
//! `bridge_rust/tests/conversions.rs`.

use baml_sdkgen_types::{Class, ClassProperty, Name, Ty};
use proc_macro2::TokenStream;
use quote::quote;

use super::function::{Receiver, doc_attrs, emit_method};
use crate::{
    SkipWarning,
    analyze::ClassShape,
    idents,
    translate_ty::{self, TyCtx},
    wire_ty,
};

/// Emit the type + conversion impls for a class the analysis marked
/// emitted, shaped by its [`ClassShape`], plus an `impl` block with its
/// method bindings. Also returns non-fatal warnings: a method whose
/// signature is not representable skips individually — it never poisons
/// the class or its sibling methods (methods play no part in the analysis
/// fixpoint).
///
/// A field-translation failure here means the analysis and the
/// translator disagree on the supported subset — a generator bug; the
/// caller escalates rather than skipping.
pub(crate) fn emit(
    name: &Name,
    class: &Class,
    ctx: &TyCtx<'_>,
) -> Result<(TokenStream, Vec<SkipWarning>), SkipWarning> {
    let mut warnings = Vec::new();

    // The class's own `<...>` params come into scope for translating its
    // fields: a `Ty::TypeVar` naming one resolves to that Rust generic
    // parameter. `boxing_for` is inherited from `ctx` (set to this class).
    let class_params: Vec<String> = class
        .generic_params
        .iter()
        .map(|param| param.as_str().to_string())
        .collect();
    let field_ctx = TyCtx {
        generic_params: class_params.as_slice(),
        ..*ctx
    };

    // Field-site boxing does not apply to method signatures: a method
    // mentioning a same-SCC class (e.g. a static factory returning the
    // class itself) is not a containment edge, so its types translate
    // without a boxing owner. Scope (class params for instance methods,
    // plus each method's own) is threaded through `emit_method` instead.
    let method_ctx = TyCtx {
        analysis: ctx.analysis,
        unions: ctx.unions,
        leaf: ctx.leaf,
        boxing_for: None,
        generic_params: &[],
    };
    let mut method_tokens: Vec<TokenStream> = Vec::new();
    // Statics bind only their own `<...>` params — no receiver exists, so
    // no class TypeVars ride the call ("no phantom class params"); the
    // Rust caller names the impl's class params (turbofish on the struct),
    // but those bindings are namespace-only and never sent. Instance
    // methods bind the class params (from the monomorphized receiver)
    // ahead of their own.
    for (methods, receiver, method_class_params) in [
        (&class.static_methods, Receiver::None, &[] as &[String]),
        (
            &class.instance_methods,
            Receiver::RefSelf,
            class_params.as_slice(),
        ),
    ] {
        for method in methods {
            match emit_method(name, method, receiver, method_class_params, &method_ctx) {
                Ok(tokens) => method_tokens.push(tokens),
                Err(warning) => warnings.push(warning),
            }
        }
    }

    let tokens = match ClassShape::of(class) {
        ClassShape::Fields(properties) => fields_struct(
            name,
            class,
            properties,
            &field_ctx,
            &class_params,
            &method_tokens,
        )?,
        ClassShape::Opaque => opaque_instance(name, class, &class_params, &method_tokens),
        ClassShape::Bridged | ClassShape::Media => {
            unreachable!("the analysis emits no class with a crossing of its own: `{name}`")
        }
    };
    Ok((tokens, warnings))
}

/// A class crossing by reference: a private engine handle, no public
/// constructor and no fields, methods that send the handle as their
/// receiver, encoding that sends the handle, and equality by key. A generic
/// class's instance is at the type arguments its Rust type names: it decodes
/// only an instance at exactly those, and its methods bind the class's
/// parameters from them.
fn opaque_instance(
    name: &Name,
    class: &Class,
    class_params: &[String],
    method_tokens: &[TokenStream],
) -> TokenStream {
    let fqn = name.to_string();
    let class_name = name.name().as_str();
    let ident = idents::ident(class_name);
    let note = format!(
        "A `{class_name}` is the runtime's own instance, held by reference: the \
         runtime returns one and receives it back as itself. It has no \
         constructor, and two values are equal when they are the same instance."
    );
    let docs = doc_attrs(Some(&match class.docstring.as_deref() {
        Some(docstring) => format!("{docstring}\n\n{note}"),
        None => note,
    }));
    let param_idents: Vec<_> = class_params.iter().map(|p| idents::ident(p)).collect();
    let type_args: Vec<_> = param_idents
        .iter()
        .map(|id| quote! { <#id as ::baml_bridge::baml_value::internal::__BamlValuePrivate>::baml_ty() })
        .collect();
    // The type parameters name what the instance is, not anything it holds:
    // a marker that owns no `T` and leaves the wrapper `Send`, `Sync` and
    // covariant whatever `T` is.
    let (bounded_generics, type_generics, marker, marker_value, wrap) = if param_idents.is_empty() {
        (
            TokenStream::new(),
            TokenStream::new(),
            TokenStream::new(),
            TokenStream::new(),
            quote! { Self },
        )
    } else {
        (
            quote! { <#(#param_idents: ::baml_bridge::BamlValue),*> },
            quote! { <#(#param_idents),*> },
            quote! { , ::std::marker::PhantomData<fn() -> (#(#param_idents,)*)> },
            quote! { , ::std::marker::PhantomData },
            quote! { |instance| Self(instance, ::std::marker::PhantomData) },
        )
    };
    let methods_impl = if method_tokens.is_empty() {
        TokenStream::new()
    } else {
        quote! {
            impl #bounded_generics #ident #type_generics {
                #(#method_tokens)*
            }
        }
    };
    quote! {
        #(#docs)*
        pub struct #ident #bounded_generics (
            ::baml_bridge::baml_value::internal::RuntimeInstance #marker
        );

        #methods_impl

        impl #bounded_generics ::std::clone::Clone for #ident #type_generics {
            fn clone(&self) -> Self {
                Self(::std::clone::Clone::clone(&self.0) #marker_value)
            }
        }

        impl #bounded_generics ::baml_bridge::baml_value::internal::Reference for #ident #type_generics {
            fn reference(&self) -> &::baml_bridge::baml_value::internal::RuntimeInstance {
                &self.0
            }
        }

        impl #bounded_generics ::std::cmp::PartialEq for #ident #type_generics {
            fn eq(&self, other: &Self) -> bool {
                self.0 == other.0
            }
        }

        impl #bounded_generics ::std::cmp::Eq for #ident #type_generics {}

        impl #bounded_generics ::std::hash::Hash for #ident #type_generics {
            fn hash<H: ::std::hash::Hasher>(&self, state: &mut H) {
                ::std::hash::Hash::hash(&self.0, state);
            }
        }

        impl #bounded_generics ::std::fmt::Debug for #ident #type_generics {
            fn fmt(&self, formatter: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                formatter.debug_tuple(#class_name).field(&self.0).finish()
            }
        }

        impl #bounded_generics ::baml_bridge::baml_value::internal::__BamlValuePrivate
            for #ident #type_generics
        {
            fn to_baml(
                &self,
                encoder: &mut ::baml_bridge::baml_value::internal::Encoder,
            ) -> ::std::result::Result<
                ::baml_bridge::wire::InboundValue,
                ::baml_bridge::EncodeError,
            > {
                self.0.to_baml(encoder)
            }

            fn from_baml(
                v: ::baml_bridge::wire::BamlOutboundValue,
            ) -> ::std::result::Result<Self, ::baml_bridge::DecodeError> {
                ::baml_bridge::baml_value::internal::RuntimeInstance::decode(
                    v,
                    #fqn,
                    &[#(#type_args),*],
                )
                .map(#wrap)
            }

            fn baml_ty() -> ::baml_bridge::wire::BamlTy {
                ::baml_bridge::baml_value::internal::class_ty(#fqn, ::std::vec![#(#type_args),*])
            }
        }
    }
}

/// A class crossing by its fields: a struct with public fields, structural
/// equality, and a field-by-field `class_value` codec.
fn fields_struct(
    name: &Name,
    class: &Class,
    properties: &[ClassProperty],
    field_ctx: &TyCtx<'_>,
    class_params: &[String],
    method_tokens: &[TokenStream],
) -> Result<TokenStream, SkipWarning> {
    let fqn = name.to_string();
    let ident = idents::ident(name.name().as_str());
    let docs = doc_attrs(class.docstring.as_deref());

    // A field is stated under the type it declares, the class's own type
    // variables spelled as the type arguments the instance is stated with
    // (bound in the emitted `to_baml_in`).
    let type_args_ident = quote::format_ident!("__baml_type_args");
    let instance_type_var = |name: &str| {
        class_params
            .iter()
            .position(|param| param == name)
            .map(|index| {
                quote! { #type_args_ident[#index].clone() }
            })
    };
    // Under the `serde` feature the JSON keys are the BAML field names, as
    // in BAML's own JSON. A class that cannot derive says why on its docs,
    // where a user missing the impl looks first.
    let derives_serde = field_ctx.analysis.derives_serde(name);
    let serde_attrs = if derives_serde {
        quote! { #[cfg_attr(feature = "serde", derive(::serde::Serialize, ::serde::Deserialize))] }
    } else {
        let note = match properties
            .iter()
            .find(|prop| !field_ctx.analysis.has_serde(&prop.ty))
        {
            Some(prop) => format!(
                " Not serializable with the `serde` feature: field `{}` holds a `bigint`, \
                 `uint8array`, function, handle or BAML standard-library class.",
                prop.name
            ),
            None => " Not serializable with the `serde` feature: BAML standard-library \
                     classes have no serde impls."
                .to_string(),
        };
        quote! { #[cfg_attr(feature = "serde", doc = "", doc = #note)] }
    };

    let mut field_defs = Vec::new();
    let mut encode_entries = Vec::new();
    let mut decode_fields = Vec::new();
    for prop in properties {
        let field_name = prop.name.as_str();
        let field_ident = idents::ident(field_name);
        let field_ty = translate_ty::translate(&prop.ty, field_ctx).map_err(|u| {
            SkipWarning::new(name, format!(
                "generator bug: analysis accepted field `{field_name}` but translation failed: {}",
                u.reason
            ))
        })?;
        let field_slot =
            wire_ty::wire_ty(&prop.ty, field_ctx, &instance_type_var).map_err(|u| {
                SkipWarning::new(
                    name,
                    format!(
                        "generator bug: analysis accepted field `{field_name}` but its type has no wire spelling: {}",
                        u.reason
                    ),
                )
            })?;
        let field_docs = doc_attrs(prop.docstring.as_deref());
        let serde_rename = (derives_serde && field_ident != field_name)
            .then(|| quote! { #[cfg_attr(feature = "serde", serde(rename = #field_name))] });
        // BAML reads a missing `json` field as `null` (the type admits it), so
        // serde must too, through user aliases of `json` as well. `Option`
        // fields already read a missing key as `None`.
        let serde_default = (derives_serde
            && matches!(&prop.ty, Ty::TypeAlias(name) if field_ctx.analysis.is_json_alias(name)))
        .then(|| quote! { #[cfg_attr(feature = "serde", serde(default))] });
        field_defs.push(quote! {
            #(#field_docs)*
            #serde_rename
            #serde_default
            pub #field_ident: #field_ty
        });
        encode_entries.push(quote! {
            (
                #field_name,
                ::baml_bridge::baml_value::internal::__BamlValuePrivate::to_baml_in(
                    &self.#field_ident,
                    &#field_slot,
                    encoder,
                )?,
            )
        });
        decode_fields.push(quote! {
            #field_ident: fields.take(#field_name)?
        });
    }

    // A field-less class still verifies the wire FQN on decode; it just
    // has nothing to take from the accessor afterwards.
    let decode_body = if properties.is_empty() {
        quote! {
            ::baml_bridge::decode::ClassFields::new(v, #fqn)?;
            ::std::result::Result::Ok(Self {})
        }
    } else {
        quote! {
            let mut fields = ::baml_bridge::decode::ClassFields::new(v, #fqn)?;
            ::std::result::Result::Ok(Self {
                #(#decode_fields,)*
            })
        }
    };

    // `<T: BamlValue, …>` on the struct + impl headers, `<T, …>` on the
    // referenced type, and one `<param>::baml_ty()` per param for the wire
    // type-arg channel. All empty for a non-generic class.
    let param_idents: Vec<_> = class_params.iter().map(|p| idents::ident(p)).collect();
    let (bounded_generics, type_generics, wire_type_args) = if param_idents.is_empty() {
        (
            TokenStream::new(),
            TokenStream::new(),
            quote! { ::std::vec![] },
        )
    } else {
        let bounded = param_idents
            .iter()
            .map(|id| quote! { #id: ::baml_bridge::BamlValue });
        let args = param_idents.iter().map(|id| {
            quote! { <#id as ::baml_bridge::baml_value::internal::__BamlValuePrivate>::baml_ty() }
        });
        (
            quote! { <#(#bounded),*> },
            quote! { <#(#param_idents),*> },
            quote! { ::std::vec![#(#args),*] },
        )
    };

    // A generic instance is stated with the type arguments of the slot it
    // fills, which can be narrower than its Rust type arguments spell (a
    // `Box<"a">` is a `Box<String>` in Rust); a non-generic class has only
    // the one instantiation.
    let arity = param_idents.len();
    // The fields are encoded one container level below the instance; a
    // field-less instance encodes none.
    let encoder_param = if properties.is_empty() {
        quote! { _encoder }
    } else {
        quote! { encoder }
    };
    let encoded_fields = quote! {
        |#encoder_param: &mut ::baml_bridge::baml_value::internal::Encoder| {
            ::std::result::Result::Ok(::std::vec![#(#encode_entries,)*])
        }
    };
    let to_baml_in = if param_idents.is_empty() {
        quote! {
            fn to_baml_in(
                &self,
                _slot: &::baml_bridge::wire::BamlTy,
                encoder: &mut ::baml_bridge::baml_value::internal::Encoder,
            ) -> ::std::result::Result<
                ::baml_bridge::wire::InboundValue,
                ::baml_bridge::EncodeError,
            > {
                ::baml_bridge::encode::class(#fqn, ::std::vec![], encoder, #encoded_fields)
            }
        }
    } else {
        quote! {
            fn to_baml_in(
                &self,
                slot: &::baml_bridge::wire::BamlTy,
                encoder: &mut ::baml_bridge::baml_value::internal::Encoder,
            ) -> ::std::result::Result<
                ::baml_bridge::wire::InboundValue,
                ::baml_bridge::EncodeError,
            > {
                let #type_args_ident = ::baml_bridge::baml_value::internal::class_slot_args::<Self>(
                    slot,
                    #fqn,
                    #arity,
                );
                ::baml_bridge::encode::class(
                    #fqn,
                    ::std::clone::Clone::clone(&#type_args_ident),
                    encoder,
                    #encoded_fields,
                )
            }
        }
    };

    let methods_impl = if method_tokens.is_empty() {
        TokenStream::new()
    } else {
        quote! {
            impl #bounded_generics #ident #type_generics {
                #(#method_tokens)*
            }
        }
    };

    Ok(quote! {
        #(#docs)*
        #[derive(Debug, Clone, PartialEq)]
        #serde_attrs
        pub struct #ident #bounded_generics {
            #(#field_defs,)*
        }

        #methods_impl

        impl #bounded_generics ::baml_bridge::baml_value::internal::__BamlValuePrivate
            for #ident #type_generics
        {
            fn to_baml(
                &self,
                encoder: &mut ::baml_bridge::baml_value::internal::Encoder,
            ) -> ::std::result::Result<
                ::baml_bridge::wire::InboundValue,
                ::baml_bridge::EncodeError,
            > {
                self.to_baml_in(
                    &<Self as ::baml_bridge::baml_value::internal::__BamlValuePrivate>::baml_ty(),
                    encoder,
                )
            }

            #to_baml_in

            fn from_baml(
                v: ::baml_bridge::wire::BamlOutboundValue,
            ) -> ::std::result::Result<Self, ::baml_bridge::DecodeError> {
                #decode_body
            }

            fn baml_ty() -> ::baml_bridge::wire::BamlTy {
                ::baml_bridge::baml_value::internal::class_ty(#fqn, #wire_type_args)
            }
        }
    })
}
