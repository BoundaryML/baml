use bex_events::ids::BoundaryId;
use indexmap::IndexMap;
use sys_types::{CallId, CancellationToken};

use crate::{
    invocation::{InvocationClock, InvocationDeadline},
    logger::TraceLogger,
    thread::TaskCancel,
};

/// The inheritable execution environment, not an active invocation or span.
///
/// Context and ancestry are fixed at capture. Cancellation sources remain
/// live and carry the original absolute deadline. Descendants need no lookup
/// of a parent's call ID, and retention owns no waiter, VM, recording producer
/// or execution-completion guard. Local mode/capture requests and reservations
/// deliberately remain on the particular invocation that received them.
#[derive(Clone)]
pub struct InheritedInvocationState {
    pub(crate) engine_id: bex_events::ids::EngineId,
    pub(crate) cancellation: TaskCancel,
    pub(crate) context: btel_types::context::Context,
    /// Parent IDs, call path and clock epoch only; never a recording producer.
    pub(crate) ancestry: Option<bex_vm::telemetry::ThreadSpawnContext>,
    pub(crate) host_environment: u64,
    pub(crate) hook_suppression: bool,
}

impl InheritedInvocationState {
    /// Fixed context visible to a dispatched host callback, without creating
    /// another invocation merely to inspect it.
    pub fn trace_context(&self) -> &btel_types::context::Context {
        &self.context
    }

    /// Concrete generated token payload observing this retained environment.
    /// Its private authority never grants cancellation of inherited sources.
    pub fn cancellation_projection(&self) -> bex_external_types::BexExternalValue {
        let payload = bex_vm::package_baml::projected_cancel_token_data(
            self.cancellation.own().clone(),
            vec![bex_vm_types::cancellation::CancellationSource::Observer(
                std::sync::Arc::new(self.cancellation.clone()),
            )],
        );
        bex_external_types::BexExternalValue::instance(
            "baml.spawn.CancelToken",
            [(
                "_handle",
                bex_external_types::BexExternalValue::RustData(payload),
            )]
            .into(),
        )
    }

    /// Observe effective cancellation without granting cancellation authority.
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    pub async fn cancelled(&self) {
        self.cancellation.cancelled().await;
    }
}

/// Transient callback dispatch capture and its concrete cancellation projection.
/// Only dispatch retains the optional physical execution owner. Exporting the
/// inherited state into a language carrier never retains that owner.
#[derive(Clone)]
pub struct InvocationCapture {
    pub runtime: std::sync::Arc<crate::BexEngine>,
    pub state: InheritedInvocationState,
    pub cancel: Option<bex_external_types::BexExternalValue>,
    #[cfg(not(target_arch = "wasm32"))]
    pub host: Option<std::sync::Arc<crate::host_instrumentation::CallbackHostInvocation>>,
}

impl bex_vm_types::BexRustData for InvocationCapture {
    fn measure(&self, _: &mut bex_vm_types::Meter) {
        // Everything here is the engine's own.
    }
}

impl InvocationCapture {
    pub fn host_environment(&self) -> u64 {
        self.state.host_environment
    }
    pub fn deadline_ns(&self) -> Option<u64> {
        self.state
            .cancellation
            .deadline()
            .and_then(|deadline| self.runtime.invocation_deadline_ns(deadline))
    }
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
    pub cancel_tokens: Vec<bex_external_types::BexExternalValue>,
    pub inherited_state: Option<InheritedInvocationState>,
    pub trace_options: Option<bex_vm_types::trace::TraceOptionsData>,
    pub trace_reservation: Option<std::sync::Arc<bex_vm_types::trace::ReservedSpanData>>,
    pub host_environment: u64,
    // A relative timeout is consumed at the first runtime binding boundary.
    // All subsequent preparation/execution paths carry only `deadline`.
    pub(crate) timeout: Option<std::time::Duration>,
    pub(crate) deadline_ns: Option<u64>,
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
    cancel_tokens: Vec<bex_external_types::BexExternalValue>,
    inherited_state: Option<InheritedInvocationState>,
    trace_options: Option<bex_vm_types::trace::TraceOptionsData>,
    trace_reservation: Option<std::sync::Arc<bex_vm_types::trace::ReservedSpanData>>,
    host_environment: u64,
    timeout: Option<std::time::Duration>,
    deadline_ns: Option<u64>,

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
            trace_options: None,
            trace_reservation: None,
            host_environment: 0,
            timeout: None,
            deadline_ns: None,
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
            trace_options: self.trace_options,
            trace_reservation: self.trace_reservation,
            host_environment: self.host_environment,
            timeout: self.timeout,
            deadline_ns: self.deadline_ns,
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
        self.cancel_tokens = tokens
            .into_iter()
            .map(bex_external_types::BexExternalValue::Handle)
            .collect();
        self
    }

    #[must_use]
    pub fn with_cancel_values(mut self, tokens: Vec<bex_external_types::BexExternalValue>) -> Self {
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

    /// Absolute deadline in the owning runtime's invocation clock domain.
    #[must_use]
    pub fn with_deadline_ns(mut self, deadline_ns: u64) -> Self {
        self.deadline_ns = Some(deadline_ns);
        self
    }

    #[must_use]
    pub fn with_trace_options(mut self, options: bex_vm_types::trace::TraceOptionsData) -> Self {
        self.trace_options = Some(options);
        self.trace_reservation = None;
        self
    }

    #[must_use]
    pub fn with_trace_reservation(
        mut self,
        reservation: std::sync::Arc<bex_vm_types::trace::ReservedSpanData>,
    ) -> Self {
        self.trace_reservation = Some(reservation);
        self.trace_options = None;
        self
    }

    #[must_use]
    pub fn with_host_environment(mut self, environment: u64) -> Self {
        self.host_environment = environment;
        self
    }
}

impl FunctionCallContext {
    pub(crate) fn bind_timeout(
        &mut self,
        runtime: bex_events::ids::EngineId,
        clock: InvocationClock,
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
        if let Some(deadline_ns) = self.deadline_ns.take() {
            let absolute = clock.deadline(deadline_ns)?;
            self.deadline = Some(self.deadline.map_or(absolute, |own| own.earliest(absolute)));
        }
        Ok(())
    }
}
