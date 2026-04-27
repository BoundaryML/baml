//! `PyMethodBinding` — one rendered factory line for a static or
//! instance method on a class. Replaces the G2-era `PyStaticMethod` /
//! `PyInstanceMethod` stubs with a single binding type plus an
//! emitter-internal `MethodKind` that drives the `staticmethod(...)`
//! wrap in `render_method_binding`.

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
    /// Drives the `staticmethod(...)` wrap and the choice of factory
    /// alias (`__define_static_method` vs. `__define_instance_method`).
    pub(crate) kind: MethodKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MethodKind {
    Static,
    Instance,
}
