use text_size::TextRange;

use crate::{
    item_data::common::{
        AssociatedTypeBindingData, AssociatedTypeBindingSourceMap, GenericParamData,
        InterfaceFieldLinkData, InterfaceFieldLinkSourceMap, lower_generic_params,
    },
    loc::{FunctionLoc, ImplLoc},
    type_ref::{TypeRefBuilder, TypeRefId, TypeRefSourceMap, TypeRefStore},
};

/// Span-free semantic data for one `implements` block, in either spelling:
/// an in-body block arrives with the class's generics and the class as its
/// for-target, so there is one shape. `origin` records how it was written,
/// for the rules that read provenance — an interface with fields is
/// implemented in-body — and must NOT influence resolution, dispatch, or
/// coherence.
#[derive(Debug, Clone, PartialEq, Eq, salsa::SalsaValue)]
pub struct ImplBlockData<'db> {
    pub for_target: TypeRefId,
    pub generics: Vec<GenericParamData>,
    pub origin: baml_compiler2_ast::ImplOrigin,
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

/// The class an `implements` block is written for, when it is for a class of
/// its own file: the class whose declared fields satisfy the interface's
/// field obligations (E0126 keeps out-of-body impls field-less) and whose
/// generics form the block's frame. `None` for a free block.
pub fn impl_enclosing_class<'db>(
    db: &'db dyn crate::Db,
    block: ImplLoc<'db>,
) -> Option<crate::loc::ClassLoc<'db>> {
    super::enumeration::impl_class(db, block)
}

/// The declared generic parameters scoping an impl block's methods, with the
/// `TypeRefStore` their bound refs live in.
pub fn impl_declared_generics<'db>(
    db: &'db dyn crate::Db,
    block: ImplLoc<'db>,
) -> (&'db [GenericParamData], &'db crate::type_ref::TypeRefStore) {
    let data = impl_block_data(db, block);
    (&data.generics, &data.type_refs)
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
    let file = block.file(db);
    let item_tree = crate::file_item_tree(db, file);
    let data = &item_tree.impls[&block.id(db)];
    let mut type_refs = TypeRefBuilder::new();
    let for_target = type_refs.lower(&data.for_target);
    let generics = lower_generic_params(&data.generics, &mut type_refs);
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
            for_target,
            generics,
            origin: data.origin,
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
