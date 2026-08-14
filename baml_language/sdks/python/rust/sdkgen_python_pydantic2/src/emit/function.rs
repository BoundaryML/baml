//! `PyFunction` — top-level factory binding.

use baml_codegen_types::{FunctionArgumentDefault, Ty};

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
    /// Default metadata matching `param_names`. `None` means the
    /// parameter is required/positional-compatible; `Some` means it is
    /// defaulted and therefore keyword-only in generated Python.
    pub(crate) arg_defaults: Vec<Option<FunctionArgumentDefault>>,
    /// Parameter types in the same order as `param_names`. Used only
    /// by `.pyi` rendering; the `.py` factory binding doesn't reference
    /// types. For companions, these are the companion's own parameter
    /// types — never the parent's.
    pub(crate) arg_tys: Vec<Ty>,
    /// Return type, used only by `.pyi` rendering. The async-ness lives
    /// in the `def` keyword (per 12d §3.4); this `Ty` is identical for
    /// the sync and async fan-out siblings.
    pub(crate) return_ty: Ty,
    /// `TypeVar` names declared on this function. Empty for non-generic
    /// functions. Surfaces only in `.pyi` rendering — the `.py` factory
    /// binding is type-erased.
    pub(crate) generic_params: Vec<String>,
    /// Joined `///` doc-comment lines from the BAML function declaration.
    /// Surfaced only by `.pyi` rendering as a `"""..."""` body so
    /// `__doc__` resolves at runtime.
    pub(crate) docstring: Option<String>,
    /// Unqualified leaf names of the function's inferred thrown types, in
    /// source order (derived from `Function.throws`). Empty for non-throwing
    /// functions. Rendered as the `Raises:` docstring block (32d) — in the
    /// `.pyi` (always) and, for free functions, the `.py` `__doc__ =` trailer.
    pub(crate) raises_names: Vec<String>,
}
