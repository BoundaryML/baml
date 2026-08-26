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
pub mod item_data;
pub mod resolve;
pub mod ty;

use std::sync::Arc;

use baml_base::{Name, SourceFile, attr::TyAttrValue};
use baml_compiler2_ast as ast;
use baml_compiler2_hir::{
    contributions::FileSymbolContributions,
    item_tree::{ItemTree, ItemTreeSourceMap},
    namespace::{NameConflict, NamespaceId, NamespaceItems},
    package::{PackageId, PackageItems, PackageItemsExtra},
    semantic_index::FileSemanticIndex,
};
pub use expand::{ExpandCtx, SapAttrs, expand_partial, stream_expand};
use indexmap::{IndexMap, IndexSet};
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

/// The files the expansion-map collectors scan: every file of every
/// non-`Stdlib` source root, in table order.
///
/// The stdlib exclusion preserves the pre-source-root behavior, where the
/// collectors scanned the project's user files and the embedded stdlib stubs
/// were held in a separate input they never saw.
fn expansion_map_files(
    db: &dyn crate::Db,
    roots: baml_base::SourceRootTable,
) -> impl Iterator<Item = SourceFile> {
    roots
        .roots(db)
        .iter()
        .filter(|root| match root.kind(db) {
            baml_base::SourceRootKind::Stdlib => false,
            baml_base::SourceRootKind::Dependency
            | baml_base::SourceRootKind::Workspace
            | baml_base::SourceRootKind::Dynamic => true,
        })
        .flat_map(|root| root.files(db).iter().copied())
}

/// Collect all @@ block attributes per type across all non-stdlib files.
pub fn collect_block_attrs(
    db: &dyn crate::Db,
    roots: baml_base::SourceRootTable,
) -> FxHashMap<Vec<Name>, Vec<Name>> {
    let mut result = FxHashMap::default();
    for file in expansion_map_files(db, roots) {
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
        // Reuse the memoized CST → AST lowering instead of re-lowering here.
        let items = &baml_compiler2_hir::file_ast(db, file).items;
        for item in items {
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

/// Collect type alias bodies (qualified path → `PpirTy`) across all
/// non-stdlib files.
pub fn collect_alias_bodies(
    db: &dyn crate::Db,
    roots: baml_base::SourceRootTable,
) -> FxHashMap<Vec<Name>, PpirTy> {
    let mut result = FxHashMap::default();
    for file in expansion_map_files(db, roots) {
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
        // Reuse the memoized CST → AST lowering instead of re-lowering here.
        let items = &baml_compiler2_hir::file_ast(db, file).items;
        for item in items {
            if let ast::Item::TypeAlias(a) = item {
                let ty = a.type_expr.as_ref().map(PpirTy::from_type_expr).unwrap_or(
                    PpirTy::CannotBeStreamed {
                        origin: ty::CannotBeStreamedOrigin::Unknown,
                        attrs: PpirTypeAttrs::default(),
                    },
                );
                let mut full_path = vec![pkg_info.package.clone()];
                full_path.extend(pkg_info.namespace_path.iter().cloned());
                full_path.push(a.name.clone());
                result.insert(full_path, ty);
            }
        }
    }
    result
}

// -- Project-wide expansion maps (memoized) -----------------------------------

/// The whole-project maps consumed by [`ppir_expansion_items`] when building a
/// file's `*$stream` companions.
///
/// Both maps are derived by scanning **every** non-stdlib file (see
/// [`collect_block_attrs`] / [`collect_alias_bodies`]). `ppir_expansion_items`
/// is a per-file query, so computing these inline made expansion `O(files²)`:
/// each of N files re-lowered all N files. Wrapping them in a single
/// root-table-keyed [`salsa::tracked`] query ([`project_expansion_maps`])
/// computes them once and shares the result across every file's expansion.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectExpansionMaps {
    /// `@@` block attributes per type, keyed by fully-qualified path.
    pub block_attrs: FxHashMap<Vec<Name>, Vec<Name>>,
    /// Type alias bodies keyed by fully-qualified path.
    pub alias_bodies: FxHashMap<Vec<Name>, PpirTy>,
}

/// # Safety
///
/// Mirrors [`baml_compiler2_hir::package::PackageItems`]'s impl. The contained
/// maps hold no Salsa-interned (`'db`) data, so storing them by value is sound;
/// `maybe_update` uses `PartialEq` for proper Salsa early-cutoff.
#[allow(unsafe_code)]
unsafe impl salsa::Update for ProjectExpansionMaps {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        // SAFETY: `old_pointer` is valid, aligned, and Salsa-owned.
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

/// Compute the project-wide [`ProjectExpansionMaps`] once, memoized by Salsa.
///
/// `roots` (the database's one [`baml_base::SourceRootTable`]) is only the
/// memo key; the body's per-root/per-file reads are tracked as dependencies
/// through `db` as usual.
#[salsa::tracked(returns(ref))]
pub fn project_expansion_maps(
    db: &dyn crate::Db,
    roots: baml_base::SourceRootTable,
) -> ProjectExpansionMaps {
    ProjectExpansionMaps {
        block_attrs: collect_block_attrs(db, roots),
        alias_bodies: collect_alias_bodies(db, roots),
    }
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

/// Build the `$stream` companion for an LLM function or class method.
///
/// The stream-expanded return type is only available in PPIR, so this cannot
/// be part of the AST-level companion expansion. Class methods use the same
/// path as top-level functions; retaining the method's `self` parameter keeps
/// the generated companion on the class while SDK lowering can hide it.
fn synthesize_llm_stream_companion(
    func: &ast::FunctionDef,
    ctx: &ExpandCtx<'_>,
) -> Option<ast::FunctionDef> {
    let Some(ast::DeclarativeMeta::Llm(llm)) = &func.declarative_meta else {
        return None;
    };
    if !llm.companion_bodies.iter().any(|(t, _)| t == "spec")
        || llm.has_tools
        || func.name.contains('$')
    {
        return None;
    }
    let return_type_spanned = func.return_type.as_ref()?;

    let ppir_ty = PpirTy::from_type_expr(return_type_spanned);
    let (stream_type, _sap_attrs) = stream_expand(&ppir_ty, ctx);
    let stream_type_expr = stream_type.to_type_expr();
    let span = func.span;
    let companion_type_args = vec![stream_type_expr, return_type_spanned.clone()];
    let return_type = ast::TypeExprKind::Path {
        segments: vec![Name::new("ai"), Name::new("stream"), Name::new("Stream")],
        generic_args: companion_type_args.clone(),
        associated_type_bindings: vec![],
        attrs: vec![],
    }
    .at(span);

    let params: Vec<ast::Param> = func
        .params
        .iter()
        .cloned()
        .map(|mut p| {
            if p.name.as_str() == "client" {
                let capability = ast::TypeExprKind::Path {
                    segments: vec![
                        Name::new("ai"),
                        Name::new("stream"),
                        Name::new("StreamingClient"),
                    ],
                    generic_args: vec![],
                    associated_type_bindings: vec![],
                    attrs: vec![],
                }
                .at(span);
                p.type_expr = Some(
                    ast::TypeExprKind::Optional {
                        inner: Box::new(capability),
                        attrs: vec![],
                    }
                    .at(span),
                );
            }
            p
        })
        .collect();

    let user_params: Vec<ast::Param> = func
        .params
        .iter()
        .filter(|p| p.name.as_str() != "client" && p.name.as_str() != "on_event")
        .cloned()
        .collect();
    let (body, source_map) = ast::synthesize_spec_stream_body(
        func.name.as_str(),
        &user_params,
        &func
            .generic_params
            .iter()
            .map(|param| param.name.clone())
            .collect::<Vec<_>>(),
        companion_type_args,
        span,
    );

    Some(ast::FunctionDef {
        name: SmolStr::new(format!("{}$stream", func.name)),
        generic_params: func.generic_params.clone(),
        params,
        defaults: func.defaults.clone(),
        return_type: Some(return_type),
        throws: None,
        body: Some(ast::FunctionBodyDef::Expr(body, source_map)),
        declarative_meta: None,
        metadata: ast::FunctionMetadata::user_facing(ast::FunctionOrigin::Companion),
        is_tagged_template_tag: func.is_tagged_template_tag,
        attributes: vec![],
        docstring: func.docstring.clone(),
        span,
        name_span: func.name_span,
    })
}

// -- Salsa queries ------------------------------------------------------------

/// Compute synthetic `*$stream` AST items for a single file in one pass.
#[salsa::tracked]
pub fn ppir_expansion_items(db: &dyn Db, file: SourceFile) -> PpirExpansionItems<'_> {
    // Reuse the memoized CST → AST lowering instead of re-lowering here.
    let items = &baml_compiler2_hir::file_ast(db, file).items;

    // Get HIR classification for the file's package (original types only)
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let package_name = pkg_info.package.clone();
    let pkg_id = PackageId::new(db, pkg_info.package);
    let package_items = baml_compiler2_hir::package::package_items(db, pkg_id);

    // Build cross-package items map for resolving foreign type references
    let all_package_items = build_all_package_items(db);

    // Get @@ block attributes and alias bodies. Memoized once per root table
    // so this per-file query doesn't re-scan (and re-lower) every file on
    // every file — which made expansion O(files²).
    let expansion_maps = project_expansion_maps(db, db.source_roots());
    let block_attrs = &expansion_maps.block_attrs;
    let alias_bodies = &expansion_maps.alias_bodies;

    let mut synthetic_items = Vec::new();
    let mut seen_class_names = FxHashSet::default();
    let mut seen_alias_names = FxHashSet::default();

    for item in items {
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
                // A $stream class does not *inherit* its base's methods — neither direct ones
                // nor those an in-body `implements` block contributes (a method valid for the
                // base need not be for its stream companion). A stream class participates in an
                // interface only if the user *explicitly* implements it (`implement Foo for
                // Bar$stream { ... }`, a separate out-of-body impl that is untouched here).
                // Dropping the cloned in-body `implements` blocks also drops the interface
                // obligations that would otherwise require those methods. Generic params are
                // preserved so that field types referencing them (e.g. `v: T`) round-trip
                // through as `Ty::TypeVar` instead of collapsing to `Ty::Error`.
                stream_class.methods.clear();
                stream_class.implements.clear();
                // Use a dummy span so the synthetic class doesn't shadow the
                // original in offset-based scope lookup (scope_at_offset
                // iterates in reverse and would find this scope first if it
                // shared the original class's span).
                stream_class.span = TextRange::default();
                stream_class.name_span = TextRange::default();

                // Transform each field in-place
                stream_class.fields.retain_mut(|field| {
                    let ppir_ty = PpirTy::from_type_expr(&field.type_expr);
                    let ctx = ExpandCtx {
                        package_name: &package_name,
                        namespace_path: &pkg_info.namespace_path,
                        package_items,
                        all_package_items: &all_package_items,
                        block_attrs,
                        alias_bodies,
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
                    for attr in field.type_expr.attrs() {
                        if !attr.name.starts_with("stream.") && !attr.name.starts_with("sap.") {
                            type_expr.attrs_mut().push(attr.clone());
                        }
                    }

                    // Replace the field's type expr (preserves field.name, field.attributes, field.span, etc.)
                    field.type_expr = ast::TypeExpr {
                        span: field.type_expr.span,
                        ..type_expr
                    };

                    // Strip stream.* from field-level attributes (preserve @alias, @description, @skip, etc.)
                    field.attributes.retain(|a| !a.name.starts_with("stream."));

                    true
                });

                // `$stream` companions for methods belong to the original
                // class, not to its stream-shaped data class. Keep them in a
                // synthetic same-name class that is merged into the original
                // class when the canonical index is rebuilt below.
                let method_streams: Vec<_> = c
                    .methods
                    .iter()
                    .filter_map(|method| {
                        let ctx = ExpandCtx {
                            package_name: &package_name,
                            namespace_path: &pkg_info.namespace_path,
                            package_items,
                            all_package_items: &all_package_items,
                            block_attrs,
                            alias_bodies,
                        };
                        synthesize_llm_stream_companion(method, &ctx)
                    })
                    .collect();

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
                if !method_streams.is_empty() {
                    let mut method_class = c.clone();
                    method_class.fields.clear();
                    method_class.methods = method_streams;
                    method_class.implements.clear();
                    method_class.attributes.clear();
                    method_class.span = TextRange::default();
                    method_class.name_span = TextRange::default();
                    synthetic_items.push(ast::Item::Class(method_class));
                }
            }
            ast::Item::TypeAlias(a) => {
                if a.name.ends_with("$stream") {
                    continue;
                }
                if !seen_alias_names.insert(a.name.clone()) {
                    continue;
                }

                let ty = a.type_expr.as_ref().map(PpirTy::from_type_expr).unwrap_or(
                    PpirTy::CannotBeStreamed {
                        origin: ty::CannotBeStreamedOrigin::Unknown,
                        attrs: PpirTypeAttrs::default(),
                    },
                );

                let ctx = ExpandCtx {
                    package_name: &package_name,
                    namespace_path: &pkg_info.namespace_path,
                    package_items,
                    all_package_items: &all_package_items,
                    block_attrs,
                    alias_bodies,
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
                    for attr in orig_spanned.attrs() {
                        if !attr.name.starts_with("stream.") && !attr.name.starts_with("sap.") {
                            new_type_expr.attrs_mut().push(attr.clone());
                        }
                    }
                }

                stream_alias.type_expr = Some(ast::TypeExpr {
                    span: a
                        .type_expr
                        .as_ref()
                        .map_or(TextRange::default(), |s| s.span),
                    ..new_type_expr
                });

                synthetic_items.push(ast::Item::TypeAlias(stream_alias));
            }
            // LLM `$stream` companions, single-path StreamingClient design:
            // `Fn$stream(args, client)` = one-turn streaming over the
            // function's own spec, returning the typed partial stream
            // `ai.stream.Stream<Out$stream, Out>`. Synthesized here (not with the
            // AST-level companions) because the body's explicit type args
            // need the stream-expanded return type, which only PPIR can
            // compute. Tools-bearing functions get no `$stream`: streaming
            // does not run the tool loop yet (`LlmBodyDef::has_tools` is the
            // conservative compile-time signal; `ai.from_spec` re-checks
            // the toolbox at runtime for the dynamic cases).
            ast::Item::Function(func) => {
                let ctx = ExpandCtx {
                    package_name: &package_name,
                    namespace_path: &pkg_info.namespace_path,
                    package_items,
                    all_package_items: &all_package_items,
                    block_attrs,
                    alias_bodies,
                };
                if let Some(companion) = synthesize_llm_stream_companion(func, &ctx) {
                    synthetic_items.push(ast::Item::Function(companion));
                }
            }
            _ => {}
        }
    }

    PpirExpansionItems::new(db, synthetic_items)
}

// -- Canonical queries (original + *$stream) ----------------------------------

/// Canonical semantic index: original AST items + PPIR synthetic *$stream items.
///
/// When a file has no synthetic `*$stream` items, the post-expansion index is
/// byte-for-byte the pre-expansion one (same AST items from `file_ast`, same
/// builder, same file range), so this delegates to HIR's already-computed index
/// instead of rebuilding scopes/bindings/contributions a second time.
pub fn file_semantic_index(db: &dyn Db, file: SourceFile) -> &FileSemanticIndex<'_> {
    let expansion = ppir_expansion_items(db, file);
    if expansion.items(db).is_empty() {
        return baml_compiler2_hir::file_semantic_index(db, file);
    }
    file_semantic_index_expanded(db, file)
}

/// The merged (original + `*$stream`) index for files that actually have
/// synthetic items. Callers must go through [`file_semantic_index`].
#[salsa::tracked(returns(ref), no_eq)]
fn file_semantic_index_expanded(db: &dyn Db, file: SourceFile) -> FileSemanticIndex<'_> {
    let tree = baml_compiler_parser::syntax_tree(db, file);
    let file_range = tree.text_range();
    // Reuse the memoized CST → AST lowering instead of re-lowering here.
    let ast_result = baml_compiler2_hir::file_ast(db, file);
    let mut items = ast_result.items.clone();

    // Merge synthetic *$stream items and class-method companions. Class
    // methods are nested under their owning class, so PPIR carries their
    // generated companions in a same-name, method-only class fragment.
    let expansion = ppir_expansion_items(db, file);
    for item in expansion.items(db).iter().cloned() {
        if let ast::Item::Class(fragment) = &item {
            if !fragment.name.ends_with("$stream") {
                if let Some(ast::Item::Class(original)) = items.iter_mut().find(
                    |item| matches!(item, ast::Item::Class(class) if class.name == fragment.name),
                ) {
                    original.methods.extend(fragment.methods.clone());
                    continue;
                }
            }
        }
        items.push(item);
    }

    // Re-run HIR builder on merged items
    baml_compiler2_hir::SemanticIndexBuilder::new(db, file)
        .with_lowering_diagnostics(ast_result.diagnostics.clone())
        .with_env_var_refs(ast_result.env_var_refs.clone())
        .build(&items, file_range)
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
///
/// `pub(crate)`: the raw `ItemTree` is the substrate the `item_data` firewall
/// queries are built on. Consumers use the enumeration
/// (`file_classes`/`file_functions`/…) and lookup (`class_data`/
/// `function_data`/…) queries, never the tree itself — that is what gives
/// per-item invalidation instead of per-file.
pub(crate) fn file_item_tree(db: &dyn Db, file: SourceFile) -> Arc<ItemTree> {
    let index = file_semantic_index(db, file);
    Arc::clone(&index.item_tree)
}

/// Canonical item-tree source map (original + *$stream types).
///
/// `pub(crate)`: spans are served by the per-item `*_source_map` firewall
/// queries in `item_data`.
pub(crate) fn file_item_tree_source_map(db: &dyn Db, file: SourceFile) -> Arc<ItemTreeSourceMap> {
    let index = file_semantic_index(db, file);
    Arc::clone(&index.item_tree_source_map)
}

/// Canonical function body — uses PPIR's item tree (includes synthetic companions).
///
/// TIR should call this instead of `baml_compiler2_hir::body::function_body`
/// so that PPIR-synthesized functions (like `$parse_stream`) are found.
///
/// Salsa-tracked (mirroring HIR's `function_body`): MIR lowering fetches the
/// callee's body at every direct-call site, and the untracked version cloned
/// the entire `ExprBody` arena out of the item tree on every one of those
/// calls. Tracking it caches the `Arc<FunctionBody>` so repeat calls are O(1).
#[salsa::tracked]
pub fn function_body<'db>(
    db: &'db dyn Db,
    function: baml_compiler2_hir::loc::FunctionLoc<'db>,
) -> Arc<baml_compiler2_hir::body::FunctionBody> {
    let file = function.file(db);
    let item_tree = file_item_tree(db, file);
    let func_data = &item_tree[function.id(db)];

    let body = match &func_data.body {
        Some(ast::FunctionBodyDef::Expr(expr_body, _source_map)) => {
            baml_compiler2_hir::body::FunctionBody::Expr(expr_body.clone())
        }
        Some(ast::FunctionBodyDef::Builtin(kind)) => {
            baml_compiler2_hir::body::FunctionBody::Builtin(*kind)
        }
        None => baml_compiler2_hir::body::FunctionBody::Missing,
    };

    Arc::new(body)
}

/// Canonical function body source map — uses PPIR's item tree.
pub fn function_body_source_map<'db>(
    db: &'db dyn Db,
    function: baml_compiler2_hir::loc::FunctionLoc<'db>,
) -> Option<ast::AstSourceMap> {
    let file = function.file(db);
    let item_tree = file_item_tree(db, file);
    let func_data = &item_tree[function.id(db)];

    match &func_data.body {
        Some(ast::FunctionBodyDef::Expr(_body, source_map)) => Some(source_map.clone()),
        _ => None,
    }
}

/// Canonical body for any body owner (rust-analyzer's `DefWithBodyId`
/// pattern). Functions go through PPIR's item tree so synthetic companions
/// are found; PPIR never synthesizes lets, so HIR's `let_body` is canonical
/// for them.
pub fn body<'db>(
    db: &'db dyn Db,
    owner: baml_compiler2_hir::body::BodyOwnerId<'db>,
) -> baml_compiler2_hir::body::OwnerBody {
    use baml_compiler2_hir::body::{BodyOwnerId, OwnerBody};
    match owner {
        BodyOwnerId::Function(function) => OwnerBody::Function(function_body(db, function)),
        BodyOwnerId::Let(let_binding) => {
            OwnerBody::Let(baml_compiler2_hir::body::let_body(db, let_binding))
        }
        BodyOwnerId::ParameterDefaults(function) => {
            OwnerBody::ParameterDefaults(function_parameter_defaults(db, function))
        }
    }
}

/// Canonical body source map for any body owner (spans only).
pub fn body_source_map<'db>(
    db: &'db dyn Db,
    owner: baml_compiler2_hir::body::BodyOwnerId<'db>,
) -> Option<ast::AstSourceMap> {
    use baml_compiler2_hir::body::BodyOwnerId;
    match owner {
        BodyOwnerId::Function(function) => function_body_source_map(db, function),
        BodyOwnerId::Let(let_binding) => {
            baml_compiler2_hir::body::let_body_source_map(db, let_binding)
        }
        BodyOwnerId::ParameterDefaults(function) => Some(
            function_parameter_defaults(db, function)
                .defaults
                .source_map
                .clone(),
        ),
    }
}

/// The scope opened for a body owner's body.
pub fn body_scope<'db>(
    db: &'db dyn Db,
    owner: baml_compiler2_hir::body::BodyOwnerId<'db>,
) -> Option<baml_compiler2_hir::scope::ScopeId<'db>> {
    use baml_compiler2_hir::body::BodyOwnerId;
    match owner {
        BodyOwnerId::Function(function) => item_data::function_scope(db, function),
        BodyOwnerId::Let(let_binding) => item_data::let_scope(db, let_binding),
        // Defaults are keyed under the FUNCTION's scope (the semantic
        // index walks them there, `ExprMetadataScope::ParameterDefault`).
        BodyOwnerId::ParameterDefaults(function) => item_data::function_scope(db, function),
    }
}

/// Every body owner in `file`: functions (methods and synthetic companions
/// included), then top-level lets, each group in source order.
pub fn file_body_owners(
    db: &dyn Db,
    file: baml_base::SourceFile,
) -> Vec<baml_compiler2_hir::body::BodyOwnerId<'_>> {
    let functions = item_data::file_functions(db, file)
        .iter()
        .copied()
        .map(Into::into);
    let lets = item_data::file_lets(db, file)
        .iter()
        .copied()
        .map(Into::into);
    functions.chain(lets).collect()
}

/// Canonical per-body type references for a function (rust-analyzer's
/// bodies-own-their-type-refs shape): every type expression written inside
/// the body, lowered once into a span-free store. Salsa-tracked over PPIR's
/// canonical body, so downstream type queries depend on structure only.
#[salsa::tracked]
pub fn function_body_type_refs<'db>(
    db: &'db dyn Db,
    function: baml_compiler2_hir::loc::FunctionLoc<'db>,
) -> Arc<baml_compiler2_hir::body_type_refs::BodyTypeRefs> {
    let body = function_body(db, function);
    let refs = match body.as_ref() {
        baml_compiler2_hir::body::FunctionBody::Expr(expr_body) => {
            baml_compiler2_hir::body_type_refs::collect_body_type_refs(expr_body).0
        }
        _ => baml_compiler2_hir::body_type_refs::BodyTypeRefs::default(),
    };
    Arc::new(refs)
}

/// The span map for a body's collected type references (the `.1` the
/// tracked ref query drops; recomputed on demand - the check layer's
/// annotation-diagnostic anchors resolve through it).
pub fn body_type_ref_spans(
    db: &dyn Db,
    owner: baml_compiler2_hir::body::BodyOwnerId<'_>,
) -> Option<baml_compiler2_hir::body_type_refs::BodyTypeRefSourceMap> {
    use baml_compiler2_hir::body::{BodyOwnerId, FunctionBody, LetBody};
    match owner {
        BodyOwnerId::Function(function) => match function_body(db, function).as_ref() {
            FunctionBody::Expr(expr_body) => {
                Some(baml_compiler2_hir::body_type_refs::collect_body_type_refs(expr_body).1)
            }
            _ => None,
        },
        BodyOwnerId::Let(let_binding) => {
            match baml_compiler2_hir::body::let_body(db, let_binding).as_ref() {
                LetBody::Expr(expr_body) => {
                    Some(baml_compiler2_hir::body_type_refs::collect_body_type_refs(expr_body).1)
                }
                LetBody::Missing => None,
            }
        }
        BodyOwnerId::ParameterDefaults(function) => Some(
            baml_compiler2_hir::body_type_refs::collect_body_type_refs(
                &function_parameter_defaults(db, function).defaults.exprs,
            )
            .1,
        ),
    }
}

/// Per-body type references for a top-level let's initializer.
#[salsa::tracked]
pub fn let_body_type_refs<'db>(
    db: &'db dyn Db,
    let_binding: baml_compiler2_hir::loc::LetLoc<'db>,
) -> Arc<baml_compiler2_hir::body_type_refs::BodyTypeRefs> {
    let body = baml_compiler2_hir::body::let_body(db, let_binding);
    let refs = match body.as_ref() {
        baml_compiler2_hir::body::LetBody::Expr(expr_body) => {
            baml_compiler2_hir::body_type_refs::collect_body_type_refs(expr_body).0
        }
        baml_compiler2_hir::body::LetBody::Missing => {
            baml_compiler2_hir::body_type_refs::BodyTypeRefs::default()
        }
    };
    Arc::new(refs)
}

/// Per-body type references for any body owner.
pub fn body_type_refs<'db>(
    db: &'db dyn Db,
    owner: baml_compiler2_hir::body::BodyOwnerId<'db>,
) -> Arc<baml_compiler2_hir::body_type_refs::BodyTypeRefs> {
    use baml_compiler2_hir::body::BodyOwnerId;
    match owner {
        BodyOwnerId::Function(function) => function_body_type_refs(db, function),
        BodyOwnerId::Let(let_binding) => let_body_type_refs(db, let_binding),
        BodyOwnerId::ParameterDefaults(function) => parameter_defaults_type_refs(db, function),
    }
}

/// Per-body type references for a function's parameter-default arena.
#[salsa::tracked]
pub fn parameter_defaults_type_refs<'db>(
    db: &'db dyn Db,
    function: baml_compiler2_hir::loc::FunctionLoc<'db>,
) -> Arc<baml_compiler2_hir::body_type_refs::BodyTypeRefs> {
    let defaults = function_parameter_defaults(db, function);
    Arc::new(baml_compiler2_hir::body_type_refs::collect_body_type_refs(&defaults.defaults.exprs).0)
}

/// Canonical function signature — uses PPIR's item tree.
pub fn function_signature<'db>(
    db: &'db dyn Db,
    function: baml_compiler2_hir::loc::FunctionLoc<'db>,
) -> Arc<baml_compiler2_hir::signature::FunctionSignature> {
    let file = function.file(db);
    let item_tree = file_item_tree(db, file);
    let func_data = &item_tree[function.id(db)];

    let params: Vec<_> = func_data
        .params
        .iter()
        .map(|p| {
            let type_expr = p
                .type_expr
                .clone()
                .unwrap_or(ast::TypeExprKind::Unknown { attrs: vec![] }.at(TextRange::default()));
            baml_compiler2_hir::signature::SignatureParam {
                name: p.name.clone(),
                ty: type_expr,
                has_default: p.default.is_some(),
            }
        })
        .collect();

    let return_type = func_data.return_type.clone();

    Arc::new(baml_compiler2_hir::signature::FunctionSignature {
        name: func_data.name.clone(),
        params,
        return_type,
        throws: func_data.throws.clone(),
    })
}

/// Canonical elaborated callable signature — uses PPIR's item tree.
pub fn elaborated_function_signature<'db>(
    db: &'db dyn Db,
    function: baml_compiler2_hir::loc::FunctionLoc<'db>,
) -> Arc<baml_compiler2_hir::signature::ElaboratedFunctionSignature> {
    let file = function.file(db);
    let item_tree = file_item_tree(db, file);
    let func_data = &item_tree[function.id(db)];

    let params: Vec<_> = func_data
        .params
        .iter()
        .map(|p| {
            let type_expr = p
                .type_expr
                .clone()
                .unwrap_or(ast::TypeExprKind::Unknown { attrs: vec![] }.at(TextRange::default()));
            baml_compiler2_hir::signature::SignatureParam {
                name: p.name.clone(),
                ty: type_expr,
                has_default: p.default.is_some(),
            }
        })
        .collect();

    let return_type = func_data.return_type.clone();
    let throws = func_data.throws.clone();
    let reserved_effect_param_names: Vec<Name> = item_tree
        .enclosing_type_generic_params(function.id(db))
        .iter()
        .map(|param| param.name.clone())
        .collect();

    Arc::new(
        baml_compiler2_hir::signature::elaborate_function_signature_parts(
            func_data.name.clone(),
            func_data
                .generic_params
                .iter()
                .map(|param| param.name.clone())
                .collect(),
            &reserved_effect_param_names,
            params,
            return_type,
            throws,
        ),
    )
}

/// Canonical function parameter defaults — uses PPIR's item tree.
pub fn function_parameter_defaults<'db>(
    db: &'db dyn Db,
    function: baml_compiler2_hir::loc::FunctionLoc<'db>,
) -> Arc<baml_compiler2_hir::signature::FunctionParameterDefaults> {
    let file = function.file(db);
    let item_tree = file_item_tree(db, file);
    let func_data = &item_tree[function.id(db)];

    Arc::new(baml_compiler2_hir::signature::FunctionParameterDefaults {
        params: func_data
            .params
            .iter()
            .map(|param| param.default.clone())
            .collect(),
        defaults: func_data.defaults.clone(),
    })
}

/// Canonical function signature source map — uses PPIR's item tree.
pub fn function_signature_source_map<'db>(
    db: &'db dyn Db,
    function: baml_compiler2_hir::loc::FunctionLoc<'db>,
) -> baml_compiler2_hir::signature::SignatureSourceMap {
    let file = function.file(db);
    let item_tree = file_item_tree(db, file);
    let func_data = &item_tree[function.id(db)];

    baml_compiler2_hir::signature::SignatureSourceMap {
        param_spans: func_data.params.iter().map(|p| p.span).collect(),
        param_type_spans: func_data
            .params
            .iter()
            .map(|p| p.type_expr.as_ref().map(|te| te.span))
            .collect(),
        return_type_span: func_data.return_type.as_ref().map(|te| te.span),
        throws_type_span: func_data.throws.as_ref().map(|te| te.span),
    }
}

/// Canonical elaborated callable signature source map — spans are unchanged by
/// bounded signature elaboration, so this mirrors `function_signature_source_map`.
pub fn elaborated_function_signature_source_map<'db>(
    db: &'db dyn Db,
    function: baml_compiler2_hir::loc::FunctionLoc<'db>,
) -> baml_compiler2_hir::signature::SignatureSourceMap {
    function_signature_source_map(db, function)
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

    // Collect matching files from the package's own roots (`package_files`),
    // then sort alphabetically by path — so edits to another package's file
    // set never invalidate this namespace.
    let package_id = PackageId::new(db, package);
    let mut matching_files: Vec<SourceFile> =
        baml_compiler2_hir::package::package_files(db, package_id)
            .iter()
            .copied()
            .filter(|file| {
                let pkg_info = baml_compiler2_hir::file_package::file_package(db, *file);
                pkg_info.namespace_path == *ns_path
            })
            .collect();
    matching_files.sort_by_key(|a| a.path(db));

    // Uses PPIR's file_symbol_contributions (canonical, includes *$stream types).
    let mut type_defs: IndexMap<Name, Vec<Contribution<'db>>> = IndexMap::new();
    let mut value_defs: IndexMap<Name, Vec<Contribution<'db>>> = IndexMap::new();

    for file in &matching_files {
        let contributions = file_symbol_contributions(db, *file);
        for (name, contrib) in &contributions.types {
            type_defs.entry(name.clone()).or_default().push(*contrib);
        }
        for (name, contrib) in &contributions.values {
            value_defs.entry(name.clone()).or_default().push(*contrib);
        }
    }

    let mut types: IndexMap<Name, Definition<'db>> = IndexMap::new();
    let mut values: IndexMap<Name, Definition<'db>> = IndexMap::new();
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

    // Consumers observe the insertion order of `namespaces`, so namespace
    // discovery must not inherit `HashSet`'s per-process randomized order.
    // Discovery reads only the package's own roots ([`package_files`]), so
    // edits to another root's file set never invalidate this fold.
    let mut ns_paths: IndexSet<Vec<Name>> = IndexSet::new();
    for file in baml_compiler2_hir::package::package_files(db, package_id) {
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, *file);
        debug_assert_eq!(pkg_info.package, *package_name);
        ns_paths.insert(pkg_info.namespace_path.clone());
    }
    let mut namespaces: IndexMap<Vec<Name>, NamespaceItems<'db>> = IndexMap::new();
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

    PackageItems {
        package: package_name,
        namespaces,
        extra,
    }
}
