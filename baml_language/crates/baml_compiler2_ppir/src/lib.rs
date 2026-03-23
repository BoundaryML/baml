//! Pre-Processed Intermediate Representation (PPIR) for compiler2.
//!
//! Pipeline: CST -> AST -> HIR (raw) -> PPIR (expansion + canonical) -> TIR.
//!
//! PPIR uses HIR's `package_items` for symbol classification (class vs enum vs alias),
//! then synthesizes `stream_*` AST items and provides canonical queries that include
//! both original and synthetic items.
//!
//! **No union simplification in PPIR.** Deferred to TIR.
//!
//! Currently a passthrough scaffold — all queries delegate directly to HIR.

use std::sync::Arc;

use baml_base::SourceFile;
use baml_compiler2_hir::{
    contributions::FileSymbolContributions,
    item_tree::ItemTree,
    namespace::NamespaceId,
    package::{PackageId, PackageItems},
    scope::ScopeId,
    semantic_index::{FileSemanticIndex, ScopeBindings},
};

// -- Db trait -----------------------------------------------------------------

#[salsa::db]
pub trait Db: baml_compiler2_hir::Db {}

// -- Canonical queries (passthrough to HIR) -----------------------------------

/// Canonical file_semantic_index — currently delegates to HIR.
/// Will later merge synthetic stream_* items into the index.
pub fn file_semantic_index<'db>(db: &'db dyn Db, file: SourceFile) -> &'db FileSemanticIndex<'db> {
    baml_compiler2_hir::file_semantic_index(db, file)
}

/// Canonical file_item_tree — currently delegates to HIR.
pub fn file_item_tree(db: &dyn Db, file: SourceFile) -> Arc<ItemTree> {
    baml_compiler2_hir::file_item_tree(db, file)
}

/// Canonical symbol contributions — currently delegates to HIR.
pub fn file_symbol_contributions<'db>(
    db: &'db dyn Db,
    file: SourceFile,
) -> Arc<FileSymbolContributions<'db>> {
    baml_compiler2_hir::file_symbol_contributions(db, file)
}

/// Canonical scope_bindings_query — currently delegates to HIR.
pub fn scope_bindings_query<'db>(db: &'db dyn Db, scope_id: ScopeId<'db>) -> ScopeBindings {
    baml_compiler2_hir::scope_bindings_query(db, scope_id)
}

/// Canonical namespace_items — currently delegates to HIR.
pub fn namespace_items<'db>(
    db: &'db dyn Db,
    namespace_id: NamespaceId<'db>,
) -> &'db baml_compiler2_hir::namespace::NamespaceItems<'db> {
    baml_compiler2_hir::namespace::namespace_items(db, namespace_id)
}

/// Canonical package_items — currently delegates to HIR.
pub fn package_items<'db>(db: &'db dyn Db, package_id: PackageId<'db>) -> &'db PackageItems<'db> {
    baml_compiler2_hir::package::package_items(db, package_id)
}
