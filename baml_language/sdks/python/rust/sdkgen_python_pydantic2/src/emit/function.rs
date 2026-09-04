//! `PyFunction` — top-level factory binding.

use std::collections::BTreeMap;

use baml_codegen_types::{FunctionArgumentDefault, Ty};

/// Async/sync marker carried by factory bindings. Each host projection fans
/// out into one sync and one async binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SyncAsync {
    Sync,
    Async,
}

/// Top-level factory binding. Renders as a `__define_function(...)`
/// three-arg call per 09b §3 / 09b2 §2. One `PyFunction` per emitted
/// line — sync and async are distinct stubs that share the authored FQN and
/// `param_names`.
pub(crate) struct PyFunction {
    /// Collision-allocated Python identifier for a direct, spec, or stream
    /// role. The async role conventionally ends in `_async`.
    pub(crate) py_name: String,
    /// Authored FQN passed to `__define_function` for every role.
    pub(crate) baml_fqn: String,
    /// `Sync` or `Async` — selects the mode literal in the call.
    pub(crate) mode: SyncAsync,
    /// Inline host parameter-name list passed to the factory.
    pub(crate) param_names: Vec<String>,
    /// Raw BAML parameter names matching `param_names`.
    pub(crate) wire_param_names: Vec<String>,
    /// Default metadata matching `param_names`. `None` means the
    /// parameter is required/positional-compatible; `Some` means it is
    /// defaulted and therefore keyword-only in generated Python.
    pub(crate) arg_defaults: Vec<Option<FunctionArgumentDefault>>,
    /// Parameter types in the same order as `param_names`. Used only by
    /// `.pyi` rendering; the `.py` factory binding doesn't reference types.
    pub(crate) arg_tys: Vec<Ty>,
    /// Return type, used only by `.pyi` rendering. The async-ness lives
    /// in the `def` keyword (per 12d §3.4); this `Ty` is identical for
    /// the sync and async fan-out siblings.
    pub(crate) return_ty: Ty,
    /// `TypeVar` names declared on this function. Empty for non-generic
    /// functions. Surfaces only in `.pyi` rendering — the `.py` factory
    /// binding is type-erased.
    pub(crate) generic_params: Vec<String>,
    /// Raw BAML `TypeVar` names matching `generic_params`.
    pub(crate) wire_generic_params: Vec<String>,
    /// Raw `TypeVar` spelling -> projected Python spelling for annotations.
    pub(crate) type_var_names: BTreeMap<String, String>,
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
