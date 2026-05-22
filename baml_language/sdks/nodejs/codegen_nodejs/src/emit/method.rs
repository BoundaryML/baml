//! `NodeMethodBinding` — one rendered factory line for a static or
//! instance method on a class.

use baml_codegen_types::{FunctionArgumentDefault, Ty};

use crate::emit::function::SyncAsync;

pub(crate) struct NodeMethodBinding {
    /// TS identifier as it appears on the LHS of the binding. Sync form
    /// is the bare method name; async form has `_async` appended.
    /// Companion forms (`<m>_stream`, `<m>__build_request`) follow the
    /// same shape as free-function companions.
    pub(crate) name: String,
    /// FQN passed as the first arg to the factory call.
    pub(crate) baml_fqn: String,
    pub(crate) mode: SyncAsync,
    /// Inline parameter-name list. For instance methods, `"self"` is
    /// already prepended at expand time so the factory call passes
    /// `["self", …]` and the receiver lands at index 0.
    pub(crate) param_names: Vec<String>,
    /// Default metadata aligned with the post-`self` portion of
    /// `param_names`. Retained for parity with `codegen_python`.
    #[allow(dead_code)]
    pub(crate) arg_defaults: Vec<Option<FunctionArgumentDefault>>,
    pub(crate) kind: MethodKind,
    /// Parameter types matching the IR's `arguments` (no `self`). The
    /// public TS method signature zips these with `param_names`
    /// (skipping the leading `"self"` for instance methods).
    pub(crate) arg_tys: Vec<Ty>,
    /// Return type, used by the typed method signature.
    pub(crate) return_ty: Ty,
    /// `TypeVar` names declared on this method.
    #[allow(dead_code)]
    pub(crate) generic_params: Vec<String>,
    /// Joined `///` doc-comment lines from the BAML method declaration.
    #[allow(dead_code)]
    pub(crate) docstring: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MethodKind {
    Static,
    Instance,
}
