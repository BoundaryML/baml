use baml_base::Name;
use baml_compiler2_hir::{
    item_tree::Attribute,
    loc::{FunctionLoc, InterfaceLoc},
    type_ref::{TypeRefBuilder, TypeRefId, TypeRefSourceMap, TypeRefStore},
};
use text_size::TextRange;

use crate::item_data::common::{
    FieldData, FunctionParamData, GenericParamData, lower_generic_params,
};

/// Span-free semantic data for an `interface` declaration.
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub struct InterfaceData<'db> {
    pub name: Name,
    /// Generic type parameters, each with its conjunction of bounds.
    pub generic_params: Vec<GenericParamData>,
    /// Every type reference in this interface's signature — bounds, `requires`
    /// targets, field types, associated-type bounds and defaults, and required
    /// method signatures. Scoped to the item.
    pub type_refs: TypeRefStore,
    /// Targets of `requires I1, I2, …`.
    pub requires: Vec<TypeRefId>,
    /// Field signatures. Interface fields cannot have default values.
    pub fields: Vec<FieldData>,
    pub associated_types: Vec<AssociatedTypeData>,
    /// EVERY method as a real function item, default and required alike
    /// (a required method is a `Function` with `body: None`) - the
    /// rust-analyzer shape: one item kind, uniform `function_signature`
    /// road. THE source of truth.
    pub methods: Vec<FunctionLoc<'db>>,
    /// TIR-era VIEW: the bodied subset of `methods`. Derived, never
    /// authored; deleted with TIR at the S16 cutover.
    pub default_methods: Vec<FunctionLoc<'db>>,
    /// TIR-era VIEW: bodyless methods re-projected into the legacy sig
    /// shape (type refs in the interface's shared store). Derived, never
    /// authored; deleted with TIR at the S16 cutover.
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
    /// Generic type parameters local to this method, each with its bounds.
    pub generic_params: Vec<GenericParamData>,
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
    /// Span of the interface's name token.
    pub name_span: TextRange,
    /// Spans for every node in [`InterfaceData::type_refs`].
    pub type_refs: TypeRefSourceMap,
    /// Name span per field, parallel to [`InterfaceData::fields`].
    pub field_name_spans: Vec<TextRange>,
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
    /// Span of just the method's name.
    pub name_span: TextRange,
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
    let item_source_map = crate::file_item_tree_source_map(db, file);
    let data = &item_tree[interface.id(db)];
    // Name spans for fields / required methods live in the item-tree source map
    // (parallel to `fields` / `required_methods`), like a class's field spans.
    let field_name_spans = item_source_map
        .interface_field_spans
        .get(&interface.id(db))
        .cloned()
        .unwrap_or_default();
    let method_name_spans = item_source_map
        .interface_method_spans
        .get(&interface.id(db));

    let mut type_refs = TypeRefBuilder::new();

    let generic_params = lower_generic_params(&data.generic_params, &mut type_refs);

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
            type_ref: type_refs.lower(&field.type_expr),
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

    // The legacy required-sig view derives from the BODYLESS function
    // items (the single source of truth), re-lowered into the interface's
    // shared store so existing consumers see identical data.
    let required_items: Vec<_> = data
        .methods
        .iter()
        .filter(|&&method| item_tree[method].body.is_none())
        .map(|&method| &item_tree[method])
        .collect();
    let required_methods = required_items
        .iter()
        .map(|method| InterfaceMethodSigData {
            name: method.name.clone(),
            generic_params: lower_generic_params(&method.generic_params, &mut type_refs),
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
            // Function items carry no attribute list; nothing consumed
            // required-method attributes (verified before the view was
            // derived), so the legacy field stays empty.
            attributes: Vec::new(),
            docstring: method.docstring.clone(),
        })
        .collect();

    let (store, spans) = type_refs.finish();

    (
        InterfaceData {
            name: data.name.clone(),
            generic_params,
            type_refs: store,
            requires,
            fields,
            associated_types,
            methods: data
                .methods
                .iter()
                .map(|method| FunctionLoc::new(db, file, *method))
                .collect(),
            default_methods: data
                .methods
                .iter()
                .filter(|&&method| item_tree[method].body.is_some())
                .map(|&method| FunctionLoc::new(db, file, method))
                .collect(),
            required_methods,
            attributes: data.attributes.clone(),
            docstring: data.docstring.clone(),
        },
        InterfaceSourceMap {
            span: data.span,
            name_span: item_source_map
                .interface_name_spans
                .get(&interface.id(db))
                .copied()
                .unwrap_or_else(|| unreachable!("name span recorded at allocation")),
            type_refs: spans,
            field_name_spans,
            associated_type_spans: data
                .associated_types
                .iter()
                .map(|assoc| AssociatedTypeSourceMap {
                    span: assoc.span,
                    name_span: assoc.name_span,
                })
                .collect(),
            required_method_spans: required_items
                .iter()
                .enumerate()
                .map(|(i, method)| InterfaceMethodSigSourceMap {
                    span: method.span,
                    name_span: method_name_spans
                        .and_then(|spans| spans.get(i))
                        .copied()
                        .unwrap_or_default(),
                    param_spans: method.params.iter().map(|param| param.span).collect(),
                })
                .collect(),
        },
    )
}
