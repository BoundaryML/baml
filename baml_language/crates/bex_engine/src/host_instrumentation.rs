//! Retained inheritance and host execution ownership have independent lifetimes.
use std::sync::Arc;

use bex_vm::telemetry::{FrameTelemetry, TelemetryState};
pub(crate) use bex_vm_types::trace::HostDefinitionKey;
pub use bex_vm_types::trace::{HostCallSite, HostDefinition};
use btel_types::{FunctionMetadata, InvocationMode, InvocationOutcome};

use crate::{
    BexEngine, CallId, EngineError, InheritedInvocationState, RootCallWork, thread::TaskCancel,
};

/// Owns an externally executed span. Ordinary host bodies also retain work
/// registration; idle stream handles retain only recording lifetime. The SDK
/// separately retains `inherited_state()`; that carrier owns no producer.
pub struct HostInvocation {
    state: InheritedInvocationState,
    telemetry: Option<TelemetryState>,
    frame: Option<FrameTelemetry>,
    output: bool,
    error: bool,
    // Execution registration drops after recording completion.
    engine: Arc<BexEngine>,
    registration: Option<RootCallWork>,
}

impl BexEngine {
    pub fn begin_host_invocation(
        self: &Arc<Self>,
        definition: &HostDefinition,
        inherited: Option<&InheritedInvocationState>,
        options: &bex_vm_types::trace::TraceOptionsData,
        caller: &HostCallSite,
        inputs: Option<&btel_snapshot::host::HostValue>,
    ) -> Result<HostInvocation, EngineError> {
        self.begin_host_with_reservation(definition, inherited, options, caller, inputs, None)
    }

    pub(crate) fn begin_host_with_reservation(
        self: &Arc<Self>,
        definition: &HostDefinition,
        inherited: Option<&InheritedInvocationState>,
        options: &bex_vm_types::trace::TraceOptionsData,
        caller: &HostCallSite,
        inputs: Option<&btel_snapshot::host::HostValue>,
        reserved_id: Option<btel_types::TelemetryId>,
    ) -> Result<HostInvocation, EngineError> {
        if inherited.is_some_and(|state| state.engine_id != self.engine_id) {
            return Err(EngineError::TypeMismatch {
                message: "host context belongs to a different runtime".into(),
            });
        }
        let cancellation = inherited.map_or_else(
            || TaskCancel::detached([]),
            |state| state.cancellation.child([]),
        );
        // A marker observes ordinary host execution, including cancelled
        // cleanup. Register the actual authority so shutdown and retained
        // descendants observe the same live source; do not suppress the body.
        let registration = RootCallWork::register_with_cancellation_check(
            Arc::clone(self),
            CallId::next(),
            cancellation.clone(),
            None,
            false,
        )?;
        let metadata = {
            let mut definitions = self
                .host_definitions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(metadata) = definitions.get(&definition.key()) {
                Arc::clone(metadata)
            } else {
                let function_id = self
                    .heap
                    .allocate_host_function_id()
                    .map_err(|error| EngineError::Other(error.to_string()))?;
                let fqn = format!(
                    "{}:{}:{}:{}:{}",
                    definition.language,
                    definition.module,
                    definition.qualified_name,
                    definition.definition_line,
                    definition.wrapper_line
                );
                let metadata = Arc::new(FunctionMetadata {
                    function_id,
                    definition_key: Some(btel_types::DefinitionKey(format!(
                        "host:{:?}",
                        definition.key()
                    ))),
                    fqn,
                    display_name: definition.display_name.clone(),
                    source_file: Some(definition.source_file.clone()),
                    source_span: None,
                    kind: btel_types::RuntimeFunctionKind::Native,
                    origin: btel_types::RuntimeFunctionOrigin::UserDefined,
                    owner_type: None,
                    parent_function: None,
                    lambda_path: None,
                    package_name: Some(definition.module.clone()),
                    namespace: vec![definition.language.clone()],
                    argument_layout: None,
                    source_map: None,
                });
                definitions.insert(definition.key(), Arc::clone(&metadata));
                metadata
            }
        };
        self.begin_external_invocation(
            &metadata,
            inherited,
            options,
            caller,
            inputs,
            reserved_id,
            cancellation,
            Some(registration),
        )
    }

    /// A host-held stream owns telemetry, but idle stream handles are not
    /// executable work that runtime shutdown must wait for.
    pub async fn begin_stream_invocation(
        self: &Arc<Self>,
        function_name: &str,
        call: &crate::FunctionCallContext,
        inputs: Option<&btel_snapshot::host::HostValue>,
    ) -> Result<HostInvocation, EngineError> {
        let (function, _) = self
            .resolved_function_names
            .get(function_name)
            .ok_or_else(|| EngineError::Other(format!("Function not found: {function_name}")))?;
        // SAFETY: named entry functions are sealed compile-time objects.
        let bex_vm_types::Object::Function(function) = (unsafe { function.get() }) else {
            return Err(EngineError::Other("stream entry is not a function".into()));
        };
        let metadata = function
            .runtime_metadata()
            .ok_or_else(|| EngineError::Other("stream entry has no function metadata".into()))?;
        let mut options = call
            .trace_reservation
            .as_ref()
            .map(|reservation| reservation.options.clone())
            .or_else(|| call.trace_options.clone())
            .unwrap_or_default();
        options.mode.get_or_insert(InvocationMode::Span);
        options.error.get_or_insert(true);
        let mut thread = self
            .new_entry_thread(call.cancel.clone(), call.inherited_state.as_ref())
            .await;
        self.prepare_cancellation(
            &mut thread,
            None,
            call.cancel.clone(),
            &call.cancel_tokens,
            call.inherited_state.clone(),
            call.deadline,
        )?;
        let cancellation = thread.cancel.clone();
        drop(thread);
        // Reject pre-cancelled/shutdown entries and claim reservations at
        // the same locked boundary as ordinary SDK invocation admission.
        // Release this transient work registration before returning an idle
        // handle; subsequent pulls register their own executable work.
        let admission = RootCallWork::register(
            Arc::clone(self),
            call.host_call_id,
            cancellation.clone(),
            call.trace_reservation.as_deref(),
        )?;
        let reserved_id = admission.reserved_id;
        self.begin_external_invocation(
            &metadata,
            call.inherited_state.as_ref(),
            &options,
            &HostCallSite {
                source_file: String::new(),
                line: 0,
            },
            inputs,
            reserved_id,
            cancellation,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn begin_external_invocation(
        self: &Arc<Self>,
        metadata: &FunctionMetadata,
        inherited: Option<&InheritedInvocationState>,
        options: &bex_vm_types::trace::TraceOptionsData,
        caller: &HostCallSite,
        inputs: Option<&btel_snapshot::host::HostValue>,
        reserved_id: Option<btel_types::TelemetryId>,
        cancellation: TaskCancel,
        registration: Option<RootCallWork>,
    ) -> Result<HostInvocation, EngineError> {
        let context = inherited.map_or_else(
            || self.launch_context.clone(),
            |state| state.context.clone(),
        );
        let context = options
            .context
            .as_ref()
            .map_or(context.clone(), |patch| context.with_patch(patch));
        let caller_pc = if caller.line == 0 {
            0 // The core's absent-call-site representation.
        } else {
            let mut sites = self
                .host_call_sites
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(id) = sites.get(caller) {
                *id
            } else {
                let id = u32::try_from(sites.len() + 1).map_err(|_| {
                    EngineError::Other("host call-site identity space exhausted".into())
                })?;
                sites.insert(caller.clone(), id);
                id
            }
        };
        let mut telemetry = if options.mode == Some(InvocationMode::Hidden) {
            None
        } else {
            match inherited.and_then(|state| state.ancestry.as_ref()) {
                Some(parent) => crate::telemetry_state::EngineTelemetry::new_child(
                    self.telemetry.as_ref(),
                    Some(parent),
                ),
                None => crate::telemetry_state::EngineTelemetry::new_root(self.telemetry.as_ref()),
            }
        };
        let frame = telemetry.as_mut().and_then(|telemetry| {
            let _scope = telemetry.host_recording_scope();
            telemetry.set_context(context.clone());
            telemetry.start_thread();
            telemetry.mark_running();
            let captured_inputs =
                inputs
                    .filter(|_| options.inputs == Some(true))
                    .and_then(|value| {
                        telemetry.capture_host_with(value, |name| {
                            self.host_capture_declarations.get(name)
                        })
                    });
            telemetry.enter_host(
                metadata,
                caller_pc,
                options.mode.unwrap_or(InvocationMode::Span),
                captured_inputs,
                reserved_id,
            )
        });
        let state = InheritedInvocationState {
            engine_id: self.engine_id,
            cancellation,
            context,
            ancestry: telemetry
                .as_ref()
                .map(TelemetryState::hidden_spawn_context)
                .or_else(|| inherited.and_then(|state| state.ancestry.clone())),
            host_environment: inherited.map_or(0, |state| state.host_environment),
            hook_suppression: inherited.is_some_and(|state| state.hook_suppression),
        };
        Ok(HostInvocation {
            state,
            telemetry,
            frame,
            output: options.output == Some(true),
            error: options.error == Some(true),
            engine: Arc::clone(self),
            registration,
        })
    }
}

impl HostInvocation {
    pub fn inherited_state(&self) -> InheritedInvocationState {
        self.state.clone()
    }

    pub fn finish(self, outcome: InvocationOutcome) {
        self.finish_with_value(outcome, None);
    }

    pub fn wants_value(&self, outcome: InvocationOutcome) -> bool {
        if outcome == InvocationOutcome::Ok {
            self.output
        } else {
            self.error
        }
    }

    pub fn finish_with_value(
        mut self,
        outcome: InvocationOutcome,
        value: Option<&btel_snapshot::host::HostValue>,
    ) {
        if outcome == InvocationOutcome::Cancelled {
            self.state.cancellation.own().cancel();
        }
        if let Some(mut telemetry) = self.telemetry.take() {
            let _scope = telemetry.host_recording_scope();
            if let Some(frame) = self.frame.take() {
                let captured = value
                    .filter(|_| self.wants_value(outcome))
                    .and_then(|value| {
                        telemetry.capture_host_with(value, |name| {
                            self.engine.host_capture_declarations.get(name)
                        })
                    });
                telemetry.complete_host(frame, outcome, captured);
            }
            telemetry.complete_thread(outcome);
        }
    }
}

impl Drop for HostInvocation {
    fn drop(&mut self) {
        // Dropping an uncompleted owner cannot fabricate successful host exit.
        if let Some(telemetry) = self.telemetry.as_mut() {
            telemetry.abandon_host();
        }
        let _ = &self.registration;
    }
}

/// Only queued/running dispatch owns this object. Inherited state never does.
pub struct CallbackHostInvocation {
    inner: std::sync::Mutex<Option<CallbackExecution>>,
}

struct CallbackExecution {
    invocation: HostInvocation,
    outcome: Option<InvocationOutcome>,
    value: Option<btel_snapshot::host::HostValue>,
}

impl CallbackHostInvocation {
    pub(crate) fn new(invocation: HostInvocation) -> Self {
        Self {
            inner: std::sync::Mutex::new(Some(CallbackExecution {
                invocation,
                outcome: None,
                value: None,
            })),
        }
    }

    pub fn wants_value(&self, outcome: InvocationOutcome) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .is_some_and(|execution| execution.invocation.wants_value(outcome))
    }

    pub fn observe(
        &self,
        outcome: InvocationOutcome,
        value: Option<btel_snapshot::host::HostValue>,
    ) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(execution) = inner.as_mut()
            && execution.outcome.is_none()
        {
            execution.outcome = Some(outcome);
            execution.value = value;
        }
    }
}

impl Drop for CallbackHostInvocation {
    fn drop(&mut self) {
        if let Some(CallbackExecution {
            invocation,
            outcome: Some(outcome),
            value,
        }) = self
            .inner
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            invocation.finish_with_value(outcome, value.as_ref());
        }
        // Without a body outcome, abandon recording rather than fabricate success.
    }
}

pub(crate) fn resolve_callback_options(
    marker: &bex_vm_types::trace::TraceOptionsData,
    call: &bex_vm_types::trace::HostCallOptions,
) -> bex_vm_types::trace::TraceOptionsData {
    let mut context = marker.context.as_deref().cloned().unwrap_or_default();
    if let Some(patch) = &call.options.context {
        context.compose((**patch).clone());
    }
    let request = |declared: Option<bool>, explicit: Option<bool>| {
        if declared == Some(true) || explicit == Some(true) {
            Some(true)
        } else {
            explicit.or(declared)
        }
    };
    bex_vm_types::trace::TraceOptionsData {
        mode: if call.reserved_id.is_some() {
            Some(InvocationMode::Span)
        } else {
            call.options
                .mode
                .or(marker.mode)
                .or(Some(InvocationMode::Span))
        },
        inputs: request(marker.inputs, call.options.inputs),
        output: request(marker.output, call.options.output),
        error: request(marker.error, call.options.error),
        context: Some(Arc::new(context)),
    }
}

pub fn capture_callback_inputs(value: &crate::BexExternalValue) -> btel_snapshot::host::HostValue {
    use btel_snapshot::{
        Limit,
        host::{HostValue as H, MAX_BYTES, MAX_DEPTH, MAX_VALUES},
    };

    use crate::BexExternalValue as V;
    fn type_copy(
        value: &baml_type::RuntimeTy,
        depth: usize,
        nodes: &mut usize,
        bytes: &mut usize,
    ) -> btel_snapshot::host::HostType {
        use baml_type::RuntimeTy as T;
        use btel_snapshot::host::HostType as O;
        if depth > MAX_DEPTH || *nodes == 0 {
            return O::Unknown;
        }
        *nodes -= 1;
        match value {
            T::Int => O::Int,
            T::String => O::String,
            T::Bool => O::Bool,
            T::Float => O::Float,
            T::Null => O::Null,
            T::List(inner) => O::List(Box::new(type_copy(inner, depth + 1, nodes, bytes))),
            T::Map { key, value } => O::Map {
                key: Box::new(type_copy(key, depth + 1, nodes, bytes)),
                value: Box::new(type_copy(value, depth + 1, nodes, bytes)),
            },
            T::Union(options) if options.len() <= *nodes => O::Union(
                options
                    .iter()
                    .map(|ty| type_copy(ty, depth + 1, nodes, bytes))
                    .collect::<Vec<_>>()
                    .into(),
            ),
            T::Class(name, args) if args.len() <= *nodes => {
                let name = name.to_string();
                if name.len() > *bytes {
                    return O::Unknown;
                }
                *bytes -= name.len();
                O::Class(
                    name,
                    args.iter()
                        .map(|ty| type_copy(ty, depth + 1, nodes, bytes))
                        .collect::<Vec<_>>()
                        .into(),
                )
            }
            T::Enum(name) => {
                let name = name.to_string();
                if name.len() > *bytes {
                    return O::Unknown;
                }
                *bytes -= name.len();
                O::Enum(name)
            }
            _ => O::Unknown,
        }
    }
    fn copy(value: &V, depth: usize, nodes: &mut usize, bytes: &mut usize) -> H {
        if depth > MAX_DEPTH {
            return H::Truncated(Limit::Depth);
        }
        if *nodes == 0 {
            return H::Truncated(Limit::Values);
        }
        *nodes -= 1;
        match value {
            V::Null => H::Null,
            V::Bool(v) => H::Bool(*v),
            V::Int(v) => H::Int(*v),
            V::Float(v) => H::Float(*v),
            V::String(v) => {
                if v.len() > *bytes {
                    H::Truncated(Limit::Bytes)
                } else {
                    *bytes -= v.len();
                    H::String(v.as_str().to_owned())
                }
            }
            V::Union { value, .. } => copy(value, depth + 1, nodes, bytes),
            V::Array { items, .. } => {
                if items.len() > *nodes {
                    return H::Truncated(Limit::Values);
                }
                let mut values = Vec::new();
                for value in items {
                    if *nodes == 0 {
                        return H::Truncated(Limit::Values);
                    }
                    values.push(copy(value, depth + 1, nodes, bytes));
                }
                H::List(values)
            }
            V::Map { entries, .. } => {
                if entries.len() > *nodes {
                    return H::Truncated(Limit::Values);
                }
                let mut values = Vec::new();
                for (key, value) in entries {
                    if *nodes == 0 {
                        return H::Truncated(Limit::Values);
                    }
                    if key.len() > *bytes {
                        return H::Truncated(Limit::Bytes);
                    }
                    *bytes -= key.len();
                    values.push((key.clone(), copy(value, depth + 1, nodes, bytes)));
                }
                H::Map(values)
            }
            V::Instance {
                class_name,
                type_args,
                fields,
            } => {
                if class_name.len() > *bytes {
                    return H::Truncated(Limit::Bytes);
                }
                if fields.len().saturating_add(type_args.len()) > *nodes {
                    return H::Truncated(Limit::Values);
                }
                *bytes -= class_name.len();
                let mut values = Vec::new();
                for (key, value) in fields {
                    if key.len() > *bytes {
                        return H::Truncated(Limit::Bytes);
                    }
                    *bytes -= key.len();
                    values.push((key.clone(), copy(value, depth + 1, nodes, bytes)));
                }
                H::Instance {
                    name: class_name.clone(),
                    fields: values,
                    type_args: type_args
                        .iter()
                        .map(|ty| type_copy(ty, depth + 1, nodes, bytes))
                        .collect(),
                }
            }
            V::Variant {
                enum_name,
                variant_name,
            } => {
                if enum_name.len().saturating_add(variant_name.len()) > *bytes {
                    return H::Truncated(Limit::Bytes);
                }
                *bytes -= enum_name.len() + variant_name.len();
                H::Enum {
                    name: enum_name.clone(),
                    variant: variant_name.clone(),
                }
            }
            _ => H::Unavailable,
        }
    }
    let mut nodes = MAX_VALUES;
    let mut bytes = MAX_BYTES;
    copy(value, 0, &mut nodes, &mut bytes)
}

pub(crate) fn callback_entry_failed(error: &EngineError) {
    static DIAGNOSTICS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    if DIAGNOSTICS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < 8 {
        tracing::warn!(%error, "host trace entry failed");
    }
}
