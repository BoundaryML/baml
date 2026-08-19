//! `baml_ide` — the IDE/analysis layer over the BAML compiler.
//!
//! Rust-analyzer's `ide` shape: features are functions over the database
//! (`baml_db::ProjectDatabase` / the compiler `Db` traits), returning plain
//! data. Nothing here computes semantics; anything that does lands in a
//! compiler crate (or `baml_db`) first and is *consumed* here.
//!
//! Current contents are the compiler-facing subset the CLI needs (symbol
//! listing, playground parameter schemas). Editor features (hover,
//! definitions, references, semantic tokens, outline, completions, describe)
//! are rebuilt into this crate as the LSP stack lands on the source-root
//! foundation.

pub mod line_index;
pub mod param_schema;
pub mod symbol_pool;
pub mod symbols;
#[cfg(test)]
mod test_support;

pub use param_schema::{FieldSchema, FieldSchemaField, ParamSchema, TypeSchema};
pub use symbol_pool::build_symbol_pool;
pub use symbols::{
    FunctionListing, FunctionOrigin, FunctionSourcePosition, FunctionSymbol, Symbol, SymbolKind,
    TestSymbol, list_functions_with_metadata, list_tests_with_metadata,
};
