//! Code generation for the `PanicClass` enum.
//!
//! Reads `NativeClassDef` entries whose `namespace_prefix` is `"baml.panics"`
//! and generates a `PanicClass` fieldless enum with `fqn()`, `name()`, `ALL`,
//! `ALL_NAMES`, and `from_name()`.
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
    let names: Vec<&str> = panics.iter().map(|p| p.name.as_str()).collect();

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

            /// Short class name (e.g. `"DivisionByZero"`).
            pub const fn name(&self) -> &'static str {
                match self {
                    #(PanicClass::#variant_idents => #names,)*
                }
            }

            /// All panic class variants.
            pub const ALL: &[PanicClass] = &[
                #(PanicClass::#variant_idents,)*
            ];

            /// All panic class short names.
            pub const ALL_NAMES: &[&str] = &[
                #(#names,)*
            ];

            /// Look up a `PanicClass` by its short name (e.g. `"DivisionByZero"`).
            pub fn from_name(name: &str) -> Option<PanicClass> {
                match name {
                    #(#names => Some(PanicClass::#variant_idents),)*
                    _ => None,
                }
            }
        }
    }
}
