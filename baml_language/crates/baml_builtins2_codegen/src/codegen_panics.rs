//! Code generation for the `PanicClass` enum.
//!
//! Reads `NativeClassDef` entries whose `namespace_prefix` is `"baml.panics"`
//! and generates a `PanicClass` fieldless enum with `fqn()`, `item()`, and `ALL`.
//!
//! Generated from `panics.baml` class definitions so that adding a new panic
//! type only requires editing the `.baml` file.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::types::NativeClassDef;

const PANICS_NAMESPACE: &str = "baml.panics";

/// Generate the `PanicClass` enum and associated methods.
pub fn generate_panic_enums(class_defs: &[NativeClassDef]) -> String {
    let panics: Vec<&NativeClassDef> = class_defs
        .iter()
        .filter(|c| c.namespace_prefix == PANICS_NAMESPACE)
        .collect();

    assert!(
        !panics.is_empty(),
        "no panic classes found in namespace {PANICS_NAMESPACE} — is panics.baml registered?"
    );

    let tokens = generate_panic_class_enum(&panics);
    crate::format_tokens(&tokens)
}

// ── PanicClass ───────────────────────────────────────────────────────────────

fn generate_panic_class_enum(panics: &[&NativeClassDef]) -> TokenStream {
    let variant_idents: Vec<_> = panics.iter().map(|p| format_ident!("{}", p.name)).collect();
    let fqns: Vec<String> = panics
        .iter()
        .map(|p| format!("{PANICS_NAMESPACE}.{}", p.name))
        .collect();
    let items: Vec<TokenStream> = panics
        .iter()
        .map(|p| crate::lang_item::lang_item_tokens(PANICS_NAMESPACE, &p.name))
        .collect();

    quote! {
        /// Panic class tag — one variant per `baml.panics.*` class.
        ///
        /// Auto-generated from `panics.baml`.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum PanicClass {
            #(#variant_idents,)*
        }

        impl PanicClass {
            /// Fully-qualified class name (e.g. `"baml.panics.DivisionByZero"`).
            pub const fn fqn(&self) -> &'static str {
                match self {
                    #(PanicClass::#variant_idents => #fqns,)*
                }
            }

            /// The class by identity: its item in the `baml` language package,
            /// which is how the runtime reaches it.
            pub const fn item(&self) -> ::baml_base::LangItem {
                match self {
                    #(PanicClass::#variant_idents => #items,)*
                }
            }

            /// All panic class variants.
            pub const ALL: &[PanicClass] = &[
                #(PanicClass::#variant_idents,)*
            ];
        }
    }
}
