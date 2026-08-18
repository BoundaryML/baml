//! Type-alias emission: a plain `pub type` item.
//!
//! Alias *references* are inlined by the pool builder (in-package,
//! non-recursive) or resolved as paths to these items, and Rust aliases
//! are transparent — so no conversion impls are needed; the item exists
//! to keep the user's named types on the SDK surface.

use baml_codegen_types::{Name, TypeAlias};
use proc_macro2::TokenStream;
use quote::quote;

use crate::{
    SkipKind, SkipWarning, idents,
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
    let rhs = translate_ty::translate(&alias.resolves_to, ctx).map_err(|u| SkipWarning {
        kind: SkipKind::Type,
        fqn: name.to_string(),
        reason: format!(
            "generator bug: analysis accepted this alias but translation failed: {}",
            u.reason
        ),
    })?;
    Ok(quote! {
        pub type #ident = #rhs;
    })
}
