//! `NodeFunction` — top-level factory binding.
//!
//! Phase 2 renders these as `export const <name>: any = BAML_PLACEHOLDER;`
//! placeholders; Phase 4 replaces them with `defineFunction(...)` calls.
//! The richer fields (`param_names`, `arg_tys`, …) are carried now so
//! Phase 3/4 don't have to re-thread them through `build_emitted`.

use baml_codegen_types::{FunctionArgumentDefault, Ty};

/// Async/sync marker carried by factory bindings. Each BAML `Function`
/// (and each of its companions) fans out into one sync and one async
/// binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SyncAsync {
    Sync,
    Async,
}

#[allow(dead_code)]
pub(crate) struct NodeFunction {
    /// TS identifier. Sync form = BAML bare name verbatim; async form =
    /// `<bare>_async`. Companion forms (`foo_stream`, `foo__build_request`)
    /// are also `NodeFunction` stubs.
    pub(crate) name: String,
    /// FQN passed as the first arg to `defineFunction`. Carries the
    /// `$<suffix>` tail for companions.
    pub(crate) baml_fqn: String,
    /// `Sync` or `Async`.
    pub(crate) mode: SyncAsync,
    /// Inline parameter-name list.
    pub(crate) param_names: Vec<String>,
    /// Default metadata matching `param_names`.
    pub(crate) arg_defaults: Vec<Option<FunctionArgumentDefault>>,
    /// Parameter types matching `param_names`. Consumed by `.d.ts`
    /// rendering in Phase 4.
    pub(crate) arg_tys: Vec<Ty>,
    /// Return type, consumed by `.d.ts` rendering in Phase 4.
    pub(crate) return_ty: Ty,
    /// `TypeVar` names declared on this function.
    pub(crate) generic_params: Vec<String>,
    /// Joined `///` doc-comment lines from the BAML function declaration.
    pub(crate) docstring: Option<String>,
    /// Unqualified leaf names of the function's inferred thrown types.
    pub(crate) raises_names: Vec<String>,
}
