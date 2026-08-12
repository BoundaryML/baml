use baml_compiler2_hir::{
    loc::{ClassLoc, FunctionLoc, ImplLoc},
    type_ref::{TypeRefBuilder, TypeRefId, TypeRefSourceMap, TypeRefStore},
};
use text_size::TextRange;

use crate::item_data::common::{
    AssociatedTypeBindingData, AssociatedTypeBindingSourceMap, GenericParamData,
    InterfaceFieldLinkData, InterfaceFieldLinkSourceMap, lower_generic_params,
};

/// What an `implements` block applies to.
///
/// Unifying the owner with the for-target makes "in-body with an explicit
/// for-target" and "out-of-body without one" both unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum ImplSubjectData<'db> {
    /// `implements I { … }` in a class body. The for-type is the class itself.
    /// `out_of_body` records the syntactic origin for diagnostics only — it must
    /// NOT influence resolution, dispatch, or coherence.
    InClass {
        class: ClassLoc<'db>,
        out_of_body: bool,
    },
    /// `implement<…> I for <for_target> { … }` — an explicit for-type plus the
    /// block's own generic parameters.
    Free {
        for_target: TypeRefId,
        generics: Vec<GenericParamData>,
    },
}

/// Span-free semantic data for one `implements` block (either kind).
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct ImplBlockData<'db> {
    pub subject: ImplSubjectData<'db>,
    /// Every type reference in this block's header — the for-target, its generic
    /// bounds, the interface target, and any associated-type bindings.
    pub type_refs: TypeRefStore,
    pub interface_target: TypeRefId,
    pub field_links: Vec<InterfaceFieldLinkData>,
    pub associated_type_bindings: Vec<AssociatedTypeBindingData>,
    pub methods: Vec<FunctionLoc<'db>>,
    /// Leading `///` docstring — populated for free `implements … for …`
    /// blocks; in-body blocks carry none today.
    pub docstring: Option<String>,
}

/// Spans for an `ImplBlock`, parallel to [`ImplBlockData`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplBlockSourceMap {
    /// Full source span of the block.
    pub span: TextRange,
    /// Spans for every node in [`ImplBlockData::type_refs`].
    pub type_refs: TypeRefSourceMap,
    /// Parallel to [`ImplBlockData::field_links`].
    pub field_links: Vec<InterfaceFieldLinkSourceMap>,
    /// Parallel to [`ImplBlockData::associated_type_bindings`].
    pub associated_type_bindings: Vec<AssociatedTypeBindingSourceMap>,
}

/// Semantic data for one `implements` block. Span-free — see the module docs.
#[salsa::tracked(returns(ref))]
pub fn impl_block_data<'db>(db: &'db dyn crate::Db, block: ImplLoc<'db>) -> ImplBlockData<'db> {
    lower(db, block).0
}

/// Spans for one `implements` block. Kept separate from [`impl_block_data`] so
/// that a whitespace-only edit invalidates this but not the semantic data.
#[salsa::tracked(returns(ref))]
pub fn impl_block_source_map<'db>(
    db: &'db dyn crate::Db,
    block: ImplLoc<'db>,
) -> ImplBlockSourceMap {
    lower(db, block).1
}

/// Type refs are allocated in a fixed order (for-target and its bounds, then the
/// interface target, then associated-type bindings) so that ids are a pure
/// function of the block's shape.
fn lower<'db>(
    db: &'db dyn crate::Db,
    block: ImplLoc<'db>,
) -> (ImplBlockData<'db>, ImplBlockSourceMap) {
    use baml_compiler2_hir::item_tree::ImplSubject;

    let file = block.file(db);
    let item_tree = crate::file_item_tree(db, file);
    let data = &item_tree.impls[&block.id(db)];

    let mut type_refs = TypeRefBuilder::new();

    let subject = match &data.subject {
        ImplSubject::InClass { class, out_of_body } => ImplSubjectData::InClass {
            class: ClassLoc::new(db, file, *class),
            out_of_body: *out_of_body,
        },
        ImplSubject::Free {
            for_target,
            generics,
        } => ImplSubjectData::Free {
            for_target: type_refs.lower(for_target),
            generics: lower_generic_params(generics, &mut type_refs),
        },
    };

    let interface_target = type_refs.lower(&data.interface_target);

    let associated_type_bindings = data
        .associated_type_bindings
        .iter()
        .map(|binding| AssociatedTypeBindingData {
            name: binding.name.clone(),
            type_ref: binding.type_expr.as_ref().map(|te| type_refs.lower(te)),
        })
        .collect();

    let (store, spans) = type_refs.finish();

    (
        ImplBlockData {
            subject,
            type_refs: store,
            interface_target,
            field_links: data
                .field_links
                .iter()
                .map(|link| InterfaceFieldLinkData {
                    interface_field: link.interface_field.clone(),
                    class_field: link.class_field.clone(),
                })
                .collect(),
            associated_type_bindings,
            methods: data
                .methods
                .iter()
                .map(|method| FunctionLoc::new(db, file, *method))
                .collect(),
            docstring: data.docstring.clone(),
        },
        ImplBlockSourceMap {
            span: data.span,
            type_refs: spans,
            field_links: data
                .field_links
                .iter()
                .map(|link| InterfaceFieldLinkSourceMap {
                    span: link.span,
                    interface_field_span: link.interface_field_span,
                    class_field_span: link.class_field_span,
                })
                .collect(),
            associated_type_bindings: data
                .associated_type_bindings
                .iter()
                .map(|binding| AssociatedTypeBindingSourceMap {
                    span: binding.span,
                    name_span: binding.name_span,
                })
                .collect(),
        },
    )
}
