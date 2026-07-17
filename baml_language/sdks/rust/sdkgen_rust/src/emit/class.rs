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

    let mut method_tokens: Vec<TokenStream> = Vec::new();
    if class_params.is_empty() {
        // Field-site boxing does not apply to method signatures: a method
        // mentioning a same-SCC class (e.g. a static factory returning the
        // class itself) is not a containment edge, so its types translate
        // without a boxing owner.
        let method_ctx = TyCtx {
            analysis: ctx.analysis,
            unions: ctx.unions,
            leaf: ctx.leaf,
            boxing_for: None,
            generic_params: &[],
        };
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
    } else {
        // Methods on a generic class need the class type parameters bound
        // into the call frame (recovered from the receiver / passed as
        // `class_type_params`) — deferred. Skip them, keeping the struct.
        let method_count = class.static_methods.len() + class.instance_methods.len();
        if method_count > 0 {
            warnings.push(SkipWarning {
                fqn: fqn.clone(),
                reason: format!(
                    "methods on generic classes are not emitted yet ({method_count} skipped)"
                ),
            });
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
        let field_ty = translate_ty::translate(&prop.ty, &field_ctx).map_err(|u| SkipWarning {
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
        // Only non-generic classes reach here with methods (generic-class
        // methods are skipped above), so the impl block needs no generics.
        quote! {
            impl #ident {
                #(#method_tokens)*
            }
        }
    };

    // `<T: BamlValue, …>` on the struct + impl header, `<T, …>` on the
    // `for` type, and one `<param>::baml_ty()` per param for the wire
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

    let tokens = quote! {
        #(#docs)*
        #[derive(Debug, Clone, PartialEq)]
        pub struct #ident #bounded_generics {
            #(#field_defs,)*
        }

        #methods_impl

        impl #bounded_generics ::baml_bridge::baml_value::internal::__BamlValuePrivate
            for #ident #type_generics
        {
            fn to_baml(&self) -> ::baml_bridge::wire::InboundValue {
                ::baml_bridge::encode::class(
                    #fqn,
                    #wire_type_args,
                    ::std::vec![#(#encode_entries,)*],
                )
            }

            fn from_baml(
                v: ::baml_bridge::wire::BamlOutboundValue,
            ) -> ::std::result::Result<Self, ::baml_bridge::DecodeError> {
                #decode_body
            }

            fn baml_ty() -> ::baml_bridge::wire::BamlTy {
                ::baml_bridge::baml_value::internal::class_ty(#fqn, #wire_type_args)
            }
        }
    };
    Ok((tokens, warnings))
}
