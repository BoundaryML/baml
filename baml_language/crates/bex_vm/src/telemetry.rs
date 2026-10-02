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
pub(crate) use errors::{ErrorBook, LandingMatch};
mod network;
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
const POLICY_CAPTURE_OUTPUT: u8 = 1 << 1;
const POLICY_CAPTURE_ERROR: u8 = 1 << 2;
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
    span_id: Option<TelemetryId>,
    pub await_duration: AwaitDuration,
    pub saved_call_path: CallPathId,
    flags: u8,
    // Requests are local to this invocation. False adds no request; policy
    // requests latched at entry (and later updates) cannot be vetoed here.
    output_request: bool,
    error_request: bool,
}

const _: () = assert!(size_of::<FrameTelemetry>() == btel_settings::layout::FRAME_TELEMETRY_BYTES);
const _: () =
    assert!(size_of::<Option<FrameTelemetry>>() == btel_settings::layout::FRAME_TELEMETRY_BYTES);

/// The type arguments a call's frame starts with, in the callee's De Bruijn
/// order: those the callee value carries (a bound method's receiver class
/// and method arguments, a closure's captured ones, a generic function
/// value's), then those the call site passes.
#[derive(Clone, Copy, Debug, Default)]
pub struct CallTypeArgs<'a> {
    pub carried: &'a [bex_vm_types::RealizedTy],
    pub passed: &'a [bex_vm_types::RealizedTy],
}

impl<'a> CallTypeArgs<'a> {
    fn is_empty(&self) -> bool {
        self.carried.is_empty() && self.passed.is_empty()
    }
    fn iter(&self) -> impl Iterator<Item = &'a bex_vm_types::RealizedTy> {
        self.carried.iter().chain(self.passed)
    }
}

/// Invocation-local tracing controls resolved by the call boundary.
#[derive(Clone, Copy, Debug, Default)]
pub struct TraceConfig {
    pub mode: Option<InvocationMode>,
    pub inputs: Option<bool>,
    pub output: Option<bool>,
    pub error: Option<bool>,
    pub reserved_id: Option<TelemetryId>,
}

impl FrameTelemetry {
    #[inline(always)]
    pub const fn span_id(self) -> Option<TelemetryId> {
        self.span_id
    }

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

pub use btel_records::{LogLevel, SpanRecord, TimingRecord};

mod host;
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
    /// When the thread was created: for a spawned future, when it was
    /// scheduled, before it waited to be admitted.
    started_at: ClockInstant,
    started: bool,
    /// A spawned future's body began running (`ThreadSpanRunning` written).
    running: bool,
    completed: bool,
    name: Option<btel_records::ThreadName>,
}

pub struct TelemetryState {
    context: btel_types::context::Context,
    #[cfg(all(not(test), not(target_arch = "wasm32")))]
    context_snapshot: Option<(btel_types::context::Context, btel_snapshot::CasId)>,
    #[cfg(all(not(test), not(target_arch = "wasm32")))]
    pending_context_snapshot: Option<btel_snapshot::Snapshot>,
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
    pub fn set_context(&mut self, context: btel_types::context::Context) {
        #[cfg(all(not(test), not(target_arch = "wasm32")))]
        if !context.same_version(&self.context) {
            self.pending_context_snapshot = None;
        }
        self.context = context;
    }

    #[cfg(all(not(test), not(target_arch = "wasm32")))]
    fn prepare_context(&mut self) {
        if self.context.is_empty()
            || self.pending_context_snapshot.is_some()
            || self
                .context_snapshot
                .as_ref()
                .is_some_and(|(context, _)| context.same_version(&self.context))
        {
            return;
        }
        if let Some(builder) = self.runtime.acquire_snapshot() {
            self.pending_context_snapshot = btel_snapshot::context::capture_with_builder(
                &self.context,
                builder,
                self.capture_scratch.shaper(),
            );
        }
    }

    pub fn new_root(
        policies: Arc<TelemetryPolicies>,
        clock: Arc<ClockEpoch>,
        #[cfg(not(target_arch = "wasm32"))] runtime: Arc<btel_processor::TelemetryRuntime>,
    ) -> Self {
        clock.attach_thread();
        let id = allocate_telemetry_id();
        let started_at = clock.read();
        Self {
            context: btel_types::context::Context::default(),
            #[cfg(all(not(test), not(target_arch = "wasm32")))]
            context_snapshot: None,
            #[cfg(all(not(test), not(target_arch = "wasm32")))]
            pending_context_snapshot: None,
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
                running: false,
                completed: false,
                name: None,
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

    /// A generic call's type arguments as `map<string, Type>`, keyed by the
    /// names the compiler gave the frame's slots. A slot without a name is
    /// left out rather than given an invented key; nothing is recorded when
    /// no slot has one.
    #[cold]
    fn capture_type_args(
        &mut self,
        function: &Function,
        type_args: CallTypeArgs<'_>,
    ) -> Option<btel_snapshot::Snapshot> {
        let mut named: Vec<(&str, &bex_vm_types::RealizedTy)> =
            Vec::with_capacity(function.type_param_names.len());
        for (name, ty) in function.type_param_names.iter().zip(type_args.iter()) {
            // A frame can bind a name twice (a lambda inside a block that
            // rebinds it); the later slot is the one its body sees.
            match named.iter_mut().find(|(seen, _)| *seen == name.as_str()) {
                Some(slot) => slot.1 = ty,
                None => named.push((name, ty)),
            }
        }
        if named.is_empty() {
            return None;
        }
        self.capture(snapshot::Input::TypeArgs(&named))
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

    /// The spawn context for a task whose body runs no BAML function (a host
    /// closure): with no callee to add an edge for, the child continues the
    /// spawner's active call path.
    pub fn hidden_spawn_context(&self) -> ThreadSpawnContext {
        ThreadSpawnContext {
            parent_id: self.thread.active_id,
            spawn_call_path: self.thread.active_call_path,
            clock: Arc::clone(&self.clock),
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

    /// Name this thread's future before it starts; its records carry it.
    pub fn set_thread_name(&mut self, name: &str) {
        assert!(!self.thread.started, "telemetry thread already started");
        self.thread.name = Some(Arc::new(Box::from(name)));
    }

    /// A spawned future's body starts running now, after waiting to be
    /// admitted. Returns that instant the first time; `None` after that and
    /// for a root thread, which runs from the start.
    pub fn mark_running(&mut self) -> Option<ClockInstant> {
        if !self.is_waiting() {
            return None;
        }
        self.thread.running = true;
        self.start_thread();
        let at = self.clock.read();
        self.write_span(SpanRecord::ThreadSpanRunning { at });
        Some(at)
    }

    /// A spawned future whose body has not begun running.
    pub fn is_waiting(&self) -> bool {
        self.thread.parent_id.is_some() && !self.thread.running
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
            name: self.thread.name.clone(),
        });
    }

    /// Charge one sysop's time to the innermost observed call path. Outside
    /// every observed frame the time belongs to no node and is dropped.
    #[inline]
    pub fn record_sysop_time(&mut self, elapsed: ClockDuration) {
        let call_path = self.thread.active_call_path;
        if call_path != CallPathId::ROOT {
            self.write_timing(TimingRecord::SysOpTime { call_path, elapsed });
        }
    }

    /// Capture a point event without promoting its invocation to a span.
    ///
    /// # Safety
    /// `data` and its reachable objects must remain live under the heap permit.
    pub unsafe fn record_log(
        &mut self,
        function: FunctionId,
        pc: u32,
        level: btel_records::LogLevel,
        event_name: Option<Arc<str>>,
        data: Value,
    ) {
        self.start_thread();
        let at = self.clock.read();
        #[cfg(all(not(test), not(target_arch = "wasm32")))]
        self.prepare_context();
        let captured_data = self.capture(snapshot::Input::Value(data));
        self.write_span(SpanRecord::Log {
            parent_id: self.thread.active_id,
            function,
            pc,
            at,
            level,
            event_name,
            captured_data,
        });
    }

    /// Open a span for an HTTP request the runtime is about to send, whatever
    /// the caller's `$trace` options: `None` at the `Low` auto level. Its
    /// parent is the innermost span, its call path the innermost observed
    /// frame's. It stays open until `close_network_span`.
    pub fn open_network_span(
        &mut self,
        request: &sys_types::network::NetworkRequest,
    ) -> Option<TelemetryId> {
        if self.auto_level == AutoTelemetryLevel::Low {
            return None;
        }
        self.start_thread();
        let url = network::sanitize_url(&request.url);
        let headers = network::sanitize_headers(&request.headers, network::Direction::Request);
        #[cfg(all(not(test), not(target_arch = "wasm32")))]
        self.prepare_context();
        let captured_request = self.capture(snapshot::Input::Network(
            &network::NetworkPayload::Request {
                method: &request.method,
                url: &url,
                headers: &headers,
                body: self.policies.http_bodies().then_some(&request.body[..]),
            },
        ));
        let id = allocate_telemetry_id();
        self.write_span(SpanRecord::NetworkSpanAnnouncement(Box::new(
            btel_records::NetworkAnnouncement {
                id,
                parent_id: self.thread.active_id,
                call_path: self.thread.active_call_path,
                started_at: self.clock.read(),
                method: request.method.as_str().into(),
                url: url.into_boxed_str(),
                request: captured_request,
            },
        )));
        Some(id)
    }

    /// Record what happened to an open request at `at`, an instant the IO
    /// side read from this run's clock. Its span may belong to another thread.
    pub fn network_event(
        &mut self,
        span: TelemetryId,
        at: ClockInstant,
        event: &sys_types::network::NetworkEventKind,
    ) {
        use sys_types::network::NetworkEventKind;

        self.start_thread();
        let bodies = self.policies.http_bodies();
        let payload = match event {
            NetworkEventKind::Connection { status, headers } => {
                let headers = network::sanitize_headers(headers, network::Direction::Response);
                self.capture(snapshot::Input::Network(
                    &network::NetworkPayload::Connection {
                        status: *status,
                        headers: &headers,
                    },
                ))
            }
            NetworkEventKind::Body(body) if bodies => self.capture(snapshot::Input::Network(
                &network::NetworkPayload::Body(body),
            )),
            NetworkEventKind::SseEvent { event, data, id } if bodies => self.capture(
                snapshot::Input::Network(&network::NetworkPayload::SseEvent {
                    event: event.as_deref(),
                    data,
                    id: id.as_deref(),
                }),
            ),
            _ => None,
        };
        self.write_span(SpanRecord::NetworkEvent(Box::new(
            btel_records::NetworkEventRecord {
                span,
                name: network::event_name(event),
                at,
                payload,
            },
        )));
    }

    /// End an open request's span. An errored one captures its error; where
    /// the error's text quotes one of `raw_urls` (the URLs the request went
    /// to, as sent), the capture has the sanitized URL instead.
    ///
    /// # Safety
    /// `error` and its reachable objects must remain live under the heap permit.
    pub unsafe fn close_network_span(
        &mut self,
        span: TelemetryId,
        at: ClockInstant,
        outcome: InvocationOutcome,
        error: Option<Value>,
        raw_urls: &[&str],
    ) {
        self.start_thread();
        let error = error.and_then(|error| {
            let mut rewrites: Vec<_> = raw_urls
                .iter()
                .flat_map(|url| network::url_rewrites(url))
                .collect();
            rewrites.sort_by_key(|(from, _)| std::cmp::Reverse(from.len()));
            self.capture_scratch.rewrites = rewrites;
            let captured = self.capture(snapshot::Input::Value(error));
            self.capture_scratch.rewrites.clear();
            captured
        });
        self.write_span(SpanRecord::NetworkSpanCompletion(Box::new(
            btel_records::NetworkCompletion {
                span,
                completed_at: at,
                outcome,
                error,
            },
        )));
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
        // SAFETY: forwarded unchanged to the trace-aware entry point.
        unsafe {
            self.enter_bytecode_with_trace(
                function,
                callee,
                visible_caller,
                caller_pc,
                caller_is_observed,
                args,
                None,
                CallTypeArgs::default(),
                register,
            )
        }
    }

    #[inline(always)]
    #[allow(
        clippy::too_many_arguments,
        reason = "entry data plus call-local tracing controls and registration hook"
    )]
    /// # Safety
    /// Every argument and its reachable objects must remain live under the VM
    /// heap permit throughout this call. Capture may traverse that graph.
    pub unsafe fn enter_bytecode_with_trace(
        &mut self,
        function: &Function,
        callee: HeapPtr,
        visible_caller: Option<HeapPtr>,
        caller_pc: u32,
        caller_is_observed: bool,
        args: &[Value],
        trace: Option<TraceConfig>,
        type_args: CallTypeArgs<'_>,
        register: impl FnOnce(Option<HeapPtr>, HeapPtr) -> (Option<FunctionId>, FunctionId),
    ) -> Option<FrameTelemetry> {
        if function.telemetry_function_id.is_none()
            || !matches!(function.kind, FunctionKind::Bytecode)
        {
            return None;
        }

        let mode_override = trace.and_then(|config| {
            config
                .reserved_id
                .map(|_| InvocationMode::Span)
                .or(config.mode)
        });
        let policy_id = function.telemetry_policy_id.load();
        if mode_override.is_none() && policy_id == btel_types::TelemetryPolicyId::NONE {
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
                    trace,
                    register,
                ));
            }
        }
        let policy = self.policy_by_id(policy_id);
        let mode = mode_override.unwrap_or_else(|| {
            if policy_id == btel_types::TelemetryPolicyId::NONE {
                InvocationMode::Span
            } else {
                default_mode(function, policy)
            }
        });

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
        let capture_inputs = trace.and_then(|config| config.inputs) == Some(true)
            || policy.capture_inputs
            || (is_ai && btel_settings::policy::AI_CAPTURE_INPUTS);
        if policy.capture_output || (is_ai && btel_settings::policy::AI_CAPTURE_OUTPUT) {
            flags |= POLICY_CAPTURE_OUTPUT;
        }
        if policy.capture_error || (is_ai && btel_settings::policy::AI_CAPTURE_ERROR) {
            flags |= POLICY_CAPTURE_ERROR;
        }

        let captured_inputs = (mode == InvocationMode::Span && capture_inputs)
            .then(|| self.capture(snapshot::Input::FunctionArgs(args)))
            .flatten();
        // Types are not user data: a span records them whether or not it
        // captures its arguments.
        let captured_type_args = (mode == InvocationMode::Span && !type_args.is_empty())
            .then(|| self.capture_type_args(function, type_args))
            .flatten();
        if captured_inputs.is_some() || captured_type_args.is_some() {
            flags |= REQUIRES_ANNOUNCEMENT;
        }
        #[cfg(all(not(test), not(target_arch = "wasm32")))]
        if mode == InvocationMode::Span {
            self.prepare_context();
        }
        let entered_at = self.clock.read();
        let span_id = if mode == InvocationMode::Span {
            let id = trace
                .and_then(|config| config.reserved_id)
                .unwrap_or_else(allocate_telemetry_id);
            self.thread.active_id = id;
            self.write_span(SpanRecord::FunctionSpanAnnouncement {
                id,
                parent_id: saved_parent_id,
                call_path,
                entered_at,
                captured_inputs,
                captured_type_args,
            });
            Some(id)
        } else {
            None
        };
        self.thread.active_call_path = call_path;

        Some(FrameTelemetry {
            entered_at,
            saved_parent_id,
            span_id,
            await_duration: AwaitDuration::ZERO,
            saved_call_path,
            flags,
            output_request: trace.and_then(|config| config.output) == Some(true),
            error_request: trace.and_then(|config| config.error) == Some(true),
        })
    }

    #[inline(always)]
    fn enter_timing(
        &mut self,
        callee: HeapPtr,
        visible_caller: Option<HeapPtr>,
        caller_pc: u32,
        caller_is_observed: bool,
        trace: Option<TraceConfig>,
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
            span_id: None,
            await_duration: AwaitDuration::ZERO,
            saved_call_path,
            flags,
            output_request: trace.and_then(|config| config.output) == Some(true),
            error_request: trace.and_then(|config| config.error) == Some(true),
        }
    }

    /// Whether an errored completion of this frame may capture its error:
    /// the frame is a span or has a policy (it may be promoted) and error
    /// capture is on. Lets the unwinder build the error's context only when
    /// some frame can record it.
    pub fn wants_error_capture(&self, telemetry: &FrameTelemetry, function: &Function) -> bool {
        let policy_id = function.telemetry_policy_id.load();
        if !telemetry.is_span() && policy_id == btel_types::TelemetryPolicyId::NONE {
            return false;
        }
        telemetry.error_request
            || telemetry.flags & POLICY_CAPTURE_ERROR != 0
            || self.policy_by_id(policy_id).capture_error
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
                if telemetry.output_request
                    || telemetry.flags & POLICY_CAPTURE_OUTPUT != 0
                    || policy.capture_output =>
            {
                value
            }
            InvocationOutcome::Errored
            | InvocationOutcome::Cancelled
            | InvocationOutcome::Panicked
                if telemetry.error_request
                    || telemetry.flags & POLICY_CAPTURE_ERROR != 0
                    || policy.capture_error =>
            {
                value
            }
            _ => None,
        };

        if telemetry.is_span() {
            let captured_value =
                capture_value.and_then(|value| self.capture(snapshot::Input::Value(value)));
            let id = telemetry.span_id().expect("span frame owns its identity");
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
                (InvocationOutcome::Panicked, false, false) => {
                    self.write_span(SpanRecord::FunctionSpanCompletionPanicked {
                        id,
                        parent_id: telemetry.saved_parent_id,
                        call_path,
                        entered_at: telemetry.entered_at,
                        exited_at,
                        await_time: telemetry.await_duration,
                        captured_value,
                    });
                }
                (InvocationOutcome::Panicked, false, true) => self.write_span(
                    SpanRecord::FunctionSpanCompletionPanickedNeedsAnnouncement {
                        id,
                        parent_id: telemetry.saved_parent_id,
                        call_path,
                        entered_at: telemetry.entered_at,
                        exited_at,
                        await_time: telemetry.await_duration,
                        captured_value,
                    },
                ),
                (InvocationOutcome::Panicked, true, false) => {
                    self.write_span(SpanRecord::FunctionSpanCompletionPanickedReentry {
                        id,
                        parent_id: telemetry.saved_parent_id,
                        call_path,
                        entered_at: telemetry.entered_at,
                        exited_at,
                        await_time: telemetry.await_duration,
                        captured_value,
                    });
                }
                (InvocationOutcome::Panicked, true, true) => self.write_span(
                    SpanRecord::FunctionSpanCompletionPanickedReentryNeedsAnnouncement {
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
                (InvocationOutcome::Panicked, false) => {
                    self.write_span(SpanRecord::LateFunctionSpanCompletionPanicked {
                        id: allocate_telemetry_id(),
                        parent_id: telemetry.saved_parent_id,
                        call_path,
                        entered_at: telemetry.entered_at,
                        exited_at,
                        await_time: telemetry.await_duration,
                        captured_value,
                    });
                }
                (InvocationOutcome::Panicked, true) => {
                    self.write_span(SpanRecord::LateFunctionSpanCompletionPanickedReentry {
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
        let completed_at = self.clock.read();
        if self.is_waiting() {
            // Cancelled before it ran: it starts and ends at the same instant.
            self.thread.running = true;
            self.write_span(SpanRecord::ThreadSpanRunning { at: completed_at });
        }
        self.write_span(SpanRecord::ThreadSpanCompletion {
            id: self.thread.id,
            parent_id: self.thread.parent_id,
            spawn_call_path: self.thread.spawn_call_path,
            clock: Arc::clone(&self.clock),
            started_at: self.thread.started_at,
            completed_at,
            outcome,
            name: self.thread.name.clone(),
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
        {
            let _ = self;
            false
        }
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
            let _ = (self, future);
            btel_records::FutureErrorLookup::Missing
        }
    }

    pub(crate) fn link_future_error(&self, future: u64, link: btel_records::FutureErrorLink) {
        #[cfg(not(target_arch = "wasm32"))]
        self.runtime.link_future_error(future, link);
        #[cfg(target_arch = "wasm32")]
        let _ = (self, future, link);
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
        {
            use btel_records::ContextReference;

            if !record.is_context_observation() {
                self.runtime.write_span(self.thread.id, record);
                return;
            }
            let mut snapshot = None;
            let reference = if self.context.is_empty() {
                ContextReference::Empty
            } else if let Some((context, id)) = &self.context_snapshot
                && context.same_version(&self.context)
            {
                ContextReference::Snapshot(*id)
            } else {
                self.prepare_context();
                if let Some(captured) = self.pending_context_snapshot.take() {
                    let id = captured.root_id();
                    snapshot = Some(captured);
                    ContextReference::Snapshot(id)
                } else {
                    ContextReference::Unavailable
                }
            };
            if self
                .runtime
                .write_span_with_context(self.thread.id, reference, snapshot, record)
                && let ContextReference::Snapshot(id) = reference
            {
                self.context_snapshot = Some((self.context.clone(), id));
            }
        }
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

/// `sha256:` and the hex digits a sanitized value has in place of `value`.
#[cfg(test)]
pub(crate) fn network_hash_for_tests(value: &str) -> String {
    network::hash(value)
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

    fn trace_entry(
        state: &mut TelemetryState,
        function: &Function,
        trace: Option<TraceConfig>,
    ) -> Option<FrameTelemetry> {
        // SAFETY: the argument is immediate and the registration hook never dereferences pointers.
        unsafe {
            state.enter_bytecode_with_trace(
                function,
                HeapPtr::null(),
                None,
                0,
                false,
                &[Value::int(7)],
                trace,
                CallTypeArgs::default(),
                |_, _| (None, function.telemetry_function_id.unwrap()),
            )
        }
    }

    #[test]
    fn invocation_modes_override_defaults_without_changing_policy() {
        for auto_level in [
            AutoTelemetryLevel::Low,
            AutoTelemetryLevel::Medium,
            AutoTelemetryLevel::High,
        ] {
            for mode in [
                InvocationMode::Hidden,
                InvocationMode::Timing,
                InvocationMode::Span,
            ] {
                for reserved in [false, true] {
                    let function = function(FunctionKind::Bytecode, None);
                    let mut state = test_state(
                        Arc::new(TelemetryPolicies::with_auto_level(auto_level)),
                        test_clock(),
                    );
                    let reserved_id = reserved.then(allocate_telemetry_id);
                    let frame = trace_entry(
                        &mut state,
                        &function,
                        Some(TraceConfig {
                            mode: Some(mode),
                            reserved_id,
                            ..TraceConfig::default()
                        }),
                    );
                    assert_eq!(frame.is_some(), reserved || mode != InvocationMode::Hidden);
                    if let Some(frame) = frame {
                        assert_eq!(frame.is_span(), reserved || mode == InvocationMode::Span);
                        assert_eq!(frame.span_id().is_some(), frame.is_span());
                        if reserved {
                            assert_eq!(frame.span_id(), reserved_id);
                        }
                        unsafe {
                            state.complete_invocation(
                                frame,
                                &function,
                                InvocationOutcome::Ok,
                                None,
                            );
                        }
                    }
                    assert_eq!(
                        function.telemetry_policy_id.load(),
                        btel_types::TelemetryPolicyId::NONE
                    );
                    let default_frame = trace_entry(&mut state, &function, None);
                    assert_eq!(
                        default_frame.is_some(),
                        auto_level != AutoTelemetryLevel::Low
                    );
                    if let Some(frame) = default_frame {
                        assert_eq!(frame.is_span(), auto_level == AutoTelemetryLevel::High);
                        unsafe {
                            state.complete_invocation(
                                frame,
                                &function,
                                InvocationOutcome::Ok,
                                None,
                            );
                        }
                    }
                    if let Some(reserved_id) = reserved_id {
                        assert!(
                            state.span_records().iter().any(|record| matches!(record,
                            SpanRecord::FunctionSpanAnnouncement { id, .. } if *id == reserved_id))
                        );
                        assert!(
                            state.span_records().iter().any(|record| matches!(record,
                            SpanRecord::FunctionSpanCompletionOk { id, .. } if *id == reserved_id))
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn invocation_span_identity_does_not_fall_back_to_ancestor() {
        let function = function(FunctionKind::Bytecode, None);
        let mut state = test_state(Arc::new(TelemetryPolicies::new()), test_clock());
        let parent = trace_entry(
            &mut state,
            &function,
            Some(TraceConfig {
                mode: Some(InvocationMode::Span),
                ..TraceConfig::default()
            }),
        )
        .unwrap();
        let parent_id = parent.span_id().unwrap();
        for mode in [
            InvocationMode::Hidden,
            InvocationMode::Timing,
            InvocationMode::Span,
        ] {
            let child = trace_entry(
                &mut state,
                &function,
                Some(TraceConfig {
                    mode: Some(mode),
                    ..TraceConfig::default()
                }),
            );
            assert_eq!(parent.span_id(), Some(parent_id));
            assert_ne!(child.and_then(FrameTelemetry::span_id), Some(parent_id));
            assert_eq!(
                child.and_then(FrameTelemetry::span_id).is_some(),
                mode == InvocationMode::Span
            );
            if let Some(child) = child {
                unsafe {
                    state.complete_invocation(child, &function, InvocationOutcome::Ok, None);
                }
            }
            assert_eq!(state.active_id(), parent_id);
        }
        unsafe {
            state.complete_invocation(parent, &function, InvocationOutcome::Ok, None);
        }
    }

    #[test]
    fn invocation_capture_requests_combine_with_policy_and_record_real_snapshots() {
        for [capture_inputs, capture_output, capture_error] in [
            [false, false, false],
            [true, false, false],
            [false, true, false],
            [false, false, true],
            [true, true, true],
        ] {
            for [inputs, output, error] in [
                [None, None, None],
                [Some(false), Some(false), Some(false)],
                [Some(true), Some(false), Some(false)],
                [Some(false), Some(true), Some(false)],
                [Some(false), Some(false), Some(true)],
                [Some(true), Some(true), Some(true)],
            ] {
                for outcome in [
                    InvocationOutcome::Ok,
                    InvocationOutcome::Errored,
                    InvocationOutcome::Cancelled,
                ] {
                    let function = function(FunctionKind::Bytecode, None);
                    let mut state = test_state(Arc::new(TelemetryPolicies::new()), test_clock());
                    state
                        .set_policy(
                            &function,
                            TelemetryPolicy {
                                capture_inputs,
                                capture_output,
                                capture_error,
                                ..TelemetryPolicy::NONE
                            },
                        )
                        .unwrap();
                    let frame = trace_entry(
                        &mut state,
                        &function,
                        Some(TraceConfig {
                            mode: Some(InvocationMode::Span),
                            inputs,
                            output,
                            error,
                            ..TraceConfig::default()
                        }),
                    )
                    .unwrap();
                    let id = frame.span_id().unwrap();
                    assert_eq!(
                        state.wants_error_capture(&frame, &function),
                        error == Some(true) || capture_error,
                        "error context construction follows additive capture requests",
                    );
                    unsafe {
                        state.complete_invocation(frame, &function, outcome, Some(Value::int(9)));
                    }
                    let expected_inputs = inputs == Some(true) || capture_inputs;
                    let expected_value = match outcome {
                        InvocationOutcome::Ok => output == Some(true) || capture_output,
                        InvocationOutcome::Errored
                        | InvocationOutcome::Cancelled
                        | InvocationOutcome::Panicked => error == Some(true) || capture_error,
                    };
                    let inputs = state
                        .span_records()
                        .iter()
                        .find_map(|record| match record {
                            SpanRecord::FunctionSpanAnnouncement {
                                id: record_id,
                                captured_inputs,
                                ..
                            } if *record_id == id => Some(captured_inputs),
                            _ => None,
                        })
                        .unwrap();
                    assert_eq!(inputs.is_some(), expected_inputs);
                    if let Some(inputs) = inputs {
                        assert!(matches!(
                            inputs.roots()[0],
                            btel_snapshot::SnapshotValue::Int(7)
                        ));
                    }
                    let output = state
                        .span_records()
                        .iter()
                        .find_map(|record| match record {
                            SpanRecord::FunctionSpanCompletionOk {
                                id: record_id,
                                captured_value,
                                ..
                            }
                            | SpanRecord::FunctionSpanCompletionOkNeedsAnnouncement {
                                id: record_id,
                                captured_value,
                                ..
                            } if *record_id == id && outcome == InvocationOutcome::Ok => {
                                Some(captured_value)
                            }
                            SpanRecord::FunctionSpanCompletionErrored {
                                id: record_id,
                                captured_value,
                                ..
                            }
                            | SpanRecord::FunctionSpanCompletionErroredNeedsAnnouncement {
                                id: record_id,
                                captured_value,
                                ..
                            } if *record_id == id && outcome == InvocationOutcome::Errored => {
                                Some(captured_value)
                            }
                            SpanRecord::FunctionSpanCompletionCancelled {
                                id: record_id,
                                captured_value,
                                ..
                            }
                            | SpanRecord::FunctionSpanCompletionCancelledNeedsAnnouncement {
                                id: record_id,
                                captured_value,
                                ..
                            } if *record_id == id && outcome == InvocationOutcome::Cancelled => {
                                Some(captured_value)
                            }
                            _ => None,
                        })
                        .unwrap();
                    assert_eq!(output.is_some(), expected_value);
                    if let Some(output) = output {
                        assert!(matches!(
                            output.roots()[0],
                            btel_snapshot::SnapshotValue::Int(9)
                        ));
                    }
                    let next = trace_entry(
                        &mut state,
                        &function,
                        Some(TraceConfig {
                            mode: Some(InvocationMode::Span),
                            ..TraceConfig::default()
                        }),
                    )
                    .unwrap();
                    assert_eq!(next.flags & POLICY_CAPTURE_OUTPUT != 0, capture_output);
                    assert_eq!(next.flags & POLICY_CAPTURE_ERROR != 0, capture_error);
                    unsafe {
                        state.complete_invocation(next, &function, InvocationOutcome::Ok, None);
                    }
                }
            }
        }
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

    fn network_records(state: &TelemetryState) -> Vec<&VmSpanRecord> {
        state
            .span_records()
            .iter()
            .filter(|record| {
                matches!(
                    record,
                    SpanRecord::NetworkSpanAnnouncement(_)
                        | SpanRecord::NetworkEvent(_)
                        | SpanRecord::NetworkSpanCompletion(_)
                )
            })
            .collect()
    }

    fn shape(capture: Option<&btel_snapshot::Snapshot>) -> Option<serde_json::Value> {
        capture.map(|snapshot| snapshot::json(snapshot, snapshot.value().unwrap()))
    }

    fn request() -> sys_types::network::NetworkRequest {
        sys_types::network::NetworkRequest {
            method: "POST".to_owned(),
            url: "https://example.com/v1/messages?key=secret".to_owned(),
            headers: vec![
                ("Content-Type".to_owned(), "application/json".to_owned()),
                ("X-Api-Key".to_owned(), "sk-secret".to_owned()),
            ],
            body: br#"{"model":"m"}"#.to_vec().into(),
        }
    }

    #[test]
    fn network_span_records_its_request_events_and_completion() {
        use serde_json::json;
        use sys_types::network::NetworkEventKind;

        let function = function(FunctionKind::Bytecode, None);
        let mut state = test_state(Arc::new(TelemetryPolicies::new()), test_clock());
        // Made from inside a span: the span is its parent.
        let frame = trace_entry(
            &mut state,
            &function,
            Some(TraceConfig {
                mode: Some(InvocationMode::Span),
                ..TraceConfig::default()
            }),
        )
        .unwrap();
        let caller = frame.span_id().unwrap();
        let call_path = state.active_call_path();
        let span = state.open_network_span(&request()).unwrap();
        // A request is not a frame: the caller stays the active span.
        assert_eq!(state.active_id(), caller);
        let at = ClockInstant::from_ticks;
        state.network_event(
            span,
            at(10),
            &NetworkEventKind::Connection {
                status: 200,
                headers: vec![
                    ("Request-Id".to_owned(), "req_1".to_owned()),
                    ("Set-Cookie".to_owned(), "session".to_owned()),
                ],
            },
        );
        state.network_event(
            span,
            at(11),
            &NetworkEventKind::SseEvent {
                event: Some("message_stop".to_owned()),
                data: "{}".to_owned(),
                id: None,
            },
        );
        state.network_event(span, at(12), &NetworkEventKind::StreamEnd);
        state.network_event(span, at(13), &NetworkEventKind::Await);
        unsafe { state.close_network_span(span, at(13), InvocationOutcome::Ok, None, &[]) };
        unsafe { state.complete_invocation(frame, &function, InvocationOutcome::Ok, None) };

        let records = network_records(&state);
        let [
            SpanRecord::NetworkSpanAnnouncement(announcement),
            SpanRecord::NetworkEvent(connection),
            SpanRecord::NetworkEvent(data),
            SpanRecord::NetworkEvent(end),
            SpanRecord::NetworkEvent(read),
            SpanRecord::NetworkSpanCompletion(completion),
        ] = records.as_slice()
        else {
            panic!("unexpected network records: {records:?}");
        };
        let url = format!(
            "https://example.com/v1/messages?key={}",
            network::hash("secret")
        );
        assert_eq!(
            (
                announcement.id,
                announcement.parent_id,
                announcement.call_path
            ),
            (span, caller, call_path)
        );
        assert_eq!((&*announcement.method, &*announcement.url), ("POST", &*url));
        assert_eq!(
            shape(announcement.request.as_ref()),
            Some(json!({"request": {
                "method": "POST",
                "url": url,
                "headers": {
                    "content-type": "application/json",
                    "x-api-key": network::hash("sk-secret"),
                },
                "body": r#"{"model":"m"}"#,
            }}))
        );
        for (event, name, ticks) in [
            (connection, "connection", 10),
            (data, "data", 11),
            (end, "end", 12),
            (read, "await", 13),
        ] {
            assert_eq!(
                (event.span, event.name.as_str(), event.at),
                (span, name, at(ticks))
            );
        }
        assert_eq!(
            shape(connection.payload.as_ref()),
            Some(json!({"status": 200, "headers": {
                "request-id": "req_1",
                "set-cookie": network::hash("session"),
            }}))
        );
        assert_eq!(
            shape(data.payload.as_ref()),
            Some(json!({"event": "message_stop", "data": "{}", "id": null}))
        );
        assert!(end.payload.is_none() && read.payload.is_none());
        assert_eq!(
            (completion.span, completion.completed_at, completion.outcome),
            (span, at(13), InvocationOutcome::Ok)
        );
        assert!(completion.error.is_none());
    }

    #[test]
    fn network_span_errors_bodies_off_and_low_level() {
        use sys_types::network::NetworkEventKind;

        let mut state = test_state(
            Arc::new(TelemetryPolicies::new().with_http_bodies(false)),
            test_clock(),
        );
        // Outside any frame: the thread is the parent.
        let span = state.open_network_span(&request()).unwrap();
        let at = ClockInstant::from_ticks;
        state.network_event(
            span,
            at(5),
            &NetworkEventKind::Body(b"secret".to_vec().into()),
        );
        unsafe {
            state.close_network_span(
                span,
                at(6),
                InvocationOutcome::Errored,
                Some(Value::int(3)),
                &[],
            );
        }
        let records = network_records(&state);
        let [
            SpanRecord::NetworkSpanAnnouncement(announcement),
            SpanRecord::NetworkEvent(data),
            SpanRecord::NetworkSpanCompletion(completion),
        ] = records.as_slice()
        else {
            panic!("unexpected network records: {records:?}");
        };
        assert_eq!(
            (announcement.parent_id, announcement.call_path),
            (state.thread.id, CallPathId::ROOT)
        );
        let captured = shape(announcement.request.as_ref()).unwrap();
        assert!(captured["request"].get("body").is_none());
        assert_eq!(captured["request"]["method"], "POST");
        assert_eq!((data.name.as_str(), data.payload.is_none()), ("data", true));
        assert_eq!(completion.outcome, InvocationOutcome::Errored);
        assert!(matches!(
            completion
                .error
                .as_ref()
                .and_then(btel_snapshot::Snapshot::value),
            Some(btel_snapshot::SnapshotValue::Int(3))
        ));

        let mut low = test_state(
            Arc::new(TelemetryPolicies::with_auto_level(AutoTelemetryLevel::Low)),
            test_clock(),
        );
        assert_eq!(low.open_network_span(&request()), None);
        assert!(network_records(&low).is_empty());
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
            type_param_names: Vec::new(),
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

    /// Records of one generic call made with `type_args`: its announced
    /// type arguments, and whether its completion depends on the
    /// announcement.
    fn type_args_of(
        function: &Function,
        inputs: bool,
        type_args: CallTypeArgs<'_>,
    ) -> (Option<serde_json::Value>, bool) {
        let mut state = test_state(Arc::new(TelemetryPolicies::new()), test_clock());
        let trace = TraceConfig {
            mode: Some(InvocationMode::Span),
            inputs: Some(inputs),
            ..TraceConfig::default()
        };
        // SAFETY: no heap values; the registration hook dereferences nothing.
        let frame = unsafe {
            state.enter_bytecode_with_trace(
                function,
                HeapPtr::null(),
                None,
                0,
                false,
                &[],
                Some(trace),
                type_args,
                |_, _| (None, function.telemetry_function_id.unwrap()),
            )
        }
        .unwrap();
        unsafe {
            state.complete_invocation(frame, function, InvocationOutcome::Ok, None);
        }
        let announced = state.span_records().iter().find_map(|record| match record {
            SpanRecord::FunctionSpanAnnouncement {
                captured_type_args, ..
            } => Some(shape(captured_type_args.as_ref())),
            _ => None,
        });
        let completed = state.span_records().last().unwrap().completion().unwrap();
        (announced.unwrap(), completed.requires_announcement)
    }

    #[test]
    fn a_generic_call_records_its_type_args_by_name_whether_or_not_inputs_are() {
        let mut generic = function(FunctionKind::Bytecode, None);
        // A method of `Box<T>`: the class's parameter rides on the receiver,
        // the method's own is passed by the call.
        generic.type_param_names = vec!["T".into(), "U".into()];
        let carried = [bex_vm_types::RealizedTy::int()];
        let passed = [bex_vm_types::RealizedTy::string()];
        let both = CallTypeArgs {
            carried: &carried,
            passed: &passed,
        };
        for inputs in [false, true] {
            assert_eq!(
                type_args_of(&generic, inputs, both),
                (
                    Some(serde_json::json!({"T": "type: int", "U": "type: string"})),
                    true
                ),
                "inputs captured: {inputs}"
            );
        }
        // Without type arguments nothing is recorded, and the completion
        // does not wait for an announcement.
        let plain = function(FunctionKind::Bytecode, None);
        assert_eq!(
            type_args_of(&plain, false, CallTypeArgs::default()),
            (None, false)
        );
        // A slot the compiler did not name is left out, never given a key.
        let mut partial = generic;
        partial.type_param_names.truncate(1);
        assert_eq!(
            type_args_of(&partial, false, both),
            (Some(serde_json::json!({"T": "type: int"})), true)
        );
        partial.type_param_names.clear();
        assert_eq!(type_args_of(&partial, false, both), (None, false));
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
        assert_eq!(size_of::<crate::vm::BytecodeFrame>(), 104 + pointer_bytes);
        assert_eq!(size_of::<crate::vm::NativeFrame>(), 24 + pointer_bytes);
        assert_eq!(size_of::<crate::vm::Frame>(), 104 + pointer_bytes);
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
                TimingRecord::ThreadSelected { .. } | TimingRecord::SysOpTime { .. } => None,
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
    fn capture_requests_survive_policy_changes_and_late_promotion() {
        for mode in [
            None,
            Some(InvocationMode::Timing),
            Some(InvocationMode::Span),
        ] {
            for initial in [false, true] {
                for current in [false, true] {
                    for override_ in [None, Some(false), Some(true)] {
                        for outcome in [
                            InvocationOutcome::Ok,
                            InvocationOutcome::Errored,
                            InvocationOutcome::Cancelled,
                        ] {
                            let function = function(FunctionKind::Bytecode, None);
                            let mut state =
                                test_state(Arc::new(TelemetryPolicies::new()), test_clock());
                            state.start_thread();
                            state
                                .set_policy(
                                    &function,
                                    TelemetryPolicy {
                                        capture_output: initial,
                                        capture_error: initial,
                                        ..TelemetryPolicy::NONE
                                    },
                                )
                                .unwrap();
                            let frame = trace_entry(
                                &mut state,
                                &function,
                                Some(TraceConfig {
                                    mode,
                                    output: override_,
                                    error: override_,
                                    ..TraceConfig::default()
                                }),
                            )
                            .unwrap();
                            let initially_span = frame.is_span();
                            state
                                .set_policy(
                                    &function,
                                    TelemetryPolicy {
                                        span_from_entry: true,
                                        capture_output: current,
                                        capture_error: current,
                                        ..TelemetryPolicy::NONE
                                    },
                                )
                                .unwrap();
                            unsafe {
                                state.complete_invocation(
                                    frame,
                                    &function,
                                    outcome,
                                    Some(Value::int(7)),
                                );
                            }
                            let recorded =
                                state.span_records().last().unwrap().completion().unwrap();
                            assert_eq!(recorded.outcome, outcome);
                            assert_eq!(recorded.late, !initially_span);
                            assert_eq!(
                                recorded.captured_value.map(|snapshot| matches!(
                                    snapshot.roots()[0],
                                    btel_snapshot::SnapshotValue::Int(7)
                                )),
                                (override_ == Some(true) || initial || current).then_some(true),
                                "{mode:?}/{initial}/{current}/{override_:?}/{outcome:?}",
                            );
                        }
                    }
                }
            }
        }
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
        // It ended without running: it began running when it completed.
        assert!(
            matches!(child.span_records(), [SpanRecord::ThreadSpanRunning { at }, SpanRecord::ThreadSpanCompletion {
            id, parent_id: Some(parent_id), spawn_call_path, clock, outcome: InvocationOutcome::Errored,
            completed_at, ..
        }] if *id == child_id && *parent_id == context.parent_id && at == completed_at
            && *spawn_call_path == context.spawn_call_path && Arc::ptr_eq(clock, &context.clock))
        );
    }
}
