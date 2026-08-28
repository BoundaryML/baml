//! `TypeScriptFunction` — shared top-level factory binding.
//!
//! Rendered as `export const <name> = defineFunction(...)` with an `_async`
//! sibling. The fields (`param_names`, `arg_tys`, …) carry the data the
//! renderer needs to emit the typed cast.

use baml_codegen_types::{FunctionArgumentDefault, Ty};

/// Async/sync marker carried by factory bindings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SyncAsync {
    Sync,
    Async,
}

/// One public TypeScript binding projected from an authored BAML function.
///
/// The projection is deliberately separate from the authored FQN: Spec and
/// Stream bindings still dispatch to the original function name. The Stream
/// binding retains TypeScript's `$stream` spelling while selecting PPIR's
/// compiler-private `Fn@stream` entry through the bridge Stream operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BindingRole {
    DirectSync,
    DirectAsync,
    SpecSync,
    SpecAsync,
    StreamSync,
    StreamAsync,
}

impl BindingRole {
    pub(crate) const fn is_async(self) -> bool {
        matches!(
            self,
            Self::DirectAsync | Self::SpecAsync | Self::StreamAsync
        )
    }

    pub(crate) const fn projection(self) -> &'static str {
        match self {
            Self::DirectSync | Self::DirectAsync => "direct",
            Self::SpecSync | Self::SpecAsync => "spec",
            Self::StreamSync | Self::StreamAsync => "stream",
        }
    }

    pub(crate) fn binding_name(self, direct_name: &str) -> String {
        match self {
            Self::DirectSync => direct_name.to_string(),
            Self::DirectAsync => format!("{direct_name}_async"),
            Self::SpecSync => format!("{direct_name}_spec"),
            Self::SpecAsync => format!("{direct_name}_spec_async"),
            Self::StreamSync => format!("{direct_name}$stream"),
            Self::StreamAsync => format!("{direct_name}$stream_async"),
        }
    }
}

pub(crate) struct TypeScriptFunction {
    /// TS identifier for this flat host projection.
    pub(crate) name: String,
    /// Authored FQN passed as the first arg to `defineFunction`. Projection
    /// suffixes are never fabricated.
    pub(crate) baml_fqn: String,
    /// `Sync` or `Async`.
    pub(crate) mode: SyncAsync,
    /// Direct/Spec/Stream host projection plus sync/async execution mode.
    pub(crate) role: BindingRole,
    /// Inline parameter-name list.
    pub(crate) param_names: Vec<String>,
    /// Parameter types matching `param_names`. Consumed when rendering the
    /// binding's `as (...) => ...` surface type in `index.ts`.
    pub(crate) arg_tys: Vec<Ty>,
    /// Default metadata matching `param_names`. `None` means the parameter
    /// remains positional; `Some` means it is rendered in the final `$opts`
    /// object and omitted from positional slots.
    pub(crate) arg_defaults: Vec<Option<FunctionArgumentDefault>>,
    /// Return type, consumed when rendering the binding's surface type.
    pub(crate) return_ty: Ty,
    /// `TypeVar` names declared on this function.
    pub(crate) generic_params: Vec<String>,
    /// Joined `///` doc-comment lines from the BAML function declaration.
    pub(crate) docstring: Option<String>,
    /// Unqualified leaf names of the function's inferred thrown types.
    pub(crate) raises_names: Vec<String>,
}
