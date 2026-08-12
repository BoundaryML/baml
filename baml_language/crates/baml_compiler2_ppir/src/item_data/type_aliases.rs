use baml_base::Name;
use baml_compiler2_hir::{
    loc::TypeAliasLoc,
    type_ref::{TypeRefBuilder, TypeRefId, TypeRefSourceMap, TypeRefStore},
};
use text_size::TextRange;

/// Span-free semantic data for a `type X = ...` declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAliasData {
    pub name: Name,
    /// The type-reference arena owned by this alias. Scoped to the item, so
    /// edits to sibling items cannot renumber these ids.
    pub type_refs: TypeRefStore,
    /// Root of the aliased type. `None` when the RHS was omitted or unparseable.
    pub value: Option<TypeRefId>,
    pub docstring: Option<String>,
}

/// Spans for a `TypeAlias`, parallel to [`TypeAliasData`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAliasSourceMap {
    /// Full source span of the declaration.
    pub span: TextRange,
    /// Span of the alias's name token.
    pub name_span: TextRange,
    /// Spans for every node in [`TypeAliasData::type_refs`].
    pub type_refs: TypeRefSourceMap,
}

/// Semantic data for one type alias. Span-free — see the module docs.
#[salsa::tracked(returns(ref))]
pub fn type_alias_data<'db>(db: &'db dyn crate::Db, alias: TypeAliasLoc<'db>) -> TypeAliasData {
    lower(db, alias).0
}

/// Spans for one type alias. Kept separate from [`type_alias_data`] so that a
/// whitespace-only edit invalidates this but not the semantic data.
#[salsa::tracked(returns(ref))]
pub fn type_alias_source_map<'db>(
    db: &'db dyn crate::Db,
    alias: TypeAliasLoc<'db>,
) -> TypeAliasSourceMap {
    lower(db, alias).1
}

/// The `*_data` and `*_source_map` queries each call this and keep one half, so
/// `lower` runs once per query — not a single shared pass. It is deterministic,
/// so the `TypeRefId`s the data half hands out validly index the source map's
/// arena. The split into two queries is purely about what each one lets
/// downstream depend on.
fn lower<'db>(
    db: &'db dyn crate::Db,
    alias: TypeAliasLoc<'db>,
) -> (TypeAliasData, TypeAliasSourceMap) {
    let file = alias.file(db);
    let item_tree = crate::file_item_tree(db, file);
    let item_source_map = crate::file_item_tree_source_map(db, file);
    let data = &item_tree[alias.id(db)];

    let mut type_refs = TypeRefBuilder::new();
    let value = data.type_expr.as_ref().map(|te| type_refs.lower(te));
    let (store, spans) = type_refs.finish();

    (
        TypeAliasData {
            name: data.name.clone(),
            type_refs: store,
            value,
            docstring: data.docstring.clone(),
        },
        TypeAliasSourceMap {
            span: data.span,
            name_span: item_source_map
                .type_alias_name_spans
                .get(&alias.id(db))
                .copied()
                .unwrap_or_else(|| unreachable!("name span recorded at allocation")),
            type_refs: spans,
        },
    )
}
