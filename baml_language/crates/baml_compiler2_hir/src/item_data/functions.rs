use baml_base::Name;
use baml_compiler2_ast as ast;
use text_size::TextRange;

use crate::{
    item_data::common::{
        AssociatedTypeBindingData, AssociatedTypeBindingSourceMap, FunctionParamData,
    },
    loc::FunctionLoc,
    type_ref::{TypeRefBuilder, TypeRefId, TypeRefSourceMap, TypeRefStore},
};

/// Span-free semantic data for a function's *signature*.
///
/// The body is deliberately absent: it has its own query (`body::function_body`)
/// so that editing a body cannot invalidate anything that only depends on the
/// signature.
///
/// This supersedes `signature::FunctionSignature`, which is memoized but still
/// carries spanned `ast::TypeExpr`s — meaning Salsa declines to overwrite it on a
/// whitespace-only edit and it serves *stale* spans from then on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionData {
    pub name: Name,
    pub generic_params: Vec<Name>,
    /// Every type reference in this function's signature — bounds, parameter
    /// types, return type, throws. Scoped to the item, so edits to sibling items
    /// cannot renumber these ids.
    pub type_refs: TypeRefStore,
    /// Parallel to `generic_params`. `Some` means `T extends <bound>`.
    pub generic_param_bounds: Vec<Option<TypeRefId>>,
    pub params: Vec<FunctionParamData>,
    pub return_type: Option<TypeRefId>,
    pub throws: Option<TypeRefId>,
    pub origin: ast::FunctionOrigin,
    pub docstring: Option<String>,
    /// Set when the fn def had a `//baml:tagged_string` marker.
    pub is_tagged_template_tag: bool,
}

/// Spans for a `Function`, parallel to [`FunctionData`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSourceMap {
    /// Full source span of the declaration.
    pub span: TextRange,
    /// Span of just the function's name.
    pub name_span: TextRange,
    /// Spans for every node in [`FunctionData::type_refs`].
    pub type_refs: TypeRefSourceMap,
    /// One span per parameter, parallel to [`FunctionData::params`].
    pub param_spans: Vec<TextRange>,
}

/// Semantic data for one function signature. Span-free — see the module docs.
#[salsa::tracked(returns(ref))]
pub fn function_data<'db>(db: &'db dyn crate::Db, function: FunctionLoc<'db>) -> FunctionData {
    lower(db, function).0
}

/// Spans for one function signature. Kept separate from [`function_data`] so that
/// a whitespace-only edit invalidates this but not the semantic data.
#[salsa::tracked(returns(ref))]
pub fn function_source_map<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> FunctionSourceMap {
    lower(db, function).1
}

/// The interface target a method was declared under, when it sits inside an
/// `implements I { … }` block — plus the associated-type bindings from that
/// block's header. `None` for class-level methods and for interface default
/// methods.
///
/// The target stays an unresolved `TypeRef`; TIR resolves it to an
/// `InterfaceLoc` lazily, so HIR construction stays independent of name
/// resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodInterfaceTarget {
    pub type_refs: TypeRefStore,
    pub target: TypeRefId,
    pub associated_type_bindings: Vec<AssociatedTypeBindingData>,
}

/// Spans for a [`MethodInterfaceTarget`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodInterfaceTargetSourceMap {
    pub type_refs: TypeRefSourceMap,
    /// Parallel to `MethodInterfaceTarget::associated_type_bindings`.
    pub associated_type_bindings: Vec<AssociatedTypeBindingSourceMap>,
}

/// See [`MethodInterfaceTarget`]. Span-free.
#[salsa::tracked(returns(ref))]
pub fn method_interface_target<'db>(
    db: &'db dyn crate::Db,
    method: FunctionLoc<'db>,
) -> Option<MethodInterfaceTarget> {
    lower_interface_target(db, method).map(|(data, _)| data)
}

/// Spans for [`method_interface_target`]. Kept separate so that a
/// whitespace-only edit invalidates this but not the semantic data.
#[salsa::tracked(returns(ref))]
pub fn method_interface_target_source_map<'db>(
    db: &'db dyn crate::Db,
    method: FunctionLoc<'db>,
) -> Option<MethodInterfaceTargetSourceMap> {
    lower_interface_target(db, method).map(|(_, source_map)| source_map)
}

fn lower_interface_target<'db>(
    db: &'db dyn crate::Db,
    method: FunctionLoc<'db>,
) -> Option<(MethodInterfaceTarget, MethodInterfaceTargetSourceMap)> {
    let item_tree = crate::file_item_tree(db, method.file(db));
    let target_expr = item_tree.method_to_iface_target.get(&method.id(db))?;
    let bindings = item_tree
        .method_to_iface_associated_type_bindings
        .get(&method.id(db))
        .map(Vec::as_slice)
        .unwrap_or_default();

    let mut type_refs = TypeRefBuilder::new();
    let target = type_refs.lower(target_expr);
    let associated_type_bindings = bindings
        .iter()
        .map(|binding| AssociatedTypeBindingData {
            name: binding.name.clone(),
            type_ref: binding.type_expr.as_ref().map(|te| type_refs.lower(te)),
        })
        .collect();
    let (store, spans) = type_refs.finish();

    Some((
        MethodInterfaceTarget {
            type_refs: store,
            target,
            associated_type_bindings,
        },
        MethodInterfaceTargetSourceMap {
            type_refs: spans,
            associated_type_bindings: bindings
                .iter()
                .map(|binding| AssociatedTypeBindingSourceMap {
                    span: binding.span,
                    name_span: binding.name_span,
                })
                .collect(),
        },
    ))
}

/// Both halves share one lowering pass — the split into two queries is purely
/// about what each one lets downstream depend on.
///
/// Type refs are allocated in a fixed order (bounds, params, return, throws) so
/// that ids are a pure function of the signature's shape.
fn lower<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> (FunctionData, FunctionSourceMap) {
    let file = function.file(db);
    let item_tree = crate::file_item_tree(db, file);
    let item_source_map = crate::file_item_tree_source_map(db, file);
    let data = &item_tree[function.id(db)];

    let mut type_refs = TypeRefBuilder::new();

    let generic_param_bounds = data
        .generic_param_bounds
        .iter()
        .map(|bound| bound.as_ref().map(|te| type_refs.lower(te)))
        .collect();

    let params: Vec<FunctionParamData> = data
        .params
        .iter()
        .map(|param| FunctionParamData {
            name: param.name.clone(),
            type_ref: param.type_expr.as_ref().map(|te| type_refs.lower(te)),
            has_default: param.default.is_some(),
        })
        .collect();

    let return_type = data.return_type.as_ref().map(|te| type_refs.lower(te));
    let throws = data.throws.as_ref().map(|te| type_refs.lower(te));

    let (store, spans) = type_refs.finish();

    (
        FunctionData {
            name: data.name.clone(),
            generic_params: data.generic_params.clone(),
            type_refs: store,
            generic_param_bounds,
            params,
            return_type,
            throws,
            origin: data.origin,
            docstring: data.docstring.clone(),
            is_tagged_template_tag: data.is_tagged_template_tag,
        },
        FunctionSourceMap {
            span: data.span,
            name_span: item_source_map
                .function_name_spans
                .get(&function.id(db))
                .copied()
                .unwrap_or_default(),
            type_refs: spans,
            param_spans: data.params.iter().map(|param| param.span).collect(),
        },
    )
}
