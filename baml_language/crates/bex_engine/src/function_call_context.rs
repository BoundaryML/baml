use bex_events::{ids::BoundaryId, prof::backend::RootProfileIntent};
use indexmap::IndexMap;
use sys_types::{CallId, CancellationToken};

use crate::logger::TraceLogger;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundaryContext {
    pub boundary_id: BoundaryId,
    pub storage_context: BoundaryStorageContext,
}

impl BoundaryContext {
    #[must_use]
    pub fn new(boundary_id: BoundaryId) -> Self {
        Self {
            boundary_id,
            storage_context: BoundaryStorageContext::default(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoundaryStorageContext {
    _private: (),
}

/// Per-call context passed to [`crate::BexEngine::call_function`].
///
/// Constructed via [`FunctionCallContextBuilder`].
pub struct FunctionCallContext {
    pub host_call_id: CallId,
    pub boundary: BoundaryContext,
    pub logger: TraceLogger,
    pub cancel: CancellationToken,
    pub profile_intent: RootProfileIntent,
    /// Explicit named bindings for a generic function call. Each binding is a
    /// static type, an explicit portable import, or a live type reference whose
    /// issuing runtime must match. The engine matches names to the callee's
    /// generic slots; caller insertion order does not select a slot. Checked
    /// interface methods carry their positional bindings in the method target.
    pub type_args: IndexMap<String, bex_external_types::TypeArgument>,
}

/// Builder for `FunctionCallContext`.
pub struct FunctionCallContextBuilder {
    host_call_id: CallId,
    boundary: BoundaryContext,
    logger: TraceLogger,
    cancel: Option<CancellationToken>,
    profile_intent: RootProfileIntent,
    type_args: IndexMap<String, bex_external_types::TypeArgument>,
}

impl FunctionCallContextBuilder {
    pub fn new(host_call_id: CallId) -> Self {
        let boundary = BoundaryContext::new(BoundaryId::new_random());
        Self {
            host_call_id,
            profile_intent: RootProfileIntent::UserRoot {
                runtime_id: boundary.boundary_id,
            },
            boundary,
            logger: TraceLogger::disabled(),
            cancel: None,
            type_args: IndexMap::new(),
        }
    }

    #[must_use]
    pub fn build(self) -> FunctionCallContext {
        FunctionCallContext {
            host_call_id: self.host_call_id,
            boundary: self.boundary,
            logger: self.logger,
            cancel: self.cancel.unwrap_or_default(),
            profile_intent: self.profile_intent,
            type_args: self.type_args,
        }
    }

    #[must_use]
    pub fn with_boundary_id(mut self, boundary_id: BoundaryId) -> Self {
        self.boundary.boundary_id = boundary_id;
        if matches!(self.profile_intent, RootProfileIntent::UserRoot { .. }) {
            self.profile_intent = RootProfileIntent::UserRoot {
                runtime_id: boundary_id,
            };
        }
        self
    }

    #[must_use]
    pub fn with_logger(mut self, logger: TraceLogger) -> Self {
        self.logger = logger;
        self
    }

    /// Seed named `TypeVar` bindings for a generic call. Insertion order should
    /// be the callee's De Bruijn order. The engine resolves them to positional
    /// slots against the callee's generic params.
    #[must_use]
    pub fn with_type_args(mut self, type_args: IndexMap<String, baml_type::RuntimeTy>) -> Self {
        self.type_args = type_args
            .into_iter()
            .map(|(name, ty)| (name, bex_external_types::TypeArgument::Named(ty)))
            .collect();
        self
    }

    /// Supply checked references or explicit portable definitions as well as
    /// static named types. Exactly one evidence kind belongs to each binding.
    #[must_use]
    pub fn with_type_bindings(
        mut self,
        bindings: IndexMap<String, bex_external_types::TypeArgument>,
    ) -> Self {
        self.type_args = bindings;
        self
    }

    #[must_use]
    pub fn with_type_defs(
        mut self,
        type_defs: IndexMap<String, bex_vm_types::types::PortableTypeDef>,
    ) -> Self {
        self.type_args.extend(
            type_defs
                .into_iter()
                .map(|(name, ty)| (name, bex_external_types::TypeArgument::Definition(ty))),
        );
        self
    }

    #[must_use]
    pub fn with_cancel_token(mut self, cancel: CancellationToken) -> Self {
        self.cancel = Some(cancel);
        self
    }

    #[must_use]
    pub fn suppress_internal_profile(mut self) -> Self {
        self.profile_intent = RootProfileIntent::SuppressInternal;
        self
    }
}
