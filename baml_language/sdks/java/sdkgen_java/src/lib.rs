//! Java SDK emitter. Stub — the API surface mirrors
//! `sdkgen_typescript_node` so the `sdk_test_harness_setup::java` target and
//! any future `baml-cli generate` wiring can be written against the final
//! signature, but both entry points panic until the real emitter lands.

use std::{collections::HashMap, path::PathBuf};

use baml_codegen_types::SymbolPool;
pub use baml_codegen_types::{NamingConvention, OutputType};

/// A user BAML source file as it should appear in the emitter's
/// inlined-baml output. `rel_path` is relative to the `baml_src/` root.
pub type UserBamlFile = (PathBuf, String);

/// Build the Java SDK output tree for `pool`. Returned paths are
/// relative to the `baml_sdk/` output root.
pub fn to_source_code(
    _pool: &SymbolPool,
    _user_baml_files: &[UserBamlFile],
    _naming_convention: NamingConvention,
) -> HashMap<PathBuf, String> {
    unimplemented!(
        "sdkgen_java::to_source_code is a stub — the Java emitter has not been implemented yet"
    );
}

/// Like [`to_source_code`], but embeds compiled BAML bytecode in the
/// generated SDK instead of inlined `.baml` source (the runtime-init
/// model the Python and TypeScript emitters use).
pub fn to_source_code_with_bytecode(
    _pool: &SymbolPool,
    _baml_bytecode: &[u8],
    _naming_convention: NamingConvention,
) -> HashMap<PathBuf, String> {
    unimplemented!(
        "sdkgen_java::to_source_code_with_bytecode is a stub — the Java emitter has not been implemented yet"
    );
}
