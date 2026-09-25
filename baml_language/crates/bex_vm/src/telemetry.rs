//! VM-side telemetry producer state.
//!
//! Native execution publishes typed records through chunk-backed transport.
//! Captures own frozen pooled graphs independent of the VM heap.
//! WASM transport integration is deferred; tests retain the same owned records.

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
use btel_settings::mode::AutoTelemetryLevel;
pub use btel_types::{
    AwaitDuration, CallPathEdge, CallPathId, ClockDuration, ClockInstant, InvocationMode,
    InvocationOutcome, TelemetryId,
};
use btel_types::{FunctionId, allocate_telemetry_id};
use rustc_hash::FxHashMap;

mod errors;
pub(crate) use errors::{ActiveHandler, ErrorBook, FrameScope, LandingMatch};
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

use btel_settings::policy::{
    AI_DEFAULT_MODE, BYTECODE_DEFAULT_MODE, CALL_PATH_CACHE_SLOTS, NATIVE_DEFAULT_MODE,
};
// The resolver below uses the measured single-entry fast path.
const _: () = assert!(CALL_PATH_CACHE_SLOTS == 1);
// Native instrumentation is unsupported; changing this requires a producer implementation.
const _: () = assert!(matches!(NATIVE_DEFAULT_MODE, InvocationMode::Hidden));

const SPAN: u8 = 1 << 0;
const CAPTURE_OUTPUT: u8 = 1 << 1;
const CAPTURE_ERROR: u8 = 1 << 2;
const REENTRY: u8 = 1 << 3;
const REQUIRES_ANNOUNCEMENT: u8 = 1 << 4;

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

const _: () = assert!(size_of::<FrameTelemetry>() == btel_settings::layout::FRAME_TELEMETRY_BYTES);
const _: () =
    assert!(size_of::<Option<FrameTelemetry>>() == btel_settings::layout::FRAME_TELEMETRY_BYTES);

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

pub use btel_records::{SpanRecord, TimingRecord};

mod snapshot;
type VmSpanRecord = SpanRecord<btel_snapshot::Snapshot, btel_snapshot::Snapshot>;
const _: () = assert!(size_of::<TimingRecord>() <= btel_settings::layout::TIMING_RECORD_MAX_BYTES);
const _: () = assert!(size_of::<VmSpanRecord>() <= btel_settings::layout::SPAN_RECORD_MAX_BYTES);

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
    capture_scratch: snapshot::Scratch,
    #[cfg(target_arch = "wasm32")]
    snapshots: btel_snapshot::SnapshotPool,
    auto_level: AutoTelemetryLevel,
    thread: ThreadTelemetry,
    call_paths: FxHashMap<CallPathKey, CallPathId>,
    call_path_keys: FxHashMap<CallPathId, CallPathKey>,
    last_call_path: [Option<(CallPathKey, CallPathId)>; CALL_PATH_CACHE_SLOTS],
    policies: Arc<TelemetryPolicies>,
    clock: Arc<ClockEpoch>,
    #[cfg(not(target_arch = "wasm32"))]
    runtime: Arc<btel_processor::TelemetryRuntime>,
    /// Exception-path bookkeeping, allocated by the first recorded raise.
    errors: Option<Box<ErrorBook>>,
    #[cfg(test)]
    timing_records: Vec<TimingRecord>,
    #[cfg(test)]
    span_records: Vec<VmSpanRecord>,
}

impl TelemetryState {
    pub fn new_root(
        policies: Arc<TelemetryPolicies>,
        clock: Arc<ClockEpoch>,
        #[cfg(not(target_arch = "wasm32"))] runtime: Arc<btel_processor::TelemetryRuntime>,
    ) -> Self {
        clock.attach_thread();
        let id = allocate_telemetry_id();
        let started_at = clock.read();
        Self {
            capture_scratch: snapshot::Scratch::default(),
            #[cfg(target_arch = "wasm32")]
            snapshots: btel_snapshot::SnapshotPool::new(
                btel_settings::snapshot::MIN_SNAPSHOT_SLOTS,
                btel_snapshot::Limits::default(),
            ),
            auto_level: policies.auto_level(),
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
            last_call_path: [None; CALL_PATH_CACHE_SLOTS],
            policies,
            clock,
            #[cfg(not(target_arch = "wasm32"))]
            runtime,
            errors: None,
            #[cfg(test)]
            timing_records: Vec::new(),
            #[cfg(test)]
            span_records: Vec::new(),
        }
    }

    #[cold]
    fn capture(&mut self, input: snapshot::Input<'_>) -> Option<btel_snapshot::Snapshot> {
        #[cfg(not(target_arch = "wasm32"))]
        let builder = self.runtime.acquire_snapshot();
        #[cfg(target_arch = "wasm32")]
        let builder = self.snapshots.try_acquire();
        #[cfg(target_arch = "wasm32")]
        assert!(builder.is_some(), "synchronous WASM capture consumption");
        // SAFETY: invocation entry/completion run with the VM heap permit held.
        // Scratch traversal never releases it or triggers VM allocation/GC.
        builder.map(|builder| unsafe { self.capture_scratch.capture(builder, input) })
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
        self.write_span(SpanRecord::ThreadSpanAnnouncement {
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

    /// The innermost observed frame's call path; `ROOT` outside any.
    pub(crate) fn active_call_path(&self) -> CallPathId {
        self.thread.active_call_path
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
    /// # Safety
    /// Every argument and its reachable objects must remain live under the VM
    /// heap permit throughout this call. Capture may traverse that graph.
    pub unsafe fn enter_bytecode(
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
        if policy_id == btel_types::TelemetryPolicyId::NONE {
            if self.auto_level == AutoTelemetryLevel::Low {
                return None;
            }
            if self.auto_level == AutoTelemetryLevel::Medium
                && !matches!(function.body_meta.as_ref(), Some(FunctionMeta::Llm { .. }))
            {
                return Some(self.enter_timing(
                    callee,
                    visible_caller,
                    caller_pc,
                    caller_is_observed,
                    register,
                ));
            }
        }
        let policy = self.policy_by_id(policy_id);
        let mode = if policy_id == btel_types::TelemetryPolicyId::NONE {
            InvocationMode::Span
        } else {
            default_mode(function, policy)
        };

        if mode == InvocationMode::Hidden {
            return None;
        }

        let is_ai = matches!(function.body_meta.as_ref(), Some(FunctionMeta::Llm { .. }));
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
        if policy.capture_output || (is_ai && btel_settings::policy::AI_CAPTURE_OUTPUT) {
            flags |= CAPTURE_OUTPUT;
        }
        if policy.capture_error || (is_ai && btel_settings::policy::AI_CAPTURE_ERROR) {
            flags |= CAPTURE_ERROR;
        }

        let captured_inputs = (mode == InvocationMode::Span
            && (policy.capture_inputs || (is_ai && btel_settings::policy::AI_CAPTURE_INPUTS)))
            .then(|| self.capture(snapshot::Input::FunctionArgs(args)))
            .flatten();
        if captured_inputs.is_some() {
            flags |= REQUIRES_ANNOUNCEMENT;
        }
        let entered_at = self.clock.read();
        if mode == InvocationMode::Span {
            let id = allocate_telemetry_id();
            self.thread.active_id = id;
            self.write_span(SpanRecord::FunctionSpanAnnouncement {
                id,
                parent_id: saved_parent_id,
                call_path,
                entered_at,
                captured_inputs,
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
    /// # Safety
    /// The result/error and its reachable objects must remain live under the VM
    /// heap permit throughout this call. Capture may traverse that graph.
    pub unsafe fn complete_invocation(
        &mut self,
        telemetry: FrameTelemetry,
        function: &Function,
        outcome: InvocationOutcome,
        value: Option<Value>,
    ) {
        let exited_at = self.clock.read();
        // SAFETY: forwarded from the caller.
        unsafe { self.complete_invocation_at(telemetry, function, outcome, value, exited_at) }
    }

    /// `complete_invocation` with an exit instant this thread already read
    /// from its clock: the frames one unwind pops share the raise's instant.
    ///
    /// # Safety
    /// As `complete_invocation`.
    #[inline(always)]
    pub unsafe fn complete_invocation_at(
        &mut self,
        telemetry: FrameTelemetry,
        function: &Function,
        outcome: InvocationOutcome,
        value: Option<Value>,
        exited_at: ClockInstant,
    ) {
        let call_path = self.thread.active_call_path;
        let policy_id = function.telemetry_policy_id.load();
        if !telemetry.is_span() && policy_id == btel_types::TelemetryPolicyId::NONE {
            self.write_timing(TimingRecord::FunctionTimingCompletion {
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
            let captured_value =
                capture_value.and_then(|value| self.capture(snapshot::Input::Value(value)));
            let id = self.thread.active_id;
            match (
                outcome,
                telemetry.is_reentry(),
                telemetry.flags & REQUIRES_ANNOUNCEMENT != 0,
            ) {
                (InvocationOutcome::Ok, false, false) => {
                    self.write_span(SpanRecord::FunctionSpanCompletionOk {
                        id,
                        parent_id: telemetry.saved_parent_id,
                        call_path,
                        entered_at: telemetry.entered_at,
                        exited_at,
                        await_time: telemetry.await_duration,
                        captured_value,
                    });
                }
                (InvocationOutcome::Ok, false, true) => {
                    self.write_span(SpanRecord::FunctionSpanCompletionOkNeedsAnnouncement {
                        id,
                        parent_id: telemetry.saved_parent_id,
                        call_path,
                        entered_at: telemetry.entered_at,
                        exited_at,
                        await_time: telemetry.await_duration,
                        captured_value,
                    });
                }
                (InvocationOutcome::Ok, true, false) => {
                    self.write_span(SpanRecord::FunctionSpanCompletionOkReentry {
                        id,
                        parent_id: telemetry.saved_parent_id,
                        call_path,
                        entered_at: telemetry.entered_at,
                        exited_at,
                        await_time: telemetry.await_duration,
                        captured_value,
                    });
                }
                (InvocationOutcome::Ok, true, true) => self.write_span(
                    SpanRecord::FunctionSpanCompletionOkReentryNeedsAnnouncement {
                        id,
                        parent_id: telemetry.saved_parent_id,
                        call_path,
                        entered_at: telemetry.entered_at,
                        exited_at,
                        await_time: telemetry.await_duration,
                        captured_value,
                    },
                ),
                (InvocationOutcome::Errored, false, false) => {
                    self.write_span(SpanRecord::FunctionSpanCompletionErrored {
                        id,
                        parent_id: telemetry.saved_parent_id,
                        call_path,
                        entered_at: telemetry.entered_at,
                        exited_at,
                        await_time: telemetry.await_duration,
                        captured_value,
                    });
                }
                (InvocationOutcome::Errored, false, true) => {
                    self.write_span(SpanRecord::FunctionSpanCompletionErroredNeedsAnnouncement {
                        id,
                        parent_id: telemetry.saved_parent_id,
                        call_path,
                        entered_at: telemetry.entered_at,
                        exited_at,
                        await_time: telemetry.await_duration,
                        captured_value,
                    });
                }
                (InvocationOutcome::Errored, true, false) => {
                    self.write_span(SpanRecord::FunctionSpanCompletionErroredReentry {
                        id,
                        parent_id: telemetry.saved_parent_id,
                        call_path,
                        entered_at: telemetry.entered_at,
                        exited_at,
                        await_time: telemetry.await_duration,
                        captured_value,
                    });
                }
                (InvocationOutcome::Errored, true, true) => self.write_span(
                    SpanRecord::FunctionSpanCompletionErroredReentryNeedsAnnouncement {
                        id,
                        parent_id: telemetry.saved_parent_id,
                        call_path,
                        entered_at: telemetry.entered_at,
                        exited_at,
                        await_time: telemetry.await_duration,
                        captured_value,
                    },
                ),
                (InvocationOutcome::Cancelled, false, false) => {
                    self.write_span(SpanRecord::FunctionSpanCompletionCancelled {
                        id,
                        parent_id: telemetry.saved_parent_id,
                        call_path,
                        entered_at: telemetry.entered_at,
                        exited_at,
                        await_time: telemetry.await_duration,
                        captured_value,
                    });
                }
                (InvocationOutcome::Cancelled, false, true) => self.write_span(
                    SpanRecord::FunctionSpanCompletionCancelledNeedsAnnouncement {
                        id,
                        parent_id: telemetry.saved_parent_id,
                        call_path,
                        entered_at: telemetry.entered_at,
                        exited_at,
                        await_time: telemetry.await_duration,
                        captured_value,
                    },
                ),
                (InvocationOutcome::Cancelled, true, false) => {
                    self.write_span(SpanRecord::FunctionSpanCompletionCancelledReentry {
                        id,
                        parent_id: telemetry.saved_parent_id,
                        call_path,
                        entered_at: telemetry.entered_at,
                        exited_at,
                        await_time: telemetry.await_duration,
                        captured_value,
                    });
                }
                (InvocationOutcome::Cancelled, true, true) => self.write_span(
                    SpanRecord::FunctionSpanCompletionCancelledReentryNeedsAnnouncement {
                        id,
                        parent_id: telemetry.saved_parent_id,
                        call_path,
                        entered_at: telemetry.entered_at,
                        exited_at,
                        await_time: telemetry.await_duration,
                        captured_value,
                    },
                ),
            }
            self.thread.active_id = telemetry.saved_parent_id;
        } else if policy::promotes(policy, elapsed, outcome, self.clock.domain()) {
            let captured_value =
                capture_value.and_then(|value| self.capture(snapshot::Input::Value(value)));
            match (outcome, telemetry.is_reentry()) {
                (InvocationOutcome::Ok, false) => {
                    self.write_span(SpanRecord::LateFunctionSpanCompletionOk {
                        id: allocate_telemetry_id(),
                        parent_id: telemetry.saved_parent_id,
                        call_path,
                        entered_at: telemetry.entered_at,
                        exited_at,
                        await_time: telemetry.await_duration,
                        captured_value,
                    });
                }
                (InvocationOutcome::Ok, true) => {
                    self.write_span(SpanRecord::LateFunctionSpanCompletionOkReentry {
                        id: allocate_telemetry_id(),
                        parent_id: telemetry.saved_parent_id,
                        call_path,
                        entered_at: telemetry.entered_at,
                        exited_at,
                        await_time: telemetry.await_duration,
                        captured_value,
                    });
                }
                (InvocationOutcome::Errored, false) => {
                    self.write_span(SpanRecord::LateFunctionSpanCompletionErrored {
                        id: allocate_telemetry_id(),
                        parent_id: telemetry.saved_parent_id,
                        call_path,
                        entered_at: telemetry.entered_at,
                        exited_at,
                        await_time: telemetry.await_duration,
                        captured_value,
                    });
                }
                (InvocationOutcome::Errored, true) => {
                    self.write_span(SpanRecord::LateFunctionSpanCompletionErroredReentry {
                        id: allocate_telemetry_id(),
                        parent_id: telemetry.saved_parent_id,
                        call_path,
                        entered_at: telemetry.entered_at,
                        exited_at,
                        await_time: telemetry.await_duration,
                        captured_value,
                    });
                }
                (InvocationOutcome::Cancelled, false) => {
                    self.write_span(SpanRecord::LateFunctionSpanCompletionCancelled {
                        id: allocate_telemetry_id(),
                        parent_id: telemetry.saved_parent_id,
                        call_path,
                        entered_at: telemetry.entered_at,
                        exited_at,
                        await_time: telemetry.await_duration,
                        captured_value,
                    });
                }
                (InvocationOutcome::Cancelled, true) => {
                    self.write_span(SpanRecord::LateFunctionSpanCompletionCancelledReentry {
                        id: allocate_telemetry_id(),
                        parent_id: telemetry.saved_parent_id,
                        call_path,
                        entered_at: telemetry.entered_at,
                        exited_at,
                        await_time: telemetry.await_duration,
                        captured_value,
                    });
                }
            }
        } else {
            self.write_timing(TimingRecord::FunctionTimingCompletion {
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
        self.write_span(SpanRecord::ThreadSpanCompletion {
            id: self.thread.id,
            parent_id: self.thread.parent_id,
            spawn_call_path: self.thread.spawn_call_path,
            clock: Arc::clone(&self.clock),
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
        if let Some((cached_key, id)) = self.last_call_path[0]
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
            self.last_call_path[0] = Some((key, id));
            return id;
        }
        // Registration precedes publishing any definition that references it.
        // Existing paths return above without even loading a registration flag.
        let (visible_caller, callee) = register(key.visible_caller, key.callee);
        let id = allocate_call_path_id(&LAST_CALL_PATH_ID);
        self.call_paths.insert(key, id);
        self.call_path_keys.insert(id, key);
        self.last_call_path[0] = Some((key, id));
        self.write_span(SpanRecord::CallPathDefined {
            call_path: id,
            parent_call_path: key.parent,
            visible_caller,
            caller_pc: key.caller_pc,
            callee,
            edge: key.edge,
        });
        id
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn is_disabled(&self) -> bool {
        self.runtime.is_disabled()
    }

    /// Whether raises should be recorded: a recording publisher is attached
    /// and recording is still enabled. Exception path only.
    #[cold]
    pub(crate) fn error_evidence(&self) -> bool {
        #[cfg(test)]
        if FORCE_ERROR_EVIDENCE.with(std::cell::Cell::get) {
            return true;
        }
        #[cfg(not(target_arch = "wasm32"))]
        return self.runtime.wants_error_evidence();
        #[cfg(target_arch = "wasm32")]
        false
    }

    #[cold]
    pub(crate) fn error_book(&mut self) -> &mut ErrorBook {
        self.errors.get_or_insert_with(Box::default)
    }

    /// Clear notes when a fresh top-level run starts on an empty stack.
    pub(crate) fn clear_error_book(&mut self) {
        if let Some(book) = &mut self.errors {
            book.clear();
        }
    }

    pub(crate) fn error_book_ref(&self) -> Option<&ErrorBook> {
        self.errors.as_deref()
    }

    pub(crate) fn take_escaped_error(&mut self) -> Option<btel_records::FutureErrorLink> {
        self.errors.as_mut().and_then(|book| book.take_escaped())
    }

    /// Emit an exception-path record for this thread.
    #[cold]
    pub(crate) fn write_error_record(&mut self, record: VmSpanRecord) {
        self.write_span(record);
    }

    /// The raise that failed `future`, stored by the thread that settled it.
    pub(crate) fn future_error(&self, future: u64) -> btel_records::FutureErrorLookup {
        #[cfg(not(target_arch = "wasm32"))]
        return self.runtime.future_error(future);
        #[cfg(target_arch = "wasm32")]
        {
            let _ = future;
            btel_records::FutureErrorLookup::Missing
        }
    }

    pub(crate) fn link_future_error(&self, future: u64, link: btel_records::FutureErrorLink) {
        #[cfg(not(target_arch = "wasm32"))]
        self.runtime.link_future_error(future, link);
        #[cfg(target_arch = "wasm32")]
        let _ = (future, link);
    }

    /// Release clock bookkeeping without claiming that this thread completed.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn abandon(&mut self) {
        if !self.thread.completed {
            self.thread.completed = true;
            self.clock.finish_thread();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn execution_scope(&self) -> btel_processor::ExecutionScope {
        self.runtime.enter()
    }

    // Tests retain VM-backed captures locally. Native production writes only
    // owned metadata/explicit deferred-capture markers to the processor.
    #[cfg_attr(
        all(not(test), target_arch = "wasm32"),
        allow(
            clippy::needless_pass_by_value,
            reason = "WASM transport integration is deferred"
        )
    )]
    #[inline(always)]
    fn write_timing(&mut self, record: TimingRecord) {
        #[cfg(test)]
        self.timing_records.push(record);
        #[cfg(all(not(test), not(target_arch = "wasm32")))]
        self.runtime.write_timing(self.thread.id, record);
        #[cfg(all(not(test), target_arch = "wasm32"))]
        let _ = (self, record);
    }

    #[inline(always)]
    fn write_span(&mut self, record: VmSpanRecord) {
        #[cfg(test)]
        self.span_records.push(record);
        #[cfg(all(not(test), not(target_arch = "wasm32")))]
        self.runtime.write_span(self.thread.id, record);
        #[cfg(all(not(test), target_arch = "wasm32"))]
        {
            let _ = self;
            drop(record);
        }
    }

    #[cfg(test)]
    pub(crate) fn timing_records(&self) -> &[TimingRecord] {
        &self.timing_records
    }

    #[cfg(test)]
    pub(crate) fn span_records(&self) -> &[VmSpanRecord] {
        &self.span_records
    }

    pub(crate) fn collect_roots(&self, roots: &mut Vec<HeapPtr>) {
        for key in self.call_paths.keys() {
            roots.extend(key.visible_caller);
            roots.push(key.callee);
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
        self.last_call_path[0] = self.last_call_path[0].map(|(mut key, id)| {
            if let Some(caller) = key.visible_caller {
                key.visible_caller = Some(roots.get(&caller).copied().unwrap_or(caller));
            }
            key.callee = roots.get(&key.callee).copied().unwrap_or(key.callee);
            (key, id)
        });
    }
}

#[inline(always)]
fn default_mode(function: &Function, policy: TelemetryPolicy) -> InvocationMode {
    match function.kind {
        FunctionKind::Native(_) | FunctionKind::SysOp(_) | FunctionKind::NativeUnresolved => {
            NATIVE_DEFAULT_MODE
        }
        FunctionKind::Bytecode if policy.span_from_entry => InvocationMode::Span,
        FunctionKind::Bytecode
            if matches!(function.body_meta.as_ref(), Some(FunctionMeta::Llm { .. })) =>
        {
            AI_DEFAULT_MODE
        }
        FunctionKind::Bytecode => BYTECODE_DEFAULT_MODE,
    }
}

#[cfg(test)]
thread_local! {
    /// Unit tests keep records locally; this makes their VMs build raises
    /// even though the shared test runtime has no recording publisher.
    pub(crate) static FORCE_ERROR_EVIDENCE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(all(test, not(target_arch = "wasm32")))]
pub(crate) fn test_runtime() -> Arc<btel_processor::TelemetryRuntime> {
    static RUNTIME: std::sync::OnceLock<Arc<btel_processor::TelemetryRuntime>> =
        std::sync::OnceLock::new();
    Arc::clone(RUNTIME.get_or_init(|| btel_processor::TelemetryRuntime::new().unwrap()))
}

#[cfg(test)]
mod tests {
    fn test_clock() -> std::sync::Arc<btel_clock::ClockEpoch> {
        btel_clock::ClockRuntime::new(btel_clock::ClockMode::Monotonic).start_run()
    }

    use super::*;

    fn test_state(policies: Arc<TelemetryPolicies>, clock: Arc<ClockEpoch>) -> TelemetryState {
        TelemetryState::new_root(
            policies,
            clock,
            #[cfg(not(target_arch = "wasm32"))]
            test_runtime(),
        )
    }

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
        let mut state = test_state(Arc::new(TelemetryPolicies::new()), test_clock());
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
                .span_records()
                .iter()
                .filter_map(|event| match event {
                    SpanRecord::CallPathDefined {
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
        state.last_call_path[0] = None;
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

    #[test]
    fn owned_captures_survive_collection_without_being_gc_roots() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let function = function(
            FunctionKind::Bytecode,
            Some(FunctionMeta::Llm {
                client: "test".into(),
            }),
        );
        let input = vm.tlab.alloc_string("input");
        let output = vm.tlab.alloc_string("output");
        let mut state = test_state(Arc::new(TelemetryPolicies::new()), test_clock());
        let frame = unsafe {
            state.enter_bytecode(
                &function,
                HeapPtr::null(),
                None,
                0,
                false,
                &[Value::object(input)],
                |_, _| (None, function.telemetry_function_id.unwrap()),
            )
        }
        .unwrap();
        unsafe {
            state.complete_invocation(
                frame,
                &function,
                InvocationOutcome::Ok,
                Some(Value::object(output)),
            );
        };
        // Isolate capture roots from the separate executable call-path cache.
        state.call_paths.clear();
        state.call_path_keys.clear();
        state.last_call_path[0] = None;
        let mut roots = Vec::new();
        state.collect_roots(&mut roots);
        assert!(roots.is_empty());
        // SAFETY: exclusive test heap, and the retained captures are its only
        // live objects. No bytecode or other VM/heap mutation runs concurrently.
        let (_, _, forwarding) = unsafe {
            vm.heap
                .collect_garbage_generational(&roots, bex_heap::CollectionLevel::Major)
        };
        state.forward_roots(&forwarding);
        roots.clear();
        state.collect_roots(&mut roots);
        assert!(roots.is_empty());
        drop(vm);
        assert!(state.span_records().iter().any(|record| matches!(record,
            SpanRecord::FunctionSpanAnnouncement { captured_inputs: Some(capture), .. }
            if matches!(capture.roots()[0], btel_snapshot::SnapshotValue::String(s) if capture.string(s).as_str() == "input"))));
        assert!(state.span_records().iter().any(|record| matches!(record,
            SpanRecord::FunctionSpanCompletionOkNeedsAnnouncement { captured_value: Some(capture), .. }
            if matches!(capture.roots()[0], btel_snapshot::SnapshotValue::String(s) if capture.string(s).as_str() == "output"))));
        drop(state);
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
            return_type: bex_vm_types::TyTemplate::Null,
            param_names: Vec::new(),
            param_types: Vec::new(),
            param_has_default: Vec::new(),
            display_type_params: Vec::new(),
            generic_param_bounds: Vec::new(),
            display_param_types: Vec::new(),
            display_return_type: "null".to_string(),
            throws_type: bex_vm_types::TyTemplate::Never,
            origin: bex_vm_types::FunctionOrigin::Internal,
            is_interface_body: false,
            native_key: None,
            body_meta,
            runtime_package: HeapPtr::null(),
        }
    }

    #[test]
    fn argument_layout_is_absent_unless_declaration_names_cover_every_slot() {
        let mut declared = function(FunctionKind::Bytecode, None);
        declared.arity = 2;
        declared.param_names = vec!["self".into(), "customer".into()];
        let layout = declared
            .runtime_metadata()
            .unwrap()
            .argument_layout
            .unwrap();
        assert_eq!(
            layout
                .slots
                .iter()
                .map(|slot| (slot.name.as_deref(), slot.receiver))
                .collect::<Vec<_>>(),
            [(Some("self"), true), (Some("customer"), false)]
        );
        // A synthesized body with slots but no declaration names: unknown,
        // never a guessed or empty layout.
        let mut synthesized = function(FunctionKind::Bytecode, None);
        synthesized.arity = 1;
        assert_eq!(
            synthesized.runtime_metadata().unwrap().argument_layout,
            None
        );
        let empty = function(FunctionKind::Bytecode, None);
        assert_eq!(
            empty.runtime_metadata().unwrap().argument_layout,
            Some(btel_types::ArgumentLayout::default())
        );
    }

    #[test]
    fn unsupported_function_cannot_be_enabled_or_create_a_call_path() {
        let mut function = function(FunctionKind::Bytecode, None);
        function.telemetry_function_id = None;
        let mut state = test_state(Arc::new(TelemetryPolicies::new()), test_clock());
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
            unsafe {
                state.enter_bytecode(&function, HeapPtr::null(), None, 0, false, &[], |_, _| {
                    panic!("unsupported function tried to register")
                })
            }
            .is_none()
        );
        assert!(state.span_records().is_empty());
        assert!(state.timing_records().is_empty());
        assert!(state.call_paths.is_empty());
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn vm_frame_sizes_stay_within_budget() {
        // Heap-debug adds an epoch to HeapPtr; the rest of each frame stays fixed.
        let pointer_bytes = size_of::<HeapPtr>();
        assert!(matches!(pointer_bytes, 8 | 16));
        assert_eq!(size_of::<crate::vm::BytecodeFrame>(), 88 + pointer_bytes);
        assert_eq!(size_of::<crate::vm::NativeFrame>(), 24 + pointer_bytes);
        assert_eq!(size_of::<crate::vm::Frame>(), 88 + pointer_bytes);
    }

    #[test]
    fn timing_completion_is_anonymous_and_exclusive() {
        let function = function(FunctionKind::Bytecode, None);
        let mut state = test_state(Arc::new(TelemetryPolicies::new()), test_clock());
        state.start_thread();
        let thread_id = state.active_id();
        let frame = unsafe {
            state.enter_bytecode(&function, HeapPtr::null(), None, 7, false, &[], |_, _| {
                (None, function.telemetry_function_id.unwrap())
            })
        }
        .unwrap();

        assert!(!frame.is_span());
        assert_eq!(state.active_id(), thread_id);
        unsafe {
            state.complete_invocation(frame, &function, InvocationOutcome::Ok, Some(Value::NULL));
        };

        assert_eq!(
            state
                .timing_records()
                .iter()
                .filter(|event| matches!(event, TimingRecord::FunctionTimingCompletion { .. }))
                .count(),
            1
        );
        assert!(
            !state
                .span_records()
                .iter()
                .any(|event| event.completion().is_some())
        );
    }

    #[test]
    fn engine_modes_preserve_policy_and_hidden_boundaries() {
        for mode in [
            AutoTelemetryLevel::Low,
            AutoTelemetryLevel::Medium,
            AutoTelemetryLevel::High,
        ] {
            for ai in [false, true] {
                let function = function(
                    FunctionKind::Bytecode,
                    ai.then(|| FunctionMeta::Llm {
                        client: "test".into(),
                    }),
                );
                let mut state = test_state(
                    Arc::new(TelemetryPolicies::with_auto_level(mode)),
                    test_clock(),
                );
                let parent = state.active_id();
                let frame = unsafe {
                    state.enter_bytecode(&function, HeapPtr::null(), None, 0, false, &[], |_, _| {
                        assert_ne!(
                            mode,
                            AutoTelemetryLevel::Low,
                            "hidden entry must not register"
                        );
                        (None, function.telemetry_function_id.unwrap())
                    })
                };
                if mode == AutoTelemetryLevel::Low {
                    assert!(frame.is_none());
                    assert_eq!(state.active_id(), parent);
                    assert!(state.span_records().is_empty());
                    // Explicit policies can observe future entries; an invocation
                    // that already entered Hidden still has no completion state.
                    state
                        .set_policy(
                            &function,
                            TelemetryPolicy {
                                promote_errors: true,
                                ..TelemetryPolicy::NONE
                            },
                        )
                        .unwrap();
                    let frame = unsafe {
                        state.enter_bytecode(
                            &function,
                            HeapPtr::null(),
                            None,
                            0,
                            false,
                            &[],
                            |_, _| (None, function.telemetry_function_id.unwrap()),
                        )
                    }
                    .unwrap();
                    unsafe {
                        state.complete_invocation(
                            frame,
                            &function,
                            InvocationOutcome::Errored,
                            None,
                        );
                    };
                    assert!(state.span_records().iter().any(|record| matches!(
                        record,
                        SpanRecord::LateFunctionSpanCompletionErrored { .. }
                            | SpanRecord::FunctionSpanCompletionErroredNeedsAnnouncement { .. }
                    )));
                } else {
                    let frame = frame.unwrap();
                    assert_eq!(frame.is_span(), ai || mode == AutoTelemetryLevel::High);
                    unsafe {
                        state.complete_invocation(
                            frame,
                            &function,
                            InvocationOutcome::Ok,
                            Some(Value::int(1)),
                        );
                    };
                    if mode == AutoTelemetryLevel::High && !ai {
                        assert!(state.span_records().iter().any(|record| matches!(
                            record,
                            SpanRecord::FunctionSpanCompletionOk {
                                captured_value: None,
                                ..
                            }
                        )));
                    }
                }
            }
            for mut hidden in [
                function(FunctionKind::NativeUnresolved, None),
                function(FunctionKind::Bytecode, None),
            ] {
                if matches!(hidden.kind, FunctionKind::Bytecode) {
                    hidden.telemetry_function_id = None;
                }
                let mut state = test_state(
                    Arc::new(TelemetryPolicies::with_auto_level(mode)),
                    test_clock(),
                );
                assert!(
                    unsafe {
                        state.enter_bytecode(
                            &hidden,
                            HeapPtr::null(),
                            None,
                            0,
                            false,
                            &[],
                            |_, _| panic!("hidden function registered"),
                        )
                    }
                    .is_none()
                );
                assert!(state.span_records().is_empty());
            }
        }
    }

    #[test]
    fn explicit_policy_overrides_auto_level_and_reset_restores_inheritance() {
        for auto_level in [
            AutoTelemetryLevel::Low,
            AutoTelemetryLevel::Medium,
            AutoTelemetryLevel::High,
        ] {
            for ai in [false, true] {
                for span_from_entry in [false, true] {
                    let function = function(
                        FunctionKind::Bytecode,
                        ai.then(|| FunctionMeta::Llm {
                            client: "test".into(),
                        }),
                    );
                    let mut state = test_state(
                        Arc::new(TelemetryPolicies::with_auto_level(auto_level)),
                        test_clock(),
                    );
                    state
                        .set_policy(
                            &function,
                            TelemetryPolicy {
                                span_from_entry,
                                promote_errors: true,
                                ..TelemetryPolicy::NONE
                            },
                        )
                        .unwrap();
                    let frame = unsafe {
                        state.enter_bytecode(
                            &function,
                            HeapPtr::null(),
                            None,
                            0,
                            false,
                            &[],
                            |_, _| (None, function.telemetry_function_id.unwrap()),
                        )
                    }
                    .unwrap();
                    assert_eq!(frame.is_span(), ai || span_from_entry);
                    unsafe {
                        state.complete_invocation(frame, &function, InvocationOutcome::Ok, None);
                    }
                    if !ai && !span_from_entry {
                        assert_eq!(state.timing_records.len(), 1);
                        assert!(!state.span_records().iter().any(|record| matches!(
                            record,
                            SpanRecord::FunctionSpanAnnouncement { .. }
                        )));
                    }

                    state.set_policy(&function, TelemetryPolicy::NONE).unwrap();
                    assert_eq!(
                        function.telemetry_policy_id.load(),
                        btel_types::TelemetryPolicyId::NONE
                    );
                    let frame = unsafe {
                        state.enter_bytecode(
                            &function,
                            HeapPtr::null(),
                            None,
                            0,
                            false,
                            &[],
                            |_, _| (None, function.telemetry_function_id.unwrap()),
                        )
                    };
                    if auto_level == AutoTelemetryLevel::Low {
                        assert!(frame.is_none());
                    } else {
                        let frame = frame.unwrap();
                        assert_eq!(
                            frame.is_span(),
                            ai || auto_level == AutoTelemetryLevel::High
                        );
                        unsafe {
                            state.complete_invocation(
                                frame,
                                &function,
                                InvocationOutcome::Ok,
                                None,
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn ai_function_is_a_span_from_entry() {
        let function = function(
            FunctionKind::Bytecode,
            Some(FunctionMeta::Llm {
                client: "test".to_string(),
            }),
        );
        let mut state = test_state(Arc::new(TelemetryPolicies::new()), test_clock());
        state.start_thread();
        let parent_id = state.active_id();
        let frame = unsafe {
            state.enter_bytecode(
                &function,
                HeapPtr::null(),
                None,
                0,
                false,
                &[Value::int(3)],
                |_, _| (None, function.telemetry_function_id.unwrap()),
            )
        }
        .unwrap();
        let span_id = state.active_id();

        assert!(frame.is_span());
        assert_ne!(span_id, parent_id);
        unsafe {
            state.complete_invocation(frame, &function, InvocationOutcome::Ok, Some(Value::int(5)));
        };
        assert_eq!(state.active_id(), parent_id);
        assert!(state.span_records().iter().any(|event| matches!(
            event,
            SpanRecord::FunctionSpanAnnouncement { id, parent_id: parent, captured_inputs, .. }
                if *id == span_id
                    && *parent == parent_id
                    && captured_inputs.as_ref().is_some_and(|capture| matches!(capture.roots()[0],btel_snapshot::SnapshotValue::Int(3)))
        )));
        assert!(state.span_records().iter().any(|event| matches!(
            event,
            SpanRecord::FunctionSpanCompletionOkNeedsAnnouncement { id, captured_value: Some(value), .. }
                if *id == span_id && matches!(value.roots()[0],btel_snapshot::SnapshotValue::Int(5))
        )));
    }

    #[test]
    fn completion_reads_current_policy_for_late_promotion() {
        let function = function(FunctionKind::Bytecode, None);
        let mut state = test_state(Arc::new(TelemetryPolicies::new()), test_clock());
        state.start_thread();
        let parent_id = state.active_id();
        let frame = unsafe {
            state.enter_bytecode(&function, HeapPtr::null(), None, 0, false, &[], |_, _| {
                (None, function.telemetry_function_id.unwrap())
            })
        }
        .unwrap();
        let updater = test_state(Arc::clone(&state.policies), Arc::clone(&state.clock));
        updater
            .set_policy(
                &function,
                TelemetryPolicy {
                    span_from_entry: false,
                    promotion_duration_threshold: Some(
                        state.clock.threshold(std::time::Duration::ZERO),
                    ),
                    promote_errors: false,
                    capture_inputs: false,
                    capture_output: true,
                    capture_error: false,
                },
            )
            .unwrap();

        // A VM created after publication sees the same table as an existing VM.
        let later = test_state(Arc::clone(&state.policies), Arc::clone(&state.clock));
        assert_eq!(
            later.policy_by_id(function.telemetry_policy_id.load()),
            state.policy_by_id(function.telemetry_policy_id.load())
        );
        unsafe {
            state.complete_invocation(frame, &function, InvocationOutcome::Ok, Some(Value::int(9)));
        };

        assert_eq!(state.active_id(), parent_id);
        assert!(state.span_records().iter().any(|event| matches!(
            event,
            SpanRecord::LateFunctionSpanCompletionOk {
                parent_id: parent,
                captured_value: Some(value),
                ..
            } if *parent == parent_id && matches!(value.roots()[0],btel_snapshot::SnapshotValue::Int(9))
        )));
        assert!(
            !state
                .timing_records()
                .iter()
                .any(|event| matches!(event, TimingRecord::FunctionTimingCompletion { .. }))
        );
    }

    #[test]
    fn completions_preserve_self_await_and_reentry() {
        let function = function(FunctionKind::Bytecode, None);
        let mut state = test_state(Arc::new(TelemetryPolicies::new()), test_clock());
        state.start_thread();
        let mut outer = unsafe {
            state.enter_bytecode(&function, HeapPtr::null(), None, 0, false, &[], |_, _| {
                (None, function.telemetry_function_id.unwrap())
            })
        }
        .unwrap();
        let mut inner = unsafe {
            state.enter_bytecode(
                &function,
                HeapPtr::null(),
                Some(HeapPtr::null()),
                1,
                true,
                &[],
                |_, _| (None, function.telemetry_function_id.unwrap()),
            )
        }
        .unwrap();
        outer.add_await(ClockDuration::from_ticks(3));
        inner.add_await(ClockDuration::from_ticks(7));

        unsafe {
            state.complete_invocation(inner, &function, InvocationOutcome::Ok, None);
        };
        unsafe {
            state.complete_invocation(outer, &function, InvocationOutcome::Ok, None);
        };

        let measurements: Vec<_> = state
            .timing_records()
            .iter()
            .filter_map(|event| match event {
                TimingRecord::FunctionTimingCompletion {
                    await_time,
                    reentry,
                    ..
                } => Some((await_time.get().get(), *reentry)),
                TimingRecord::ThreadSelected { .. } => None,
            })
            .collect();
        // Completion must carry each invocation's own wait, without rolling
        // the child's duration into its parent or losing reentry attribution.
        assert_eq!(measurements, [(7, true), (3, false)]);
    }

    #[test]
    fn specialized_completions_preserve_outcome_reentry_and_entry_dependency() {
        for outcome in [
            InvocationOutcome::Ok,
            InvocationOutcome::Errored,
            InvocationOutcome::Cancelled,
        ] {
            for reentry in [false, true] {
                for inputs in [false, true] {
                    let function = function(FunctionKind::Bytecode, None);
                    let mut state = test_state(Arc::new(TelemetryPolicies::new()), test_clock());
                    state.start_thread();
                    state
                        .set_policy(
                            &function,
                            TelemetryPolicy {
                                span_from_entry: true,
                                capture_inputs: inputs,
                                ..TelemetryPolicy::NONE
                            },
                        )
                        .unwrap();
                    let outer = unsafe {
                        state.enter_bytecode(
                            &function,
                            HeapPtr::null(),
                            None,
                            0,
                            false,
                            &[Value::int(3)],
                            |_, _| (None, function.telemetry_function_id.unwrap()),
                        )
                    }
                    .unwrap();
                    let inner = reentry.then(|| {
                        unsafe {
                            state.enter_bytecode(
                                &function,
                                HeapPtr::null(),
                                Some(HeapPtr::null()),
                                1,
                                true,
                                &[Value::int(4)],
                                |_, _| {
                                    (
                                        Some(function.telemetry_function_id.unwrap()),
                                        function.telemetry_function_id.unwrap(),
                                    )
                                },
                            )
                        }
                        .unwrap()
                    });
                    let frame = inner.unwrap_or(outer);
                    let id = state.active_id();
                    // A later policy cannot manufacture or erase entry-side dependencies.
                    state
                        .set_policy(
                            &function,
                            TelemetryPolicy {
                                span_from_entry: true,
                                capture_inputs: !inputs,
                                ..TelemetryPolicy::NONE
                            },
                        )
                        .unwrap();
                    unsafe {
                        state.complete_invocation(frame, &function, outcome, Some(Value::int(7)));
                    };
                    let completed = state.span_records().last().unwrap().completion().unwrap();
                    assert_eq!(completed.id, id);
                    assert_eq!(completed.outcome, outcome);
                    assert_eq!(completed.reentry, reentry);
                    assert_eq!(completed.requires_announcement, inputs);
                    assert!(!completed.late);
                    assert!(state.span_records().iter().any(|record| matches!(record,
                        SpanRecord::FunctionSpanAnnouncement {id: announced, captured_inputs, ..}
                        if *announced == id && captured_inputs.is_some() == inputs)));
                    if reentry {
                        unsafe {
                            state.complete_invocation(
                                outer,
                                &function,
                                InvocationOutcome::Ok,
                                None,
                            );
                        };
                    }
                }
            }
        }
    }

    #[test]
    fn all_late_completion_variants_are_announcement_independent() {
        for outcome in [
            InvocationOutcome::Ok,
            InvocationOutcome::Errored,
            InvocationOutcome::Cancelled,
        ] {
            for reentry in [false, true] {
                let function = function(FunctionKind::Bytecode, None);
                let mut state = test_state(Arc::new(TelemetryPolicies::new()), test_clock());
                state.start_thread();
                let outer = unsafe {
                    state.enter_bytecode(&function, HeapPtr::null(), None, 0, false, &[], |_, _| {
                        (None, function.telemetry_function_id.unwrap())
                    })
                }
                .unwrap();
                let inner = reentry.then(|| {
                    unsafe {
                        state.enter_bytecode(
                            &function,
                            HeapPtr::null(),
                            Some(HeapPtr::null()),
                            1,
                            true,
                            &[],
                            |_, _| {
                                (
                                    Some(function.telemetry_function_id.unwrap()),
                                    function.telemetry_function_id.unwrap(),
                                )
                            },
                        )
                    }
                    .unwrap()
                });
                state
                    .set_policy(
                        &function,
                        TelemetryPolicy {
                            span_from_entry: true,
                            capture_inputs: true,
                            ..TelemetryPolicy::NONE
                        },
                    )
                    .unwrap();
                unsafe {
                    state.complete_invocation(inner.unwrap_or(outer), &function, outcome, None);
                };
                let completed = state.span_records().last().unwrap().completion().unwrap();
                assert_eq!(completed.outcome, outcome);
                assert_eq!(completed.reentry, reentry);
                assert!(completed.late);
                assert!(!completed.requires_announcement);
                assert!(
                    !state.span_records().iter().any(|record| matches!(
                        record,
                        SpanRecord::FunctionSpanAnnouncement { .. }
                    ))
                );
                if reentry {
                    unsafe {
                        state.complete_invocation(outer, &function, InvocationOutcome::Ok, None);
                    };
                }
            }
        }
    }

    #[test]
    fn native_policy_updates_are_rejected() {
        let state = test_state(Arc::new(TelemetryPolicies::new()), test_clock());
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
                    let mut state = test_state(Arc::new(TelemetryPolicies::new()), test_clock());
                    state.start_thread();
                    let policy = |capture| TelemetryPolicy {
                        span_from_entry: true,
                        capture_output: capture,
                        capture_error: capture,
                        ..TelemetryPolicy::NONE
                    };
                    state.set_policy(&function, policy(initial)).unwrap();
                    let frame = unsafe {
                        state.enter_bytecode(
                            &function,
                            HeapPtr::null(),
                            None,
                            0,
                            false,
                            &[],
                            |_, _| (None, function.telemetry_function_id.unwrap()),
                        )
                    }
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
                    unsafe {
                        state.complete_invocation(frame, &function, outcome, Some(Value::int(7)));
                    };
                    let recorded = state.span_records().last().unwrap().completion().unwrap();
                    assert!(!recorded.late, "an entry-selected span must remain a span");
                    assert_eq!(recorded.outcome, outcome);
                    assert_eq!(
                        recorded
                            .captured_value
                            .map(|s| matches!(s.roots()[0], btel_snapshot::SnapshotValue::Int(7))),
                        (initial || current).then_some(true)
                    );
                }
            }
        }
    }

    #[test]
    fn spawned_thread_has_explicit_parent_and_structural_call_path() {
        let mut parent = test_state(Arc::new(TelemetryPolicies::new()), test_clock());
        parent.start_thread();
        let function = function(FunctionKind::Bytecode, None);
        let context = parent.spawn_context(None, 11, HeapPtr::null(), |_, _| {
            (None, function.telemetry_function_id.unwrap())
        });

        let mut child = test_state(Arc::clone(&parent.policies), Arc::clone(&parent.clock));
        let child_id = child.active_id();
        child.configure_spawn(&context);
        child.start_thread();

        assert_ne!(child_id, context.parent_id);
        assert!(child.span_records().iter().any(|event| matches!(
            event,
            SpanRecord::ThreadSpanAnnouncement {
                id,
                parent_id: Some(parent_id),
                spawn_call_path,
                ..
            } if *id == child_id
                && *parent_id == context.parent_id
                && *spawn_call_path == context.spawn_call_path
        )));
        child.complete_thread(InvocationOutcome::Errored);
        child
            .span_records
            .retain(|record| !matches!(record, SpanRecord::ThreadSpanAnnouncement { .. }));
        assert!(
            matches!(child.span_records(), [SpanRecord::ThreadSpanCompletion {
            id, parent_id: Some(parent_id), spawn_call_path, clock, outcome: InvocationOutcome::Errored, ..
        }] if *id == child_id && *parent_id == context.parent_id
            && *spawn_call_path == context.spawn_call_path && Arc::ptr_eq(clock, &context.clock))
        );
    }
}
