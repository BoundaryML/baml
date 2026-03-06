//! `baml_lsp2_actions` — IDE action layer built on top of compiler2.
//!
//! Modeled after ruff's `ty_ide` crate: regular functions (not Salsa queries)
//! that take `&dyn Db` and return domain types. Internally they call Salsa
//! queries from `baml_compiler2_hir` and `baml_compiler2_tir` for cached data.
//!
//! ## Phase 1
//!
//! - `check_file(db, file) -> Vec<Diagnostic>` — aggregates parse + HIR + TIR
//!   diagnostics for a single file.
//!
//! ## Phase 2
//!
//! - `file_outline(db, file) -> &Vec<OutlineItem>` — Salsa tracked query that
//!   builds a hierarchical symbol tree from `file_symbol_contributions` and
//!   `file_item_tree`. Cached per file revision.
//! - `search_symbols(db, files, query) -> Vec<SymbolInfo>` — regular function
//!   that iterates files calling `file_outline` and filters by query string.
//!   Used for `workspace/symbol` and as a helper for `textDocument/documentSymbol`.
//!
//! ## Phase 3
//!
//! - `definition_at(db, file, offset) -> Option<Location>` — go-to-definition
//!   at a cursor position. Finds the token under the cursor, resolves the name
//!   via `resolve_name_at`, and maps the result to a `Location`.
//!
//! ## Phase 4
//!
//! - `type_at(db, file, offset) -> Option<TypeInfo>` — structured type and
//!   signature info at a cursor position. Returns a `TypeInfo` enum variant
//!   describing what the name refers to: function signature, class with fields,
//!   enum with variants, type alias expansion, or local variable type.
//!   The `TypeInfo::to_hover_markdown()` method formats it for LSP hover.
//!
//! ## Phase 9
//!
//! - `file_actions(db, file) -> Vec<FileAction>` — code lenses for functions
//!   (Run in Playground) and tests (Run Test). Uses `file_symbol_contributions`
//!   — purely structural, no type inference needed.
//! - `fixes_at(db, file, range) -> Vec<Fix>` — quick-fixes at a range.
//!   Initially minimal: "Open in Playground" unconditionally.

pub mod actions;
pub mod annotations;
pub mod check;
pub mod completions;
pub mod definition;
pub mod fixes;
pub mod outline;
pub mod search;
pub mod tokens;
pub mod type_info;
pub mod usages;
pub mod utils;

// ── Db trait ──────────────────────────────────────────────────────────────────

/// Database trait for `baml_lsp2_actions` queries.
///
/// Extends `baml_compiler2_tir::Db`, which itself extends
/// `baml_compiler2_hir::Db` and `baml_workspace::Db`. This crate can add
/// Salsa-tracked queries (e.g. `file_outline` in Phase 2) that require a `Db`
/// implementor to also satisfy the compiler2 trait chain.
#[salsa::db]
pub trait Db: baml_compiler2_tir::Db {}

// ── Public API re-exports ─────────────────────────────────────────────────────

pub use actions::{FileAction, FileActionKind, file_actions};
pub use annotations::{AnnotationKind, InlineAnnotation, annotations};
// Re-export `DefinitionKind` so callers (e.g. bex_project) don't need to
// depend on `baml_compiler2_hir` directly just for type conversions.
pub use baml_compiler2_hir::contributions::DefinitionKind;
pub use check::check_file;
pub use completions::{Completion, CompletionKind, completions_at};
pub use definition::{Location, definition_at};
pub use fixes::{Fix, FixKind, fixes_at};
pub use outline::{OutlineItem, file_outline};
pub use search::{SymbolInfo, search_symbols};
pub use tokens::{SemanticToken, SemanticTokenType, TOKEN_TYPES, semantic_tokens};
pub use type_info::{TypeInfo, type_at};
pub use usages::usages_at;
