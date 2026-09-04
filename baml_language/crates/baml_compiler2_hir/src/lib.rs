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
pub mod body_type_refs;
mod builder;
pub mod contributions;
pub mod diagnostic;
pub mod file_package;
pub mod ids;
pub mod inputs;
pub mod item_tree;
pub mod loc;
pub mod namespace;
pub mod package;
pub mod scope;
pub mod semantic_index;
pub mod signature;
pub mod type_ref;

use std::sync::Arc;

use baml_base::SourceFile;
pub use builder::{KNOWN_TYPE_ATTRS, SemanticIndexBuilder};
pub use semantic_index::{ExprMetadataKey, ExprMetadataScope, PathResolution};

use crate::{
    contributions::FileSymbolContributions,
    item_tree::ItemTree,
    semantic_index::{FileSemanticIndex, ScopeBindings},
};

// ── Db trait ─────────────────────────────────────────────────────────────────

/// Database trait for `compiler2_hir` queries — the base of the compiler2
/// `Db` trait chain.
///
/// Provides the source-root table (which files exist, grouped into packages)
/// plus the compile-cache seed inputs ([`inputs`]). Use `file_semantic_index`
/// for HIR queries.
#[salsa::db]
pub trait Db: salsa::Database {
    /// The ordered set of source roots in this database.
    ///
    /// See [`baml_base::SourceRootTable`] for the order invariant
    /// (`Stdlib` < `Dependency` < `Workspace` < `Dynamic`).
    fn source_roots(&self) -> baml_base::SourceRootTable;

    /// Per-file throw-analysis facts seeded from a previous compile.
    ///
    /// When present, `throw_inference::file_throw_facts` returns the seeded
    /// facts for a file instead of re-walking its body — the bytecode
    /// cache's per-file reuse sets this for files whose content is
    /// unchanged (facts are a pure function of file content + name
    /// resolution, and the cache's dirty-set analysis re-walks any file
    /// whose resolution-relevant surroundings changed). Defaults to `None`:
    /// every other database compiles honestly.
    fn seeded_throw_facts(&self) -> Option<inputs::SeededThrowFacts> {
        None
    }

    /// The stdlib packages' resolved typed interfaces from a previous compile,
    /// keyed by package name.
    ///
    /// When present, `package_interface::package_interface` returns the seeded
    /// interface for a stdlib package instead of re-deriving it from source —
    /// removing the cold-typecheck floor a fresh process otherwise pays to
    /// re-normalize every stdlib signature/class/alias before it can typecheck
    /// user code. The stdlib is a compiler-build constant (no user file can
    /// contribute to a stdlib package), so the CLI caches this once per compiler
    /// build under `bex_cache::stdlib_interface_key` and seeds it back on every
    /// compile. Defaults to `None`: every other database compiles honestly.
    fn seeded_stdlib_interface(&self) -> Option<inputs::SeededStdlibInterface> {
        None
    }

    /// Per-function `callable_throws` values from a previous compile, keyed by
    /// (source path, item-tree `LocalItemId`).
    ///
    /// When present, `callable::callable_throws` returns the seeded `Ty` for a
    /// clean function instead of inferring its body — the bytecode cache sets
    /// this for functions the per-file reuse plan proved unchanged (both their
    /// own body and their transitive throw contributors are stable, per the
    /// throws-taint closure). Cutting `callable_throws` removes the last cold
    /// `infer_scope_types` pull a dirty file otherwise forces on its clean
    /// callees. Defaults to `None`: every other database infers honestly.
    fn seeded_callable_throws(&self) -> Option<inputs::SeededCallableThrows> {
        None
    }

    /// Source-less dependency packages mounted into this database as serialized
    /// `PackageInterface` blobs, keyed by the package name (the mount alias).
    ///
    /// When present (BEP-066 mounted-package linking), each entry makes its name a *dependency*
    /// of every user package (`package_dependencies`) whose `package_interface`
    /// is served straight from the blob — the mounted package has **no source
    /// files** (`package_items` is empty; that is the point). Cross-package
    /// resolution for a mounted name goes through the interface rows instead of
    /// raw items. Names colliding with the reserved package set (the stdlib
    /// packages, `user`, `root`, `env`) are ignored entirely — see
    /// `crate::package::mounted_package_names`. Defaults to `None`:
    /// every other database resolves dependencies from source only.
    fn mounted_packages(&self) -> Option<inputs::MountedPackages> {
        None
    }
}

// ── compiler2_all_files ───────────────────────────────────────────────────────

/// Returns all files visible to compiler2 queries, in source-root table order.
///
/// `Stdlib` roots come FIRST and `Dynamic` roots LAST (the table's order
/// invariant): everything
/// assigned by whole-program iteration order downstream (emit's
/// `GlobalIndex`/`ObjectIndex` slots, MIR's `class_type_tags`) then gives the
/// stdlib a stable prefix of every index space, independent of user code.
/// That stability is what lets a precompiled stdlib `Program` slice (keyed
/// only by compiler build) be spliced into any project's compile. User edits
/// only ever shift *user* indices.
///
/// Whole-program consumers only (check drivers, emit, MIR tags, caches).
/// Package-scoped readers use [`package::package_files`] so edits in one
/// root cannot invalidate another package's file-set-derived queries.
pub fn compiler2_all_files(db: &dyn Db) -> Vec<baml_base::SourceFile> {
    let roots = db.source_roots().roots(db);
    debug_assert!(
        roots.is_sorted_by_key(|root| root.kind(db)),
        "source-root table order invariant violated (Stdlib < Dependency < Workspace < Dynamic)"
    );
    roots
        .iter()
        .flat_map(|root| root.files(db).iter().copied())
        .collect()
}

// ── file_ast ──────────────────────────────────────────────────────────────────

/// The CST → AST lowering output for one file: top-level items plus the
/// lowering diagnostics and `env.*` references produced along the way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileAst {
    pub items: Vec<baml_compiler2_ast::Item>,
    pub diagnostics: Vec<baml_compiler2_ast::LoweringDiagnostic>,
    pub env_var_refs: Vec<baml_compiler2_ast::EnvVarRef>,
}

/// CST → AST lowering for one file, computed once and shared.
///
/// Salsa-tracked because several different consumers need a file's AST items:
/// both `file_semantic_index` queries (HIR + PPIR), `ppir_expansion_items`,
/// PPIR's two project-wide expansion-map collectors, and the LSP check pass.
/// Before this query existed each of them re-lowered the syntax tree from
/// scratch; the repeated CST traversal was ~31% of cold-compile CPU on the
/// test corpus (see `crates/tools_compile_profile/README.md`, July 2026 audit).
#[salsa::tracked(returns(ref))]
pub fn file_ast(db: &dyn Db, file: SourceFile) -> FileAst {
    let tree = baml_compiler_parser::syntax_tree(db, file);
    let path = file.path(db);
    let package = file_package::file_package(db, file);
    let test_owner = if package.namespace_path.is_empty() {
        "root".to_string()
    } else {
        format!(
            "root.{}",
            package
                .namespace_path
                .iter()
                .map(baml_base::Name::as_str)
                .collect::<Vec<_>>()
                .join(".")
        )
    };
    let lower = if file.is_session_submission(db) {
        baml_compiler2_ast::lower_session_file_with_path_and_test_owner
    } else {
        baml_compiler2_ast::lower_file_with_path_and_test_owner
    };
    let (items, diagnostics, env_var_refs) = lower(&tree, Some(path.as_path()), Some(&test_owner));
    FileAst {
        items,
        diagnostics,
        env_var_refs,
    }
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
    // CST → AST lowering is shared via `file_ast` instead of being redone here.
    let ast = file_ast(db, file);

    let builder = SemanticIndexBuilder::new(db, file);
    builder
        .with_lowering_diagnostics(ast.diagnostics.clone())
        .with_env_var_refs(ast.env_var_refs.clone())
        .build(&ast.items, file_range)
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
///
/// `pub(crate)`: the raw `ItemTree` is an implementation detail behind the
/// PPIR item-data firewall (`baml_compiler2_ppir::item_data`). Consumers use
/// the enumeration (`file_classes`/`file_functions`/…) and lookup
/// (`class_data`/`function_data`/…) queries there, never the tree itself.
pub(crate) fn file_item_tree(db: &dyn Db, file: SourceFile) -> Arc<ItemTree> {
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

/// Returns the env var references found in a file's expression bodies.
pub fn file_env_var_refs(db: &dyn Db, file: SourceFile) -> &[baml_compiler2_ast::EnvVarRef] {
    &file_semantic_index(db, file).env_var_refs
}
