//! The item layer compiler2 reads: per-item firewall queries over HIR's item
//! tree, name resolution, and the body/signature queries TIR and MIR consume.
//!
//! Pipeline: CST -> AST -> HIR -> PPIR -> TIR.

pub mod item_data;
pub mod resolve;

use std::sync::Arc;

use baml_base::{Name, SourceFile};
use baml_compiler2_ast as ast;
use baml_compiler2_hir::{
    contributions::FileSymbolContributions,
    item_tree::{ItemTree, ItemTreeSourceMap},
    namespace::{NameConflict, NamespaceId, NamespaceItems},
    package::{PackageItems, PackageItemsExtra},
    semantic_index::FileSemanticIndex,
};
use indexmap::{IndexMap, IndexSet};
use text_size::TextRange;

// -- Db trait -----------------------------------------------------------------

#[salsa::db]
pub trait Db: baml_compiler2_hir::Db {}

// -- Item queries -------------------------------------------------------------

pub fn file_semantic_index(db: &dyn Db, file: SourceFile) -> &FileSemanticIndex<'_> {
    baml_compiler2_hir::file_semantic_index(db, file)
}

/// Symbol contributions for a file.
pub fn file_symbol_contributions(
    db: &dyn Db,
    file: SourceFile,
) -> Arc<FileSymbolContributions<'_>> {
    let index = file_semantic_index(db, file);
    Arc::clone(&index.symbol_contributions)
}

/// The file's item tree.
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

/// The file's item-tree source map.
///
/// `pub(crate)`: spans are served by the per-item `*_source_map` firewall
/// queries in `item_data`.
pub(crate) fn file_item_tree_source_map(db: &dyn Db, file: SourceFile) -> Arc<ItemTreeSourceMap> {
    let index = file_semantic_index(db, file);
    Arc::clone(&index.item_tree_source_map)
}

/// A function's body.
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

/// A function's body source map.
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

/// The body of any body owner (rust-analyzer's `DefWithBodyId` pattern).
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

/// The body source map of any body owner (spans only).
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

/// Every body owner in `file`: functions (methods included), then top-level
/// lets, each group in source order.
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

/// Per-body type references for a function (rust-analyzer's
/// bodies-own-their-type-refs shape): every type expression written inside
/// the body, lowered once into a span-free store. Salsa-tracked, so
/// downstream type queries depend on structure only.
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

/// A function's signature.
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
                .unwrap_or(ast::TypeExprKind::Missing { attrs: vec![] }.at(TextRange::default()));
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

/// A function's elaborated callable signature.
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
                .unwrap_or(ast::TypeExprKind::Missing { attrs: vec![] }.at(TextRange::default()));
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

/// A function's parameter defaults.
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

/// A function's signature source map.
pub fn function_signature_source_map<'db>(
    db: &'db dyn Db,
    function: baml_compiler2_hir::loc::FunctionLoc<'db>,
) -> baml_compiler2_hir::signature::SignatureSourceMap {
    let file = function.file(db);
    let item_tree = file_item_tree(db, file);
    let func_data = &item_tree[function.id(db)];

    baml_compiler2_hir::signature::SignatureSourceMap {
        param_spans: func_data.params.iter().map(|p| p.span).collect(),
        param_name_spans: func_data.params.iter().map(|p| p.name_span).collect(),
        param_type_spans: func_data
            .params
            .iter()
            .map(|p| p.type_expr.as_ref().map(|te| te.span))
            .collect(),
        return_type_span: func_data.return_type.as_ref().map(|te| te.span),
        throws_type_span: func_data.throws.as_ref().map(|te| te.span),
    }
}

/// Elaborated callable signature source map — spans are unchanged by
/// bounded signature elaboration, so this mirrors `function_signature_source_map`.
pub fn elaborated_function_signature_source_map<'db>(
    db: &'db dyn Db,
    function: baml_compiler2_hir::loc::FunctionLoc<'db>,
) -> baml_compiler2_hir::signature::SignatureSourceMap {
    function_signature_source_map(db, function)
}

/// The items of one namespace.
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

    // Collect matching files from the package's own root, then sort
    // alphabetically by path — so edits to another package's file set never
    // invalidate this namespace.
    let mut matching_files: Vec<SourceFile> = package
        .files(db)
        .iter()
        .copied()
        .filter(|file| {
            let pkg_info = baml_compiler2_hir::file_package::file_package(db, *file);
            pkg_info.namespace_path == *ns_path
        })
        .collect();
    matching_files.sort_by_key(|a| a.path(db));

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

/// The items of one package.
#[salsa::tracked(returns(ref))]
pub fn package_items<'db>(db: &'db dyn Db, root: baml_base::SourceRoot) -> PackageItems<'db> {
    // Consumers observe the insertion order of `namespaces`, so namespace
    // discovery must not inherit `HashSet`'s per-process randomized order.
    // Discovery reads only the package's own root, so edits to another
    // root's file set never invalidate this fold.
    let mut ns_paths: IndexSet<Vec<Name>> = IndexSet::new();
    for file in root.files(db) {
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, *file);
        debug_assert_eq!(pkg_info.root, root);
        ns_paths.insert(pkg_info.namespace_path.clone());
    }
    let mut namespaces: IndexMap<Vec<Name>, NamespaceItems<'db>> = IndexMap::new();
    let mut all_conflicts: Vec<NameConflict<'db>> = Vec::new();
    for ns_path in ns_paths {
        let ns_id = NamespaceId::new(db, root, ns_path.clone());
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
        root,
        namespaces,
        extra,
    }
}
