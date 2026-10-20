use baml_base::Name;
use baml_compiler2_ast as ast;
use text_size::TextRange;

use crate::{
    loc::LetLoc,
    type_ref::{TypeRefBuilder, TypeRefId, TypeRefSourceMap, TypeRefStore},
};

/// Span-free semantic data for a top-level `let` binding.
///
/// The initializer is deliberately absent: it has its own query
/// (`body::let_body`), so editing an initializer cannot invalidate anything that
/// only depends on the binding itself — in particular, an annotated global's
/// type, which every reader takes from here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LetData {
    pub name: Name,
    pub origin: ast::LetOrigin,
    /// The type-reference arena owned by this binding. Scoped to the item, so
    /// edits to sibling items cannot renumber these ids.
    pub type_refs: TypeRefStore,
    /// Root of the written `: T`. `None` when the binding is unannotated, in
    /// which case its type is its initializer's.
    pub annotation: Option<TypeRefId>,
    pub docstring: Option<String>,
}

/// Spans for a `Let`, parallel to [`LetData`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LetSourceMap {
    /// Full source span of the declaration.
    pub span: TextRange,
    /// Span of just the binding's name.
    pub name_span: TextRange,
    /// Spans for every node in [`LetData::type_refs`].
    pub type_refs: TypeRefSourceMap,
}

/// Semantic data for one `let` binding. Span-free — see the module docs.
#[salsa::tracked(returns(ref))]
pub fn let_data<'db>(db: &'db dyn crate::Db, binding: LetLoc<'db>) -> LetData {
    lower(db, binding).0
}

/// Spans for one `let` binding. Kept separate from [`let_data`] so that a
/// whitespace-only edit invalidates this but not the semantic data.
#[salsa::tracked(returns(ref))]
pub fn let_source_map<'db>(db: &'db dyn crate::Db, binding: LetLoc<'db>) -> LetSourceMap {
    lower(db, binding).1
}

/// The `*_data` and `*_source_map` queries each call this and keep one half;
/// it is deterministic, so the `TypeRefId`s the data half hands out validly
/// index the source map's arena.
fn lower<'db>(db: &'db dyn crate::Db, binding: LetLoc<'db>) -> (LetData, LetSourceMap) {
    let item_tree = crate::file_item_tree(db, binding.file(db));
    let data = &item_tree[binding.id(db)];

    let mut type_refs = TypeRefBuilder::new();
    let annotation = data
        .type_annotation
        .as_ref()
        .map(|annotation| type_refs.lower(annotation));
    let (store, spans) = type_refs.finish();

    (
        LetData {
            name: data.name.clone(),
            origin: data.origin,
            type_refs: store,
            annotation,
            docstring: data.docstring.clone(),
        },
        LetSourceMap {
            span: data.span,
            name_span: data.name_span,
            type_refs: spans,
        },
    )
}
