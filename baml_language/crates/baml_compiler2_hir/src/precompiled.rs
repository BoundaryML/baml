//! Precompiled per-file AST cache for the frozen stdlib.
//!
//! `file_semantic_index` lexes, parses, and AST-lowers a file before running the
//! (cheaper) `SemanticIndexBuilder`. The stdlib is frozen per compiler version,
//! so that lex/parse/lower output is deterministic. This module lets a consumer
//! embed it (built once by `baml_builtins2_prebuilt`) and install it, so
//! `file_semantic_index` skips straight to the builder for `<builtin>/` files.
//!
//! The builder re-interns all `'db`-bound handles (scope ids, symbol
//! contributions) in the runtime database, so the resulting `FileSemanticIndex`
//! is identical to the from-source one — only the lex/parse/lower work is saved.
//!
//! The cache is optional: when unset (e.g. tests, LSP), `file_semantic_index`
//! falls back to parsing from source, so behavior is unchanged.

use std::{collections::HashMap, sync::OnceLock};

use baml_compiler2_ast::{EnvVarRef, Item};
use text_size::TextRange;

/// Pre-lowered AST for one stdlib file: the `(items, env_var_refs)` output of
/// `baml_compiler2_ast::lower_file_with_path` plus the file's text range.
///
/// Lowering diagnostics are omitted: the frozen, error-free stdlib produces
/// none (asserted at artifact-build time), so the builder is fed an empty set,
/// matching the from-source path.
#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PrecompiledFile {
    pub items: Vec<Item>,
    pub env_var_refs: Vec<EnvVarRef>,
    pub range_start: u32,
    pub range_end: u32,
}

impl PrecompiledFile {
    pub fn file_range(&self) -> TextRange {
        TextRange::new(self.range_start.into(), self.range_end.into())
    }
}

static PRECOMPILED_BUILTINS: OnceLock<HashMap<String, PrecompiledFile>> = OnceLock::new();

/// Install the precompiled stdlib AST cache (first writer wins; subsequent calls
/// are ignored). Keyed by builtin virtual path (e.g. `<builtin>/baml/string.baml`).
pub fn set_precompiled_builtins(map: HashMap<String, PrecompiledFile>) {
    let _ = PRECOMPILED_BUILTINS.set(map);
}

/// Precompiled AST for a builtin file by virtual path, if the cache is installed
/// and contains it.
pub fn precompiled_builtin(path: &str) -> Option<&'static PrecompiledFile> {
    PRECOMPILED_BUILTINS.get()?.get(path)
}
