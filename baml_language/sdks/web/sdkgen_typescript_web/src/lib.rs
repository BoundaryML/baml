//! Browser TypeScript SDK emitter.
//!
//! The generated surface intentionally stays identical to
//! `sdkgen_typescript_node`. The only difference is the bridge package that
//! generated modules dispatch through.

use std::{collections::HashMap, path::PathBuf};

use baml_codegen_types::SymbolPool;
pub use baml_codegen_types::{NamingConvention, OutputType};

const NODE_BRIDGE: &str = "@boundaryml/baml-bridge";
const WEB_BRIDGE: &str = "@boundaryml/baml-bridge-web";

pub fn to_source_code(
    pool: &SymbolPool,
    baml_bytecode: &[u8],
    naming_convention: NamingConvention,
) -> HashMap<PathBuf, String> {
    sdkgen_typescript_node::to_source_code(pool, baml_bytecode, naming_convention)
        .into_iter()
        .map(|(path, source)| (path, source.replace(NODE_BRIDGE, WEB_BRIDGE)))
        .collect()
}

pub fn to_source_code_with_bytecode(
    pool: &SymbolPool,
    baml_bytecode: &[u8],
    naming_convention: NamingConvention,
) -> HashMap<PathBuf, String> {
    to_source_code(pool, baml_bytecode, naming_convention)
}
