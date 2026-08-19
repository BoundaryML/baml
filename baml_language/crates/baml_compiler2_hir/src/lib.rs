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
pub mod item_tree;
pub mod loc;
pub mod nameres;
pub mod namespace;
pub mod package;
pub mod scope;
pub mod semantic_index;
pub mod signature;
pub mod type_ref;

use std::sync::Arc;

use baml_base::SourceFile;
pub use builder::SemanticIndexBuilder;
pub use semantic_index::{ExprMetadataKey, ExprMetadataScope, PathResolution};

use crate::{
    contributions::FileSymbolContributions,
    item_tree::ItemTree,
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
    // Builtin stubs come FIRST: everything assigned by whole-project
    // iteration order downstream (emit's `GlobalIndex`/`ObjectIndex` slots,
    // MIR's `class_type_tags`) then gives the stdlib a stable prefix of every
    // index space, independent of user code. That stability is what lets a
    // precompiled stdlib `Program` slice (keyed only by compiler build) be
    // spliced into any project's compile. User edits only ever shift *user*
    // indices. Within each group the order is unchanged (sorted project
    // files; fixed stub order), and no package receives contributions from
    // both groups, so HIR per-package merge order is unaffected.
    let mut files: Vec<baml_base::SourceFile> = db
        .compiler2_extra_files()
        .map(|extra| extra.files(db).clone())
        .unwrap_or_default();
    files.extend(
        db.project()
            .files(db)
            .iter()
            .copied()
            .filter(|file| !file.path(db).to_string_lossy().starts_with("<builtin>/")),
    );
    files
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

// Safety: `FileAst` holds only plain (non-`'db`) data, so storing it by value
// in a Salsa slot is sound. This manual `Update` impl uses `PartialEq` so the
// query gets early-cutoff (dependents skip re-running when the AST is
// unchanged) rather than the always-`true` behavior of a no-eq value.
#[allow(unsafe_code)]
unsafe impl salsa::Update for FileAst {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        #[allow(unsafe_code)]
        let old = unsafe { &*old_pointer };
        if old == &new_value {
            false
        } else {
            #[allow(unsafe_code)]
            unsafe {
                std::ptr::drop_in_place(old_pointer);
                std::ptr::write(old_pointer, new_value);
            }
            true
        }
    }
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
    let opts = baml_compiler2_ast::LowerFileOpts {
        test_owner: Some(&test_owner),
        // The reserved `baml` package defines the builtin names themselves
        // (`type json = ...`), so it is exempt from the reserved-name check.
        in_builtin_package: package.package.as_str() == "baml",
    };
    let (items, diagnostics, env_var_refs) = lower(&tree, Some(path.as_path()), opts);
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
