use baml_base::Name;
use text_size::TextRange;

use crate::{
    item_data::common::{FieldData, GenericParamData, lower_generic_params},
    item_tree::ClassAttrs,
    loc::{ClassLoc, FunctionLoc},
    type_ref::{TypeRefBuilder, TypeRefSourceMap, TypeRefStore},
};

/// Span-free semantic data for a `class` declaration.
///
/// `methods` holds `FunctionLoc`s, not `LocalItemId`s: a `Loc` already carries
/// its file, so a consumer can go straight to `function_data` without knowing
/// which file the class came from.
#[derive(Debug, Clone, PartialEq, Eq, salsa::SalsaValue)]
pub struct ClassData<'db> {
    pub name: Name,
    /// Generic type parameters, each with its conjunction of bounds.
    pub generic_params: Vec<GenericParamData>,
    /// Every type reference in this class's *signature* — bounds and field
    /// types. Scoped to the item, so edits to sibling items cannot renumber
    /// these ids.
    pub type_refs: TypeRefStore,
    pub fields: Vec<FieldData>,
    pub methods: Vec<FunctionLoc<'db>>,
    /// The class's lowered `@@` attributes.
    pub attrs: ClassAttrs,
    pub docstring: Option<String>,
}

/// Spans for a `Class`, parallel to [`ClassData`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassSourceMap {
    /// Full source span of the declaration.
    pub span: TextRange,
    /// Span of the class's name token.
    pub name_span: TextRange,
    /// Spans for every node in [`ClassData::type_refs`].
    pub type_refs: TypeRefSourceMap,
    /// Name span per field, parallel to [`ClassData::fields`].
    pub field_name_spans: Vec<TextRange>,
}

/// Semantic data for one class. Span-free — see the module docs.
#[salsa::tracked(returns(ref))]
pub fn class_data<'db>(db: &'db dyn crate::Db, class: ClassLoc<'db>) -> ClassData<'db> {
    lower(db, class).0
}

/// Spans for one class. Kept separate from [`class_data`] so that a
/// whitespace-only edit invalidates this but not the semantic data.
#[salsa::tracked(returns(ref))]
pub fn class_source_map<'db>(db: &'db dyn crate::Db, class: ClassLoc<'db>) -> ClassSourceMap {
    lower(db, class).1
}

/// The `*_data` and `*_source_map` queries each call this and keep one half, so
/// `lower` runs once per query — not a single shared pass. It is deterministic,
/// so the `TypeRefId`s the data half hands out validly index the source map's
/// arena. The split into two queries is purely about what each one lets
/// downstream depend on.
///
/// Type refs are allocated in a fixed order (bounds, then fields) so that ids
/// are a pure function of the class's shape.
fn lower<'db>(db: &'db dyn crate::Db, class: ClassLoc<'db>) -> (ClassData<'db>, ClassSourceMap) {
    let file = class.file(db);
    let item_tree = crate::file_item_tree(db, file);
    let item_source_map = crate::file_item_tree_source_map(db, file);
    let data = &item_tree[class.id(db)];

    let mut type_refs = TypeRefBuilder::new();

    let generic_params = lower_generic_params(&data.generic_params, &mut type_refs);

    let fields = data
        .fields
        .iter()
        .map(|field| FieldData {
            name: field.name.clone(),
            type_ref: type_refs.lower(&field.type_expr),
            attrs: field.attrs.clone(),
            docstring: field.docstring.clone(),
        })
        .collect();

    let (store, spans) = type_refs.finish();

    (
        ClassData {
            name: data.name.clone(),
            generic_params,
            type_refs: store,
            fields,
            methods: data
                .methods
                .iter()
                .map(|method| FunctionLoc::new(db, file, *method))
                .collect(),
            attrs: data.attrs.clone(),
            docstring: data.docstring.clone(),
        },
        ClassSourceMap {
            span: data.span,
            name_span: item_source_map
                .class_name_spans
                .get(&class.id(db))
                .copied()
                .unwrap_or_else(|| unreachable!("name span recorded at allocation")),
            type_refs: spans,
            field_name_spans: item_source_map
                .class_field_spans
                .get(&class.id(db))
                .cloned()
                .unwrap_or_default(),
        },
    )
}
