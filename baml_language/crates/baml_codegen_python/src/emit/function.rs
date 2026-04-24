//! `PyFunction` — stub for a top-level factory binding.

/// Async/sync marker carried by factory-binding stubs. Each BAML
/// `Function` (and each of its companions) fans out into one sync
/// and one async stub.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SyncAsync {
    Sync,
    Async,
}

/// Stub for a top-level factory binding. Renders as `foo = None` in
/// G2 — no factory call yet. G5 swaps the RHS for the
/// `__define_function(...)` three-arg call.
pub(crate) struct PyFunction {
    /// Python identifier. Sync form = BAML bare name verbatim;
    /// async form = `<bare>_async`. Companion forms (`foo_stream`,
    /// `foo__build_request`, …) are also `PyFunction` stubs.
    pub(crate) py_name: String,
    /// The FQN string that G5 will pass as the factory's first arg
    /// (e.g. `"root.lorem.extract_resume$build_request"`). Captured
    /// now so the mapping from `SymbolPool` key → BAML FQN is a
    /// build-time concern, not a render-time one.
    #[allow(dead_code)]
    pub(crate) baml_fqn: String,
    /// `Sync` or `Async`.
    #[allow(dead_code)]
    pub(crate) mode: SyncAsync,
    // deferred to G5: param_names: Vec<String>,
    // deferred to G5: the __define_function call RHS.
}
