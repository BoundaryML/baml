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
    /// Named `TypeVar` bindings for a generic call. Each entry is
    /// `TypeVar name -> concrete type`; insertion order is the callee's De
    /// Bruijn order. Sourced from a host SDK call (`CallFunctionArgs.type_args`)
    /// or from internal Rust callers invoking generic stdlib functions like
    /// `baml.json.to_string<T>` (which bind their `T` by name here). The engine
    /// lowers these to the positional `type_args` slot by matching names against
    /// the callee's generic params in `set_entry_point_with_type_args`.
    /// Empty for non-generic / internal calls.
    pub type_args: IndexMap<String, baml_type::RuntimeTy>,
    /// Definition graphs accompanying entries in `type_args`. Only host
    /// reflected runtime types populate this map.
    pub type_defs: IndexMap<String, bex_vm_types::types::PortableTypeDef>,
}

/// Builder for `FunctionCallContext`.
pub struct FunctionCallContextBuilder {
    host_call_id: CallId,
    boundary: BoundaryContext,
    logger: TraceLogger,
    cancel: Option<CancellationToken>,
    profile_intent: RootProfileIntent,
    type_args: Option<IndexMap<String, baml_type::RuntimeTy>>,
    type_defs: Option<IndexMap<String, bex_vm_types::types::PortableTypeDef>>,
}

impl FunctionCallContextBuilder {
    pub fn new(host_call_id: CallId) -> Self {
        let boundary = BoundaryContext::new(BoundaryId::new_random());
        Self {
            host_call_id,
            profile_intent: RootProfileIntent::UserBoundary {
                boundary_id: boundary.boundary_id,
            },
            boundary,
            logger: TraceLogger::disabled(),
            cancel: None,
            type_args: None,
            type_defs: None,
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
            type_args: self.type_args.unwrap_or_default(),
            type_defs: self.type_defs.unwrap_or_default(),
        }
    }

    #[must_use]
    pub fn with_boundary_id(mut self, boundary_id: BoundaryId) -> Self {
        self.boundary.boundary_id = boundary_id;
        if matches!(self.profile_intent, RootProfileIntent::UserBoundary { .. }) {
            self.profile_intent = RootProfileIntent::UserBoundary { boundary_id };
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
        self.type_args = Some(type_args);
        self
    }

    #[must_use]
    pub fn with_type_defs(
        mut self,
        type_defs: IndexMap<String, bex_vm_types::types::PortableTypeDef>,
    ) -> Self {
        self.type_defs = Some(type_defs);
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
