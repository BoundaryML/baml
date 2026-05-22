//! `NodeFunction` — top-level factory binding.

use baml_codegen_types::{FunctionArgumentDefault, Ty};

/// Async/sync marker carried by factory bindings. Each BAML `Function`
/// (and each of its companions) fans out into one sync and one async
/// binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SyncAsync {
    Sync,
    Async,
}

/// Top-level factory binding.
pub(crate) struct NodeFunction {
    /// TS identifier. Sync form = BAML bare name (with companion suffix
    /// translated via `bare_callable_name`); async form = `<bare>_async`.
    pub(crate) name: String,
    /// FQN that will be passed to the factory call.
    pub(crate) baml_fqn: String,
    pub(crate) mode: SyncAsync,
    /// Inline parameter-name list passed as the third arg to
    /// `defineFunction`. Sourced from `Function.arguments[i].name` for
    /// free functions and from the companion's own `arguments` for
    /// companions.
    pub(crate) param_names: Vec<String>,
    /// Default metadata matching `param_names`. Retained for parity with
    /// `codegen_python`; the TS factory binding doesn't use it today.
    #[allow(dead_code)]
    pub(crate) arg_defaults: Vec<Option<FunctionArgumentDefault>>,
    /// Parameter types in the same order as `param_names`. Drives the
    /// `as (a: A, …) => R` typed-signature assertion on the `.ts` and
    /// the `export declare const …: (a: A, …) => R` line in the `.d.ts`.
    pub(crate) arg_tys: Vec<Ty>,
    /// Return type, used by the typed-signature assertion in both
    /// outputs.
    pub(crate) return_ty: Ty,
    /// `TypeVar` names declared on this function. Empty for non-generic
    /// functions.
    #[allow(dead_code)]
    pub(crate) generic_params: Vec<String>,
    /// Joined `///` doc-comment lines from the BAML function declaration.
    #[allow(dead_code)]
    pub(crate) docstring: Option<String>,
}
