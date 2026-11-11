mod codegen;
mod codegen_adapters;
mod codegen_errors;
mod codegen_io;
mod codegen_panics;
pub mod escape_modeled;
mod extract;
mod lang_item;
mod rust_ident;
pub mod triage;
mod triage_table;
mod types;

pub use codegen::{
    ScalarNative, generate_native_trait, generate_native_trait_for, scalar_native_entries,
};

/// The [`ScalarNative`] for the builtin with native key `path`, if any;
/// extracted once per process from the embedded `baml` sources.
pub fn scalar_native_entry(path: &str) -> Option<&'static ScalarNative> {
    static ENTRIES: std::sync::OnceLock<std::collections::BTreeMap<String, ScalarNative>> =
        std::sync::OnceLock::new();
    ENTRIES
        .get_or_init(|| {
            extract_native_builtins_for("baml")
                .map(|(builtins, _, _)| scalar_native_entries(&builtins))
                .unwrap_or_default()
        })
        .get(path)
}
pub use codegen_adapters::{
    AdapterGenError, AdapterProblem, compiled_adapter_entry, generate_adapters,
};
pub use codegen_errors::generate_error_enums;
pub use codegen_io::{
    generate_io_adapter, generate_io_structs, generate_io_traits, generate_runtime_io,
    generate_sys_op_enum,
};
pub use codegen_panics::generate_panic_enums;
pub use extract::{
    ExtractNativeBuiltinsError, extract_native_builtins, extract_native_builtins_for,
};
pub use types::{
    BamlType, BuiltinPipeline, NativeBuiltin, NativeClassDef, NativeClassField, Param, Receiver,
    ReceiverType,
};

/// Format a `TokenStream` into a pretty-printed Rust source string.
fn format_tokens(tokens: &proc_macro2::TokenStream) -> String {
    let code_string = tokens.to_string();
    let syntax_tree = syn::parse_file(&code_string).expect("Failed to parse generated code");
    prettyplease::unparse(&syntax_tree)
}

#[cfg(test)]
mod scalar_native_tests {
    #[test]
    fn scalar_natives_are_the_infallible_vm_free_scalar_methods() {
        let sqrt = super::scalar_native_entry("baml.Float.sqrt").expect("sqrt is scalar");
        assert_eq!(
            sqrt.call,
            "<bex_vm::package_baml::PackageBamlImpl as bex_vm::package_baml::BamlClassFloat>::sqrt"
        );
        assert_eq!(sqrt.params, ["f64"]);
        assert_eq!(sqrt.ret, "f64");
        let pow = super::scalar_native_entry("baml.Float.pow").expect("pow is scalar");
        assert_eq!(pow.params, ["f64", "f64"]);
        // Fallible, string-taking and unknown builtins have none.
        assert!(super::scalar_native_entry("baml.Float.ifloor").is_none());
        assert!(super::scalar_native_entry("baml.Float.parse").is_none());
        assert!(super::scalar_native_entry("baml.no.such").is_none());
    }
}
