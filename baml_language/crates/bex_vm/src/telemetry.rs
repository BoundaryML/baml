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
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use bex_vm_types::{Function, FunctionKind, FunctionMeta, HeapPtr, Value};
use btel_clock::ClockEpoch;
pub use btel_types::{
    AwaitDuration, CallPathEdge, CallPathId, ClockDuration, ClockInstant, InvocationMode,
    InvocationOutcome, TelemetryId,
};
use btel_types::{FunctionId, allocate_telemetry_id};
use rustc_hash::FxHashMap;

mod policy;
pub use policy::{TelemetryPolicies, TelemetryPolicy};

/// Spawn ancestry and the parent's immutable clock epoch. This is per-thread,
/// never per-frame; children must not silently select a different clock.
#[derive(Clone, Debug)]
pub struct ThreadSpawnContext {
    pub parent_id: TelemetryId,
    pub spawn_call_path: CallPathId,
    pub clock: Arc<ClockEpoch>,
}

impl PartialEq for ThreadSpawnContext {
    fn eq(&self, other: &Self) -> bool {
        self.parent_id == other.parent_id
            && self.spawn_call_path == other.spawn_call_path
            && self.clock.metadata().epoch == other.clock.metadata().epoch
    }
}

const SPAN: u8 = 1 << 0;
const CAPTURE_OUTPUT: u8 = 1 << 1;
const CAPTURE_ERROR: u8 = 1 << 2;
const REENTRY: u8 = 1 << 3;

static LAST_CALL_PATH_ID: AtomicU32 = AtomicU32::new(0);

fn allocate_call_path_id(last: &AtomicU32) -> CallPathId {
    // Exhaustion stays terminal even if a caller catches the panic. A wrapping
    // fetch_add could otherwise reuse live IDs after the first overflow.
    let previous = last
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .expect("call-path identity space exhausted");
    CallPathId::new_non_root(previous + 1).expect("call-path identity is nonzero")
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct CallPathKey {
    parent: CallPathId,
    visible_caller: Option<HeapPtr>,
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

#[derive(Clone, Debug)]
pub enum ProducerEvent {
    ThreadStarted {
        clock: Arc<ClockEpoch>,
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
        visible_caller: Option<FunctionId>,
        caller_pc: u32,
        callee: FunctionId,
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
    policies: Arc<TelemetryPolicies>,
    clock: Arc<ClockEpoch>,
    #[cfg(test)]
    events: Vec<ProducerEvent>,
}

impl TelemetryState {
    pub fn new_root(policies: Arc<TelemetryPolicies>, clock: Arc<ClockEpoch>) -> Self {
        clock.attach_thread();
        let id = allocate_telemetry_id();
        let started_at = clock.read();
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
            policies,
            clock,
            #[cfg(test)]
            events: Vec::new(),
        }
    }

    pub fn configure_spawn(&mut self, context: &ThreadSpawnContext) {
        assert!(
            Arc::ptr_eq(&self.clock, &context.clock),
            "child must inherit parent clock"
        );
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
            clock: Arc::clone(&self.clock),
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
        visible_caller: Option<HeapPtr>,
        caller_pc: u32,
        callee: HeapPtr,
        register: impl FnOnce(Option<HeapPtr>, HeapPtr) -> (Option<FunctionId>, FunctionId),
    ) -> ThreadSpawnContext {
        let spawn_call_path = self.resolve_call_path(
            CallPathKey {
                parent: self.thread.active_call_path,
                visible_caller,
                caller_pc,
                callee,
                edge: CallPathEdge::Spawn,
            },
            register,
        );
        ThreadSpawnContext {
            parent_id: self.thread.active_id,
            spawn_call_path,
            clock: Arc::clone(&self.clock),
        }
    }

    #[inline(always)]
    #[allow(
        clippy::too_many_arguments,
        reason = "entry data plus a statically dispatched cold registration hook"
    )]
    pub fn enter_bytecode(
        &mut self,
        function: &Function,
        callee: HeapPtr,
        visible_caller: Option<HeapPtr>,
        caller_pc: u32,
        caller_is_observed: bool,
        args: &[Value],
        register: impl FnOnce(Option<HeapPtr>, HeapPtr) -> (Option<FunctionId>, FunctionId),
    ) -> Option<FrameTelemetry> {
        if function.telemetry_function_id.is_none()
            || !matches!(function.kind, FunctionKind::Bytecode)
        {
            return None;
        }

        let policy_id = function.telemetry_policy_id.load();
        let is_ai = matches!(function.body_meta.as_ref(), Some(FunctionMeta::Llm { .. }));
        if policy_id == btel_types::TelemetryPolicyId::NONE && !is_ai {
            return Some(self.enter_timing(
                callee,
                visible_caller,
                caller_pc,
                caller_is_observed,
                register,
            ));
        }

        let policy = self.policy_by_id(policy_id);
        let mode = default_mode(function, policy);
        if mode == InvocationMode::Hidden {
            return None;
        }

        let saved_call_path = self.thread.active_call_path;
        let reentry = caller_is_observed
            && visible_caller == Some(callee)
            && self
                .call_path_keys
                .get(&saved_call_path)
                .is_some_and(|key| key.callee == callee);
        let call_path = if reentry {
            saved_call_path
        } else {
            self.resolve_call_path(
                CallPathKey {
                    parent: saved_call_path,
                    visible_caller,
                    caller_pc,
                    callee,
                    edge: CallPathEdge::Synchronous,
                },
                register,
            )
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
        let entered_at = self.clock.read();
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
        visible_caller: Option<HeapPtr>,
        caller_pc: u32,
        caller_is_observed: bool,
        register: impl FnOnce(Option<HeapPtr>, HeapPtr) -> (Option<FunctionId>, FunctionId),
    ) -> FrameTelemetry {
        let saved_call_path = self.thread.active_call_path;
        let reentry = caller_is_observed
            && visible_caller == Some(callee)
            && self
                .call_path_keys
                .get(&saved_call_path)
                .is_some_and(|key| key.callee == callee);
        let call_path = if reentry {
            saved_call_path
        } else {
            self.resolve_call_path(
                CallPathKey {
                    parent: saved_call_path,
                    visible_caller,
                    caller_pc,
                    callee,
                    edge: CallPathEdge::Synchronous,
                },
                register,
            )
        };
        let flags = if reentry { REENTRY } else { 0 };
        let entered_at = self.clock.read();
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
        let exited_at = self.clock.read();
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
        } else if policy.promotes(elapsed, outcome, self.clock.domain()) {
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
            completed_at: self.clock.read(),
            outcome,
        });
        self.clock.finish_thread();
    }

    pub fn clock(&self) -> &Arc<ClockEpoch> {
        &self.clock
    }
    pub fn is_root_thread(&self) -> bool {
        self.thread.parent_id.is_none()
    }

    #[inline(always)]
    fn policy_by_id(&self, policy_id: u16) -> TelemetryPolicy {
        self.policies.get(policy_id)
    }

    // Kept internal until the configuration API is designed. Native policies
    // are immutable; only bytecode definitions may publish a new policy.
    #[allow(
        dead_code,
        reason = "external policy configuration is deliberately deferred"
    )]
    pub(crate) fn set_policy(
        &self,
        function: &Function,
        policy: TelemetryPolicy,
    ) -> Result<(), &'static str> {
        if function.telemetry_function_id.is_none() {
            return Err("telemetry is unsupported for this function");
        }
        if !matches!(function.kind, FunctionKind::Bytecode) {
            return Err("native telemetry policies are immutable");
        }
        self.policies.publish(&function.telemetry_policy_id, policy)
    }

    #[inline(always)]
    fn resolve_call_path(
        &mut self,
        key: CallPathKey,
        register: impl FnOnce(Option<HeapPtr>, HeapPtr) -> (Option<FunctionId>, FunctionId),
    ) -> CallPathId {
        if let Some((cached_key, id)) = self.last_call_path
            && cached_key == key
        {
            return id;
        }
        self.resolve_call_path_slow(key, register)
    }

    #[cold]
    #[inline(never)]
    fn resolve_call_path_slow(
        &mut self,
        key: CallPathKey,
        register: impl FnOnce(Option<HeapPtr>, HeapPtr) -> (Option<FunctionId>, FunctionId),
    ) -> CallPathId {
        if let Some(id) = self.call_paths.get(&key) {
            let id = *id;
            self.last_call_path = Some((key, id));
            return id;
        }
        // Registration precedes publishing any definition that references it.
        // Existing paths return above without even loading a registration flag.
        let (visible_caller, callee) = register(key.visible_caller, key.callee);
        let id = allocate_call_path_id(&LAST_CALL_PATH_ID);
        self.call_paths.insert(key, id);
        self.call_path_keys.insert(id, key);
        self.last_call_path = Some((key, id));
        self.emit(ProducerEvent::CallPathDefined {
            call_path: id,
            parent_call_path: key.parent,
            visible_caller,
            caller_pc: key.caller_pc,
            callee,
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
            roots.extend(key.visible_caller);
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
                ProducerEvent::CallPathDefined { .. }
                | ProducerEvent::ThreadStarted { .. }
                | ProducerEvent::ThreadCompleted { .. }
                | ProducerEvent::Timing { .. } => {}
            }
        }
    }

    pub(crate) fn forward_roots(&mut self, roots: &HashMap<HeapPtr, HeapPtr>) {
        let old_paths = std::mem::take(&mut self.call_paths);
        self.call_path_keys.clear();
        for (mut key, id) in old_paths {
            if let Some(caller) = key.visible_caller {
                key.visible_caller = Some(roots.get(&caller).copied().unwrap_or(caller));
            }
            key.callee = roots.get(&key.callee).copied().unwrap_or(key.callee);
            self.call_paths.insert(key, id);
            self.call_path_keys.insert(id, key);
        }
        self.last_call_path = self.last_call_path.map(|(mut key, id)| {
            if let Some(caller) = key.visible_caller {
                key.visible_caller = Some(roots.get(&caller).copied().unwrap_or(caller));
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
                ProducerEvent::CallPathDefined { .. }
                | ProducerEvent::ThreadStarted { .. }
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
    fn test_clock() -> std::sync::Arc<btel_clock::ClockEpoch> {
        btel_clock::ClockRuntime::new(btel_clock::ClockMode::Monotonic).start_run()
    }

    use super::*;

    #[test]
    fn call_path_exhaustion_cannot_resume_with_reused_ids() {
        assert_eq!(allocate_call_path_id(&AtomicU32::new(0)).get(), 1);
        let last = AtomicU32::new(u32::MAX - 1);
        assert_eq!(allocate_call_path_id(&last).get(), u32::MAX);
        for _ in 0..2 {
            assert!(std::panic::catch_unwind(|| allocate_call_path_id(&last)).is_err());
            assert_eq!(last.load(Ordering::Relaxed), u32::MAX);
        }
    }

    #[test]
    fn call_path_definitions_survive_relocation_and_collection() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let caller = vm
            .tlab
            .alloc_function(Box::new(function(FunctionKind::Bytecode, None)))
            .unwrap();
        let callee = vm
            .tlab
            .alloc_function(Box::new(function(FunctionKind::Bytecode, None)))
            .unwrap();
        let mut state = TelemetryState::new_root(Arc::new(TelemetryPolicies::new()), test_clock());
        let register = |caller: Option<HeapPtr>, callee| unsafe {
            // SAFETY: this single-threaded test owns the live functions and
            // never collects while resolving a call path.
            (
                caller.map(|ptr| vm.heap.register_telemetry_function(ptr).unwrap()),
                vm.heap.register_telemetry_function(callee).unwrap(),
            )
        };
        let key = CallPathKey {
            parent: CallPathId::ROOT,
            visible_caller: Some(caller),
            caller_pc: 7,
            callee,
            edge: CallPathEdge::Synchronous,
        };
        let path = state.resolve_call_path(key, register);
        assert_eq!(
            state.resolve_call_path(key, |_, _| panic!("last-path hit registered")),
            path
        );
        let spawn = state.spawn_context(Some(caller), 11, callee, register);
        assert_ne!(spawn.spawn_call_path, path);
        assert_eq!(
            state.resolve_call_path(key, |_, _| panic!("cached path registered")),
            path
        );

        let definitions = |state: &TelemetryState| {
            state
                .events()
                .iter()
                .filter_map(|event| match event {
                    ProducerEvent::CallPathDefined {
                        call_path,
                        parent_call_path,
                        visible_caller,
                        caller_pc,
                        callee,
                        edge,
                    } => Some((
                        *call_path,
                        *parent_call_path,
                        *visible_caller,
                        *caller_pc,
                        *callee,
                        *edge,
                    )),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let before = definitions(&state);
        assert_eq!(before.len(), 2);
        let caller_id = before[0].2.unwrap();
        let callee_id = before[0].4;
        assert_ne!(caller_id, callee_id);
        assert_eq!(before[1].2, Some(caller_id));
        assert_eq!(before[1].4, callee_id);
        assert_eq!(before[1].5, CallPathEdge::Spawn);
        assert!(vm.heap.function_metadata(vm.proof(), caller_id).is_some());
        assert!(vm.heap.function_metadata(vm.proof(), callee_id).is_some());

        let mut roots = Vec::new();
        state.collect_roots(&mut roots);
        // SAFETY: the test has exclusive heap access and supplies every live
        // cache pointer. No bytecode is running and no frames/captures exist.
        let (_, _, forwarding) = unsafe {
            vm.heap
                .collect_garbage_generational(&roots, bex_heap::CollectionLevel::Major)
        };
        state.forward_roots(&forwarding);
        assert_eq!(definitions(&state), before);
        let moved = CallPathKey {
            visible_caller: Some(forwarding[&caller]),
            callee: forwarding[&callee],
            ..key
        };
        assert_eq!(
            state.resolve_call_path(moved, |_, _| panic!("moved cache registered")),
            path
        );
        assert!(vm.heap.function_metadata(vm.proof(), callee_id).is_some());

        // Drop only the pointer cache, retaining the actual emitted events.
        // Definitions must not keep executable objects alive or need forwarding.
        state.call_paths.clear();
        state.call_path_keys.clear();
        state.last_call_path = None;
        roots.clear();
        state.collect_roots(&mut roots);
        assert!(roots.is_empty());
        // SAFETY: as above; the test has discarded all live heap references.
        let (stats, _, forwarding) = unsafe {
            vm.heap
                .collect_garbage_generational(&roots, bex_heap::CollectionLevel::Major)
        };
        state.forward_roots(&forwarding);
        assert_eq!(stats.live_count, 0);
        assert!(vm.heap.function_metadata(vm.proof(), caller_id).is_none());
        assert!(vm.heap.function_metadata(vm.proof(), callee_id).is_none());
        assert_eq!(definitions(&state), before);
    }

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
            telemetry_function_id: Some(
                btel_types::FunctionIdAllocator::default()
                    .allocate()
                    .unwrap(),
            ),
            telemetry_registration: bex_vm_types::FunctionRegistration::default(),
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
            origin: bex_vm_types::FunctionOrigin::Internal,
            is_interface_body: false,
            native_key: None,
            body_meta,
            runtime_package: HeapPtr::null(),
        }
    }

    #[test]
    fn unsupported_function_cannot_be_enabled_or_create_a_call_path() {
        let mut function = function(FunctionKind::Bytecode, None);
        function.telemetry_function_id = None;
        let mut state = TelemetryState::new_root(Arc::new(TelemetryPolicies::new()), test_clock());
        let policy = TelemetryPolicy {
            span_from_entry: true,
            ..TelemetryPolicy::NONE
        };
        assert!(state.set_policy(&function, policy).is_err());
        // Even an incorrectly assigned policy cannot override capability.
        state
            .policies
            .publish(&function.telemetry_policy_id, policy)
            .unwrap();
        assert!(
            state
                .enter_bytecode(
                    &function,
                    HeapPtr::null(),
                    None,
                    0,
                    false,
                    &[],
                    |_, _| panic!("unsupported function tried to register")
                )
                .is_none()
        );
        assert!(state.events().is_empty());
        assert!(state.call_paths.is_empty());
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn vm_frame_sizes_stay_within_budget() {
        assert_eq!(size_of::<crate::vm::BytecodeFrame>(), 96);
        assert_eq!(size_of::<crate::vm::NativeFrame>(), 32);
        assert_eq!(size_of::<crate::vm::Frame>(), 96);
    }

    #[test]
    fn timing_completion_is_anonymous_and_exclusive() {
        let function = function(FunctionKind::Bytecode, None);
        let mut state = TelemetryState::new_root(Arc::new(TelemetryPolicies::new()), test_clock());
        state.start_thread();
        let thread_id = state.active_id();
        let frame = state
            .enter_bytecode(&function, HeapPtr::null(), None, 7, false, &[], |_, _| {
                (None, function.telemetry_function_id.unwrap())
            })
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
        let mut state = TelemetryState::new_root(Arc::new(TelemetryPolicies::new()), test_clock());
        state.start_thread();
        let parent_id = state.active_id();
        let frame = state
            .enter_bytecode(
                &function,
                HeapPtr::null(),
                None,
                0,
                false,
                &[Value::int(3)],
                |_, _| (None, function.telemetry_function_id.unwrap()),
            )
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
        let mut state = TelemetryState::new_root(Arc::new(TelemetryPolicies::new()), test_clock());
        state.start_thread();
        let parent_id = state.active_id();
        let frame = state
            .enter_bytecode(&function, HeapPtr::null(), None, 0, false, &[], |_, _| {
                (None, function.telemetry_function_id.unwrap())
            })
            .unwrap();
        let updater =
            TelemetryState::new_root(Arc::clone(&state.policies), Arc::clone(&state.clock));
        updater
            .set_policy(
                &function,
                TelemetryPolicy {
                    span_from_entry: false,
                    promote_after: Some(state.clock.threshold(std::time::Duration::ZERO)),
                    promote_errors: false,
                    capture_inputs: false,
                    capture_output: true,
                    capture_error: false,
                },
            )
            .unwrap();

        // A VM created after publication sees the same table as an existing VM.
        let later = TelemetryState::new_root(Arc::clone(&state.policies), Arc::clone(&state.clock));
        assert_eq!(
            later.policy_by_id(function.telemetry_policy_id.load()),
            state.policy_by_id(function.telemetry_policy_id.load())
        );
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
    fn completions_preserve_self_await_and_reentry() {
        let function = function(FunctionKind::Bytecode, None);
        let mut state = TelemetryState::new_root(Arc::new(TelemetryPolicies::new()), test_clock());
        state.start_thread();
        let mut outer = state
            .enter_bytecode(&function, HeapPtr::null(), None, 0, false, &[], |_, _| {
                (None, function.telemetry_function_id.unwrap())
            })
            .unwrap();
        let mut inner = state
            .enter_bytecode(
                &function,
                HeapPtr::null(),
                Some(HeapPtr::null()),
                1,
                true,
                &[],
                |_, _| (None, function.telemetry_function_id.unwrap()),
            )
            .unwrap();
        outer.add_await(ClockDuration::from_ticks(3));
        inner.add_await(ClockDuration::from_ticks(7));

        state.complete_invocation(inner, &function, InvocationOutcome::Ok, None);
        state.complete_invocation(outer, &function, InvocationOutcome::Ok, None);

        let measurements: Vec<_> = state
            .events()
            .iter()
            .filter_map(|event| match event {
                ProducerEvent::Timing {
                    await_time,
                    reentry,
                    ..
                } => Some((await_time.get().get(), *reentry)),
                _ => None,
            })
            .collect();
        // Completion must carry each invocation's own wait, without rolling
        // the child's duration into its parent or losing reentry attribution.
        assert_eq!(measurements, [(7, true), (3, false)]);
    }

    #[test]
    fn native_policy_updates_are_rejected() {
        let state = TelemetryState::new_root(Arc::new(TelemetryPolicies::new()), test_clock());
        let function = function(FunctionKind::NativeUnresolved, None);
        assert!(
            state
                .set_policy(
                    &function,
                    TelemetryPolicy {
                        span_from_entry: true,
                        ..TelemetryPolicy::NONE
                    }
                )
                .is_err()
        );
        assert_eq!(
            function.telemetry_policy_id.load(),
            btel_types::TelemetryPolicyId::NONE
        );
    }

    #[test]
    fn entry_capture_survives_policy_updates_for_output_and_error() {
        for outcome in [InvocationOutcome::Ok, InvocationOutcome::Errored] {
            for initial in [false, true] {
                for current in [false, true] {
                    let function = function(FunctionKind::Bytecode, None);
                    let mut state =
                        TelemetryState::new_root(Arc::new(TelemetryPolicies::new()), test_clock());
                    state.start_thread();
                    let policy = |capture| TelemetryPolicy {
                        span_from_entry: true,
                        capture_output: capture,
                        capture_error: capture,
                        ..TelemetryPolicy::NONE
                    };
                    state.set_policy(&function, policy(initial)).unwrap();
                    let frame = state
                        .enter_bytecode(&function, HeapPtr::null(), None, 0, false, &[], |_, _| {
                            (None, function.telemetry_function_id.unwrap())
                        })
                        .unwrap();
                    // Updating to zero must not erase an entry requirement.
                    state
                        .set_policy(
                            &function,
                            if current {
                                policy(true)
                            } else {
                                TelemetryPolicy::NONE
                            },
                        )
                        .unwrap();
                    state.complete_invocation(frame, &function, outcome, Some(Value::int(7)));
                    let Some(ProducerEvent::Span {
                        captured_value,
                        outcome: recorded,
                        ..
                    }) = state.events().last()
                    else {
                        panic!("an entry-selected span must remain a span");
                    };
                    assert_eq!(*recorded, outcome);
                    assert_eq!(
                        *captured_value,
                        (initial || current).then_some(Value::int(7))
                    );
                }
            }
        }
    }

    #[test]
    fn spawned_thread_has_explicit_parent_and_structural_call_path() {
        let mut parent = TelemetryState::new_root(Arc::new(TelemetryPolicies::new()), test_clock());
        parent.start_thread();
        let function = function(FunctionKind::Bytecode, None);
        let context = parent.spawn_context(None, 11, HeapPtr::null(), |_, _| {
            (None, function.telemetry_function_id.unwrap())
        });

        let mut child =
            TelemetryState::new_root(Arc::clone(&parent.policies), Arc::clone(&parent.clock));
        let child_id = child.active_id();
        child.configure_spawn(&context);
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
