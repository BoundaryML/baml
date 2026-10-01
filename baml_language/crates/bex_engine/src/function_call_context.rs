use bex_events::ids::BoundaryId;
use indexmap::IndexMap;
use sys_types::{CallId, CancellationToken};

use crate::{invocation::InvocationDeadline, logger::TraceLogger, thread::TaskCancel};

/// Retained cancellation inputs and absolute deadline for callback/re-entry,
/// independent of the parent's waiter or active-call registration.
#[derive(Clone)]
pub struct InheritedInvocationState {
    pub(crate) engine_id: bex_events::ids::EngineId,
    pub(crate) cancellation: TaskCancel,
}

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
    /// Actual generated `baml.spawn.CancelToken` handles, including composites.
    pub cancel_tokens: Vec<bex_external_types::Handle>,
    pub inherited_state: Option<InheritedInvocationState>,
    // A relative timeout is consumed at the first runtime binding boundary.
    // All subsequent preparation/execution paths carry only `deadline`.
    pub(crate) timeout: Option<std::time::Duration>,
    pub(crate) deadline: Option<InvocationDeadline>,
    pub(crate) bound_runtime: Option<bex_events::ids::EngineId>,

    /// Named `TypeVar` bindings for a generic call. Each entry is
    /// `TypeVar name -> concrete type`; insertion order is the callee's De
    /// Bruijn order. Sourced from a host SDK call (`CallFunctionArgs.type_args`)
    /// or from internal Rust callers invoking generic stdlib functions like
    /// `baml.json.from_string<T>` (which binds `T` by name here). The engine
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
    cancel_tokens: Vec<bex_external_types::Handle>,
    inherited_state: Option<InheritedInvocationState>,
    timeout: Option<std::time::Duration>,

    type_args: Option<IndexMap<String, baml_type::RuntimeTy>>,
    type_defs: Option<IndexMap<String, bex_vm_types::types::PortableTypeDef>>,
}

impl FunctionCallContextBuilder {
    pub fn new(host_call_id: CallId) -> Self {
        let boundary = BoundaryContext::new(BoundaryId::new_random());
        Self {
            host_call_id,
            boundary,
            logger: TraceLogger::disabled(),
            cancel: None,
            cancel_tokens: Vec::new(),
            inherited_state: None,
            timeout: None,
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
            cancel_tokens: self.cancel_tokens,
            inherited_state: self.inherited_state,
            timeout: self.timeout,
            deadline: None,
            bound_runtime: None,

            type_args: self.type_args.unwrap_or_default(),
            type_defs: self.type_defs.unwrap_or_default(),
        }
    }

    #[must_use]
    pub fn with_boundary_id(mut self, boundary_id: BoundaryId) -> Self {
        self.boundary.boundary_id = boundary_id;
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
    pub fn with_cancel_tokens(mut self, tokens: Vec<bex_external_types::Handle>) -> Self {
        self.cancel_tokens = tokens;
        self
    }

    #[must_use]
    pub fn with_inherited_state(mut self, state: InheritedInvocationState) -> Self {
        self.inherited_state = Some(state);
        self
    }

    #[must_use]
    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

impl FunctionCallContext {
    pub(crate) fn bind_timeout(
        &mut self,
        runtime: bex_events::ids::EngineId,
    ) -> Result<(), crate::EngineError> {
        if self.bound_runtime.is_some_and(|owner| owner != runtime)
            || self
                .inherited_state
                .as_ref()
                .is_some_and(|state| state.engine_id != runtime)
        {
            return Err(crate::EngineError::TypeMismatch {
                message: "invocation context belongs to a different runtime".into(),
            });
        }
        self.bound_runtime = Some(runtime);
        if let Some(timeout) = self.timeout.take() {
            self.deadline = Some(InvocationDeadline::after(timeout)?);
        }
        Ok(())
    }
}
