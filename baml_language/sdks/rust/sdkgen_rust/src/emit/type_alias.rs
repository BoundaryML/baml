//! Type-alias emission: a plain `pub type` item.
//!
//! Alias references resolve as paths to these items, and Rust aliases
//! are transparent — so no conversion impls are needed; the item exists
//! to keep the user's named types on the SDK surface.

use baml_sdkgen_types::{Name, TypeAlias};
use proc_macro2::TokenStream;
use quote::quote;

use crate::{
    SkipWarning, idents,
    translate_ty::{self, TyCtx},
};

/// Emit the `pub type` item for an alias the analysis marked emitted.
///
/// A translation failure here means the analysis and the translator
/// disagree on the supported subset — a generator bug; the caller
/// escalates rather than skipping.
pub(crate) fn emit(
    name: &Name,
    alias: &TypeAlias,
    ctx: &TyCtx<'_>,
) -> Result<TokenStream, SkipWarning> {
    let ident = idents::ident(name.name().as_str());
    // The recursive stdlib `json` alias has no Rust `type` of its own
    // expansion; its item names the bridge's JSON value instead.
    if translate_ty::is_canonical_json(name) {
        return Ok(quote! {
            /// An untyped JSON value: BAML's `json`, as `serde_json::Value`
            /// (see `baml_bridge::Json` for how numbers cross).
            pub type #ident = ::baml_bridge::Json;
        });
    }
    let rhs = translate_ty::translate(&alias.resolves_to, ctx).map_err(|u| {
        SkipWarning::new(
            name,
            format!(
                "generator bug: analysis accepted this alias but translation failed: {}",
                u.reason
            ),
        )
    })?;
    Ok(quote! {
        pub type #ident = #rhs;
    })
}
