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
    analyze::{self, ClassShape},
    idents,
    translate_ty::{self, TyCtx},
    unions, wire_ty,
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
        ClassShape::Opaque => {
            debug_assert!(
                class_params.is_empty(),
                "the analysis skips a generic opaque class: `{name}`"
            );
            opaque_instance(name, class, ctx, &method_tokens)
        }
        ClassShape::Bridged | ClassShape::Media => {
            unreachable!("the analysis emits no class with a crossing of its own: `{name}`")
        }
    };
    Ok((tokens, warnings))
}

/// A class crossing by reference: a private engine handle, no public
/// constructor and no fields, methods that send the handle as their
/// receiver, encoding that sends the handle, and equality by key.
fn opaque_instance(
    name: &Name,
    class: &Class,
    ctx: &TyCtx<'_>,
    method_tokens: &[TokenStream],
) -> TokenStream {
    let fqn = name.to_string();
    let class_name = ctx.analysis.names.type_name(name);
    let ident = idents::ident(class_name);
    let note = format!(
        "A `{class_name}` is the runtime's own instance, held by reference: the \
         runtime returns one and receives it back as itself. It has no \
         constructor, and two values are equal when one is a clone of the other."
    );
    let docs = doc_attrs(Some(&match class.docstring.as_deref() {
        Some(docstring) => format!("{docstring}\n\n{note}"),
        None => note,
    }));
    let methods_impl = if method_tokens.is_empty() {
        TokenStream::new()
    } else {
        quote! {
            impl #ident {
                #(#method_tokens)*
            }
        }
    };
    quote! {
        #(#docs)*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct #ident(::baml_bridge::baml_value::internal::RuntimeInstance);

        #methods_impl

        impl ::baml_bridge::baml_value::internal::__BamlValuePrivate for #ident {
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
                ::baml_bridge::baml_value::internal::RuntimeInstance::decode(v, #fqn).map(Self)
            }

            fn baml_ty() -> ::baml_bridge::wire::BamlTy {
                ::baml_bridge::baml_value::internal::class_ty(#fqn, ::std::vec![])
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
    let ident = idents::ident(field_ctx.analysis.names.type_name(name));
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
    // Under the `serde` feature the JSON keys are the fields' `@json.rename`
    // or BAML names, as in BAML's own JSON. A class that cannot derive says
    // why on its docs, where a user missing the impl looks first.
    let derives_serde = field_ctx.analysis.derives_serde(name);
    let fallback = properties.iter().find(|prop| prop.json_fallback);
    let flatten = properties.iter().find(|prop| prop.json_flatten);
    let serde_attrs = if derives_serde {
        // A `@json.flatten` class writes through its own impl, and a
        // `@json.fallback` class reads and writes through its own.
        match (fallback, flatten) {
            (Some(_), _) => TokenStream::new(),
            (None, Some(_)) => {
                quote! { #[cfg_attr(feature = "serde", derive(::serde::Deserialize))] }
            }
            (None, None) => quote! {
                #[cfg_attr(feature = "serde", derive(::serde::Serialize, ::serde::Deserialize))]
            },
        }
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

    // Field attributes are for serde's derive, which a `@json.fallback` class
    // does without.
    let derived_fields = derives_serde && fallback.is_none();
    let mut field_defs = Vec::new();
    let mut literal_checks = Vec::new();
    let mut serialize_entries = Vec::new();
    let mut fallback_fields = Vec::new();
    let mut fallback_tags = Vec::new();
    let mut encode_entries = Vec::new();
    let mut decode_fields = Vec::new();
    let mut required_fields = Vec::new();
    let mut optional_fields = Vec::new();
    for prop in properties {
        let field_name = prop.name.as_str();
        let field_ident = idents::ident(field_ctx.analysis.names.field(name, &prop.name));
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
        let json_key = analyze::json_key(prop);
        let serde_rename = (derived_fields && !prop.json_flatten && field_ident != json_key)
            .then(|| quote! { #[cfg_attr(feature = "serde", serde(rename = #json_key))] });
        let serde_flatten = (derived_fields && prop.json_flatten)
            .then(|| quote! { #[cfg_attr(feature = "serde", serde(flatten))] });
        let omit_null = class.json_omit_nulls && is_optional(&prop.ty, name, field_ctx.analysis);
        let serde_omit = (derived_fields && omit_null).then(|| {
            quote! {
                #[cfg_attr(
                    feature = "serde",
                    serde(default, skip_serializing_if = "::std::option::Option::is_none")
                )]
            }
        });
        // Rust has no literal types, so the check a literal field stands for
        // is written out: without it, every string fits a tag.
        let literal = match &prop.ty {
            Ty::Literal(literal, ..) => Some(literal),
            _ => None,
        };
        let literal_check = literal.filter(|_| derived_fields).and_then(|literal| {
            literal_check(
                field_ctx.analysis.names.type_name(name),
                field_name,
                literal,
            )
        });
        let serde_literal = literal_check.as_ref().map(|(path, _)| {
            quote! { #[cfg_attr(feature = "serde", serde(deserialize_with = #path))] }
        });
        literal_checks.extend(literal_check.map(|(_, check)| check));
        if let Some(check) = literal.and_then(tag_kind_check) {
            let missing = format!("missing required field `{json_key}`");
            fallback_tags.push(quote! {
                if !__baml_raw.get(#json_key).is_some_and(#check) {
                    return ::std::result::Result::Err(
                        <__D::Error as ::serde::de::Error>::custom(#missing),
                    );
                }
            });
        }
        fallback_fields.push(if prop.json_fallback {
            quote! { #field_ident: __baml_raw }
        } else {
            let value = literal.and_then(literal_value).unwrap_or_else(|| {
                quote! { ::std::default::Default::default() }
            });
            quote! { #field_ident: #value }
        });
        serialize_entries.push(if prop.json_flatten {
            let declared = properties
                .iter()
                .filter(|other| !other.json_flatten)
                .map(analyze::json_key);
            quote! {
                let declared: &[&str] = &[#(#declared),*];
                for (key, value) in &self.#field_ident {
                    if !declared.contains(&key.as_str()) {
                        map.serialize_entry(key, value)?;
                    }
                }
            }
        } else if omit_null {
            quote! {
                if let ::std::option::Option::Some(value) = &self.#field_ident {
                    map.serialize_entry(#json_key, value)?;
                }
            }
        } else {
            quote! { map.serialize_entry(#json_key, &self.#field_ident)?; }
        });
        // BAML reads a missing `json` field as `null` (the type admits it), so
        // serde must too, through user aliases of `json` as well. `Option`
        // fields already read a missing key as `None`.
        let serde_default = (derived_fields
            && matches!(&prop.ty, Ty::TypeAlias(name) if field_ctx.analysis.is_json_alias(name)))
        .then(|| quote! { #[cfg_attr(feature = "serde", serde(default))] });
        field_defs.push(quote! {
            #(#field_docs)*
            #serde_rename
            #serde_flatten
            #serde_omit
            #serde_literal
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
        if is_optional(&prop.ty, name, field_ctx.analysis) {
            optional_fields.push(field_ident);
        } else {
            required_fields.push((field_ident, field_ty));
        }
    }
    let constructor = constructor(
        &idents::ident(field_ctx.analysis.names.constructor(name)),
        &ident,
        &required_fields,
        &optional_fields,
    );

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

    let methods_impl = quote! {
        impl #bounded_generics #ident #type_generics {
            #constructor
            #(#method_tokens)*
        }
    };

    let serialize_generics = if param_idents.is_empty() {
        TokenStream::new()
    } else {
        quote! { <#(#param_idents: ::baml_bridge::BamlValue + ::serde::Serialize),*> }
    };
    let deserialize_bounds = param_idents
        .iter()
        .map(|id| quote! { #id: ::baml_bridge::BamlValue + ::serde::Deserialize<'de> });
    let serde_impls = match (derives_serde, fallback, flatten) {
        (true, Some(raw), _) => {
            let raw = idents::ident(field_ctx.analysis.names.field(name, &raw.name));
            quote! {
                #[cfg(feature = "serde")]
                impl #serialize_generics ::serde::Serialize for #ident #type_generics {
                    fn serialize<__S: ::serde::Serializer>(
                        &self,
                        serializer: __S,
                    ) -> ::std::result::Result<__S::Ok, __S::Error> {
                        ::serde::Serialize::serialize(&self.#raw, serializer)
                    }
                }

                #[cfg(feature = "serde")]
                impl<'de #(, #deserialize_bounds)*> ::serde::Deserialize<'de>
                    for #ident #type_generics
                {
                    fn deserialize<__D: ::serde::Deserializer<'de>>(
                        deserializer: __D,
                    ) -> ::std::result::Result<Self, __D::Error> {
                        let __baml_raw: ::baml_bridge::Map<
                            ::std::string::String,
                            ::baml_bridge::Json,
                        > = ::serde::Deserialize::deserialize(deserializer)?;
                        #(#fallback_tags)*
                        ::std::result::Result::Ok(Self {
                            #(#fallback_fields,)*
                        })
                    }
                }
            }
        }
        // `serde(flatten)` would write a key twice when the map holds a key a
        // declared field writes. The declared field wins, as in BAML's JSON.
        (true, None, Some(_)) => quote! {
            #[cfg(feature = "serde")]
            impl #serialize_generics ::serde::Serialize for #ident #type_generics {
                fn serialize<__S: ::serde::Serializer>(
                    &self,
                    serializer: __S,
                ) -> ::std::result::Result<__S::Ok, __S::Error> {
                    use ::serde::ser::SerializeMap as _;
                    let mut map = serializer.serialize_map(::std::option::Option::None)?;
                    #(#serialize_entries)*
                    map.end()
                }
            }
        },
        _ => TokenStream::new(),
    };

    Ok(quote! {
        #(#docs)*
        #[derive(Debug, Clone, PartialEq)]
        #serde_attrs
        pub struct #ident #bounded_generics {
            #(#field_defs,)*
        }

        #(#literal_checks)*

        #serde_impls

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

/// The `serde_json::Value` method that says a tag's value is of `literal`'s
/// JSON kind. The value itself is whatever the server sent.
fn tag_kind_check(literal: &baml_base::Literal) -> Option<TokenStream> {
    match literal {
        baml_base::Literal::String(_) => {
            Some(quote! { ::baml_bridge::serde_json::Value::is_string })
        }
        baml_base::Literal::Bool(_) => {
            Some(quote! { ::baml_bridge::serde_json::Value::is_boolean })
        }
        baml_base::Literal::Int(_) | baml_base::Literal::Float(_) => {
            Some(quote! { ::baml_bridge::serde_json::Value::is_number })
        }
        baml_base::Literal::Bigint(_) => None,
    }
}

/// The value of a literal field, as its Rust type. `None` for the literals
/// a class holding them gets no serde impls for.
fn literal_value(literal: &baml_base::Literal) -> Option<TokenStream> {
    match literal {
        baml_base::Literal::String(value) => {
            let value = value.as_str();
            Some(quote! { ::std::string::String::from(#value) })
        }
        baml_base::Literal::Int(value) => Some(quote! { #value }),
        baml_base::Literal::Bool(value) => Some(quote! { #value }),
        baml_base::Literal::Float(value) => {
            let value = value.parse::<f64>().ok()?;
            Some(quote! { #value })
        }
        baml_base::Literal::Bigint(_) => None,
    }
}

/// The `deserialize_with` function that rejects any value but `literal` for
/// a field, and the path serde names it by. `None` for the literals with no
/// check to write.
fn literal_check(
    class: &str,
    field: &str,
    literal: &baml_base::Literal,
) -> Option<(String, TokenStream)> {
    let (rust_ty, expected) = match literal {
        baml_base::Literal::String(value) => {
            let value = value.as_str();
            (quote! { ::std::string::String }, quote! { #value })
        }
        baml_base::Literal::Int(_) => (quote! { ::core::primitive::i64 }, literal_value(literal)?),
        baml_base::Literal::Bool(_) => {
            (quote! { ::core::primitive::bool }, literal_value(literal)?)
        }
        baml_base::Literal::Float(_) | baml_base::Literal::Bigint(_) => return None,
    };
    // The length keeps `A_b` + `c` and `A` + `b_c` apart.
    let path = format!("__baml_literal_{}_{class}_{field}", class.len());
    let function = idents::ident(&path);
    let message = format!("`{class}.{field}` holds another value than its literal");
    let check = quote! {
        #[cfg(feature = "serde")]
        #[doc(hidden)]
        #[allow(non_snake_case, clippy::bool_comparison)]
        fn #function<'de, __D: ::serde::Deserializer<'de>>(
            deserializer: __D,
        ) -> ::std::result::Result<#rust_ty, __D::Error> {
            let value = <#rust_ty as ::serde::Deserialize<'de>>::deserialize(deserializer)?;
            if value == #expected {
                ::std::result::Result::Ok(value)
            } else {
                ::std::result::Result::Err(<__D::Error as ::serde::de::Error>::custom(#message))
            }
        }
    };
    Some((path, check))
}

/// Whether a field of `owner` is optional: its type has a `null` arm
/// (`T?`, `T | null`), or it names an alias that resolves to one. Such a
/// field is an `Option<_>` in Rust, so the constructor leaves it `None`.
///
/// The `null` arm is stripped and the rest wrapped outside any boxing, so
/// a nullable union is always `Option<_>`. Alias items are transparent
/// `pub type`s and never box their body, so an alias field is too, unless
/// the field itself boxes it (`Box<Alias>`, on a containment cycle). The
/// `json` alias and its aliases admit `null` but are a
/// `serde_json::Value`; they stay constructor arguments.
fn is_optional(ty: &Ty, owner: &Name, analysis: &crate::analyze::Analysis) -> bool {
    match ty {
        Ty::TypeAlias(alias) => {
            !analysis.needs_box(owner, alias) && alias_is_nullable(alias, analysis)
        }
        _ => is_nullable(ty),
    }
}

fn is_nullable(ty: &Ty) -> bool {
    matches!(ty, Ty::Union(items) if unions::strip_null(items).1)
}

/// Whether `alias` resolves, through any chain of aliases, to a nullable
/// union (and so its Rust `pub type` is an `Option<_>`).
fn alias_is_nullable(alias: &Name, analysis: &crate::analyze::Analysis) -> bool {
    let mut seen = std::collections::HashSet::new();
    let mut current = alias;
    loop {
        if analysis.is_json_alias(current) || !seen.insert(current) {
            return false;
        }
        match analysis.alias_body(current) {
            Some(Ty::TypeAlias(next)) => current = next,
            Some(body) => return is_nullable(body),
            None => return false,
        }
    }
}

/// The field constructor: every required field as an argument, in
/// declaration order, and every optional field `None`. Generated for every
/// fields struct, so a caller can build any class through it and keep
/// compiling when the class later gains an optional field.
fn constructor(
    fn_ident: &proc_macro2::Ident,
    type_ident: &proc_macro2::Ident,
    required: &[(proc_macro2::Ident, TokenStream)],
    optional: &[proc_macro2::Ident],
) -> TokenStream {
    let args = required
        .iter()
        .map(|(field, _)| field.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let call = format!("{type_ident}::{fn_ident}({args})");
    let mut docs = vec![if optional.is_empty() {
        format!(" A `{type_ident}` from its fields, in declaration order.")
    } else {
        format!(
            " A `{type_ident}` from its required fields, in declaration order, every optional field `None`."
        )
    }];
    docs.extend(
        [
            "",
            " Build the value through this constructor rather than a struct literal,",
            " setting any optional field with struct update syntax: the call keeps",
            " compiling when the class gains another optional field.",
            "",
            " ```ignore",
        ]
        .map(String::from),
    );
    docs.push(match optional.first() {
        Some(field) => {
            format!(" let value = {type_ident} {{ {field}: Some(/* ... */), ..{call} }};")
        }
        None => format!(" let value = {call};"),
    });
    docs.push(" ```".to_string());
    // Clippy asks a `new()` without arguments for a `Default` impl. The
    // SDK gives none on purpose: `Default` would also fill required fields,
    // and a forgotten one would be sent as `""` or `0`.
    let new_without_default = required
        .is_empty()
        .then(|| quote! { #[allow(clippy::new_without_default)] });
    let too_many_arguments =
        (required.len() > 7).then(|| quote! { #[allow(clippy::too_many_arguments)] });
    let params = required.iter().map(|(field, ty)| quote! { #field: #ty });
    let required = required.iter().map(|(field, _)| field);
    quote! {
        #(#[doc = #docs])*
        #new_without_default
        #too_many_arguments
        pub fn #fn_ident(#(#params),*) -> Self {
            Self {
                #(#required,)*
                #(#optional: ::std::option::Option::None,)*
            }
        }
    }
}
