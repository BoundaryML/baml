//! `TypeScriptMethodBinding` — one shared rendered factory line for a static or
//! instance method on a class, rendered inside the parent class's body:
//! static methods as `static x = defineFunction(...)`, instance methods as
//! `x = defineInstanceFunction(...).bind(this)`.

use baml_codegen_types::{FunctionArgumentDefault, Ty};

use crate::emit::function::{BindingRole, SyncAsync};

pub(crate) struct TypeScriptMethodBinding {
    /// TS identifier on the LHS of the binding. Sync form is the bare
    /// method name; async form has `_async` appended.
    pub(crate) name: String,
    /// FQN passed as the first arg to the factory call.
    pub(crate) baml_fqn: String,
    pub(crate) mode: SyncAsync,
    /// Direct/Spec/Stream host projection plus sync/async execution mode.
    pub(crate) role: BindingRole,
    /// Static vs. instance — drives the Phase 4 binding shape.
    pub(crate) kind: MethodKind,
    /// Source arguments before the first defaulted parameter. Instance-method
    /// receiver `self` is not included here.
    pub(crate) required_args: Vec<RequiredArg>,
    /// Source arguments starting at the first defaulted parameter.
    pub(crate) optional_args: Vec<OptionalArg>,
    /// Return type, consumed when rendering the binding's surface type.
    pub(crate) return_ty: Ty,
    /// `TypeVar` names declared on this method.
    pub(crate) generic_params: Vec<String>,
    /// Joined `///` doc-comment lines from the BAML method declaration.
    pub(crate) docstring: Option<String>,
    /// Unqualified leaf names of the method's inferred thrown types.
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

impl TypeScriptMethodBinding {
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
