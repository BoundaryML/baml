//! Code generation for the `ErrorClass` enum.
//!
//! Reads `NativeClassDef` entries whose `namespace_prefix` is `"baml.errors"`
//! (primary error namespace) or any additional error-bearing namespace listed in
//! [`EXTRA_ERROR_NAMESPACES`] and generates an `ErrorClass` fieldless enum with
//! `fqn()`, `name()`, `ALL`, `ALL_NAMES`, and `from_name()`.
//!
//! Generated from `.baml` class definitions so that adding a new error type
//! only requires editing the source file.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::types::NativeClassDef;

const ERRORS_NAMESPACE: &str = "baml.errors";

/// Additional namespaces whose classes are included in `ErrorClass`.
///
/// Classes from these namespaces have their full FQN (`<namespace>.<ClassName>`)
/// used as the FQN in the generated enum, unlike `baml.errors.*` which always
/// uses the `ERRORS_NAMESPACE` prefix.
///
/// Currently empty — populated by future packages (e.g. `reflect.Package`
/// adding `baml.reflect.CompileError`).
const EXTRA_ERROR_NAMESPACES: &[&str] = &[];

/// Generate the `ErrorClass` enum and associated methods.
pub fn generate_error_enums(class_defs: &[NativeClassDef]) -> String {
    let errors: Vec<&NativeClassDef> = class_defs
        .iter()
        .filter(|c| {
            c.namespace_prefix == ERRORS_NAMESPACE
                || EXTRA_ERROR_NAMESPACES.contains(&c.namespace_prefix.as_str())
        })
        .collect();

    if errors.is_empty() {
        return format!(
            "compile_error!(\"no error classes found in namespace {ERRORS_NAMESPACE}\")"
        );
    }

    let tokens = generate_error_class_enum(&errors);
    crate::format_tokens(&tokens)
}

// ── ErrorClass ──────────────────────────────────────────────────────────────

fn generate_error_class_enum(errors: &[&NativeClassDef]) -> TokenStream {
    let variant_idents: Vec<_> = errors.iter().map(|e| format_ident!("{}", e.name)).collect();
    // Use each class's actual namespace prefix as the FQN base so that classes from
    // extra namespaces get `<namespace>.ClassName` rather than `baml.errors.ClassName`.
    let fqns: Vec<String> = errors
        .iter()
        .map(|e| format!("{}.{}", e.namespace_prefix, e.name))
        .collect();
    let names: Vec<&str> = errors.iter().map(|e| e.name.as_str()).collect();

    quote! {
        /// Error class tag — one variant per `baml.errors.*` class.
        ///
        /// Auto-generated from `errors.baml`.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum ErrorClass {
            #(#variant_idents,)*
        }

        impl ErrorClass {
            /// Fully-qualified class name (e.g. `"baml.errors.Timeout"`).
            pub const fn fqn(&self) -> &'static str {
                match self {
                    #(ErrorClass::#variant_idents => #fqns,)*
                }
            }

            /// Short class name (e.g. `"Timeout"`).
            pub const fn name(&self) -> &'static str {
                match self {
                    #(ErrorClass::#variant_idents => #names,)*
                }
            }

            /// All error class variants.
            pub const ALL: &[ErrorClass] = &[
                #(ErrorClass::#variant_idents,)*
            ];

            /// All error class short names.
            pub const ALL_NAMES: &[&str] = &[
                #(#names,)*
            ];

            /// Look up an `ErrorClass` by its short name (e.g. `"Timeout"`).
            pub fn from_name(name: &str) -> Option<ErrorClass> {
                match name {
                    #(#names => Some(ErrorClass::#variant_idents),)*
                    _ => None,
                }
            }
        }
    }
}
