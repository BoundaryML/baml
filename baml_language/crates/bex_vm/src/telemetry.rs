//! VM-side telemetry producer state.
//!
//! This module intentionally stops at logical events. Production builds consume
//! and discard those values; the record buffer and transport are a later layer.

#![allow(unsafe_code)]
#![allow(
    clippy::inline_always,
    reason = "producer fast paths are benchmarked and keep cold work out of line"
)]

use std::{
    collections::HashMap,
    mem::size_of,
    sync::atomic::{AtomicU32, Ordering},
};

use bex_vm_types::{Function, FunctionKind, FunctionMeta, FunctionOrigin, HeapPtr, Value};
use btel_types::allocate_telemetry_id;
pub use btel_types::{
    AwaitDuration, CallPathEdge, CallPathId, ClockDuration, ClockInstant, InvocationMode,
    InvocationOutcome, TelemetryId, ThreadSpawnContext,
};
use rustc_hash::FxHashMap;

const SPAN: u8 = 1 << 0;
const CAPTURE_OUTPUT: u8 = 1 << 1;
const CAPTURE_ERROR: u8 = 1 << 2;
const REENTRY: u8 = 1 << 3;

static NEXT_CALL_PATH_ID: AtomicU32 = AtomicU32::new(1);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct CallPathKey {
    parent: CallPathId,
    actual_caller: Option<HeapPtr>,
    caller_pc: u32,
    callee: HeapPtr,
    edge: CallPathEdge,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FrameTelemetry {
    pub entered_at: ClockInstant,
    pub saved_parent_id: TelemetryId,
    pub await_duration: AwaitDuration,
    pub saved_call_path: CallPathId,
    flags: u8,
}

const _: () = assert!(size_of::<FrameTelemetry>() == 32);
const _: () = assert!(size_of::<Option<FrameTelemetry>>() == 32);

impl FrameTelemetry {
    #[inline(always)]
    pub const fn is_span(self) -> bool {
        self.flags & SPAN != 0
    }

    #[inline(always)]
    pub const fn is_reentry(self) -> bool {
        self.flags & REENTRY != 0
    }

    #[inline(always)]
    pub fn add_await(&mut self, elapsed: ClockDuration) {
        self.await_duration = self.await_duration.saturating_add(elapsed);
    }
}

#[derive(Clone, Copy, Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "resolved policy flags are independent and intentionally direct"
)]
pub struct TelemetryPolicy {
    pub span_from_entry: bool,
    pub promote_after: Option<ClockDuration>,
    pub promote_errors: bool,
    pub capture_inputs: bool,
    pub capture_output: bool,
    pub capture_error: bool,
}

impl TelemetryPolicy {
    pub const NONE: Self = Self {
        span_from_entry: false,
        promote_after: None,
        promote_errors: false,
        capture_inputs: false,
        capture_output: false,
        capture_error: false,
    };

    #[inline(always)]
    fn promotes(self, elapsed: ClockDuration, outcome: InvocationOutcome) -> bool {
        self.span_from_entry
            || self.promote_errors && outcome == InvocationOutcome::Errored
            || self
                .promote_after
                .is_some_and(|threshold| elapsed >= threshold)
    }
}

#[derive(Clone, Debug)]
pub enum ProducerEvent {
    ThreadStarted {
        id: TelemetryId,
        parent_id: Option<TelemetryId>,
        spawn_call_path: CallPathId,
        started_at: ClockInstant,
    },
    ThreadCompleted {
        id: TelemetryId,
        started_at: ClockInstant,
        completed_at: ClockInstant,
        outcome: InvocationOutcome,
    },
    SpanStarted {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        captured_inputs: Box<[Value]>,
    },
    Timing {
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        outcome: InvocationOutcome,
        reentry: bool,
    },
    Span {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        outcome: InvocationOutcome,
        reentry: bool,
        captured_value: Option<Value>,
    },
    LateSpan {
        id: TelemetryId,
        parent_id: TelemetryId,
        call_path: CallPathId,
        entered_at: ClockInstant,
        exited_at: ClockInstant,
        await_time: AwaitDuration,
        outcome: InvocationOutcome,
        reentry: bool,
        captured_value: Option<Value>,
    },
    CallPathDefined {
        call_path: CallPathId,
        parent_call_path: CallPathId,
        actual_caller: Option<HeapPtr>,
        caller_pc: u32,
        callee: HeapPtr,
        edge: CallPathEdge,
    },
}

#[derive(Debug)]
pub struct ThreadTelemetry {
    pub active_id: TelemetryId,
    pub active_call_path: CallPathId,
    id: TelemetryId,
    parent_id: Option<TelemetryId>,
    spawn_call_path: CallPathId,
    started_at: ClockInstant,
    started: bool,
    completed: bool,
}

pub struct TelemetryState {
    thread: ThreadTelemetry,
    call_paths: FxHashMap<CallPathKey, CallPathId>,
    call_path_keys: FxHashMap<CallPathId, CallPathKey>,
    last_call_path: Option<(CallPathKey, CallPathId)>,
    policies: Vec<TelemetryPolicy>,
    #[cfg(test)]
    events: Vec<ProducerEvent>,
}

impl TelemetryState {
    pub fn new_root() -> Self {
        let id = allocate_telemetry_id();
        let started_at = ClockInstant::now();
        Self {
            thread: ThreadTelemetry {
                active_id: id,
                active_call_path: CallPathId::ROOT,
                id,
                parent_id: None,
                spawn_call_path: CallPathId::ROOT,
                started_at,
                started: false,
                completed: false,
            },
            call_paths: FxHashMap::default(),
            call_path_keys: FxHashMap::default(),
            last_call_path: None,
            policies: vec![TelemetryPolicy::NONE],
            #[cfg(test)]
            events: Vec::new(),
        }
    }

    pub fn configure_spawn(&mut self, context: ThreadSpawnContext) {
        assert!(!self.thread.started, "telemetry thread already started");
        self.thread.parent_id = Some(context.parent_id);
        self.thread.spawn_call_path = context.spawn_call_path;
        self.thread.active_call_path = context.spawn_call_path;
    }

    pub fn start_thread(&mut self) {
        if self.thread.started {
            return;
        }
        self.thread.started = true;
        self.emit(ProducerEvent::ThreadStarted {
            id: self.thread.id,
            parent_id: self.thread.parent_id,
            spawn_call_path: self.thread.spawn_call_path,
            started_at: self.thread.started_at,
        });
    }

    #[inline(always)]
    pub fn active_id(&self) -> TelemetryId {
        self.thread.active_id
    }

    pub fn spawn_context(
        &mut self,
        actual_caller: Option<HeapPtr>,
        caller_pc: u32,
        callee: HeapPtr,
    ) -> ThreadSpawnContext {
        let spawn_call_path = self.resolve_call_path(CallPathKey {
            parent: self.thread.active_call_path,
            actual_caller,
            caller_pc,
            callee,
            edge: CallPathEdge::Spawn,
        });
        ThreadSpawnContext {
            parent_id: self.thread.active_id,
            spawn_call_path,
        }
    }

    #[inline(always)]
    pub fn enter_bytecode(
        &mut self,
        function: &Function,
        callee: HeapPtr,
        actual_caller: Option<HeapPtr>,
        caller_pc: u32,
        caller_is_observed: bool,
        args: &[Value],
    ) -> Option<FrameTelemetry> {
        if !matches!(function.kind, FunctionKind::Bytecode)
            || function.origin == FunctionOrigin::Internal
        {
            return None;
        }

        let policy_id = function.telemetry_policy_id.load();
        let is_ai = matches!(function.body_meta.as_ref(), Some(FunctionMeta::Llm { .. }));
        if policy_id == btel_types::TelemetryPolicyId::NONE && !is_ai {
            return Some(self.enter_timing(callee, actual_caller, caller_pc, caller_is_observed));
        }

        let policy = self.policy_by_id(policy_id);
        let mode = default_mode(function, policy);
        if mode == InvocationMode::Hidden {
            return None;
        }

        let saved_call_path = self.thread.active_call_path;
        let reentry = caller_is_observed
            && actual_caller == Some(callee)
            && self
                .call_path_keys
                .get(&saved_call_path)
                .is_some_and(|key| key.callee == callee);
        let call_path = if reentry {
            saved_call_path
        } else {
            self.resolve_call_path(CallPathKey {
                parent: saved_call_path,
                actual_caller,
                caller_pc,
                callee,
                edge: CallPathEdge::Synchronous,
            })
        };

        let saved_parent_id = self.thread.active_id;
        let mut flags = if reentry { REENTRY } else { 0 };
        if mode == InvocationMode::Span {
            flags |= SPAN;
        }
        if policy.capture_output || is_ai {
            flags |= CAPTURE_OUTPUT;
        }
        if policy.capture_error || is_ai {
            flags |= CAPTURE_ERROR;
        }

        let captured_inputs = (mode == InvocationMode::Span && (policy.capture_inputs || is_ai))
            .then(|| args.to_vec().into_boxed_slice());
        let entered_at = ClockInstant::now();
        if mode == InvocationMode::Span {
            let id = allocate_telemetry_id();
            self.thread.active_id = id;
            self.emit(ProducerEvent::SpanStarted {
                id,
                parent_id: saved_parent_id,
                call_path,
                entered_at,
                captured_inputs: captured_inputs.unwrap_or_default(),
            });
        }
        self.thread.active_call_path = call_path;

        Some(FrameTelemetry {
            entered_at,
            saved_parent_id,
            await_duration: AwaitDuration::ZERO,
            saved_call_path,
            flags,
        })
    }

    #[inline(always)]
    fn enter_timing(
        &mut self,
        callee: HeapPtr,
        actual_caller: Option<HeapPtr>,
        caller_pc: u32,
        caller_is_observed: bool,
    ) -> FrameTelemetry {
        let saved_call_path = self.thread.active_call_path;
        let reentry = caller_is_observed
            && actual_caller == Some(callee)
            && self
                .call_path_keys
                .get(&saved_call_path)
                .is_some_and(|key| key.callee == callee);
        let call_path = if reentry {
            saved_call_path
        } else {
            self.resolve_call_path(CallPathKey {
                parent: saved_call_path,
                actual_caller,
                caller_pc,
                callee,
                edge: CallPathEdge::Synchronous,
            })
        };
        let flags = if reentry { REENTRY } else { 0 };
        let entered_at = ClockInstant::now();
        self.thread.active_call_path = call_path;
        FrameTelemetry {
            entered_at,
            saved_parent_id: self.thread.active_id,
            await_duration: AwaitDuration::ZERO,
            saved_call_path,
            flags,
        }
    }

    #[inline(always)]
    pub fn complete_invocation(
        &mut self,
        telemetry: FrameTelemetry,
        function: &Function,
        outcome: InvocationOutcome,
        value: Option<Value>,
    ) {
        let exited_at = ClockInstant::now();
        let call_path = self.thread.active_call_path;
        let policy_id = function.telemetry_policy_id.load();
        if !telemetry.is_span() && policy_id == btel_types::TelemetryPolicyId::NONE {
            self.emit(ProducerEvent::Timing {
                call_path,
                entered_at: telemetry.entered_at,
                exited_at,
                await_time: telemetry.await_duration,
                outcome,
                reentry: telemetry.is_reentry(),
            });
            self.thread.active_call_path = telemetry.saved_call_path;
            return;
        }

        let elapsed = telemetry.entered_at.elapsed_until(exited_at);
        let policy = self.policy_by_id(policy_id);
        let capture_value = match outcome {
            InvocationOutcome::Ok
                if telemetry.flags & CAPTURE_OUTPUT != 0 || policy.capture_output =>
            {
                value
            }
            InvocationOutcome::Errored
                if telemetry.flags & CAPTURE_ERROR != 0 || policy.capture_error =>
            {
                value
            }
            _ => None,
        };

        if telemetry.is_span() {
            let id = self.thread.active_id;
            self.emit(ProducerEvent::Span {
                id,
                parent_id: telemetry.saved_parent_id,
                call_path,
                entered_at: telemetry.entered_at,
                exited_at,
                await_time: telemetry.await_duration,
                outcome,
                reentry: telemetry.is_reentry(),
                captured_value: capture_value,
            });
            self.thread.active_id = telemetry.saved_parent_id;
        } else if policy.promotes(elapsed, outcome) {
            self.emit(ProducerEvent::LateSpan {
                id: allocate_telemetry_id(),
                parent_id: telemetry.saved_parent_id,
                call_path,
                entered_at: telemetry.entered_at,
                exited_at,
                await_time: telemetry.await_duration,
                outcome,
                reentry: telemetry.is_reentry(),
                captured_value: capture_value,
            });
        } else {
            self.emit(ProducerEvent::Timing {
                call_path,
                entered_at: telemetry.entered_at,
                exited_at,
                await_time: telemetry.await_duration,
                outcome,
                reentry: telemetry.is_reentry(),
            });
        }
        self.thread.active_call_path = telemetry.saved_call_path;
    }

    pub fn complete_thread(&mut self, outcome: InvocationOutcome) {
        if self.thread.completed {
            return;
        }
        self.start_thread();
        self.thread.completed = true;
        self.emit(ProducerEvent::ThreadCompleted {
            id: self.thread.id,
            started_at: self.thread.started_at,
            completed_at: ClockInstant::now(),
            outcome,
        });
    }

    #[inline(always)]
    fn policy_by_id(&self, policy_id: u16) -> TelemetryPolicy {
        let id = usize::from(policy_id);
        self.policies
            .get(id)
            .copied()
            .unwrap_or(TelemetryPolicy::NONE)
    }

    #[cfg(test)]
    pub(crate) fn install_policy(&mut self, policy: TelemetryPolicy) -> u16 {
        let id = u16::try_from(self.policies.len()).expect("telemetry policy table exhausted");
        self.policies.push(policy);
        id
    }

    #[inline(always)]
    fn resolve_call_path(&mut self, key: CallPathKey) -> CallPathId {
        if let Some((cached_key, id)) = self.last_call_path
            && cached_key == key
        {
            return id;
        }
        self.resolve_call_path_slow(key)
    }

    #[cold]
    #[inline(never)]
    fn resolve_call_path_slow(&mut self, key: CallPathKey) -> CallPathId {
        if let Some(id) = self.call_paths.get(&key) {
            let id = *id;
            self.last_call_path = Some((key, id));
            return id;
        }
        let raw = NEXT_CALL_PATH_ID.fetch_add(1, Ordering::Relaxed);
        assert_ne!(raw, 0, "call-path identity space exhausted");
        let id = CallPathId::new_non_root(raw).expect("call-path identity is nonzero");
        self.call_paths.insert(key, id);
        self.call_path_keys.insert(id, key);
        self.last_call_path = Some((key, id));
        self.emit(ProducerEvent::CallPathDefined {
            call_path: id,
            parent_call_path: key.parent,
            actual_caller: key.actual_caller,
            caller_pc: key.caller_pc,
            callee: key.callee,
            edge: key.edge,
        });
        id
    }

    #[cfg(test)]
    #[inline(always)]
    fn emit(&mut self, event: ProducerEvent) {
        self.events.push(event);
    }

    #[cfg(not(test))]
    #[inline(always)]
    fn emit(&mut self, event: ProducerEvent) {
        let _ = self;
        drop(event);
    }

    #[cfg(test)]
    pub(crate) fn events(&self) -> &[ProducerEvent] {
        &self.events
    }

    pub(crate) fn collect_roots(&self, roots: &mut Vec<HeapPtr>) {
        for key in self.call_paths.keys() {
            roots.extend(key.actual_caller);
            roots.push(key.callee);
        }
        #[cfg(test)]
        for event in &self.events {
            match event {
                ProducerEvent::SpanStarted {
                    captured_inputs, ..
                } => roots.extend(captured_inputs.iter().filter_map(Value::as_object_ptr)),
                ProducerEvent::Span { captured_value, .. }
                | ProducerEvent::LateSpan { captured_value, .. } => {
                    roots.extend(captured_value.iter().filter_map(Value::as_object_ptr));
                }
                ProducerEvent::CallPathDefined {
                    actual_caller,
                    callee,
                    ..
                } => {
                    roots.extend(*actual_caller);
                    roots.push(*callee);
                }
                ProducerEvent::ThreadStarted { .. }
                | ProducerEvent::ThreadCompleted { .. }
                | ProducerEvent::Timing { .. } => {}
            }
        }
    }

    pub(crate) fn forward_roots(&mut self, roots: &HashMap<HeapPtr, HeapPtr>) {
        let old_paths = std::mem::take(&mut self.call_paths);
        self.call_path_keys.clear();
        for (mut key, id) in old_paths {
            if let Some(caller) = key.actual_caller {
                key.actual_caller = Some(roots.get(&caller).copied().unwrap_or(caller));
            }
            key.callee = roots.get(&key.callee).copied().unwrap_or(key.callee);
            self.call_paths.insert(key, id);
            self.call_path_keys.insert(id, key);
        }
        self.last_call_path = self.last_call_path.map(|(mut key, id)| {
            if let Some(caller) = key.actual_caller {
                key.actual_caller = Some(roots.get(&caller).copied().unwrap_or(caller));
            }
            key.callee = roots.get(&key.callee).copied().unwrap_or(key.callee);
            (key, id)
        });
        #[cfg(test)]
        for event in &mut self.events {
            match event {
                ProducerEvent::SpanStarted {
                    captured_inputs, ..
                } => {
                    for value in captured_inputs {
                        forward_value(value, roots);
                    }
                }
                ProducerEvent::Span { captured_value, .. }
                | ProducerEvent::LateSpan { captured_value, .. } => {
                    if let Some(value) = captured_value {
                        forward_value(value, roots);
                    }
                }
                ProducerEvent::CallPathDefined {
                    actual_caller,
                    callee,
                    ..
                } => {
                    if let Some(caller) = actual_caller {
                        *caller = roots.get(caller).copied().unwrap_or(*caller);
                    }
                    *callee = roots.get(callee).copied().unwrap_or(*callee);
                }
                ProducerEvent::ThreadStarted { .. }
                | ProducerEvent::ThreadCompleted { .. }
                | ProducerEvent::Timing { .. } => {}
            }
        }
    }
}

#[cfg(test)]
fn forward_value(value: &mut Value, roots: &HashMap<HeapPtr, HeapPtr>) {
    if let Some(ptr) = value.as_object_ptr()
        && let Some(&forwarded) = roots.get(&ptr)
    {
        *value = Value::object(forwarded);
    }
}

#[inline(always)]
fn default_mode(function: &Function, policy: TelemetryPolicy) -> InvocationMode {
    match function.kind {
        FunctionKind::Native(_) | FunctionKind::SysOp(_) | FunctionKind::NativeUnresolved => {
            InvocationMode::Hidden
        }
        FunctionKind::Bytecode if function.origin == FunctionOrigin::Internal => {
            InvocationMode::Hidden
        }
        FunctionKind::Bytecode
            if policy.span_from_entry
                || matches!(function.body_meta.as_ref(), Some(FunctionMeta::Llm { .. })) =>
        {
            InvocationMode::Span
        }
        FunctionKind::Bytecode => InvocationMode::Timing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn function(kind: FunctionKind, body_meta: Option<FunctionMeta>) -> Function {
        Function {
            name: "telemetry_test".to_string(),
            source_file: String::new(),
            docstring: None,
            declared_name: Some("telemetry_test".to_string()),
            arity: 0,
            real_local_count: 0,
            bytecode: bex_vm_types::Bytecode::default(),
            kind,
            telemetry_policy_id: btel_types::TelemetryPolicyId::none(),
            local_names: Vec::new(),
            debug_locals: Vec::new(),
            span: baml_type::Span::fake(),
            return_type: bex_vm_types::TyTemplate::Null {
                attr: baml_type::TyAttr::default(),
            },
            param_names: Vec::new(),
            param_types: Vec::new(),
            param_has_default: Vec::new(),
            display_type_params: Vec::new(),
            generic_param_bounds: Vec::new(),
            display_param_types: Vec::new(),
            display_return_type: "null".to_string(),
            throws_type: bex_vm_types::TyTemplate::Never {
                attr: baml_type::TyAttr::default(),
            },
            origin: FunctionOrigin::UserDefined,
            is_interface_body: false,
            native_key: None,
            body_meta,
            runtime_package: HeapPtr::null(),
        }
    }

    #[test]
    fn frame_layout_is_32_bytes() {
        assert_eq!(size_of::<FrameTelemetry>(), 32);
        assert_eq!(size_of::<Option<FrameTelemetry>>(), 32);
        #[cfg(target_pointer_width = "64")]
        {
            assert_eq!(size_of::<crate::vm::BytecodeFrame>(), 96);
            assert_eq!(size_of::<crate::vm::NativeFrame>(), 24);
            assert_eq!(size_of::<crate::vm::Frame>(), 96);
        }
    }

    #[test]
    fn hidden_native_has_no_invocation_lifecycle() {
        let function = function(FunctionKind::NativeUnresolved, None);
        let mut state = TelemetryState::new_root();
        state.start_thread();
        let before = state.events().len();

        assert!(
            state
                .enter_bytecode(&function, HeapPtr::null(), None, 0, false, &[])
                .is_none()
        );
        assert_eq!(state.events().len(), before);
    }

    #[test]
    fn timing_completion_is_anonymous_and_exclusive() {
        let function = function(FunctionKind::Bytecode, None);
        let mut state = TelemetryState::new_root();
        state.start_thread();
        let thread_id = state.active_id();
        let frame = state
            .enter_bytecode(&function, HeapPtr::null(), None, 7, false, &[])
            .unwrap();

        assert!(!frame.is_span());
        assert_eq!(state.active_id(), thread_id);
        state.complete_invocation(frame, &function, InvocationOutcome::Ok, Some(Value::NULL));

        assert_eq!(
            state
                .events()
                .iter()
                .filter(|event| matches!(event, ProducerEvent::Timing { .. }))
                .count(),
            1
        );
        assert!(!state.events().iter().any(|event| matches!(
            event,
            ProducerEvent::Span { .. } | ProducerEvent::LateSpan { .. }
        )));
    }

    #[test]
    fn ai_function_is_a_span_from_entry() {
        let function = function(
            FunctionKind::Bytecode,
            Some(FunctionMeta::Llm {
                client: "test".to_string(),
            }),
        );
        let mut state = TelemetryState::new_root();
        state.start_thread();
        let parent_id = state.active_id();
        let frame = state
            .enter_bytecode(&function, HeapPtr::null(), None, 0, false, &[Value::int(3)])
            .unwrap();
        let span_id = state.active_id();

        assert!(frame.is_span());
        assert_ne!(span_id, parent_id);
        state.complete_invocation(frame, &function, InvocationOutcome::Ok, Some(Value::int(5)));
        assert_eq!(state.active_id(), parent_id);
        assert!(state.events().iter().any(|event| matches!(
            event,
            ProducerEvent::SpanStarted { id, parent_id: parent, captured_inputs, .. }
                if *id == span_id
                    && *parent == parent_id
                    && captured_inputs.as_ref() == [Value::int(3)]
        )));
        assert!(state.events().iter().any(|event| matches!(
            event,
            ProducerEvent::Span { id, captured_value: Some(value), .. }
                if *id == span_id && *value == Value::int(5)
        )));
    }

    #[test]
    fn completion_reads_current_policy_for_late_promotion() {
        let function = function(FunctionKind::Bytecode, None);
        let mut state = TelemetryState::new_root();
        state.start_thread();
        let parent_id = state.active_id();
        let frame = state
            .enter_bytecode(&function, HeapPtr::null(), None, 0, false, &[])
            .unwrap();
        let policy_id = state.install_policy(TelemetryPolicy {
            span_from_entry: false,
            promote_after: Some(ClockDuration::ZERO),
            promote_errors: false,
            capture_inputs: false,
            capture_output: true,
            capture_error: false,
        });
        function.telemetry_policy_id.store(policy_id);

        state.complete_invocation(frame, &function, InvocationOutcome::Ok, Some(Value::int(9)));

        assert_eq!(state.active_id(), parent_id);
        assert!(state.events().iter().any(|event| matches!(
            event,
            ProducerEvent::LateSpan {
                parent_id: parent,
                captured_value: Some(value),
                ..
            } if *parent == parent_id && *value == Value::int(9)
        )));
        assert!(
            !state
                .events()
                .iter()
                .any(|event| matches!(event, ProducerEvent::Timing { .. }))
        );
    }

    #[test]
    fn await_duration_is_invocation_local() {
        let function = function(FunctionKind::Bytecode, None);
        let mut state = TelemetryState::new_root();
        state.start_thread();
        let mut outer = state
            .enter_bytecode(&function, HeapPtr::null(), None, 0, false, &[])
            .unwrap();
        let mut inner = state
            .enter_bytecode(
                &function,
                HeapPtr::null(),
                Some(HeapPtr::null()),
                1,
                true,
                &[],
            )
            .unwrap();
        outer.add_await(ClockDuration::from_ticks(3));
        inner.add_await(ClockDuration::from_ticks(7));

        assert_eq!(outer.await_duration.get().get(), 3);
        assert_eq!(inner.await_duration.get().get(), 7);
        assert!(inner.is_reentry());
    }

    #[test]
    fn spawned_thread_has_explicit_parent_and_structural_call_path() {
        let mut parent = TelemetryState::new_root();
        parent.start_thread();
        let context = parent.spawn_context(None, 11, HeapPtr::null());

        let mut child = TelemetryState::new_root();
        let child_id = child.active_id();
        child.configure_spawn(context);
        child.start_thread();

        assert_ne!(child_id, context.parent_id);
        assert!(child.events().iter().any(|event| matches!(
            event,
            ProducerEvent::ThreadStarted {
                id,
                parent_id: Some(parent_id),
                spawn_call_path,
                ..
            } if *id == child_id
                && *parent_id == context.parent_id
                && *spawn_call_path == context.spawn_call_path
        )));
    }
}
