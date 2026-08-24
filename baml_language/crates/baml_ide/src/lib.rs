//! `baml_ide` — the IDE/analysis layer over the BAML compiler.
//!
//! Rust-analyzer's `ide` shape: features are functions over the database
//! (`baml_db::ProjectDatabase` / the compiler `Db` traits), returning plain
//! data. Nothing here computes semantics; anything that does lands in a
//! compiler crate (or `baml_db`) first and is *consumed* here.
//!
//! One query surface, several consumers: the LSP serves editors from it and
//! `baml describe` serves agents and the terminal from it, so a fact about a
//! symbol is computed in exactly one place. The shared core is
//! [`resolve`] (position → symbol and name → symbol addressing), [`search`]
//! (one engine: substring mode for `workspace/symbol`, ranked word/docstring
//! mode for topic queries), [`render`] (one type/signature renderer), and
//! the structured extraction in [`info`]; features (definitions, references,
//! semantic tokens, outline, inlay hints, lenses) are views over that core.
//! [`completion`] is being rebuilt additively rather than ported — the old
//! implementation's context detection was the known-broken part — so a
//! position it has not classified yet offers nothing instead of guessing.
//! Consumers do presentation only: markdown lives with the
//! LSP protocol layer, ANSI with the CLI painter, and `--json` is serde on
//! the structs here.

pub mod actions;
pub mod annotations;
pub mod cfg;
pub mod completion;
pub mod cursor_context;
pub mod definition;
pub mod describe;
pub mod env_vars;
pub mod export;
pub mod info;
pub mod line_index;
pub mod listing;
pub mod outline;
pub mod param_schema;
pub mod render;
pub mod resolve;
pub mod search;
pub mod symbol_pool;
pub mod symbols;
pub mod syntax;
#[cfg(test)]
mod test_support;
pub mod tokens;
pub mod usages;

pub use actions::{FileAction, FileActionKind, file_actions};
pub use annotations::{AnnotationKind, InlineAnnotation, file_annotations};
// Re-exported so protocol/CLI consumers convert kinds without depending on
// the compiler crates directly.
pub use baml_compiler2_hir::contributions::DefinitionKind;
pub use cfg::ast_control_flow_graph;
pub use completion::{
    Completion, CompletionInsert, CompletionKind, CompletionRelevance, completions,
};
pub use cursor_context::{CursorContext, find_source_file, playground_cursor_context};
pub use definition::definition_at;
pub use describe::{
    DepRef, MethodRef, RefSite, SymbolDescription, describe, describe_by_definition,
    describe_item_member,
};
pub use env_vars::all_env_var_names;
pub use export::{PackageExport, export_package};
pub use info::{FunctionParamInfo, MethodSig, TypeInfo, type_at, type_info_for_definition};
pub use listing::{
    ListingEntry, ResolvedTarget, list_namespace_items, list_package_items,
    non_workspace_package_names, resolve_builtin_type_target, resolve_target,
};
pub use outline::{OutlineItem, file_outline};
pub use param_schema::{FieldSchema, FieldSchemaField, ParamSchema, TypeSchema};
pub use resolve::{Location, SymbolTarget, symbol_at, target_definition};
pub use search::{SearchHit, SymbolInfo, search_ranked, search_symbols};
pub use symbol_pool::build_symbol_pool;
pub use symbols::{
    FunctionListing, FunctionOrigin, FunctionSourcePosition, FunctionSymbol, Symbol, SymbolKind,
    TestSymbol, list_functions_with_metadata, list_tests_with_metadata,
};
// Editor primitive: cursor-position token lookup. First-class API — callers
// (e.g. the playground cursor context) must not depend on feature modules'
// internals.
pub use syntax::find_token_at_offset;
pub use tokens::{
    ModifierSet, SemanticToken, SemanticTokenType, TOKEN_MODIFIERS, TOKEN_TYPES,
    semantic_highlight_style, semantic_tokens, semantic_tokens_in_range,
};
pub use usages::usages_at;
