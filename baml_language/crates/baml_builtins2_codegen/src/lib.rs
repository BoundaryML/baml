mod codegen;
mod codegen_errors;
mod codegen_io;
mod codegen_panics;
mod extract;
mod types;

pub use codegen::generate_native_trait;
pub use codegen_errors::generate_error_enums;
pub use codegen_io::{
    generate_io_adapter, generate_io_structs, generate_io_traits, generate_runtime_io,
    generate_sys_op_enum,
};
pub use codegen_panics::generate_panic_enums;
pub use extract::{ExtractNativeBuiltinsError, extract_native_builtins};
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
