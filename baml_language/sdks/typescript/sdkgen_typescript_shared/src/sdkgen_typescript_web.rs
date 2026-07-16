//! Web TypeScript SDK emitter for browser and workerd runtimes.
//!
//! The generated surface intentionally stays identical to the Node.js SDK.
//! Only the runtime bridge package differs.

use std::{collections::HashMap, path::PathBuf};

use baml_codegen_types::SymbolPool;
pub use baml_codegen_types::{NamingConvention, OutputType};

const RUNTIME_PACKAGE: &str = "@boundaryml/baml-bridge-web";

pub fn to_source_code(
    pool: &SymbolPool,
    baml_bytecode: &[u8],
    naming_convention: NamingConvention,
) -> HashMap<PathBuf, String> {
    crate::to_source_code(
        pool,
        baml_bytecode,
        naming_convention,
        crate::GeneratorConfig::new(RUNTIME_PACKAGE),
    )
}

pub fn to_source_code_with_bytecode(
    pool: &SymbolPool,
    baml_bytecode: &[u8],
    naming_convention: NamingConvention,
) -> HashMap<PathBuf, String> {
    to_source_code(pool, baml_bytecode, naming_convention)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_sdk_uses_web_runtime_package() {
        let output = to_source_code(&SymbolPool::new(), &[], NamingConvention::PreserveCase);
        let root = &output[&PathBuf::from("index.ts")];
        let typemap = &output[&PathBuf::from("_typemap.ts")];

        assert!(root.contains("pnpm add @boundaryml/baml-bridge-web"));
        assert!(root.contains("from \"@boundaryml/baml-bridge-web\";"));
        assert!(typemap.contains("from \"@boundaryml/baml-bridge-web\";"));
        assert!(!root.contains("from \"@boundaryml/baml-bridge\";"));
        assert!(!typemap.contains("from \"@boundaryml/baml-bridge\";"));
    }
}
