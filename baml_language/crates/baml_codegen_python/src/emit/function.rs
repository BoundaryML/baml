//! `PyFunction` — top-level factory binding.

/// Async/sync marker carried by factory bindings. Each BAML
/// `Function` (and each of its companions) fans out into one sync
/// and one async binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SyncAsync {
    Sync,
    Async,
}

/// Top-level factory binding. Renders as a `__define_function(...)`
/// three-arg call per 09b §3 / 09b2 §2. One `PyFunction` per emitted
/// line — sync and async are distinct stubs that share their FQN and
/// `param_names` (the call-time positional parameter names, sourced
/// from `Function.arguments` for free functions and from each
/// companion's own `arguments` for companions).
pub(crate) struct PyFunction {
    /// Python identifier. Sync form = BAML bare name verbatim;
    /// async form = `<bare>_async`. Companion forms (`foo_stream`,
    /// `foo__build_request`, …) are also `PyFunction` stubs.
    pub(crate) py_name: String,
    /// FQN passed as the first arg to `__define_function`. For free
    /// functions this is `"<pkg>.<ns>.<bare>"`; for companions it
    /// carries the `$<suffix>` tail (`"…$stream"`, `"…$build_request"`).
    pub(crate) baml_fqn: String,
    /// `Sync` or `Async` — selects the mode literal in the call.
    pub(crate) mode: SyncAsync,
    /// Inline parameter-name list passed as the third arg. Sourced
    /// from `Function.arguments[i].name` for free functions and from
    /// the inner companion's `arguments` for companions; never from
    /// the parent for companion bindings.
    pub(crate) param_names: Vec<String>,
}
