use baml_base::Name;
use baml_compiler2_hir::{
    item_tree::Attribute,
    loc::{FunctionLoc, InterfaceLoc},
    type_ref::{TypeRefBuilder, TypeRefId, TypeRefSourceMap, TypeRefStore},
};
use text_size::TextRange;

use crate::item_data::common::{FieldData, FunctionParamData};

/// Span-free semantic data for an `interface` declaration.
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct InterfaceData<'db> {
    pub name: Name,
    pub generic_params: Vec<Name>,
    /// Every type reference in this interface's signature — bounds, `requires`
    /// targets, field types, associated-type bounds and defaults, and required
    /// method signatures. Scoped to the item.
    pub type_refs: TypeRefStore,
    /// Parallel to `generic_params`. `Some` means `T extends <bound>`.
    pub generic_param_bounds: Vec<Option<TypeRefId>>,
    /// Targets of `requires I1, I2, …`.
    pub requires: Vec<TypeRefId>,
    /// Field signatures. Interface fields cannot have default values.
    pub fields: Vec<FieldData>,
    pub associated_types: Vec<AssociatedTypeData>,
    /// Default methods (with bodies). Implementing classes inherit them.
    pub default_methods: Vec<FunctionLoc<'db>>,
    /// Required methods (no body). Implementing classes must provide one.
    pub required_methods: Vec<InterfaceMethodSigData>,
    pub attributes: Vec<Attribute>,
    pub docstring: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssociatedTypeData {
    pub name: Name,
    pub bound: Option<TypeRefId>,
    pub default: Option<TypeRefId>,
}

/// A required (no-body) method signature on an interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceMethodSigData {
    pub name: Name,
    pub generic_params: Vec<Name>,
    pub generic_param_bounds: Vec<Option<TypeRefId>>,
    pub params: Vec<FunctionParamData>,
    pub return_type: Option<TypeRefId>,
    pub throws: Option<TypeRefId>,
    pub attributes: Vec<Attribute>,
    pub docstring: Option<String>,
}

/// Spans for an `Interface`, parallel to [`InterfaceData`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceSourceMap {
    /// Full source span of the declaration.
    pub span: TextRange,
    /// Spans for every node in [`InterfaceData::type_refs`].
    pub type_refs: TypeRefSourceMap,
    /// Parallel to [`InterfaceData::associated_types`].
    pub associated_type_spans: Vec<AssociatedTypeSourceMap>,
    /// Parallel to [`InterfaceData::required_methods`].
    pub required_method_spans: Vec<InterfaceMethodSigSourceMap>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssociatedTypeSourceMap {
    pub span: TextRange,
    pub name_span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceMethodSigSourceMap {
    pub span: TextRange,
    /// One span per parameter, parallel to `InterfaceMethodSigData::params`.
    pub param_spans: Vec<TextRange>,
}

/// Semantic data for one interface. Span-free — see the module docs.
#[salsa::tracked(returns(ref))]
pub fn interface_data<'db>(
    db: &'db dyn crate::Db,
    interface: InterfaceLoc<'db>,
) -> InterfaceData<'db> {
    lower(db, interface).0
}

/// Spans for one interface. Kept separate from [`interface_data`] so that a
/// whitespace-only edit invalidates this but not the semantic data.
#[salsa::tracked(returns(ref))]
pub fn interface_source_map<'db>(
    db: &'db dyn crate::Db,
    interface: InterfaceLoc<'db>,
) -> InterfaceSourceMap {
    lower(db, interface).1
}

/// Type refs are allocated in a fixed order (bounds, requires, fields,
/// associated types, then required methods) so that ids are a pure function of
/// the interface's shape.
fn lower<'db>(
    db: &'db dyn crate::Db,
    interface: InterfaceLoc<'db>,
) -> (InterfaceData<'db>, InterfaceSourceMap) {
    let file = interface.file(db);
    let item_tree = crate::file_item_tree(db, file);
    let data = &item_tree[interface.id(db)];

    let mut type_refs = TypeRefBuilder::new();

    let generic_param_bounds = data
        .generic_param_bounds
        .iter()
        .map(|bound| bound.as_ref().map(|te| type_refs.lower(te)))
        .collect();

    let requires = data
        .requires
        .iter()
        .map(|target| type_refs.lower(target))
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

    let associated_types = data
        .associated_types
        .iter()
        .map(|assoc| AssociatedTypeData {
            name: assoc.name.clone(),
            bound: assoc.bound.as_ref().map(|te| type_refs.lower(te)),
            default: assoc.default.as_ref().map(|te| type_refs.lower(te)),
        })
        .collect();

    let required_methods = data
        .required_methods
        .iter()
        .map(|method| InterfaceMethodSigData {
            name: method.name.clone(),
            generic_params: method.generic_params.clone(),
            generic_param_bounds: method
                .generic_param_bounds
                .iter()
                .map(|bound| bound.as_ref().map(|te| type_refs.lower(te)))
                .collect(),
            params: method
                .params
                .iter()
                .map(|param| FunctionParamData {
                    name: param.name.clone(),
                    type_ref: param.type_expr.as_ref().map(|te| type_refs.lower(te)),
                    has_default: param.default.is_some(),
                })
                .collect(),
            return_type: method.return_type.as_ref().map(|te| type_refs.lower(te)),
            throws: method.throws.as_ref().map(|te| type_refs.lower(te)),
            attributes: method.attributes.clone(),
            docstring: method.docstring.clone(),
        })
        .collect();

    let (store, spans) = type_refs.finish();

    (
        InterfaceData {
            name: data.name.clone(),
            generic_params: data.generic_params.clone(),
            type_refs: store,
            generic_param_bounds,
            requires,
            fields,
            associated_types,
            default_methods: data
                .default_methods
                .iter()
                .map(|method| FunctionLoc::new(db, file, *method))
                .collect(),
            required_methods,
            attributes: data.attributes.clone(),
            docstring: data.docstring.clone(),
        },
        InterfaceSourceMap {
            span: data.span,
            type_refs: spans,
            associated_type_spans: data
                .associated_types
                .iter()
                .map(|assoc| AssociatedTypeSourceMap {
                    span: assoc.span,
                    name_span: assoc.name_span,
                })
                .collect(),
            required_method_spans: data
                .required_methods
                .iter()
                .map(|method| InterfaceMethodSigSourceMap {
                    span: method.span,
                    param_spans: method.params.iter().map(|param| param.span).collect(),
                })
                .collect(),
        },
    )
}
