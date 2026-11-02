//! Named tuple struct emission, for BAML tuples wider than the native Rust
//! tuples `baml_bridge` implements its value trait for (arity 1 through
//! [`crate::translate_ty::MAX_BRIDGE_TUPLE_ARITY`]).
//!
//! The struct is a tuple struct (`Tuple13_IntString…(pub i64, pub String,
//! …)`), so its slots read as a native tuple's do, `.0` through `.N-1`, and
//! it converts exactly as a native tuple does: inbound as a list node
//! stating the tuple type, each slot encoded against its own slot of that
//! type; outbound from a `tuple_value` of exactly its arity, each slot
//! decoded at its own type. Any other shape or arity is a decode error.
//!
//! Under the `serde` feature it is a JSON array, as a BAML tuple is.

use proc_macro2::TokenStream;
use quote::quote;

use crate::{
    idents,
    translate_ty::{self, TyCtx},
    unions::TupleStruct,
    wire_ty,
};

/// Emit the struct and its conversions for one named tuple, or `None` when
/// a slot does not translate: the struct is registered for every symbol
/// that mentions the tuple, and one that is skipped (a slot references a
/// skipped type) leaves no reference to it.
pub(crate) fn emit(tuple: &TupleStruct, ctx: &TyCtx<'_>) -> Option<TokenStream> {
    let ident = idents::ident(&tuple.rust_name);
    let name_str = tuple.rust_name.as_str();
    let arity = tuple.items.len();

    let param_idents: Vec<_> = tuple
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

    let own_type_var = wire_ty::own_type_var(&tuple.generic_params);
    let mut slot_types = Vec::with_capacity(arity);
    let mut slot_wire_types = Vec::with_capacity(arity);
    for item in &tuple.items {
        slot_types.push(translate_ty::translate(item, ctx).ok()?);
        slot_wire_types.push(wire_ty::wire_ty(item, ctx, &own_type_var).ok()?);
    }

    let encodes = slot_types.iter().enumerate().map(|(index, slot_ty)| {
        let field = syn_index(index);
        quote! {
            ::baml_bridge::baml_value::internal::__BamlValuePrivate::to_baml_in(
                &self.#field,
                &::baml_bridge::baml_value::internal::tuple_element_slot::<Self, #slot_ty>(
                    slot, #index,
                ),
                encoder,
            )?
        }
    });
    let decodes = slot_types.iter().map(|slot_ty| {
        quote! {
            <#slot_ty as ::baml_bridge::baml_value::internal::__BamlValuePrivate>::from_baml(
                items.next().expect("arity checked"),
            )?
        }
    });

    let has_serde = tuple.items.iter().all(|item| ctx.analysis.has_serde(item));
    let serde_impls = if has_serde {
        let serialize_generics = if param_idents.is_empty() {
            TokenStream::new()
        } else {
            quote! { <#(#param_idents: ::baml_bridge::BamlValue + ::serde::Serialize),*> }
        };
        let serialize_slots = (0..arity).map(|index| {
            let field = syn_index(index);
            quote! { ::serde::ser::SerializeTuple::serialize_element(&mut seq, &self.#field)?; }
        });
        let deserialize_slots = slot_types.iter().map(|slot_ty| {
            quote! {
                <#slot_ty as ::serde::Deserialize<'de>>::deserialize(
                    items.next().expect("arity checked"),
                )
                .map_err(<__D::Error as ::serde::de::Error>::custom)?
            }
        });
        let wrong_arity = format!("expected a JSON array of {arity} elements for `{name_str}`");
        quote! {
            #[cfg(feature = "serde")]
            impl #serialize_generics ::serde::Serialize for #ident #type_generics {
                fn serialize<__S: ::serde::Serializer>(
                    &self,
                    serializer: __S,
                ) -> ::std::result::Result<__S::Ok, __S::Error> {
                    let mut seq = serializer.serialize_tuple(#arity)?;
                    #(#serialize_slots)*
                    ::serde::ser::SerializeTuple::end(seq)
                }
            }

            #[cfg(feature = "serde")]
            impl<'de #(, #param_idents: ::baml_bridge::BamlValue + ::serde::Deserialize<'de>)*>
                ::serde::Deserialize<'de> for #ident #type_generics
            {
                fn deserialize<__D: ::serde::Deserializer<'de>>(
                    deserializer: __D,
                ) -> ::std::result::Result<Self, __D::Error> {
                    let items = <::std::vec::Vec<::baml_bridge::serde_json::Value>
                        as ::serde::Deserialize>::deserialize(deserializer)?;
                    if items.len() != #arity {
                        return ::std::result::Result::Err(
                            <__D::Error as ::serde::de::Error>::custom(#wrong_arity),
                        );
                    }
                    let mut items = items.into_iter();
                    ::std::result::Result::Ok(Self(#(#deserialize_slots,)*))
                }
            }
        }
    } else {
        TokenStream::new()
    };

    let doc = format!(
        " The BAML tuple `{}`: wider than the native Rust tuples the bridge \
         converts, so a named tuple struct whose slots are `.0` through `.{}`.",
        render_tuple(&tuple.items),
        arity - 1
    );
    Some(quote! {
        #[doc = #doc]
        #[derive(Debug, Clone, PartialEq)]
        pub struct #ident #bounded_generics (#(pub #slot_types,)*);

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

            fn to_baml_in(
                &self,
                slot: &::baml_bridge::wire::BamlTy,
                encoder: &mut ::baml_bridge::baml_value::internal::Encoder,
            ) -> ::std::result::Result<
                ::baml_bridge::wire::InboundValue,
                ::baml_bridge::EncodeError,
            > {
                let values = encoder.nested(|encoder| {
                    ::std::result::Result::Ok(::std::vec![#(#encodes,)*])
                })?;
                ::std::result::Result::Ok(
                    ::baml_bridge::baml_value::internal::tuple_value(slot, values),
                )
            }

            fn from_baml(
                v: ::baml_bridge::wire::BamlOutboundValue,
            ) -> ::std::result::Result<Self, ::baml_bridge::DecodeError> {
                let mut items =
                    ::baml_bridge::baml_value::internal::tuple_items(v, #arity, #name_str)?;
                ::std::result::Result::Ok(Self(#(#decodes,)*))
            }

            fn baml_ty() -> ::baml_bridge::wire::BamlTy {
                ::baml_bridge::baml_value::internal::tuple_ty(::std::vec![
                    #(#slot_wire_types,)*
                ])
            }
        }
    })
}

/// A tuple-struct field index (`self.0`), spelled unsuffixed.
fn syn_index(index: usize) -> proc_macro2::Literal {
    proc_macro2::Literal::usize_unsuffixed(index)
}

fn render_tuple(items: &[baml_sdkgen_types::Ty]) -> String {
    let items = items
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>();
    format!("({})", items.join(", "))
}
