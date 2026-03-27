//! Pre-Processed Intermediate Representation (PPIR) for compiler2.
//!
//! Pipeline: CST -> AST -> HIR (raw) -> PPIR (expansion + canonical) -> TIR.
//!
//! PPIR uses HIR's `package_items` for symbol classification (class vs enum vs alias),
//! then synthesizes `*$stream` AST items and provides canonical queries that include
//! both original and synthetic items.
//!
//! **No union simplification in PPIR.** Deferred to TIR.

pub mod expand;
pub mod ty;

use std::sync::Arc;

use baml_base::{Name, SourceFile, attr::TyAttrValue};
use baml_compiler2_ast as ast;
use baml_compiler2_hir::{
    contributions::FileSymbolContributions,
    item_tree::ItemTree,
    namespace::{NameConflict, NamespaceId, NamespaceItems},
    package::{PackageId, PackageItems, PackageItemsExtra},
    scope::ScopeId,
    semantic_index::{FileSemanticIndex, ScopeBindings},
};
pub use expand::{ExpandCtx, SapAttrs, expand_partial, stream_expand};
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::SmolStr;
use text_size::TextRange;
pub use ty::{CannotBeStreamedOrigin, PpirRawField, PpirTy, PpirTypeAttrs};

// -- Db trait -----------------------------------------------------------------

#[salsa::db]
pub trait Db: baml_compiler2_hir::Db {}

// -- Tracked structs ----------------------------------------------------------

/// Per-file synthetic AST items produced by PPIR expansion.
#[salsa::tracked]
pub struct PpirExpansionItems<'db> {
    #[tracked]
    #[returns(ref)]
    pub items: Vec<ast::Item>,
}

// -- Block attributes ---------------------------------------------------------

/// Collect all @@ block attributes per type across all files.
pub fn collect_block_attrs(
    db: &dyn crate::Db,
    project: baml_workspace::Project,
) -> FxHashMap<Vec<Name>, Vec<Name>> {
    let mut result = FxHashMap::default();
    for file in project.files(db) {
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
                let mut full_path = vec![pkg_info.package.clone()];
                full_path.extend(pkg_info.namespace_path.iter().cloned());
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

/// Collect type alias bodies (qualified path → `PpirTy`) across all files.
pub fn collect_alias_bodies(
    db: &dyn crate::Db,
    project: baml_workspace::Project,
) -> FxHashMap<Vec<Name>, PpirTy> {
    let mut result = FxHashMap::default();
    for file in project.files(db) {
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, *file);
        let cst = baml_compiler_parser::syntax_tree(db, *file);
        let (items, _) = ast::lower_file(&cst);
        for item in &items {
            if let ast::Item::TypeAlias(a) = item {
                let ty = a
                    .type_expr
                    .as_ref()
                    .map(|s| PpirTy::from_type_expr(&s.expr))
                    .unwrap_or(PpirTy::CannotBeStreamed {
                        origin: ty::CannotBeStreamedOrigin::Unknown,
                        attrs: PpirTypeAttrs::default(),
                    });
                let mut full_path = vec![pkg_info.package.clone()];
                full_path.extend(pkg_info.namespace_path.iter().cloned());
                full_path.push(a.name.clone());
                result.insert(full_path, ty);
            }
        }
    }
    result
}

// -- Helpers ------------------------------------------------------------------

/// Build a map of all packages' items for cross-package type classification.
fn build_all_package_items(
    db: &dyn crate::Db,
) -> FxHashMap<Name, &baml_compiler2_hir::package::PackageItems<'_>> {
    let mut result = FxHashMap::default();
    for file in baml_compiler2_hir::compiler2_all_files(db) {
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
        let pkg_name = pkg_info.package.clone();
        result.entry(pkg_name.clone()).or_insert_with(|| {
            let pkg_id = PackageId::new(db, pkg_name);
            baml_compiler2_hir::package::package_items(db, pkg_id)
        });
    }
    result
}

fn make_raw_attr_no_args(name: &str) -> ast::RawAttribute {
    ast::RawAttribute {
        name: SmolStr::new(name),
        args: Vec::new(),
        span: TextRange::default(),
    }
}

// -- Salsa queries ------------------------------------------------------------

/// Compute synthetic `*$stream` AST items for a single file in one pass.
#[salsa::tracked]
pub fn ppir_expansion_items(db: &dyn Db, file: SourceFile) -> PpirExpansionItems<'_> {
    let cst = baml_compiler_parser::syntax_tree(db, file);
    let (items, _) = ast::lower_file(&cst);

    // Get HIR classification for the file's package (original types only)
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let package_name = pkg_info.package.clone();
    let pkg_id = PackageId::new(db, pkg_info.package);
    let package_items = baml_compiler2_hir::package::package_items(db, pkg_id);

    // Build cross-package items map for resolving foreign type references
    let all_package_items = build_all_package_items(db);

    // Get @@ block attributes and alias bodies
    let project = db.project();
    let block_attrs = collect_block_attrs(db, project);
    let alias_bodies = collect_alias_bodies(db, project);

    let mut synthetic_items = Vec::new();
    let mut seen_class_names = FxHashSet::default();
    let mut seen_alias_names = FxHashSet::default();

    for item in &items {
        match item {
            ast::Item::Class(c) => {
                if c.name.ends_with("$stream") {
                    continue;
                }
                if !seen_class_names.insert(c.name.clone()) {
                    continue;
                }

                // Clone the original class and rename it
                let mut stream_class = c.clone();
                stream_class.name = SmolStr::new(format!("{}$stream", c.name));
                // $stream classes don't have methods or generic params
                stream_class.methods.clear();
                stream_class.generic_params.clear();
                // Use a dummy span so the synthetic class doesn't shadow the
                // original in offset-based scope lookup (scope_at_offset
                // iterates in reverse and would find this scope first if it
                // shared the original class's span).
                stream_class.span = TextRange::default();
                stream_class.name_span = TextRange::default();

                // Transform each field in-place
                stream_class.fields.retain_mut(|field| {
                    let ppir_ty = field
                        .type_expr
                        .as_ref()
                        .map(|s| PpirTy::from_type_expr(&s.expr))
                        .unwrap_or(PpirTy::CannotBeStreamed {
                            origin: ty::CannotBeStreamedOrigin::Unknown,
                            attrs: PpirTypeAttrs::default(),
                        });
                    let ctx = ExpandCtx {
                        package_name: &package_name,
                        namespace_path: &pkg_info.namespace_path,
                        package_items,
                        all_package_items: &all_package_items,
                        block_attrs: &block_attrs,
                        alias_bodies: &alias_bodies,
                    };
                    let (stream_type, sap_attrs) = stream_expand(&ppir_ty, &ctx);

                    // Build the new type expr from stream_expand result
                    let mut type_expr = stream_type.to_type_expr();

                    // Build type-level SAP attrs
                    if sap_attrs.parse_without_null == TyAttrValue::Set {
                        type_expr
                            .attrs_mut()
                            .push(make_raw_attr_no_args("sap.parse_without_null"));
                    }
                    if sap_attrs.pending_never == TyAttrValue::Set {
                        type_expr
                            .attrs_mut()
                            .push(make_raw_attr_no_args("sap.pending_never"));
                    }
                    if sap_attrs.in_progress_never == TyAttrValue::Set {
                        type_expr
                            .attrs_mut()
                            .push(make_raw_attr_no_args("sap.in_progress_never"));
                    }

                    // Preserve non-stream type attributes from the original outermost TypeExpr
                    if let Some(orig_spanned) = &field.type_expr {
                        for attr in orig_spanned.expr.attrs() {
                            if !attr.name.starts_with("stream.") && !attr.name.starts_with("sap.") {
                                type_expr.attrs_mut().push(attr.clone());
                            }
                        }
                    }

                    // Replace the field's type expr (preserves field.name, field.attributes, field.span, etc.)
                    field.type_expr = Some(ast::SpannedTypeExpr {
                        expr: type_expr,
                        span: field
                            .type_expr
                            .as_ref()
                            .map_or(TextRange::default(), |s| s.span),
                    });

                    // Strip stream.* from field-level attributes (preserve @alias, @description, @skip, etc.)
                    field.attributes.retain(|a| !a.name.starts_with("stream."));

                    true
                });

                // Transform class-level attributes: strip stream.*, add sap.* equivalents
                let has_stream_done = stream_class
                    .attributes
                    .iter()
                    .any(|a| a.name == "stream.done");
                stream_class
                    .attributes
                    .retain(|a| !a.name.starts_with("stream."));
                if has_stream_done {
                    stream_class
                        .attributes
                        .push(make_raw_attr_no_args("sap.in_progress_never"));
                }

                synthetic_items.push(ast::Item::Class(stream_class));
            }
            ast::Item::TypeAlias(a) => {
                if a.name.ends_with("$stream") {
                    continue;
                }
                if !seen_alias_names.insert(a.name.clone()) {
                    continue;
                }

                let ty = a
                    .type_expr
                    .as_ref()
                    .map(|s| PpirTy::from_type_expr(&s.expr))
                    .unwrap_or(PpirTy::CannotBeStreamed {
                        origin: ty::CannotBeStreamedOrigin::Unknown,
                        attrs: PpirTypeAttrs::default(),
                    });

                let ctx = ExpandCtx {
                    package_name: &package_name,
                    namespace_path: &pkg_info.namespace_path,
                    package_items,
                    all_package_items: &all_package_items,
                    block_attrs: &block_attrs,
                    alias_bodies: &alias_bodies,
                };
                let expanded_body = expand_partial(&ty, &ctx);

                // Clone original alias and modify
                let mut stream_alias = a.clone();
                stream_alias.name = SmolStr::new(format!("{}$stream", a.name));
                // Use dummy spans to avoid shadowing the original in scope_at_offset.
                stream_alias.span = TextRange::default();
                stream_alias.name_span = TextRange::default();

                let mut new_type_expr = expanded_body.to_type_expr();

                // Preserve non-stream type attributes from original alias body
                if let Some(orig_spanned) = &a.type_expr {
                    for attr in orig_spanned.expr.attrs() {
                        if !attr.name.starts_with("stream.") && !attr.name.starts_with("sap.") {
                            new_type_expr.attrs_mut().push(attr.clone());
                        }
                    }
                }

                stream_alias.type_expr = Some(ast::SpannedTypeExpr {
                    expr: new_type_expr,
                    span: a
                        .type_expr
                        .as_ref()
                        .map_or(TextRange::default(), |s| s.span),
                });

                synthetic_items.push(ast::Item::TypeAlias(stream_alias));
            }
            _ => {}
        }
    }

    PpirExpansionItems::new(db, synthetic_items)
}

// -- Canonical queries (original + *$stream) ----------------------------------

/// Canonical semantic index: original AST items + PPIR synthetic *$stream items.
#[salsa::tracked(returns(ref), no_eq)]
pub fn file_semantic_index(db: &dyn Db, file: SourceFile) -> FileSemanticIndex<'_> {
    let tree = baml_compiler_parser::syntax_tree(db, file);
    let file_range = tree.text_range();
    let (mut items, _) = ast::lower_file(&tree);

    // Merge synthetic *$stream items
    let expansion = ppir_expansion_items(db, file);
    items.extend(expansion.items(db).iter().cloned());

    // Re-run HIR builder on merged items
    baml_compiler2_hir::SemanticIndexBuilder::new(db, file).build(&items, file_range)
}

/// Canonical symbol contributions (original + *$stream types).
pub fn file_symbol_contributions(
    db: &dyn Db,
    file: SourceFile,
) -> Arc<FileSymbolContributions<'_>> {
    let index = file_semantic_index(db, file);
    Arc::clone(&index.symbol_contributions)
}

/// Canonical item tree (original + *$stream types).
pub fn file_item_tree(db: &dyn Db, file: SourceFile) -> Arc<ItemTree> {
    let index = file_semantic_index(db, file);
    Arc::clone(&index.item_tree)
}

/// Returns the `ScopeBindings` for a given scope (canonical index).
pub fn scope_bindings_query<'db>(db: &'db dyn Db, scope_id: ScopeId<'db>) -> ScopeBindings {
    let file = scope_id.file(db);
    let index = file_semantic_index(db, file);
    let local_id = scope_id.file_scope_id(db);
    index.scope_bindings[local_id.index() as usize].clone()
}

/// Canonical namespace items (original + *$stream types).
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

    let mut matching_files: Vec<SourceFile> = baml_compiler2_hir::compiler2_all_files(db)
        .into_iter()
        .filter(|file| {
            let pkg_info = baml_compiler2_hir::file_package::file_package(db, *file);
            pkg_info.package == *package && pkg_info.namespace_path == *ns_path
        })
        .collect();
    matching_files.sort_by_key(|a| a.path(db));

    // Uses PPIR's file_symbol_contributions (canonical, includes *$stream types).
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

/// Canonical package items (original + *$stream types).
#[salsa::tracked(returns(ref))]
pub fn package_items<'db>(db: &'db dyn Db, package_id: PackageId<'db>) -> PackageItems<'db> {
    let package_name = package_id.name(db);

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
            shadows: vec![],
        }))
    };

    PackageItems { namespaces, extra }
}
