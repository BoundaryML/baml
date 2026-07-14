//! Class emission: a plain struct with public fields plus the value
//! conversions — an owned tree, so a Rust host can never build the
//! self-referencing values the engine's boundary cannot carry.
//!
//! The impl shape is pinned by the hand-written executable spec in
//! `bridge_rust/tests/conversions.rs`.

use baml_codegen_types::{Class, Name};
use proc_macro2::TokenStream;
use quote::quote;

use super::function::doc_attrs;
use crate::{
    SkipWarning, idents,
    translate_ty::{self, TyCtx},
};

/// Emit the struct + conversion impls for a class the analysis marked
/// emitted. Also returns non-fatal warnings (methods are not emitted
/// yet).
///
/// A translation failure here means the analysis and the translator
/// disagree on the supported subset — a generator bug; the caller
/// escalates rather than skipping.
pub(crate) fn emit(
    name: &Name,
    class: &Class,
    ctx: &TyCtx<'_>,
) -> Result<(TokenStream, Vec<SkipWarning>), SkipWarning> {
    let fqn = name.to_string();
    let mut warnings = Vec::new();
    let method_count = class.static_methods.len() + class.instance_methods.len();
    if method_count > 0 {
        warnings.push(SkipWarning {
            fqn: fqn.clone(),
            reason: format!("methods are not emitted yet ({method_count} skipped)"),
        });
    }

    let ident = idents::ident(name.name.as_str());
    let docs = doc_attrs(class.docstring.as_deref());

    let mut field_defs = Vec::new();
    let mut encode_entries = Vec::new();
    let mut decode_fields = Vec::new();
    for prop in &class.properties {
        let field_name = prop.name.as_str();
        let field_ident = idents::ident(field_name);
        let field_ty = translate_ty::translate(&prop.ty, ctx).map_err(|u| SkipWarning {
            fqn: fqn.clone(),
            reason: format!(
                "generator bug: analysis accepted field `{field_name}` but translation failed: {}",
                u.reason
            ),
        })?;
        let field_docs = doc_attrs(prop.docstring.as_deref());
        field_defs.push(quote! {
            #(#field_docs)*
            pub #field_ident: #field_ty
        });
        encode_entries.push(quote! {
            (
                #field_name,
                ::baml_rs::baml_value::internal::__BamlValuePrivate::to_baml(&self.#field_ident),
            )
        });
        decode_fields.push(quote! {
            #field_ident: fields.take(#field_name)?
        });
    }

    // A field-less class still verifies the wire FQN on decode; it just
    // has nothing to take from the accessor afterwards.
    let decode_body = if class.properties.is_empty() {
        quote! {
            ::baml_rs::decode::ClassFields::new(v, #fqn)?;
            ::std::result::Result::Ok(Self {})
        }
    } else {
        quote! {
            let mut fields = ::baml_rs::decode::ClassFields::new(v, #fqn)?;
            ::std::result::Result::Ok(Self {
                #(#decode_fields,)*
            })
        }
    };

    let tokens = quote! {
        #(#docs)*
        #[derive(Debug, Clone, PartialEq)]
        pub struct #ident {
            #(#field_defs,)*
        }

        impl ::baml_rs::baml_value::internal::__BamlValuePrivate for #ident {
            fn to_baml(&self) -> ::baml_rs::wire::InboundValue {
                ::baml_rs::encode::class(#fqn, ::std::vec![#(#encode_entries,)*])
            }

            fn from_baml(
                v: ::baml_rs::wire::BamlOutboundValue,
            ) -> ::std::result::Result<Self, ::baml_rs::DecodeError> {
                #decode_body
            }
        }
    };
    Ok((tokens, warnings))
}
