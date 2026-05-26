//! Go SDK emitter. Stub — the API surface mirrors `codegen_python`
//! so the `sdk_test_build::go` target and any future `baml-cli
//! generate` wiring can be written against the final signature, but
//! `to_source_code` panics until the real emitter lands.

use std::{collections::HashMap, path::PathBuf};

use baml_codegen_types::SymbolPool;
pub use baml_codegen_types::{NamingConvention, OutputType};

/// A user BAML source file as it should appear in the emitter's
/// inlined-baml output. `rel_path` is relative to the `baml_src/` root.
pub type UserBamlFile = (PathBuf, String);

/// Build the Go SDK output tree for `pool`. Returned paths are
/// relative to the `baml_sdk/` output root.
pub fn to_source_code(
    _pool: &SymbolPool,
    _user_baml_files: &[UserBamlFile],
    _naming_convention: NamingConvention,
) -> HashMap<PathBuf, String> {
    unimplemented!(
        "codegen_go::to_source_code is a stub — the Go emitter has not been implemented yet"
    );
}
