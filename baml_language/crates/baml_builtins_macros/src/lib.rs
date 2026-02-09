//! Proc macros for defining BAML built-in functions with ergonomic Rust-like syntax.
//!
//! This crate transforms Rust-like module/struct/fn declarations into the
//! `BuiltinSignature` definitions used by `baml_builtins`.

use proc_macro::TokenStream;
use syn::parse_macro_input;

mod codegen_accessors;
mod codegen_builtins;
mod codegen_native;
mod codegen_sys_ops;
mod collect;
mod parse;
mod util;

use collect::CollectedBuiltins;
use parse::BuiltinsInput;

/// Define builtin function signatures, path constants, and macro helpers.
///
/// This is the main entry point invoked by `baml_builtins::with_builtins!`.
#[proc_macro]
pub fn define_builtins(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as BuiltinsInput);
    let collected = CollectedBuiltins::from_modules(&input.modules);
    codegen_builtins::generate(&collected).into()
}

/// Generate a `NativeFunctions` trait from the same builtin definitions.
///
/// Generates:
/// - Required `baml_*` methods with clean Rust types
/// - Default `__baml_*` glue methods that handle Value conversion
/// - Default `get_native_fn` method for path lookup
#[proc_macro]
pub fn generate_native_trait(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as BuiltinsInput);
    let collected = CollectedBuiltins::from_modules(&input.modules);
    codegen_native::generate(&collected).into()
}

/// Generate per-module traits for `sys_op` implementations.
///
/// Generates one trait per DSL module (e.g., `SysOpFs`, `SysOpHttp`, `SysOpLlm`)
/// with clean typed methods and glue wiring.
#[proc_macro]
pub fn generate_sys_op_traits(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as BuiltinsInput);
    let collected = CollectedBuiltins::from_modules(&input.modules);
    codegen_sys_ops::generate(&collected).into()
}

/// Generate the complete `builtin_types` module from `with_builtins!` DSL.
///
/// Generates:
/// - `pub mod owned` with owned structs + `AsBexExternalValue` impls
/// - Accessor structs with typed field getters + `into_owned()`
#[proc_macro]
pub fn generate_builtin_accessors(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as BuiltinsInput);
    let collected = CollectedBuiltins::from_modules(&input.modules);
    codegen_accessors::generate(&collected).into()
}
