//! `PyMethodBinding` — one rendered factory line for a static or
//! instance method on a class. Replaces the G2-era `PyStaticMethod` /
//! `PyInstanceMethod` stubs with a single binding type plus an
//! emitter-internal `MethodKind` that drives the `staticmethod(...)`
//! wrap in `render_method_binding`.

use baml_codegen_types::{FunctionArgumentDefault, Ty};

use crate::emit::{TypeVarMap, function::SyncAsync};

pub(crate) struct PyMethodBinding {
    /// Python identifier as it appears on the LHS of the binding. Sync
    /// form is the bare method name; async form has `_async` appended.
    /// Companion forms (`<m>_stream`, `<m>__build_request`) follow the
    /// same shape as free-function companions.
    pub(crate) py_name: String,
    /// FQN passed as the first arg to the factory call.
    pub(crate) baml_fqn: String,
    pub(crate) mode: SyncAsync,
    /// Source arguments before the first defaulted parameter. Instance-method
    /// receiver `self` is not included here.
    pub(crate) required_args: Vec<RequiredArg>,
    /// Source arguments starting at the first defaulted parameter.
    pub(crate) optional_args: Vec<OptionalArg>,
    /// Drives the `staticmethod(...)` wrap on static methods. Both
    /// kinds route through the same `_define_function` factory; the
    /// wrap is what blocks Python's descriptor protocol from injecting
    /// the class as positional arg 0 on statics.
    pub(crate) kind: MethodKind,
    /// Return type, used only by `.pyi` rendering.
    pub(crate) return_ty: Ty,
    /// `TypeVar` names declared on this method. Empty for non-generic
    /// methods. Surfaces only in `.pyi` rendering — the `.py` factory
    /// binding is type-erased.
    pub(crate) generic_params: Vec<String>,
    /// Raw→emitted map for `generic_params` (the method's own `<…>` scope),
    /// merged with the enclosing class's map for instance methods and threaded
    /// into the `.pyi` signature `TranslateCtx`. Empty for non-generic methods.
    pub(crate) type_var_map: TypeVarMap,
    /// Joined `///` doc-comment lines from the BAML method declaration.
    /// Surfaced only by `.pyi` rendering as a `"""..."""` body, since
    /// `.py` factory bindings have no meaningful body.
    pub(crate) docstring: Option<String>,
    /// Unqualified leaf names of the method's inferred thrown types, in
    /// source order (derived from `Function.throws`). Empty for non-throwing
    /// methods. Rendered as the `Raises:` block in the `.pyi` only (32d
    /// decision — methods get no `.py` runtime `__doc__` trailer).
    pub(crate) raises_names: Vec<String>,
}

#[derive(Clone)]
pub(crate) struct RequiredArg {
    pub(crate) name: String,
    pub(crate) ty: Ty,
}

#[derive(Clone)]
pub(crate) struct OptionalArg {
    pub(crate) name: String,
    pub(crate) ty: Ty,
    pub(crate) default: FunctionArgumentDefault,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MethodKind {
    Static,
    Instance,
}

impl PyMethodBinding {
    pub(crate) fn required_names(&self) -> Vec<String> {
        self.required_args
            .iter()
            .map(|arg| arg.name.clone())
            .collect()
    }

    pub(crate) fn optional_names(&self) -> Vec<String> {
        self.optional_args
            .iter()
            .map(|arg| arg.name.clone())
            .collect()
    }

    pub(crate) fn runtime_required_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        if matches!(self.kind, MethodKind::Instance) {
            names.push("self".to_string());
        }
        names.extend(self.required_names());
        names
    }
}
