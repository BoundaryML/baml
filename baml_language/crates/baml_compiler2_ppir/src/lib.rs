//! Pre-Processed Intermediate Representation (PPIR) for compiler2.
//!
//! Depends on HIR. Uses HIR's `raw_package_items` for namespace-aware
//! symbol classification during stream expansion. Hosts the canonical
//! queries (`file_semantic_index`, `namespace_items`, `package_items`)
//! that include both original AST items and synthetic `stream_*` items.
//!
//! Pipeline: syntax → HIR (raw) → PPIR (expansion + canonical) → TIR.
//!
//! **No union simplification in PPIR.** Synthesized field types may contain
//! redundant unions (e.g., `null | null | string`, `never | int`). This is
//! deliberate — union simplification (flattening, dedup, never-removal,
//! literal subsumption) is deferred to TIR, which already has the machinery
//! for type normalization. Keeping PPIR output unsimplified makes the
//! expansion logic simpler and easier to reason about.

pub mod desugar;
pub mod synthesize;
pub mod ty;

use std::sync::Arc;

use baml_base::{Name, SourceFile};
use baml_compiler2_ast as ast;
use baml_compiler2_hir::{
    contributions::FileSymbolContributions,
    item_tree::ItemTree,
    namespace::{NameConflict, NamespaceId, NamespaceItems},
    package::{PackageId, PackageItems, PackageItemsExtra},
    semantic_index::{FileSemanticIndex, ScopeBindings},
};
pub use desugar::{
    PpirDesugaredClass, PpirDesugaredField, PpirDesugaredTypeAlias, PpirStreamStartsAs,
    build_ppir_field, default_sap_starts_as, desugar_field, stream_expand,
};
use rustc_hash::{FxHashMap, FxHashSet};
pub use ty::{PpirRawField, PpirTy, PpirTypeAttrs};

//
// ──────────────────────────────────────────────────────── DATABASE ─────
//

/// Database trait for PPIR queries.
///
/// Extends `baml_compiler2_hir::Db`. PPIR uses HIR's `raw_package_items`
/// for namespace-aware symbol classification, then hosts the canonical
/// queries (`file_semantic_index`, `namespace_items`, `package_items`) that
/// include both original and synthetic `stream_*` items.
#[salsa::db]
pub trait Db: baml_compiler2_hir::Db {}

//
// ───────────────────────────────────────────────────── TRACKED STRUCTS ─────
//

/// Per-file result of PPIR desugaring.
/// Contains desugared data for classes and type aliases.
#[salsa::tracked]
pub struct PpirDesugaredItems<'db> {
    #[tracked]
    #[returns(ref)]
    pub classes: Vec<PpirDesugaredClass>,
    #[tracked]
    #[returns(ref)]
    pub type_aliases: Vec<PpirDesugaredTypeAlias>,
}

/// Per-file synthetic AST items produced by PPIR expansion.
/// Contains `stream_*` class and type alias definitions ready to merge into HIR.
#[salsa::tracked]
pub struct PpirExpansionItems<'db> {
    #[tracked]
    #[returns(ref)]
    pub items: Vec<ast::Item>,
}

//
// ──────────────────────────────────────────────── BLOCK ATTRIBUTES ─────
//

/// Collect all @@ block attributes per type.
/// Scans AST items (classes and enums) across all user files.
/// Keyed by fully-qualified path (`namespace_path` + name) to avoid collisions
/// between types with the same bare name in different namespaces.
/// Returns path → list of all block attribute names (e.g., `stream.done`, `stream.not_null`).
pub fn collect_block_attrs(
    db: &dyn crate::Db,
    project: baml_workspace::Project,
) -> FxHashMap<Vec<Name>, Vec<Name>> {
    let mut result = FxHashMap::default();
    for file in project.files(db) {
        if file
            .path(db)
            .to_str()
            .is_some_and(|p| p.starts_with("<builtin>/") || p.starts_with("<generated:"))
        {
            continue;
        }
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, *file);
        let cst = baml_compiler_parser::syntax_tree(db, *file);
        let (items, _) = ast::lower_file(&cst);
        for item in &items {
            let (name, item_attrs) = match item {
                ast::Item::Class(c) => (&c.name, &c.attributes),
                ast::Item::Enum(e) => (&e.name, &e.attributes),
                _ => continue,
            };
            let attr_names: Vec<Name> = item_attrs.iter().map(|a| a.name.clone()).collect();
            if !attr_names.is_empty() {
                let mut full_path = pkg_info.namespace_path.clone();
                full_path.push(name.clone());
                result
                    .entry(full_path)
                    .or_insert_with(Vec::new)
                    .extend(attr_names);
            }
        }
    }
    result
}

fn is_builtin_or_generated(db: &dyn crate::Db, file: SourceFile) -> bool {
    file.path(db)
        .to_str()
        .is_some_and(|p| p.starts_with("<builtin>/") || p.starts_with("<generated:"))
}

//
// ────────────────────────────────────────────────────────── SALSA QUERIES ─────
//

/// Compute desugared stream data for a single file.
///
/// Uses HIR's `raw_package_items` for namespace-aware type classification.
/// For each class: runs `stream_expand` per field to compute `stream_type`,
/// synthesizes `@sap.*` attributes (`sap_in_progress_never`, `sap_starts_as`).
/// For each type alias: runs `stream_expand` on the alias body.
#[salsa::tracked]
pub fn ppir_desugared_items(db: &dyn Db, file: SourceFile) -> PpirDesugaredItems<'_> {
    if is_builtin_or_generated(db, file) {
        return PpirDesugaredItems::new(db, Vec::new(), Vec::new());
    }

    let cst = baml_compiler_parser::syntax_tree(db, file);
    let (items, _) = ast::lower_file(&cst);

    // Get HIR classification for the file's package (raw = original types only)
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package);
    let package_items = baml_compiler2_hir::package::raw_package_items(db, pkg_id);

    // Get @@ block attributes
    let project = db.project();
    let block_attrs = collect_block_attrs(db, project);

    let mut desugared_classes = Vec::new();
    let mut desugared_aliases = Vec::new();
    let mut seen_class_names = FxHashSet::default();
    let mut seen_alias_names = FxHashSet::default();

    for item in &items {
        match item {
            ast::Item::Class(c) => {
                if c.name.starts_with("stream_") {
                    continue;
                }
                if !seen_class_names.insert(c.name.clone()) {
                    continue;
                }

                let ppir_fields: Vec<PpirRawField> =
                    c.fields.iter().map(build_ppir_field).collect();
                let desugared_fields: Vec<PpirDesugaredField> = ppir_fields
                    .iter()
                    .map(|pf| desugar_field(pf, package_items, &block_attrs))
                    .collect();

                desugared_classes.push(PpirDesugaredClass {
                    name: c.name.clone(),
                    fields: desugared_fields,
                });
            }
            ast::Item::TypeAlias(a) => {
                if a.name.starts_with("stream_") {
                    continue;
                }
                if !seen_alias_names.insert(a.name.clone()) {
                    continue;
                }

                let ty = a
                    .type_expr
                    .as_ref()
                    .map(|s| PpirTy::from_type_expr(&s.expr, &[]))
                    .unwrap_or(PpirTy::Unknown {
                        attrs: PpirTypeAttrs::default(),
                    });

                let expanded_body = stream_expand(&ty, package_items);

                desugared_aliases.push(PpirDesugaredTypeAlias {
                    name: a.name.clone(),
                    expanded_body,
                });
            }
            _ => {}
        }
    }

    PpirDesugaredItems::new(db, desugared_classes, desugared_aliases)
}

/// Compute synthetic AST items for a single file's stream_* definitions.
#[salsa::tracked]
pub fn ppir_expansion_items(db: &dyn Db, file: SourceFile) -> PpirExpansionItems<'_> {
    let desugared = ppir_desugared_items(db, file);
    if desugared.classes(db).is_empty() && desugared.type_aliases(db).is_empty() {
        return PpirExpansionItems::new(db, Vec::new());
    }

    // Collect original class attributes for @@stream.done detection and passthrough
    let cst = baml_compiler_parser::syntax_tree(db, file);
    let (items, _) = ast::lower_file(&cst);
    let original_class_attrs: FxHashMap<Name, Vec<ast::RawAttribute>> = items
        .iter()
        .filter_map(|item| match item {
            ast::Item::Class(c) => Some((c.name.clone(), c.attributes.clone())),
            _ => None,
        })
        .collect();

    let synthetic = synthesize::synthesize_stream_items(
        desugared.classes(db),
        desugared.type_aliases(db),
        &original_class_attrs,
    );
    PpirExpansionItems::new(db, synthetic)
}

//
// ────────────────────────────────── CANONICAL QUERIES (original + stream_*) ─────
//
// These are the queries that TIR and all downstream consumers use.
// They re-run HIR's SemanticIndexBuilder on original + synthetic items.
//

/// Canonical semantic index: original AST items + PPIR synthetic stream_* items.
/// Re-runs HIR's `SemanticIndexBuilder` on the merged item list.
#[salsa::tracked(returns(ref), no_eq)]
pub fn file_semantic_index(db: &dyn Db, file: SourceFile) -> FileSemanticIndex<'_> {
    let tree = baml_compiler_parser::syntax_tree(db, file);
    let file_range = tree.text_range();
    let (mut items, _) = ast::lower_file(&tree);

    // Merge synthetic stream_* items
    let expansion = ppir_expansion_items(db, file);
    items.extend(expansion.items(db).iter().cloned());

    // Re-run HIR builder on merged items
    baml_compiler2_hir::SemanticIndexBuilder::new(db, file).build(&items, file_range)
}

/// Canonical symbol contributions (original + stream_* types).
pub fn file_symbol_contributions(
    db: &dyn Db,
    file: SourceFile,
) -> Arc<FileSymbolContributions<'_>> {
    let index = file_semantic_index(db, file);
    Arc::clone(&index.symbol_contributions)
}

/// Canonical item tree (original + stream_* types).
pub fn file_item_tree(db: &dyn Db, file: SourceFile) -> Arc<ItemTree> {
    let index = file_semantic_index(db, file);
    Arc::clone(&index.item_tree)
}

/// Returns the `ScopeBindings` for a given scope (canonical index).
pub fn scope_bindings_query<'db>(
    db: &'db dyn Db,
    scope_id: baml_compiler2_hir::scope::ScopeId<'db>,
) -> ScopeBindings {
    let file = scope_id.file(db);
    let index = file_semantic_index(db, file);
    let local_id = scope_id.file_scope_id(db);
    index.scope_bindings[local_id.index() as usize].clone()
}

/// Canonical namespace items (original + stream_* types).
#[salsa::tracked(returns(ref))]
pub fn namespace_items<'db>(
    db: &'db dyn Db,
    namespace_id: NamespaceId<'db>,
) -> NamespaceItems<'db> {
    use baml_compiler2_hir::{
        contributions::{Contribution, Definition},
        namespace::{ConflictEntry, NamespaceItemsExtra},
    };

    let package = namespace_id.package(db);
    let ns_path = namespace_id.path(db);

    // Collect matching files, then sort alphabetically by path.
    let mut matching_files: Vec<SourceFile> = baml_compiler2_hir::compiler2_all_files(db)
        .into_iter()
        .filter(|file| {
            let pkg_info = baml_compiler2_hir::file_package::file_package(db, *file);
            pkg_info.package == *package && pkg_info.namespace_path == *ns_path
        })
        .collect();
    matching_files.sort_by_key(|a| a.path(db));

    // Accumulate all contributions per name (preserving file order).
    // Uses PPIR's file_symbol_contributions (canonical, includes stream_* types).
    let mut type_defs: FxHashMap<Name, Vec<Contribution<'db>>> = FxHashMap::default();
    let mut value_defs: FxHashMap<Name, Vec<Contribution<'db>>> = FxHashMap::default();

    for file in &matching_files {
        let contributions = file_symbol_contributions(db, *file);
        for (name, contrib) in &contributions.types {
            type_defs.entry(name.clone()).or_default().push(*contrib);
        }
        for (name, contrib) in &contributions.values {
            value_defs.entry(name.clone()).or_default().push(*contrib);
        }
    }

    // First definition wins; collect conflicts for names with len > 1.
    let mut types: FxHashMap<Name, Definition<'db>> = FxHashMap::default();
    let mut values: FxHashMap<Name, Definition<'db>> = FxHashMap::default();
    let mut conflicts: Vec<NameConflict<'db>> = Vec::new();

    for (name, contribs) in type_defs {
        types.insert(name.clone(), contribs[0].definition);
        if contribs.len() > 1 {
            conflicts.push(NameConflict {
                name,
                entries: contribs
                    .into_iter()
                    .map(|c| ConflictEntry {
                        definition: c.definition,
                        name_span: c.name_span,
                    })
                    .collect(),
            });
        }
    }
    for (name, contribs) in value_defs {
        values.insert(name.clone(), contribs[0].definition);
        if contribs.len() > 1 {
            conflicts.push(NameConflict {
                name,
                entries: contribs
                    .into_iter()
                    .map(|c| ConflictEntry {
                        definition: c.definition,
                        name_span: c.name_span,
                    })
                    .collect(),
            });
        }
    }

    // Sort conflicts by name for deterministic output.
    conflicts.sort_by(|a, b| a.name.cmp(&b.name));

    let extra = if conflicts.is_empty() {
        None
    } else {
        Some(Box::new(NamespaceItemsExtra { conflicts }))
    };

    NamespaceItems {
        types,
        values,
        extra,
    }
}

/// Canonical package items (original + stream_* types).
#[salsa::tracked(returns(ref))]
pub fn package_items<'db>(db: &'db dyn Db, package_id: PackageId<'db>) -> PackageItems<'db> {
    let package_name = package_id.name(db);

    // Discover all unique namespace paths for this package.
    let mut ns_paths: std::collections::HashSet<Vec<Name>> = std::collections::HashSet::new();
    for file in baml_compiler2_hir::compiler2_all_files(db) {
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
        if pkg_info.package == *package_name {
            ns_paths.insert(pkg_info.namespace_path.clone());
        }
    }

    let mut namespaces: FxHashMap<Vec<Name>, NamespaceItems<'db>> = FxHashMap::default();
    let mut all_conflicts: Vec<NameConflict<'db>> = Vec::new();
    for ns_path in ns_paths {
        let ns_id = NamespaceId::new(db, package_name.clone(), ns_path.clone());
        let items = namespace_items(db, ns_id);
        all_conflicts.extend(items.conflicts().iter().cloned());
        namespaces.insert(ns_path, items.clone());
    }

    all_conflicts.sort_by(|a, b| a.name.cmp(&b.name));

    let extra = if all_conflicts.is_empty() {
        None
    } else {
        Some(Box::new(PackageItemsExtra {
            conflicts: all_conflicts,
        }))
    };

    PackageItems { namespaces, extra }
}
