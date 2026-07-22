//! Procedural macro backing `baml_type`'s `Ty` family.
//!
//! [`ty_family!`](ty_family) takes a single tagged definition of the master
//! `Ty` enum plus a declaration of the family members (`RuntimeTy`,
//! `RealizedTy`, `ConcreteTy`, `ConcreteRealizedTy`) and generates, from that
//! one source of truth:
//!
//! - each member enum (deep self-recursive, or shallow over a designated child
//!   type), with only the variants whose axis is in the member's include-set;
//! - the companion ("satellite") structs threaded per member (e.g.
//!   `RuntimeFunctionParamTy`);
//! - the mechanical `attr`/`with_attr` accessors;
//! - the full conversion matrix (`From` widenings and `TryFrom` narrowings,
//!   owned and by-reference) between every comparable pair of members.
//!
//! The semantic impls (`render_with`, `Display`, the `lower_to_runtime`
//! boundary) stay hand-written on the generated types.

use proc_macro::TokenStream;
use quote::quote;
use syn::parse_macro_input;

use crate::parse::{Family, FamilyInput};

mod convert;
mod emit;
mod parse;

/// Generate the `Ty` family. See the crate docs for the DSL and what is
/// emitted.
#[proc_macro]
pub fn ty_family(input: TokenStream) -> TokenStream {
    let parsed = parse_macro_input!(input as FamilyInput);
    match Family::from_input(parsed) {
        Ok(family) => {
            let types = emit::emit(&family);
            let conversions = convert::emit_conversions(&family);
            quote! { #types #conversions }.into()
        }
        Err(err) => err.to_compile_error().into(),
    }
}
