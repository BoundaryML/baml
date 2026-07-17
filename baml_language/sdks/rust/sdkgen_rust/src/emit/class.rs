//! Class emission: a plain struct with public fields plus the value
//! conversions — an owned tree, so a Rust host can never build the
//! self-referencing values the engine's boundary cannot carry.
//!
//! The impl shape is pinned by the hand-written executable spec in
//! `bridge_rust/tests/conversions.rs`.

use baml_codegen_types::{Class, Name};
use proc_macro2::TokenStream;
use quote::quote;

use super::function::{Receiver, doc_attrs, emit_method};
use crate::{
    SkipWarning, idents,
    translate_ty::{self, TyCtx},
};

/// Emit the struct + conversion impls for a class the analysis marked
/// emitted, plus an `impl` block with its method bindings. Also returns
/// non-fatal warnings: a method whose signature is not representable
/// skips individually — it never poisons the class or its sibling
/// methods (methods play no part in the analysis fixpoint).
///
/// A field-translation failure here means the analysis and the
/// translator disagree on the supported subset — a generator bug; the
/// caller escalates rather than skipping.
pub(crate) fn emit(
    name: &Name,
    class: &Class,
    ctx: &TyCtx<'_>,
) -> Result<(TokenStream, Vec<SkipWarning>), SkipWarning> {
    let fqn = name.to_string();
    let mut warnings = Vec::new();

    // Field-site boxing does not apply to method signatures: a method
    // mentioning a same-SCC class (e.g. a static factory returning the
    // class itself) is not a containment edge, so its types translate
    // without a boxing owner.
    let method_ctx = TyCtx {
        analysis: ctx.analysis,
        unions: ctx.unions,
        leaf: ctx.leaf,
        boxing_for: None,
    };
    let mut method_tokens: Vec<TokenStream> = Vec::new();
    for (methods, receiver) in [
        (&class.static_methods, Receiver::None),
        (&class.instance_methods, Receiver::RefSelf),
    ] {
        for method in methods {
            match emit_method(name, method, receiver, &method_ctx) {
                Ok(tokens) => method_tokens.push(tokens),
                Err(warning) => warnings.push(warning),
            }
        }
    }

    let ident = idents::ident(name.name().as_str());
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
                ::baml_bridge::baml_value::internal::__BamlValuePrivate::to_baml(&self.#field_ident),
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

    let methods_impl = if method_tokens.is_empty() {
        TokenStream::new()
    } else {
        quote! {
            impl #ident {
                #(#method_tokens)*
            }
        }
    };

    let tokens = quote! {
        #(#docs)*
        #[derive(Debug, Clone, PartialEq)]
        pub struct #ident {
            #(#field_defs,)*
        }

        #methods_impl

        impl ::baml_bridge::baml_value::internal::__BamlValuePrivate for #ident {
            fn to_baml(&self) -> ::baml_bridge::wire::InboundValue {
                ::baml_bridge::encode::class(#fqn, ::std::vec![#(#encode_entries,)*])
            }

            fn from_baml(
                v: ::baml_bridge::wire::BamlOutboundValue,
            ) -> ::std::result::Result<Self, ::baml_bridge::DecodeError> {
                #decode_body
            }
        }
    };
    Ok((tokens, warnings))
}
