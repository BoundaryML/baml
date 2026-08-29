//! `TypeScriptFunction` — shared top-level factory binding.
//!
//! Rendered as `export const <name> = defineFunction(...)` with an `_async`
//! sibling. The fields (`param_names`, `arg_tys`, …) carry the data the
//! renderer needs to emit the typed cast.

use baml_codegen_types::{FunctionArgumentDefault, Ty};

/// Async/sync marker carried by factory bindings. Each BAML `Function`
/// (and each of its companions) fans out into one sync and one async
/// binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SyncAsync {
    Sync,
    Async,
}

pub(crate) struct TypeScriptFunction {
    /// TS identifier. Sync form = BAML name verbatim; async form =
    /// `<name>_async`. Companion forms keep their `$` suffix verbatim
    /// (`foo$stream`, `foo$build_request`) and are also `TypeScriptFunction` stubs.
    pub(crate) name: String,
    /// FQN passed as the first arg to `defineFunction`. Carries the
    /// `$<suffix>` tail for companions.
    pub(crate) baml_fqn: String,
    /// `Sync` or `Async`.
    pub(crate) mode: SyncAsync,
    /// Inline parameter-name list.
    pub(crate) param_names: Vec<String>,
    /// Parameter types matching `param_names`. Consumed when rendering the
    /// binding's `as (...) => ...` surface type in `index.ts`.
    pub(crate) arg_tys: Vec<Ty>,
    /// Default metadata matching `param_names`. `None` means the parameter
    /// remains positional; `Some` means it is rendered in the final `$opts`
    /// object and omitted from positional slots.
    pub(crate) arg_defaults: Vec<Option<FunctionArgumentDefault>>,
    /// Return type, consumed when rendering the binding's surface type.
    pub(crate) return_ty: Ty,
    /// `TypeVar` names declared on this function.
    pub(crate) generic_params: Vec<String>,
    /// Joined `///` doc-comment lines from the BAML function declaration.
    pub(crate) docstring: Option<String>,
    /// Unqualified leaf names of the function's inferred thrown types.
    pub(crate) raises_names: Vec<String>,
}
