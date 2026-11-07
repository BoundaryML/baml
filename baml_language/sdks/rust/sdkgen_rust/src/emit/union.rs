//! Synthesized union-enum emission: one variant per (null-stripped) arm,
//! `From` per concrete payload arm so call sites can pass bare arm values
//! through the generated `impl Into<_>` parameters, and trial-order decode.
//!
//! A union with a `TypeVar` arm (`T | string`) becomes a *generic* enum
//! (`TOrString<T>`). The `TypeVar` arm gets no `From` impl: a blanket
//! `impl<T> From<T>` would overlap every concrete arm's `From` (they
//! coincide when `T` is instantiated to that arm's type), which Rust's
//! coherence rejects — so those arms are constructed by variant.
//!
//! The impl shape is pinned by the hand-written executable spec in
//! `bridge_rust/tests/conversions.rs`.
//!
//! Under the `serde` feature a union is untagged, as in BAML's own JSON: an
//! arm serializes as its payload (a literal arm as its string), and
//! deserializing takes the first arm, in declaration order, that the value
//! fits. A union with a `@json.fallback` arm picks by tag instead, with the
//! tree in [`baml_type::json_tags`]: reaching an arm is final, and the
//! fallback takes a tag value no arm has. The impls are
//! written out rather than derived with
//! `#[serde(untagged)]`: serde's untagged buffer cannot read a float while
//! `serde_json`'s `arbitrary_precision` feature is on, and `baml_bridge`
//! turns it on. Values are buffered as a `serde_json::Value` (the bridge's
//! re-exported `serde_json`) instead, and each arm reads an owned copy, so
//! a type parameter needs only the `Deserialize<'de>` bound serde's derive
//! puts on an enclosing class.

use baml_sdkgen_types::{
    Ty,
    json_tags::{TagTree, tag_tree},
};
use proc_macro2::TokenStream;
use quote::quote;

use crate::{
    SkipWarning, idents,
    translate_ty::{self, TyCtx},
    unions::{UnionArmKind, UnionEnum},
    wire_ty,
};

/// Emit the enum + `From` impls + conversions for one synthesized union.
///
/// A translation failure here means the registry and the translator
/// disagree on the supported subset — a generator bug; the caller
/// escalates rather than skipping.
pub(crate) fn emit(union_enum: &UnionEnum, ctx: &TyCtx<'_>) -> Result<TokenStream, SkipWarning> {
    let ident = idents::ident(&union_enum.rust_name);

    // `<T: BamlValue, …>` on the enum + impl headers, `<T, …>` on the
    // referenced type. Empty for a fully concrete union.
    let param_idents: Vec<_> = union_enum
        .generic_params
        .iter()
        .map(|p| idents::ident(p))
        .collect();
    let (bounded_generics, type_generics) = if param_idents.is_empty() {
        (TokenStream::new(), TokenStream::new())
    } else {
        let bounded = param_idents
            .iter()
            .map(|id| quote! { #id: ::baml_bridge::BamlValue });
        (quote! { <#(#bounded),*> }, quote! { <#(#param_idents),*> })
    };

    let mut variant_defs = Vec::new();
    let mut from_impls = Vec::new();
    let mut encode_arms = Vec::new();
    let mut throw_arms = Vec::new();
    let mut trial_decodes = Vec::new();
    let mut indexed_decodes = Vec::new();
    let mut baml_ty_options = Vec::new();
    let mut serialize_arms = Vec::new();
    let mut deserialize_trials = Vec::new();
    // Each class an arm can be, with the arm's read: the tree below chooses
    // among them, and a class behind an alias reads as its arm.
    let mut tagged_arms: Vec<(Vec<(String, baml_base::Literal)>, TokenStream)> = Vec::new();
    let mut fallback_arm = None;
    let name_str = union_enum.rust_name.as_str();
    let arity = union_enum.arms.len();
    // Each arm is the exact type it declares, a type variable arm spelled as
    // the Rust type the enum is instantiated with.
    let own_type_var = wire_ty::own_type_var(&union_enum.generic_params);
    for (index, arm) in union_enum.arms.iter().enumerate() {
        let variant = idents::ident(&arm.variant);
        match &arm.kind {
            UnionArmKind::Payload(ty) => {
                let payload = translate_ty::translate(ty, ctx).map_err(|u| SkipWarning {
                    fqn: union_enum.rust_name.clone(),
                    // A synthesized union has no package of its own, and a
                    // generator bug is always worth listing.
                    local: true,
                    reason: format!(
                        "generator bug: registered union arm failed to translate: {}",
                        u.reason
                    ),
                })?;
                let arm_ty = wire_ty::wire_ty(ty, ctx, &own_type_var).map_err(|u| SkipWarning {
                    fqn: union_enum.rust_name.clone(),
                    local: true,
                    reason: format!(
                        "generator bug: registered union arm has no wire spelling: {}",
                        u.reason
                    ),
                })?;
                baml_ty_options.push(arm_ty.clone());
                variant_defs.push(quote! { #variant(#payload) });
                // A `TypeVar` arm gets no `From` (coherence — see the module
                // docs); it is constructed by naming the variant.
                if !matches!(ty, Ty::TypeVar(..)) {
                    from_impls.push(quote! {
                        impl #bounded_generics ::std::convert::From<#payload>
                            for #ident #type_generics
                        {
                            fn from(value: #payload) -> Self {
                                Self::#variant(value)
                            }
                        }
                    });
                }
                encode_arms.push(quote! {
                    Self::#variant(value) => {
                        ::baml_bridge::baml_value::internal::__BamlValuePrivate::to_baml_in(
                            value,
                            &options[#index],
                            encoder,
                        )
                    }
                });
                throw_arms.push(quote! {
                    Self::#variant(value) => {
                        ::baml_bridge::baml_value::internal::__BamlValuePrivate::throw_in(
                            value,
                            &options[#index],
                            encoder,
                        )
                    }
                });
                indexed_decodes.push(quote! {
                    if ::baml_bridge::decode::selected_type_matches(&#arm_ty, &selected_type) {
                        return <#payload as ::baml_bridge::baml_value::internal::__BamlValuePrivate>::from_baml(v)
                            .map(Self::#variant);
                    }
                });
                trial_decodes.push(quote! {
                    if let ::std::result::Result::Ok(value) =
                        <#payload as ::baml_bridge::baml_value::internal::__BamlValuePrivate>::from_baml(
                            v.clone(),
                        )
                    {
                        return ::std::result::Result::Ok(Self::#variant(value));
                    }
                });
                serialize_arms.push(quote! {
                    Self::#variant(value) => ::serde::Serialize::serialize(value, serializer)
                });
                let read_arm = quote! {
                    <#payload as ::serde::Deserialize<'de>>::deserialize(json)
                        .map(Self::#variant)
                        .map_err(<__D::Error as ::serde::de::Error>::custom)
                };
                for tag in ctx.analysis.json_tags_in(ty) {
                    if tag.fallback {
                        fallback_arm = Some((read_arm.clone(), tag.literals.clone()));
                    } else {
                        tagged_arms.push((tag.literals.clone(), read_arm.clone()));
                    }
                }
                deserialize_trials.push(quote! {
                    if let ::std::result::Result::Ok(value) =
                        <#payload as ::serde::Deserialize<'de>>::deserialize(
                            ::std::clone::Clone::clone(&json),
                        )
                    {
                        return ::std::result::Result::Ok(Self::#variant(value));
                    }
                });
            }
            UnionArmKind::StringLiteral(value) => {
                let value = value.as_str();
                baml_ty_options.push(quote! {
                    ::baml_bridge::baml_value::internal::literal_string_ty(#value)
                });
                variant_defs.push(quote! { #variant });
                encode_arms.push(quote! {
                    Self::#variant => {
                        ::baml_bridge::baml_value::internal::__BamlValuePrivate::to_baml(
                            &<::std::string::String as ::std::convert::From<&str>>::from(#value),
                            encoder,
                        )
                    }
                });
                throw_arms.push(quote! {
                    Self::#variant => {
                        ::baml_bridge::baml_value::internal::__BamlValuePrivate::throw_in(
                            &<::std::string::String as ::std::convert::From<&str>>::from(#value),
                            &options[#index],
                            encoder,
                        )
                    }
                });
                indexed_decodes.push(quote! {
                    if ::baml_bridge::decode::selected_type_matches(
                        &::baml_bridge::baml_value::internal::literal_string_ty(#value),
                        &selected_type,
                    ) {
                        let value = <::std::string::String as ::baml_bridge::baml_value::internal::__BamlValuePrivate>::from_baml(v)?;
                        if value == #value {
                            return ::std::result::Result::Ok(Self::#variant);
                        }
                        return ::std::result::Result::Err(
                            ::baml_bridge::decode::no_union_arm(#name_str, &::baml_bridge::wire::BamlOutboundValue {
                                value: ::std::option::Option::Some(
                                    ::baml_bridge::wire::baml_outbound_value::Value::StringValue(value),
                                ),
                            }),
                        );
                    }
                });
                trial_decodes.push(quote! {
                    if let ::std::result::Result::Ok(value) =
                        <::std::string::String as ::baml_bridge::baml_value::internal::__BamlValuePrivate>::from_baml(
                            v.clone(),
                        )
                        && value == #value
                    {
                        return ::std::result::Result::Ok(Self::#variant);
                    }
                });
                serialize_arms.push(quote! {
                    Self::#variant => serializer.serialize_str(#value)
                });
                deserialize_trials.push(quote! {
                    if json.as_str() == ::std::option::Option::Some(#value) {
                        return ::std::result::Result::Ok(Self::#variant);
                    }
                });
            }
        }
    }

    // Only a payload arm fills a slot from the union's; a string-literal arm
    // is a string that implies its own type.
    let has_payload = union_enum
        .arms
        .iter()
        .any(|arm| matches!(arm.kind, UnionArmKind::Payload(_)));
    // A thrown union value is thrown as its arm, filling the arm's slot: an
    // arm that is a host error the runtime transports is raised.
    let to_baml_in = if has_payload {
        quote! {
            fn to_baml_in(
                &self,
                slot: &::baml_bridge::wire::BamlTy,
                encoder: &mut ::baml_bridge::baml_value::internal::Encoder,
            ) -> ::std::result::Result<
                ::baml_bridge::wire::InboundValue,
                ::baml_bridge::EncodeError,
            > {
                let options =
                    ::baml_bridge::baml_value::internal::union_slot_options::<Self>(slot, #arity);
                match self {
                    #(#encode_arms,)*
                }
            }

            fn throw_in(
                &self,
                slot: &::baml_bridge::wire::BamlTy,
                encoder: &mut ::baml_bridge::baml_value::internal::Encoder,
            ) -> ::std::result::Result<
                ::baml_bridge::baml_value::internal::HostThrow,
                ::baml_bridge::EncodeError,
            > {
                let options =
                    ::baml_bridge::baml_value::internal::union_slot_options::<Self>(slot, #arity);
                match self {
                    #(#throw_arms,)*
                }
            }
        }
    } else {
        quote! {
            fn to_baml_in(
                &self,
                _slot: &::baml_bridge::wire::BamlTy,
                encoder: &mut ::baml_bridge::baml_value::internal::Encoder,
            ) -> ::std::result::Result<
                ::baml_bridge::wire::InboundValue,
                ::baml_bridge::EncodeError,
            > {
                match self {
                    #(#encode_arms,)*
                }
            }
        }
    };

    let has_serde = union_enum.arms.iter().all(|arm| match &arm.kind {
        UnionArmKind::Payload(ty) => ctx.analysis.has_serde(ty),
        UnionArmKind::StringLiteral(_) => true,
    });
    let serde_impls = if has_serde {
        let no_arm = format!("value matches no arm of union `{name_str}`");
        let by_tag = fallback_arm.and_then(|(fallback_arm, fallback_literals)| {
            let literals: Vec<_> = tagged_arms.iter().map(|(tags, _)| tags.clone()).collect();
            let tree = tag_tree(&literals, &fallback_literals).ok()?;
            let reads: Vec<_> = tagged_arms.iter().map(|(_, read)| read).collect();
            let pick = pick_by_tag(&tree, &reads);
            Some(quote! {
                if json.is_object() {
                    #pick
                    return #fallback_arm;
                }
            })
        });
        let serialize_generics = if param_idents.is_empty() {
            TokenStream::new()
        } else {
            quote! { <#(#param_idents: ::baml_bridge::BamlValue + ::serde::Serialize),*> }
        };
        quote! {
            #[cfg(feature = "serde")]
            impl #serialize_generics ::serde::Serialize for #ident #type_generics {
                fn serialize<__S: ::serde::Serializer>(
                    &self,
                    serializer: __S,
                ) -> ::std::result::Result<__S::Ok, __S::Error> {
                    match self {
                        #(#serialize_arms,)*
                    }
                }
            }

            #[cfg(feature = "serde")]
            impl<'de #(, #param_idents: ::baml_bridge::BamlValue + ::serde::Deserialize<'de>)*>
                ::serde::Deserialize<'de> for #ident #type_generics
            {
                fn deserialize<__D: ::serde::Deserializer<'de>>(
                    deserializer: __D,
                ) -> ::std::result::Result<Self, __D::Error> {
                    let json = <::baml_bridge::serde_json::Value as ::serde::Deserialize>::deserialize(
                        deserializer,
                    )?;
                    #by_tag
                    #(#deserialize_trials)*
                    ::std::result::Result::Err(<__D::Error as ::serde::de::Error>::custom(#no_arm))
                }
            }
        }
    } else {
        TokenStream::new()
    };

    Ok(quote! {
        #[derive(Debug, Clone, PartialEq)]
        pub enum #ident #bounded_generics {
            #(#variant_defs,)*
        }

        #(#from_impls)*

        #serde_impls

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
                let (selected_type, v) = ::baml_bridge::decode::union_selected(v)?;
                if let ::std::option::Option::Some(selected_type) = selected_type {
                    #(#indexed_decodes)*
                    return ::std::result::Result::Err(
                        ::baml_bridge::decode::no_union_arm(#name_str, &v),
                    );
                }
                let v = ::baml_bridge::decode::unwrap(v);
                #(#trial_decodes)*
                ::std::result::Result::Err(::baml_bridge::decode::no_union_arm(#name_str, &v))
            }

            fn baml_ty() -> ::baml_bridge::wire::BamlTy {
                ::baml_bridge::baml_value::internal::union_ty(::std::vec![
                    #(#baml_ty_options,)*
                ])
            }
        }
    })
}

/// The code that returns the member `tree` picks for the object `json`, and
/// falls out of the end for a tag value no member has. The same tree the
/// compiler checked and the engine walks.
fn pick_by_tag(tree: &TagTree<baml_base::Literal>, reads: &[&TokenStream]) -> TokenStream {
    let (key, arms) = match tree {
        TagTree::Member(member) => {
            let read = reads[*member];
            return quote! { return #read; };
        }
        TagTree::Read { key, arms } => (key, arms),
    };
    let values = arms.iter().map(|(literal, next)| {
        let holds = literal_holds(literal);
        let next = pick_by_tag(next, reads);
        quote! { ::std::option::Option::Some(__tag) if #holds => { #next } }
    });
    let kind = arms.first().map_or_else(
        || quote! { true },
        |(literal, _)| match literal {
            baml_base::Literal::String(_) => quote! { __tag.is_string() },
            baml_base::Literal::Bool(_) => quote! { __tag.is_boolean() },
            _ => quote! { __tag.is_number() },
        },
    );
    let wrong_kind = format!("the tag `{key}` holds the wrong kind of value");
    let missing = format!("missing required field `{key}`");
    quote! {
        match json.get(#key) {
            #(#values)*
            ::std::option::Option::Some(__tag) if #kind => {}
            ::std::option::Option::Some(_) => {
                return ::std::result::Result::Err(
                    <__D::Error as ::serde::de::Error>::custom(#wrong_kind),
                );
            }
            ::std::option::Option::None => {
                return ::std::result::Result::Err(
                    <__D::Error as ::serde::de::Error>::custom(#missing),
                );
            }
        }
    }
}

/// Whether `__tag` holds `literal`.
fn literal_holds(literal: &baml_base::Literal) -> TokenStream {
    match literal {
        baml_base::Literal::String(expected) => {
            let expected = expected.as_str();
            quote! { __tag.as_str() == ::std::option::Option::Some(#expected) }
        }
        baml_base::Literal::Int(expected) => {
            quote! { __tag.as_i64() == ::std::option::Option::Some(#expected) }
        }
        baml_base::Literal::Bool(expected) => {
            quote! { __tag.as_bool() == ::std::option::Option::Some(#expected) }
        }
        baml_base::Literal::Float(expected) => {
            let expected = expected.parse::<f64>().unwrap_or(f64::NAN);
            quote! { __tag.as_f64() == ::std::option::Option::Some(#expected) }
        }
        baml_base::Literal::Bigint(_) => quote! { false },
    }
}
