//! `NodeMethodBinding` — one rendered factory line for a static or
//! instance method on a class. Phase 2 builds these (so the fan-out and
//! sort logic are a faithful port) but does not render them — methods
//! live inside their parent class's body, which is a placeholder until
//! Phase 4.

use baml_codegen_types::{FunctionArgumentDefault, Ty};

use crate::emit::function::SyncAsync;

#[allow(dead_code)]
pub(crate) struct NodeMethodBinding {
    /// TS identifier on the LHS of the binding. Sync form is the bare
    /// method name; async form has `_async` appended.
    pub(crate) name: String,
    /// FQN passed as the first arg to the factory call.
    pub(crate) baml_fqn: String,
    pub(crate) mode: SyncAsync,
    /// Inline parameter-name list. For instance methods, `"self"` is
    /// already prepended at expand time.
    pub(crate) param_names: Vec<String>,
    /// Default metadata matching `arg_tys` (receiver `self` not included).
    pub(crate) arg_defaults: Vec<Option<FunctionArgumentDefault>>,
    /// Static vs. instance — drives the Phase 4 binding shape.
    pub(crate) kind: MethodKind,
    /// Parameter types matching the IR's `arguments` (no `self`).
    pub(crate) arg_tys: Vec<Ty>,
    /// Return type, consumed when rendering the binding's surface type.
    pub(crate) return_ty: Ty,
    /// `TypeVar` names declared on this method.
    pub(crate) generic_params: Vec<String>,
    /// Joined `///` doc-comment lines from the BAML method declaration.
    pub(crate) docstring: Option<String>,
    /// Unqualified leaf names of the method's inferred thrown types.
    pub(crate) raises_names: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MethodKind {
    Static,
    Instance,
}
