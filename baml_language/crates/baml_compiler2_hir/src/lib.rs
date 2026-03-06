//! `baml_compiler2_hir` — Scope-tree-based HIR for the compiler2 pipeline.
//!
//! Provides per-file `FileSemanticIndex` with:
//! - Scope tree (Project → Package → Namespace* → File → items)
//! - Item tree (position-independent item storage)
//! - Expression → scope mappings
//! - Per-scope `ScopeBindings` (let-bindings + parameters)
//! - `FileSymbolContributions` (names exported to the package namespace)
//!
//! Phase 2 adds:
//! - Projection queries: `file_symbol_contributions`, `file_item_tree`, `scope_bindings_query`
//! - Per-item queries: `function_signature`, `function_body`
//! - Cross-file aggregation: `namespace_items`, `package_items`

pub mod body;
mod builder;
pub mod contributions;
pub mod diagnostic;
pub mod file_package;
pub mod ids;
pub mod item_tree;
pub mod loc;
pub mod namespace;
pub mod package;
pub mod scope;
pub mod semantic_index;
pub mod signature;

use std::sync::Arc;

use baml_base::SourceFile;

use crate::{
    builder::SemanticIndexBuilder,
    contributions::FileSymbolContributions,
    item_tree::ItemTree,
    semantic_index::{FileSemanticIndex, ScopeBindings},
};

// ── Db trait ─────────────────────────────────────────────────────────────────

/// Database trait for compiler2_hir queries.
///
/// Extends `baml_workspace::Db`. Use `file_semantic_index` for HIR queries.
///
/// The `compiler2_extra_files()` method provides access to compiler2-only
/// builtin stub files that must NOT be in the shared `project.files()` list
/// (because the v1 parser cannot handle compiler2-specific syntax like generic
/// type parameters or `$rust_type` fields). Implementors that have such files
/// should override this to return the appropriate `Compiler2ExtraFiles` handle.
#[salsa::db]
pub trait Db: baml_workspace::Db {
    /// Returns the compiler2-only extra files, or `None` if not configured.
    ///
    /// The default implementation returns `None`, meaning no extra files.
    /// `ProjectDatabase` overrides this to return the v2 builtin stubs.
    fn compiler2_extra_files(&self) -> Option<baml_workspace::Compiler2ExtraFiles> {
        None
    }
}

// ── compiler2_all_files ───────────────────────────────────────────────────────

/// Returns all files visible to compiler2 HIR queries.
///
/// This is the union of:
/// - `db.project().files()` — user files and v1 builtin stubs
/// - `db.compiler2_extra_files().files()` — compiler2-only builtin stubs
///   (e.g., `Array<T>`, `Map<K,V>`, `String`, `Media` from `baml_builtins2`)
///
/// The v1 compiler only sees `project.files()`, while compiler2 HIR queries
/// (`namespace_items`, `package_items`) use this combined view.
pub fn compiler2_all_files(db: &dyn Db) -> Vec<baml_base::SourceFile> {
    let mut files: Vec<baml_base::SourceFile> = db.project().files(db).to_vec();
    if let Some(extra) = db.compiler2_extra_files() {
        files.extend_from_slice(extra.files(db));
    }
    files
}

// ── file_semantic_index ───────────────────────────────────────────────────────

/// Coarse per-file query — always re-runs on file change (`no_eq`).
///
/// Projection queries (`file_symbol_contributions`, `file_item_tree`,
/// `scope_bindings`) provide Salsa early-cutoff via `Arc` equality.
#[salsa::tracked(returns(ref), no_eq)]
pub fn file_semantic_index<'db>(db: &'db dyn Db, file: SourceFile) -> FileSemanticIndex<'db> {
    let tree = baml_compiler_parser::syntax_tree(db, file);
    let file_range = tree.text_range();
    let (items, _ast_diagnostics) = baml_compiler2_ast::lower_file(&tree);
    SemanticIndexBuilder::new(db, file).build(items, file_range)
}

// ── Projection helpers ────────────────────────────────────────────────────────
//
// These are plain functions (not Salsa-tracked) that extract fields from the
// `FileSemanticIndex`. The early-cutoff is achieved at the level of
// `namespace_items` / `package_items` which use `PartialEq` on their results.

/// Returns the symbol contributions for a file (clones the Arc — O(1)).
///
/// Not tracked — callers that need Salsa cut-off should use the
/// `namespace_items` query which re-reads this and uses `PartialEq`.
pub fn file_symbol_contributions<'db>(
    db: &'db dyn Db,
    file: SourceFile,
) -> Arc<FileSymbolContributions<'db>> {
    let index = file_semantic_index(db, file);
    Arc::clone(&index.symbol_contributions)
}

/// Returns the item tree for a file (clones the Arc — O(1)).
///
/// Not tracked — the item tree is cached via `file_semantic_index`.
/// This helper is for convenience in downstream queries.
pub fn file_item_tree(db: &dyn Db, file: SourceFile) -> Arc<ItemTree> {
    let index = file_semantic_index(db, file);
    Arc::clone(&index.item_tree)
}

/// Returns the `ScopeBindings` for a given scope.
///
/// Not tracked — callers use the pre-interned `ScopeId` to look up bindings.
pub fn scope_bindings_query<'db>(
    db: &'db dyn Db,
    scope_id: crate::scope::ScopeId<'db>,
) -> ScopeBindings {
    let file = scope_id.file(db);
    let index = file_semantic_index(db, file);
    let local_id = scope_id.file_scope_id(db);
    index.scope_bindings[local_id.index() as usize].clone()
}
