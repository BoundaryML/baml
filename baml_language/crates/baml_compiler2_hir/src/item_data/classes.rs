use baml_base::Name;
use text_size::TextRange;

use crate::{
    item_data::common::{
        AssociatedTypeBindingData, AssociatedTypeBindingSourceMap, FieldData,
        InterfaceFieldLinkData, InterfaceFieldLinkSourceMap,
    },
    item_tree::Attribute,
    loc::{ClassLoc, FunctionLoc},
    type_ref::{TypeRefBuilder, TypeRefId, TypeRefSourceMap, TypeRefStore},
};

/// Span-free semantic data for a `class` declaration.
///
/// `methods` holds `FunctionLoc`s, not `LocalItemId`s: a `Loc` already carries
/// its file, so a consumer can go straight to `function_data` without knowing
/// which file the class came from.
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct ClassData<'db> {
    pub name: Name,
    pub generic_params: Vec<Name>,
    /// Every type reference in this class's *signature* — bounds, field types,
    /// and `implements` targets. Scoped to the item, so edits to sibling items
    /// cannot renumber these ids.
    pub type_refs: TypeRefStore,
    /// Parallel to `generic_params`. `Some` means `T extends <bound>`.
    pub generic_param_bounds: Vec<Option<TypeRefId>>,
    pub fields: Vec<FieldData>,
    pub methods: Vec<FunctionLoc<'db>>,
    pub implements: Vec<ImplementsData>,
    pub attributes: Vec<Attribute>,
    pub docstring: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplementsData {
    pub target: TypeRefId,
    pub field_links: Vec<InterfaceFieldLinkData>,
    pub associated_type_bindings: Vec<AssociatedTypeBindingData>,
    /// Syntactic origin, for diagnostics only — it must NOT influence
    /// resolution, dispatch, or coherence.
    pub is_out_of_body: bool,
}

/// Spans for a `Class`, parallel to [`ClassData`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassSourceMap {
    /// Full source span of the declaration.
    pub span: TextRange,
    /// Spans for every node in [`ClassData::type_refs`].
    pub type_refs: TypeRefSourceMap,
    /// Name span per field, parallel to [`ClassData::fields`].
    pub field_name_spans: Vec<TextRange>,
    /// Parallel to [`ClassData::implements`].
    pub implements: Vec<ImplementsSourceMap>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplementsSourceMap {
    pub span: TextRange,
    pub field_links: Vec<InterfaceFieldLinkSourceMap>,
    pub associated_type_bindings: Vec<AssociatedTypeBindingSourceMap>,
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

/// Both halves share one lowering pass — the split into two queries is purely
/// about what each one lets downstream depend on.
///
/// Type refs are allocated in a fixed order (bounds, then fields, then
/// `implements` targets) so that ids are a pure function of the class's shape.
fn lower<'db>(db: &'db dyn crate::Db, class: ClassLoc<'db>) -> (ClassData<'db>, ClassSourceMap) {
    let file = class.file(db);
    let item_tree = crate::file_item_tree(db, file);
    let item_source_map = crate::file_item_tree_source_map(db, file);
    let data = &item_tree[class.id(db)];

    let mut type_refs = TypeRefBuilder::new();

    let generic_param_bounds = data
        .generic_param_bounds
        .iter()
        .map(|bound| bound.as_ref().map(|te| type_refs.lower(te)))
        .collect();

    let fields = data
        .fields
        .iter()
        .map(|field| FieldData {
            name: field.name.clone(),
            type_ref: field.type_expr.as_ref().map(|te| type_refs.lower(te)),
            attributes: field.attributes.clone(),
            docstring: field.docstring.clone(),
        })
        .collect();

    let implements: Vec<ImplementsData> = data
        .implements
        .iter()
        .map(|block| ImplementsData {
            target: type_refs.lower(&block.target),
            field_links: block
                .field_links
                .iter()
                .map(|link| InterfaceFieldLinkData {
                    interface_field: link.interface_field.clone(),
                    class_field: link.class_field.clone(),
                })
                .collect(),
            associated_type_bindings: block
                .associated_type_bindings
                .iter()
                .map(|binding| AssociatedTypeBindingData {
                    name: binding.name.clone(),
                    type_ref: binding.type_expr.as_ref().map(|te| type_refs.lower(te)),
                })
                .collect(),
            is_out_of_body: block.is_out_of_body,
        })
        .collect();

    let (store, spans) = type_refs.finish();

    let implements_spans = data
        .implements
        .iter()
        .map(|block| ImplementsSourceMap {
            span: block.span,
            field_links: block
                .field_links
                .iter()
                .map(|link| InterfaceFieldLinkSourceMap {
                    span: link.span,
                    interface_field_span: link.interface_field_span,
                    class_field_span: link.class_field_span,
                })
                .collect(),
            associated_type_bindings: block
                .associated_type_bindings
                .iter()
                .map(|binding| AssociatedTypeBindingSourceMap {
                    span: binding.span,
                    name_span: binding.name_span,
                })
                .collect(),
        })
        .collect();

    (
        ClassData {
            name: data.name.clone(),
            generic_params: data.generic_params.clone(),
            type_refs: store,
            generic_param_bounds,
            fields,
            methods: data
                .methods
                .iter()
                .map(|method| FunctionLoc::new(db, file, *method))
                .collect(),
            implements,
            attributes: data.attributes.clone(),
            docstring: data.docstring.clone(),
        },
        ClassSourceMap {
            span: data.span,
            type_refs: spans,
            field_name_spans: item_source_map
                .class_field_spans
                .get(&class.id(db))
                .cloned()
                .unwrap_or_default(),
            implements: implements_spans,
        },
    )
}
