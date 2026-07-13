use baml_base::Name;
use baml_compiler2_ast as ast;
use text_size::TextRange;

use crate::{
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionParamData {
    pub name: Name,
    pub type_ref: Option<TypeRefId>,
    /// Whether a default expression was supplied. The expression itself lives in
    /// `signature::function_parameter_defaults`.
    pub has_default: bool,
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
