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
pub use builder::SemanticIndexBuilder;
pub use semantic_index::PathResolution;

use crate::{
    contributions::FileSymbolContributions,
    item_tree::{ItemTree, ItemTreeSourceMap},
    semantic_index::{FileSemanticIndex, ScopeBindings},
};

// ── Db trait ─────────────────────────────────────────────────────────────────

/// Database trait for `compiler2_hir` queries.
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
/// - `db.project().files()` excluding legacy `<builtin>/...` v1 builtin files
/// - `db.compiler2_extra_files().files()` — compiler2-owned builtin sources
///   (e.g., `Array<T>`, `Map<K,V>`, `String`, `Media` from `baml_builtins2`)
///
/// The v1 compiler continues to see `project.files()` including the legacy
/// builtin BAML sources. Compiler2 HIR queries (`namespace_items`,
/// `package_items`) intentionally ignore those legacy builtin files once the
/// compiler2-owned builtin stdlib is present, so there is only one builtin
/// source of truth in the compiler2 package graph.
pub fn compiler2_all_files(db: &dyn Db) -> Vec<baml_base::SourceFile> {
    let mut files: Vec<baml_base::SourceFile> = db
        .project()
        .files(db)
        .iter()
        .copied()
        .filter(|file| !file.path(db).to_string_lossy().starts_with("<builtin>/"))
        .collect();
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
pub fn file_semantic_index(db: &dyn Db, file: SourceFile) -> FileSemanticIndex<'_> {
    let tree = baml_compiler_parser::syntax_tree(db, file);
    let file_range = tree.text_range();
    let (items, lowering_diags, env_var_refs) =
        baml_compiler2_ast::lower_file_with_file_id(&tree, file.file_id(db));

    let builder = SemanticIndexBuilder::new(db, file);
    builder
        .with_lowering_diagnostics(lowering_diags)
        .with_env_var_refs(env_var_refs)
        .build(&items, file_range)
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
pub fn file_symbol_contributions(
    db: &dyn Db,
    file: SourceFile,
) -> Arc<FileSymbolContributions<'_>> {
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

/// Returns the item tree source map for a file (clones the Arc — O(1)).
///
/// Not tracked — the source map is cached via `file_semantic_index`.
pub fn file_item_tree_source_map(db: &dyn Db, file: SourceFile) -> Arc<ItemTreeSourceMap> {
    let index = file_semantic_index(db, file);
    Arc::clone(&index.item_tree_source_map)
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

/// Returns the env var references found in a file's expression bodies.
pub fn file_env_var_refs(db: &dyn Db, file: SourceFile) -> &[baml_compiler2_ast::EnvVarRef] {
    &file_semantic_index(db, file).env_var_refs
}

/// Returns the scope-level `PathResolution` for a multi-segment `Path` expression.
///
/// Not tracked — callers should use the cached `file_semantic_index` result.
/// Returns `None` if `expr_id` was not recorded (i.e., single-segment paths
/// or non-path expressions).
pub fn path_resolution_query(
    db: &dyn Db,
    file: baml_base::SourceFile,
    expr_id: baml_compiler2_ast::ExprId,
) -> Option<PathResolution> {
    let index = file_semantic_index(db, file);
    index.path_resolution(expr_id).cloned()
}
