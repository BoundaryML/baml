use baml_base::Name;
use baml_compiler2_ast as ast;
use baml_compiler2_hir::{
    loc::FunctionLoc,
    type_ref::{TypeRefBuilder, TypeRefId, TypeRefSourceMap, TypeRefStore},
};
use text_size::TextRange;

use crate::item_data::common::{
    AssociatedTypeBindingData, AssociatedTypeBindingSourceMap, FunctionParamData, GenericParamData,
    lower_generic_params,
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
    /// Generic type parameters, each with its conjunction of bounds.
    pub generic_params: Vec<GenericParamData>,
    /// Every type reference in this function's signature — bounds, parameter
    /// types, return type, throws. Scoped to the item, so edits to sibling items
    /// cannot renumber these ids.
    pub type_refs: TypeRefStore,
    pub params: Vec<FunctionParamData>,
    pub return_type: Option<TypeRefId>,
    pub throws: Option<TypeRefId>,
    pub metadata: ast::FunctionMetadata,
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

/// How many generic parameters the function's enclosing type contributes.
///
/// Zero for a free function, and for a method whose generics live on an
/// out-of-body `implements` block. A method on a generic class reports the
/// class's, which callers thread as type-arg operands alongside the function's
/// own — see the IO-builtin arity in `baml_compiler2_mir`.
#[salsa::tracked]
pub fn enclosing_type_generic_param_count<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> usize {
    crate::file_item_tree(db, function.file(db))
        .enclosing_type_generic_params(function.id(db))
        .len()
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

/// The span-free facts a function's `declarative_meta` exposes for an LLM
/// (`{ client …; prompt … }`) function: its declared client name (itself optional).
/// [`function_llm_meta`] wraps this in an `Option`, where `None` marks a non-LLM
/// function — so a client name can never be attached to one.
///
/// The full [`ast::LlmBodyDef`] (prompt template, interpolation spans, companion
/// bodies) carries spans, so it stays behind a body-ish read. Metadata consumers
/// that only need these facts front the item tree through this projection and get
/// early cutoff — editing an unrelated function no longer invalidates them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionLlmMeta {
    pub client_name: Option<Name>,
}

/// The [`FunctionLlmMeta`] projection for one function, or `None` when it is not an
/// LLM function. See [`FunctionLlmMeta`] for why the full LLM body is excluded.
#[salsa::tracked(returns(ref))]
pub fn function_llm_meta<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> Option<FunctionLlmMeta> {
    let item_tree = crate::file_item_tree(db, function.file(db));
    item_tree[function.id(db)]
        .declarative_meta
        .as_ref()
        .map(|ast::DeclarativeMeta::Llm(llm)| FunctionLlmMeta {
            client_name: llm.client.clone(),
        })
}

/// The source geometry of an LLM function's prompt literal (the literal's
/// range plus every `${…}` construct inside it), or `None` for non-LLM
/// functions and unusable prompts. Recorded at CST lowering — the desugared
/// spec body's spans alias the prompt, so consumers classify prose vs code
/// through this instead.
#[salsa::tracked(returns(ref))]
pub fn llm_prompt_spans<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> Option<ast::LlmPromptSpans> {
    let item_tree = crate::file_item_tree(db, function.file(db));
    item_tree[function.id(db)]
        .declarative_meta
        .as_ref()
        .and_then(|ast::DeclarativeMeta::Llm(llm)| llm.prompt_spans.clone())
}

/// Span-free semantic data for a function's *elaborated* signature — the
/// canonical callable view TIR consumes.
///
/// Elaboration (performed by the shared `hir::signature` machinery, which this
/// wraps) keeps the user-written top-level throws contract optional but makes
/// every nested function-type throws surface explicit:
/// - immediate callback parameter roots with omitted throws are opened to a
///   fresh synthetic effect parameter
/// - immediate function-valued return roots derive their omitted throws from
///   any immediate callback parameters they expose
/// - every other omitted nested function-type throws becomes `never`
///
/// This is the **tracked** successor of `ppir::elaborated_function_signature`
/// (an untracked fn returning spanned `TypeExpr`s — zero memoization, and
/// unsafe to memoize as-is because Salsa would retain its stale spans).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElaboratedFunctionData {
    pub name: Name,
    pub user_generic_params: Vec<Name>,
    /// Fresh `__effect_param_N` names minted for callback params with omitted
    /// throws, in parameter order.
    pub synthetic_effect_params: Vec<Name>,
    /// The arena for the *elaborated* types below — distinct from
    /// [`FunctionData::type_refs`], which holds the raw signature.
    pub type_refs: TypeRefStore,
    pub params: Vec<ElaboratedParamData>,
    pub return_type: Option<TypeRefId>,
    pub throws: Option<TypeRefId>,
}

/// A parameter of an *elaborated* signature.
///
/// Unlike [`FunctionParamData`](crate::item_data::FunctionParamData) — where
/// `type_ref: Option` mirrors the user writing or omitting an annotation —
/// `type_ref` here is total: elaboration substitutes `Unknown` for a missing
/// annotation before this struct is built, so "no type" is unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElaboratedParamData {
    pub name: Name,
    pub type_ref: TypeRefId,
    pub has_default: bool,
}

/// Spans for an [`ElaboratedFunctionData`]. Synthetic nodes (effect params,
/// filled `never`s) carry empty ranges — anchor to the owning item instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElaboratedFunctionSourceMap {
    pub type_refs: TypeRefSourceMap,
}

/// Elaborated signature for one function. Span-free — see the module docs.
#[salsa::tracked(returns(ref))]
pub fn elaborated_function_data<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> ElaboratedFunctionData {
    lower_elaborated(db, function).0
}

/// Spans for one elaborated signature. Kept separate from
/// [`elaborated_function_data`] so that a whitespace-only edit invalidates this
/// but not the semantic data.
#[salsa::tracked(returns(ref))]
pub fn elaborated_function_source_map<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> ElaboratedFunctionSourceMap {
    lower_elaborated(db, function).1
}

/// Runs the existing (TypeExpr-based, shared-with-HIR) elaboration, then lowers
/// its output into a span-free arena. Only the representation changes here; the
/// elaboration semantics live in one place.
fn lower_elaborated<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> (ElaboratedFunctionData, ElaboratedFunctionSourceMap) {
    let sig = crate::elaborated_function_signature(db, function);

    let mut type_refs = TypeRefBuilder::new();
    let params = sig
        .params
        .iter()
        .map(|param| ElaboratedParamData {
            name: param.name.clone(),
            type_ref: type_refs.lower(&param.ty),
            has_default: param.has_default,
        })
        .collect();
    let return_type = sig.return_type.as_ref().map(|te| type_refs.lower(te));
    let throws = sig.throws.as_ref().map(|te| type_refs.lower(te));
    let (store, spans) = type_refs.finish();

    (
        ElaboratedFunctionData {
            name: sig.name.clone(),
            user_generic_params: sig.user_generic_params.clone(),
            synthetic_effect_params: sig.synthetic_effect_params.clone(),
            type_refs: store,
            params,
            return_type,
            throws,
        },
        ElaboratedFunctionSourceMap { type_refs: spans },
    )
}

/// The item a method belongs to, as a `Loc`.
///
/// Mirrors `item_tree::MethodOwner` (see its docs for the ownership rules —
/// notably, in-body `implements I { … }` methods are owned by their *class*).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update)]
pub enum MethodOwner<'db> {
    Class(baml_compiler2_hir::loc::ClassLoc<'db>),
    Interface(baml_compiler2_hir::loc::InterfaceLoc<'db>),
    /// An out-of-body `implements<…> I for T { … }` block.
    FreeImpl(baml_compiler2_hir::loc::ImplLoc<'db>),
}

/// The item `method` belongs to, or `None` for a top-level function.
///
/// Replaces the `classes.values().find(|c| c.methods.contains(&id))` scan
/// family — O(items) per lookup, with class-only and class-plus-interface
/// copies drifting apart across crates.
#[salsa::tracked]
pub fn method_owner<'db>(
    db: &'db dyn crate::Db,
    method: FunctionLoc<'db>,
) -> Option<MethodOwner<'db>> {
    use baml_compiler2_hir::item_tree;

    let file = method.file(db);
    let owner = *crate::file_item_tree(db, file)
        .method_owners
        .get(&method.id(db))?;

    Some(match owner {
        item_tree::MethodOwner::Class(id) => {
            MethodOwner::Class(baml_compiler2_hir::loc::ClassLoc::new(db, file, id))
        }
        item_tree::MethodOwner::Interface(id) => {
            MethodOwner::Interface(baml_compiler2_hir::loc::InterfaceLoc::new(db, file, id))
        }
        item_tree::MethodOwner::FreeImpl(id) => {
            MethodOwner::FreeImpl(baml_compiler2_hir::loc::ImplLoc::new(db, file, id))
        }
    })
}

/// Whether `function` has a body - the ONLY distinction between a
/// default and a required interface method (r-a's shape); resolution
/// and signatures never consult it, body lowering and the `default.`
/// delegation gate do.
#[salsa::tracked]
pub fn function_has_body<'db>(db: &'db dyn crate::Db, function: FunctionLoc<'db>) -> bool {
    let item_tree = crate::file_item_tree(db, function.file(db));
    item_tree[function.id(db)].body.is_some()
}

/// A REQUIRED interface method: a bodyless function item owned by an
/// interface. Signature/resolution consumers treat it like any other
/// method (the r-a shape); BODY-LOWERING consumers (MIR, emit) skip it -
/// there is nothing to compile, exactly as before it was an item.
#[salsa::tracked]
pub fn is_required_interface_method<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> bool {
    !function_has_body(db, function)
        && matches!(method_owner(db, function), Some(MethodOwner::Interface(_)))
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

/// The `*_data` and `*_source_map` queries each call this and keep one half, so
/// `lower` runs once per query — not a single shared pass. It is deterministic,
/// so the `TypeRefId`s the data half hands out validly index the source map's
/// arena. The split into two queries is purely about what each one lets
/// downstream depend on.
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

    let generic_params = lower_generic_params(&data.generic_params, &mut type_refs);

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
            generic_params,
            type_refs: store,
            params,
            return_type,
            throws,
            metadata: data.metadata,
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
