//! `PyMethodBinding` — one rendered factory line for a static or
//! instance method on a class. Replaces the G2-era `PyStaticMethod` /
//! `PyInstanceMethod` stubs with a single binding type plus an
//! emitter-internal `MethodKind` that drives the `staticmethod(...)`
//! wrap in `render_method_binding`.

use baml_codegen_types::{FunctionArgumentDefault, Ty};

use crate::emit::function::SyncAsync;

pub(crate) struct PyMethodBinding {
    /// Python identifier as it appears on the LHS of the binding. Sync
    /// form is the bare method name; async form has `_async` appended.
    /// Companion forms (`<m>_stream`, `<m>__build_request`) follow the
    /// same shape as free-function companions.
    pub(crate) py_name: String,
    /// FQN passed as the first arg to the factory call.
    pub(crate) baml_fqn: String,
    pub(crate) mode: SyncAsync,
    /// Inline parameter-name list. For instance methods, `"self"` is
    /// already prepended at expand time so the renderer is a straight
    /// walk.
    pub(crate) param_names: Vec<String>,
    /// Default metadata matching `arg_tys` (receiver `self` is not included
    /// for instance methods).
    pub(crate) arg_defaults: Vec<Option<FunctionArgumentDefault>>,
    /// Drives the `staticmethod(...)` wrap on static methods. Both
    /// kinds route through the same `_define_function` factory; the
    /// wrap is what blocks Python's descriptor protocol from injecting
    /// the class as positional arg 0 on statics.
    pub(crate) kind: MethodKind,
    /// Parameter types matching the IR's `arguments` (no `self`). Used
    /// only by `.pyi` rendering. For instance methods the `.pyi`
    /// renderer prepends `self` (no annotation) before zipping the
    /// remaining `param_names` with `arg_tys`.
    pub(crate) arg_tys: Vec<Ty>,
    /// Return type, used only by `.pyi` rendering.
    pub(crate) return_ty: Ty,
    /// `TypeVar` names declared on this method. Empty for non-generic
    /// methods. Surfaces only in `.pyi` rendering — the `.py` factory
    /// binding is type-erased.
    pub(crate) generic_params: Vec<String>,
    /// Joined `///` doc-comment lines from the BAML method declaration.
    /// Surfaced only by `.pyi` rendering as a `"""..."""` body, since
    /// `.py` factory bindings have no meaningful body.
    pub(crate) docstring: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MethodKind {
    Static,
    Instance,
}
