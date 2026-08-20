//! BEX Engine - The async runtime that drives the VM.
//!
//! This crate provides `BexEngine`, which executes BAML programs by:
//! - Creating a VM instance for each function call
//! - Driving the VM execution loop
//!
//! The architecture is inspired by Deno's embedding of V8:
//! - VM executes synchronously until it needs external I/O
//! - Engine manages async operations and feeds results back
//! - Communication via `VmExecState` enum (yield points)
//!
//! # External Operations
//!
//! External operations (LLM calls, HTTP requests, file I/O) are dispatched via
//! the `SysOp` enum using static dispatch. This avoids dynamic dispatch
//! overhead and makes the system more macro-friendly.
//!
//! # Resources
//!
//! Resources (file handles, connections, etc.) are stored in a `ResourceRegistry`.
//! External ops can store resources and return their ID to the VM. Later ops
//! can retrieve resources by ID. The VM only sees integer IDs.
//!
//! # Garbage Collection Coordination
//!
//! The engine coordinates GC through a [`HeapPermitManager`]. Every
//! `call_function` invocation holds an [`ActiveHeapPermit`] for the duration
//! of its VM's execution; the permit is released at async safepoints (e.g.
//! during `Await`). GC is a "request-and-wait" operation:
//!
//! 1. **Trigger**: [`BexEngine::collect_garbage`] calls
//!    [`HeapPermitManager::request_park`], which drains all semaphore permits.
//! 2. **Park**: running VMs release their permits at the next safepoint; new
//!    `call_function` invocations block in `HeapPermitManager::new_permit`
//!    because the manager's holders mutex is held by the GC.
//! 3. **Collect roots**: [`HeapGuard`] iterates the live permit holders (via
//!    weak references) and calls each `RootHaver::collect_roots`, unioned
//!    with `BexHeap::collect_handle_roots` for FFI-held objects.
//! 4. **GC**: `BexHeap::collect_garbage_generational` runs under the guard;
//!    produces a forwarding map.
//! 5. **Fixup**: `HeapGuard` calls each parked holder's
//!    `RootHaver::forward_roots`. The `BexVm` impl of `forward_roots` also
//!    invalidates the VM's TLAB so post-GC allocations refill from the new
//!    Gen0 cursor.
//! 6. **Resume**: dropping the `HeapGuard` releases the semaphore; parked
//!    VMs re-acquire and continue.
//!
//! ## Safety Invariants
//!
//! - A VM can only mutate its own heap state while holding an
//!   [`ActiveHeapPermit`]. GC cannot start until every active permit has
//!   been released.
//! - Handles are registered in `BexHeap::handles` before any GC could
//!   observe them, and the write lock on that table serializes against
//!   GC's `update_handles`.
//! - New `call_function` invocations block on the holders mutex during
//!   GC, so no fresh permit enters circulation mid-collection.
//!
//! # Unsafe Code
//!
//! This module uses unsafe code for:
//! - `PermitCell<T>` Send/Sync: single-threaded access is enforced by the
//!   semaphore/holders-mutex pair.
//! - Direct heap access during value conversion (always under an active
//!   permit, witnessed by a `PermitProof` parameter).
//!
//! Safety is ensured by the permit/guard coordination system described above.

#![allow(unsafe_code)]

mod conversion;
mod function_call_context;
mod future;
mod inbound_config;
pub mod logger;
mod thread;
pub mod trace_heap;
mod trace_value_encode;
use std::{
    collections::{HashMap, VecDeque},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use ::bex_heap::{HeapPermit as _, Tlab};
// Re-export event types for callers.
use ::bex_vm_types::{RootHaver, types::FutureId};
use ::core::sync::atomic::AtomicBool;
use async_trait::async_trait;
use bex_events::prof::backend::{
    BoundaryEndStatus, BoundaryHandle, ProfilerSession, RootAdmission, RootProfileIntent,
    RootProfiler, ValueLossReason, ValueRole,
};
pub use bex_events::{
    FunctionMetadataTable, ProgramMetadata,
    ids::{
        BexCallId, BexThreadId, CallRef, EngineId, FunctionId, ProcessEuid, ProgramId, ThreadRef,
    },
};
pub use bex_external_types::{BexExternalValue, RuntimeTy, TypeName, UnionMetadata};
use bex_heap::BexHeap;
// Re-export GcStats for users of the engine
pub use bex_heap::GcStats;
pub use bex_heap::{ActiveHeapPermit, HeapGuard, HeapPermitManager, InactiveHeapPermit};
use bex_vm::{
    BexVm, VmCallCaptureKind, VmCallInputCapture, VmCallInputCaptureHook, VmCaptureMask,
    VmEventSourceLocation, VmExecState,
};
use bex_vm_types::{
    FunctionMeta, FunctionOrigin, GlobalIndex, GlobalPool, HeapPtr, Object, SharedGlobals, SysOp,
    TaskGroupInner, UnscheduledFuture, Value, ValueKind, VmGlobals,
};
pub use conversion::test_arg_to_external;
// Re-export CancellationToken for callers.
pub use function_call_context::{
    BoundaryContext, BoundaryStorageContext, FunctionCallContext, FunctionCallContextBuilder,
};
pub use inbound_config::{InboundUnionAmbiguityPolicy, register_inbound_union_ambiguity_policy};
use indexmap::IndexMap;
pub use sys_types::{CallId, ClassDefinition, ClassFieldDefinition};
use sys_types::{OpError, SysOpResult};
use thiserror::Error;
pub use tokio_util::sync::CancellationToken;

/// Compiler implementation injected by an assembly crate above the runtime.
/// Every call receives only owned data and returns an owned, compiler-neutral
/// artifact, so no compiler database can leak into the engine or heap.
pub trait RuntimeCompiler: Send + Sync + 'static {
    fn compile(
        &self,
        request: bex_vm_types::RuntimeCompileRequest,
    ) -> Result<bex_vm_types::RuntimeCompileArtifact, Vec<bex_vm_types::RuntimeCompileDiagnostic>>;
}

/// Runtime-owned schema data for one sys-op plus handles that keep every
/// contributing package stable across the async permit release/GC window.
struct RuntimeSchemaOverlay {
    classes: indexmap::IndexMap<baml_type::TypeName, sys_types::ClassDefinition>,
    enums: indexmap::IndexMap<baml_type::TypeName, sys_types::EnumDefinition>,
    named_owners: indexmap::IndexMap<String, bex_external_types::Handle>,
}

/// Sets the VM park request flag for the lifetime of a pending GC park request.
///
/// In particular, dropping the future returned by [`BexEngine::collect_garbage`]
/// while it is waiting for active heap permits must not leave every VM believing
/// that a park is still requested.
#[cfg(not(target_arch = "wasm32"))]
struct ParkRequestGuard {
    park_requested: Arc<AtomicBool>,
}

#[cfg(not(target_arch = "wasm32"))]
impl ParkRequestGuard {
    fn new(park_requested: Arc<AtomicBool>) -> Self {
        park_requested.store(true, Ordering::Relaxed);
        Self { park_requested }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for ParkRequestGuard {
    fn drop(&mut self) {
        self.park_requested.store(false, Ordering::Relaxed);
    }
}

pub use crate::{
    future::{FutureManager, FutureManagerGuard, FutureManagerInner},
    thread::BexThread,
};
use crate::{
    logger::{TraceLogMetadata, TraceLogger},
    trace_heap::TraceHeap,
    trace_value_encode::encode_trace_snapshot_body_bounded,
};

const SPAWN_CLOSURE_FQN: &str = "baml.<spawn-closure>";
const SPAWN_CLOSURE_DISPLAY_NAME: &str = "<spawn-closure>";

/// Definitions attached to runtime-minted type arguments for one sys-op call.
///
/// The definition metadata is copied into the sys-op context for prompt/SAP
/// work. Handles keep the corresponding heap definitions rooted while an
/// asynchronous sys-op has released the VM's heap permit, and form the landing
/// side table used to allocate parsed values with their original nominal identity.
#[derive(Default)]
struct RuntimeTypeOverlay {
    class_definitions: indexmap::IndexMap<baml_type::TypeName, sys_types::ClassDefinition>,
    class_handles: indexmap::IndexMap<String, bex_external_types::Handle>,
    enum_definitions: indexmap::IndexMap<baml_type::TypeName, sys_types::EnumDefinition>,
    enum_handles: indexmap::IndexMap<String, bex_external_types::Handle>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnhandledSpawnError {
    pub report_id: usize,
    pub value: BexExternalValue,
    pub trace: Vec<bex_vm::StackFrame>,
    pub cancelled: bool,
}

impl UnhandledSpawnError {
    pub fn into_engine_error(self) -> EngineError {
        EngineError::UnhandledThrow {
            value: Box::new(self.value),
            trace: self.trace,
        }
    }
}

pub type UnhandledSpawnErrorHandler = Arc<dyn Fn(UnhandledSpawnError) + Send + Sync + 'static>;

#[derive(Clone)]
enum RootedUnhandledValue {
    Inline(Value),
    Handle(bex_external_types::Handle),
}

#[derive(Clone)]
struct RootedUnhandledSpawnError {
    report_id: usize,
    value: RootedUnhandledValue,
    trace: Vec<bex_vm::StackFrame>,
    cancelled: bool,
}

struct UnhandledSpawnState {
    handler: Option<UnhandledSpawnErrorHandler>,
    queued: VecDeque<UnhandledSpawnError>,
    delivering: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EngineLifecycle {
    Running,
    Closing,
    Closed,
}

/// Reserved header-table row for calls whose function identity cannot be
/// resolved (e.g. runtime-synthesized functions absent from the compile-time
/// pool). Ring `CallFunction` records do NOT reference this row today:
/// unresolvable callees are stamped `function_id: 0` ("unassigned" — see
/// `BexVm::prof_enter_call`) and the consumer transcodes ids verbatim. The
/// row (id = max real id + 2) is reserved in every header for consumers that
/// want a display bucket for such records; re-pointing the id-0 paths at it
/// is an open cross-team item.
const UNKNOWN_FUNCTION_FQN: &str = "baml.<unknown-function>";
const UNKNOWN_FUNCTION_DISPLAY_NAME: &str = "<unknown-function>";

/// Outcome of running a single [`BexThread`] to termination.
///
/// Used by [`BexEngine::run_thread_event_loop`] to distinguish whether the
/// thread is the root (whose value must be returned to the host) or a
/// spawned child (whose value has already been written into the
/// [`FutureManager`] for the awaiter to pick up).
#[allow(clippy::large_enum_variant)]
pub(crate) enum ThreadOutcome {
    /// Root thread completed normally; return this value to the host.
    RootValue(BexExternalValue),
    /// Spawned child thread settled — `FutureManager` already updated.
    SettledChild(ChildSettleKind),
}

/// How a spawned child settled its future — drives the profiling
/// `EndThread` status (children settle as `Ok(SettledChild)` even when
/// cancelled or errored, so the kind must travel in the outcome).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChildSettleKind {
    Fulfilled,
    Cancelled,
    Errored,
}

struct ThreadBoundaryLeaseGuard {
    session: Arc<ProfilerSession>,
    lease: Option<bex_events::prof::backend::BoundaryThreadLease>,
}

impl ThreadBoundaryLeaseGuard {
    fn new(
        session: Arc<ProfilerSession>,
        lease: bex_events::prof::backend::BoundaryThreadLease,
    ) -> Self {
        Self {
            session,
            lease: Some(lease),
        }
    }

    fn handle(&self) -> Option<bex_events::prof::backend::BoundaryHandle> {
        self.lease
            .as_ref()
            .map(bex_events::prof::backend::BoundaryThreadLease::handle)
    }
}

impl Drop for ThreadBoundaryLeaseGuard {
    fn drop(&mut self) {
        let Some(mut lease) = self.lease.take() else {
            return;
        };
        if let Some(registry) = self.session.boundary_registry() {
            registry.finish_thread(&mut lease);
        }
    }
}

/// §7 follow-up 11: closes a spawned thread's profiling lifecycle if the
/// task future is dropped before its event loop takes over (abnormal host
/// teardown — e.g. dropping the runtime while the task is queued on a
/// `TaskGroup` ticket or parked on the heap permit). By that point the
/// Spawn arm has emitted `StartThread` and `set_entry_point` the entry
/// `CallFunction`; nothing else would close them. Armed at the top of the
/// task body; also the closer for the queued-then-cancelled early return;
/// defused once `run_thread_event_loop` is entered (its wrapper owns
/// `EndThread` on every return path from there). Emits via the TLS ring
/// lookup, so dropping from any thread is sound. Drops *after* the loop
/// has started (mid-await teardown) are out of scope — the artifact is
/// torn-tail territory there anyway.
struct SpawnProfCloser {
    engine: Arc<BexEngine>,
    prof_thread_id: u64,
    entry_call_id: bex_events::ids::BexCallId,
    awaited: Option<(u64, u32)>,
    armed: bool,
    boundary_lease: Option<ThreadBoundaryLeaseGuard>,
}

impl SpawnProfCloser {
    fn defuse(&mut self) {
        self.armed = false;
    }
}

impl Drop for SpawnProfCloser {
    fn drop(&mut self) {
        use bex_events::prof::record::{FunctionEndStatus, RawRecord, ThreadEndStatus};
        if !self.armed || !self.engine.profiler_session.is_on() {
            return;
        }
        let thread_id = bex_events::ids::BexThreadId(self.prof_thread_id);
        let boundary_handle = self
            .boundary_lease
            .as_ref()
            .and_then(ThreadBoundaryLeaseGuard::handle);
        if self.entry_call_id.0 != 0 {
            let ts_ticks = bex_events::prof::clock::now_ticks();
            let committed = match self.awaited {
                Some((await_ns, await_count)) => {
                    self.engine.prof_emit(&RawRecord::EndFunctionAwaited {
                        status: FunctionEndStatus::Cancelled,
                        thread_id,
                        call_id: self.entry_call_id,
                        ts_ticks,
                        await_ns,
                        await_count,
                    })
                }
                None => self.engine.prof_emit(&RawRecord::EndFunction {
                    // Dropped-before-run is a cancellation, not a failure.
                    status: FunctionEndStatus::Cancelled,
                    thread_id,
                    call_id: self.entry_call_id,
                    ts_ticks,
                }),
            };
            if !committed {
                self.engine.prof_record_transport_loss(boundary_handle);
            }
        }
        if !self.engine.prof_emit(&RawRecord::EndThread {
            status: ThreadEndStatus::Cancelled,
            thread_id,
            ts_ticks: bex_events::prof::clock::now_ticks(),
        }) {
            self.engine.prof_record_transport_loss(boundary_handle);
        }
    }
}

// ============================================================================
// Engine Types
// ============================================================================

/// Information about a user-callable function, used by `baml run --list`.
#[derive(Debug, Clone)]
pub struct UserFunctionInfo {
    pub qualified_name: String,
    pub display_name: String,
    pub origin: FunctionOrigin,
    pub param_names: Vec<String>,
    pub param_types: Vec<RuntimeTy>,
    pub param_has_default: Vec<bool>,
    pub return_type: RuntimeTy,
    pub display_type_params: Vec<String>,
    pub display_param_types: Vec<String>,
    pub display_return_type: String,
    /// Filesystem path of the source file containing the function.
    /// Empty string for builtins and synthesized functions.
    /// Exposed for BEP-027 §"`baml.argv`": `argv[1]` under root-main `baml run`
    /// is the path to the file containing `main`.
    pub source_file: String,
    /// `true` when the function carries `FunctionMeta::Llm` and was declared
    /// with an LLM client and backtick prompt.
    /// compiler synthesized the LLM dispatch body. Surfaced here so
    /// `baml run --list` can annotate LLM functions inline without
    /// reaching back into the heap to inspect `body_meta`.
    pub is_llm: bool,
}

pub struct BexCallResult {
    pub value: Result<BexExternalValue, EngineError>,
    pub entry_call_ref: CallRef,
}

#[derive(Clone)]
struct RootValueCaptureContext {
    call_ref: CallRef,
    #[cfg(not(target_arch = "wasm32"))]
    backend: BackendValueCaptureContext,
}

#[derive(Clone)]
struct LogCaptureContext {
    boundary_id: bex_events::ids::BoundaryId,
    logger: TraceLogger,
}

#[derive(Clone)]
struct CallValueCaptureContext {
    #[cfg(not(target_arch = "wasm32"))]
    backend: BackendValueCaptureContext,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
struct BackendValueCaptureContext {
    session: Arc<ProfilerSession>,
    boundary: BoundaryHandle,
}

#[cfg(not(target_arch = "wasm32"))]
impl BackendValueCaptureContext {
    fn capture_with(
        &self,
        call_ref: CallRef,
        role: ValueRole,
        manual_eligible: bool,
        copy: impl FnOnce(
            &mut bex_events::prof::backend::Reservation,
        ) -> Result<crate::trace_heap::TraceSnapshot, ValueLossReason>,
    ) {
        let mut reservation = match self.session.reserve_value_work(manual_eligible) {
            Ok(reservation) => reservation,
            Err(reason) => {
                self.session.record_value_loss(
                    self.boundary,
                    call_ref,
                    role,
                    reason,
                    manual_eligible,
                );
                return;
            }
        };
        let snapshot = match copy(&mut reservation) {
            Ok(snapshot) => snapshot,
            Err(reason) => {
                self.session.record_value_loss(
                    self.boundary,
                    call_ref,
                    role,
                    reason,
                    manual_eligible,
                );
                return;
            }
        };
        let Some(single_value_bytes) = self.session.single_value_bytes() else {
            self.session.record_value_loss(
                self.boundary,
                call_ref,
                role,
                ValueLossReason::StoreUnavailable,
                manual_eligible,
            );
            return;
        };
        match encode_trace_snapshot_body_bounded(&snapshot, &mut reservation, single_value_bytes) {
            Ok(body) => self.session.record_encoded_value(
                self.boundary,
                call_ref,
                role,
                &body,
                manual_eligible,
                reservation,
            ),
            Err(reason) => self.session.record_value_loss(
                self.boundary,
                call_ref,
                role,
                reason,
                manual_eligible,
            ),
        }
    }

    fn capture_error(
        &self,
        event: bex_vm::VmErrorCaptureEvent,
        copy: impl FnOnce(
            &mut bex_events::prof::backend::Reservation,
            Value,
        ) -> Result<crate::trace_heap::TraceSnapshot, ValueLossReason>,
    ) {
        let bex_vm::VmErrorCaptureEvent {
            id,
            value,
            manual_eligible: _,
            mut reservation,
        } = event;
        let snapshot = match copy(&mut reservation, value) {
            Ok(snapshot) => snapshot,
            Err(reason) => {
                self.session.record_error_value_loss(id, reason);
                return;
            }
        };
        let Some(single_value_bytes) = self.session.single_value_bytes() else {
            self.session
                .record_error_value_loss(id, ValueLossReason::StoreUnavailable);
            return;
        };
        match encode_trace_snapshot_body_bounded(&snapshot, &mut reservation, single_value_bytes) {
            Ok(body) => self
                .session
                .record_encoded_error_value(id, &body, reservation),
            Err(reason) => self.session.record_error_value_loss(id, reason),
        }
    }
}

impl CallValueCaptureContext {
    fn input_capture_hook(&self) -> Arc<dyn VmCallInputCaptureHook> {
        Arc::new(EngineCallInputCaptureHook {
            #[cfg(not(target_arch = "wasm32"))]
            backend: self.backend.clone(),
        })
    }
}

struct EngineCallInputCaptureHook {
    #[cfg(not(target_arch = "wasm32"))]
    backend: BackendValueCaptureContext,
}

impl VmCallInputCaptureHook for EngineCallInputCaptureHook {
    fn capture_call_input(&self, capture: VmCallInputCapture<'_>) {
        #[cfg(not(target_arch = "wasm32"))]
        self.backend.capture_with(
            CallRef {
                process_euid: capture.call.process_euid,
                engine_id: capture.call.engine_id,
                thread_id: capture.call.thread_id,
                call_id: capture.call.call_id,
            },
            ValueRole::Input,
            capture.manual,
            |reservation| {
                TraceHeap::copy_named_values_bounded(
                    capture.heap,
                    capture.permit,
                    capture.entries,
                    reservation,
                )
            },
        );
    }
}

/// Internal call argument after host binding has distinguished omission from
/// explicit null. This is intentionally not part of the external bridge value
/// surface.
#[derive(Debug, Clone)]
pub enum BexCallArg {
    Provided(Box<BexExternalValue>),
    OmittedDefault,
}

enum CallableArgs {
    Positional(Vec<BexExternalValue>),
    Named {
        required: indexmap::IndexMap<String, BexExternalValue>,
        optional: indexmap::IndexMap<String, BexExternalValue>,
    },
}

// ============================================================================
// Span Tracking (per-invocation, NOT on Arc<BexEngine>)
// ============================================================================

/// RAII guard that owns a `CallId` slot in `BexEngine::active_calls`. The
/// slot is inserted by [`Self::register`] and removed on drop, matching
/// the lifetime of the `call_function` invocation. The constructor and
/// the registry insert are atomic — there is no window during which the
/// slot exists without an owning guard, so a panic at the registration
/// site cannot leak entries.
#[derive(Clone)]
struct ActiveCall {
    cancel: CancellationToken,
    pending: bool,
}

struct ActiveCallGuard {
    engine: Arc<BexEngine>,
    call_id: CallId,
}

struct ShutdownGuard {
    engine: Arc<BexEngine>,
    completed: bool,
}

impl ShutdownGuard {
    fn complete(mut self) {
        *self
            .engine
            .lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = EngineLifecycle::Closed;
        self.completed = true;
        self.engine.lifecycle_changed.notify_waiters();
    }
}

impl Drop for ShutdownGuard {
    fn drop(&mut self) {
        if self.completed {
            return;
        }
        let mut lifecycle = self
            .engine
            .lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *lifecycle == EngineLifecycle::Closing {
            *lifecycle = EngineLifecycle::Running;
        }
        drop(lifecycle);
        self.engine.lifecycle_changed.notify_waiters();
    }
}

impl ActiveCallGuard {
    /// Atomically reserve `call_id` in `engine.active_calls` and return a
    /// guard that will release the slot on drop. Returns
    /// [`EngineError::DuplicateCallId`] if the id is already in flight.
    fn register(
        engine: Arc<BexEngine>,
        call_id: CallId,
        cancel: CancellationToken,
    ) -> Result<(Self, CancellationToken), EngineError> {
        let lifecycle = engine
            .lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *lifecycle != EngineLifecycle::Running {
            return Err(EngineError::ShuttingDown);
        }
        let mut map = engine
            .active_calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let cancel = match map.get_mut(&call_id) {
            Some(existing) if existing.pending => {
                existing.pending = false;
                existing.cancel.clone()
            }
            Some(_) => return Err(EngineError::DuplicateCallId { call_id }),
            None => {
                map.insert(
                    call_id,
                    ActiveCall {
                        cancel: cancel.clone(),
                        pending: false,
                    },
                );
                cancel
            }
        };
        drop(map);
        drop(lifecycle);
        Ok((Self { engine, call_id }, cancel))
    }

    fn reserve_cancelled(engine: &BexEngine, call_id: CallId) {
        let lifecycle = engine
            .lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut map = engine
            .active_calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match map.get(&call_id) {
            Some(existing) => existing.cancel.cancel(),
            None if *lifecycle == EngineLifecycle::Running => {
                let cancel = CancellationToken::new();
                cancel.cancel();
                map.insert(
                    call_id,
                    ActiveCall {
                        cancel,
                        pending: true,
                    },
                );
            }
            None => {}
        }
        drop(map);
        drop(lifecycle);
    }
}

impl Drop for ActiveCallGuard {
    fn drop(&mut self) {
        // `unwrap_or_else(into_inner)` so a poisoned mutex during unwind
        // doesn't double-panic.
        let mut map = self
            .engine
            .active_calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        map.remove(&self.call_id);
        let no_active_calls = map.values().all(|call| call.pending);
        drop(map);
        if no_active_calls {
            self.engine.lifecycle_changed.notify_waiters();
        }
    }
}

/// Errors that can occur during engine execution.
#[derive(Debug, PartialEq, Error, Clone)]
pub enum EngineError {
    #[error("BAML engine is shutting down")]
    ShuttingDown,

    #[error("Function call with ID {call_id} not found")]
    FunctionCallNotFound { call_id: CallId },

    #[error("Future with ID {future_id} not found")]
    FutureNotFound { future_id: FutureId },

    #[error("Function not found: {name}")]
    FunctionNotFound { name: String },

    /// Function exists, but its `FunctionKind` is not invokable as an
    /// engine entry point. Only bytecode functions can be called via
    /// [`BexEngine::call_function`] / [`BexEngine::call_function_bound_args`]:
    /// native (`$rust_function`) entries would re-enter the VM through
    /// `YieldToCall` with no bytecode frame to return to. Sysops + builtins
    /// reach their natives from inside a calling bytecode body.
    #[error("Function `{name}` is not invokable as an entry point (kind: {kind})")]
    NotInvokableAsEntry { name: String, kind: String },

    #[error("VM internal error: {0}")]
    VmInternalError(bex_vm::errors::VmInternalError),

    #[error("{}", format_vm_internal_error(source, trace))]
    TracedVmInternalError {
        source: bex_vm::errors::VmInternalError,
        trace: Vec<bex_vm::StackFrame>,
    },

    /// Either a BAML panic or a BAML error value.
    #[error("{}", format_unhandled_throw(value, trace))]
    UnhandledThrow {
        value: Box<BexExternalValue>,
        trace: Vec<bex_vm::StackFrame>,
    },

    /// Clean process-termination request from `baml.sys.exit(code)`.
    /// The caller is expected to honor this as the process exit code.
    /// BAML `int` is `i64`, so the signal carries the full value; the
    /// caller clamps into its shell's range (typically 0..=255 on Unix).
    #[error("baml.sys.exit({code})")]
    Exit { code: i64 },

    #[error("Cannot convert object of type {type_name}")]
    CannotConvert { type_name: String },

    #[error("Type mismatch: {message}")]
    TypeMismatch { message: String },

    #[error("Schema inconsistency: {message}")]
    SchemaInconsistency { message: String },

    #[cfg(feature = "heap_debug")]
    #[error("Snapshot not possible for type: {type_name}")]
    CannotSnapshot { type_name: String },

    #[error("A function call with ID {call_id} is already in progress")]
    DuplicateCallId { call_id: CallId },

    #[error("Package initialization failed: {0}")]
    InitFailed(String),

    #[error("{0}")]
    Other(String),
}

fn format_vm_internal_error(
    err: &bex_vm::errors::VmInternalError,
    trace: &[bex_vm::StackFrame],
) -> String {
    use std::fmt::Write;
    let mut out = bex_vm::format_traceback(
        trace
            .iter()
            .map(|f| (f.file_path.as_str(), f.error_line, f.function_name.as_str())),
    );
    write!(out, "VM internal error: {err}").unwrap();
    out
}

/// Recognize an uncaught `baml.panics.Exit { code }` and pull its `code`
/// field out. Returns `None` for any other value — the caller should fall
/// back to the normal unhandled-throw path.
///
/// Exit lives in the regular panic class hierarchy so BAML code can catch
/// it (like Python's `SystemExit`); at the engine boundary we recognize it
/// by class tag rather than routing it through a separate `VmError`
/// variant, so the VM's unwinder stays ignorant of which panic classes
/// are "special" and the special-casing lives in exactly one place — here.
fn extract_exit_code(value: &BexExternalValue) -> Option<i64> {
    match value {
        BexExternalValue::Instance {
            class_name, fields, ..
        } if class_name == bex_vm_types::PanicClass::Exit.fqn() => match fields.get("code")? {
            BexExternalValue::Int(code) => Some(*code),
            _ => None,
        },
        _ => None,
    }
}

fn format_unhandled_throw(value: &BexExternalValue, trace: &[bex_vm::StackFrame]) -> String {
    use std::fmt::Write;
    let mut out = bex_vm::format_traceback(trace.iter().map(|loc| {
        (
            loc.file_path.as_str(),
            loc.error_line,
            loc.function_name.as_str(),
        )
    }));
    write!(out, "uncaught throw: {}", value.render_readable()).unwrap();
    out
}

/// Fully-qualified name of the cancellation panic class.
pub const CANCELLED_PANIC_CLASS: &str = "baml.panics.Cancelled";

/// True iff `err` is an unhandled `baml.panics.Cancelled` panic.
///
/// Centralizes the cancellation-classification logic that bridges (`bridge_cffi`,
/// `bridge_typescript`, `bridge_python`, `bridge_wasm`) and `baml_lsp_server` need
/// for mapping `EngineError` → host-specific cancellation indicator.
pub fn is_cancelled_engine_error(err: &EngineError) -> bool {
    matches!(
        err,
        EngineError::UnhandledThrow { value, .. }
            if matches!(
                value.as_ref(),
                BexExternalValue::Instance { class_name, .. }
                    if class_name == CANCELLED_PANIC_CLASS
            )
    )
}

/// Synthesize an `EngineError::UnhandledThrow` representing a cancellation.
///
/// Used when the engine produces a cancellation outside an active VM (pre-call
/// fail-fast and post-completion "cancel wins" race). Mirrors the shape of a
/// `baml.panics.Cancelled` instance produced by the VM's `Await` opcode so
/// host bridges can detect both cases by inspecting `class_name`.
///
/// Exposed publicly so bridges and other layers that need to surface a
/// cancellation outside the engine's normal control flow can produce one
/// in the canonical shape rather than rolling their own.
pub fn cancelled_unhandled_throw() -> EngineError {
    let mut fields = indexmap::IndexMap::new();
    fields.insert(
        "message".to_string(),
        BexExternalValue::String("operation cancelled".into()),
    );
    EngineError::UnhandledThrow {
        value: Box::new(BexExternalValue::Instance {
            class_name: CANCELLED_PANIC_CLASS.to_string(),
            type_args: vec![],
            fields,
        }),
        trace: Vec::new(),
    }
}

// ============================================================================
// BexEngine
// ============================================================================

/// The async runtime that drives VM execution.
///
/// `BexEngine` is the main entry point for executing BAML programs.
/// It owns the compiled program and the unified heap shared across all VMs.
///
/// # Thread Safety and Concurrent Execution
///
/// `BexEngine` supports concurrent function execution. Each `call_function`
/// invocation creates its own `BexVm` with an exclusive Thread-Local Allocation
/// Buffer (TLAB), enabling parallel execution without contention.
///
/// ## Why Concurrent Calls Are Safe
///
/// - **No global mutable state**: BAML has no global variables, so independent
///   function calls cannot race with each other.
///
/// - **TLAB isolation**: Each VM allocates into its own exclusive heap region.
///   The only synchronization is atomic TLAB chunk allocation (rare operation,
///   approximately once per 1024 allocations).
///
/// - **Lock-free field writes**: Object field mutations are direct memory writes
///   with no locking overhead, enabled by TLAB exclusivity during execution.
///
/// ## Usage Example
///
/// ```ignore
/// use std::sync::Arc;
///
/// let engine = Arc::new(BexEngine::new(bytecode, sys_ops)?);
///
/// // Concurrent calls are safe - each gets its own VM and TLAB
/// let (result1, result2) = tokio::join!(
///     engine.call_function("process_order", order1_args),
///     engine.call_function("process_order", order2_args),
/// );
///
/// // Or with explicit spawning:
/// let engine_clone = Arc::clone(&engine);
/// let handle = tokio::spawn(async move {
///     engine_clone.call_function("background_task", vec![]).await
/// });
/// ```
///
/// ## Handle Sharing (Advanced)
///
/// If you pass the same `Handle` to multiple concurrent calls that both mutate
/// the referenced object, you may observe a data race. This requires deliberate
/// action (obtaining a handle, sharing it, mutating in parallel) and is not
/// something that happens accidentally in normal BAML usage.
///
/// # Architecture
///
/// ```text
/// BexEngine (owns)
///     ├── Arc<BexHeap>     ─── shared across all VMs
///     ├── GlobalPool       ─── global variable definitions
///     └── function index   ─── name → ObjectIndex lookup
///
/// call_function() creates:
///     └── BexVm (temporary)
///         └── Tlab ─── exclusive allocation region from shared heap
/// ```
pub struct BexEngine {
    process_euid: ProcessEuid,
    engine_id: EngineId,
    program_metadata: ProgramMetadata,
    /// Function identity by heap address: maps each compile-time
    /// `Object::Function`'s stable `HeapPtr` (as a raw address) to its
    /// `FunctionId`. Call notifications carry the
    /// resolved function pointer, so per-call identity resolution is one
    /// hash lookup — never a name scan.
    /// Synthetic id for spawn-closure child roots (see `SPAWN_CLOSURE_FQN`).
    next_thread_id: AtomicU64,
    /// The unified heap (shared across all VM instances)
    heap: Arc<BexHeap>,
    /// Frozen global variables shared across every post-`$init` VM.
    ///
    /// Populated once during `$init` and immutable thereafter; cloning is a
    /// cheap refcount bump (see `VmGlobals::Shared`). The VM rejects any
    /// `StoreGlobal` against this view as a `VmInternalError`.
    ///
    /// Stored as a [`SharedGlobals`] (rather than a plain `Arc<[Value]>`)
    /// so the GC can trace + forward `Value::object(HeapPtr)` entries: the
    /// engine registers a clone of this `SharedGlobals` as a [`RootHaver`]
    /// permit holder during `BexEngine::new`. Without that registration any
    /// runtime heap object stored in a top-level `let` global was invisible
    /// to GC and could be reclaimed mid-call.
    globals: SharedGlobals,
    /// Permit holder that keeps `globals` registered with the
    /// `HeapPermitManager`. The engine never `acquire()`s this — it exists
    /// purely so the GC's `collect_roots`/`forward_roots` walks reach the
    /// frozen globals pool.
    _globals_permit: bex_heap::InactiveHeapPermit<SharedGlobals>,
    /// Resolved function/class/enum names for lookup
    resolved_function_names: HashMap<String, (HeapPtr, bex_vm_types::FunctionKind)>,
    /// Resolved class names for instance allocation (`IndexMap` preserves definition order)
    resolved_class_names: indexmap::IndexMap<String, HeapPtr>,
    /// Resolved enum names for variant allocation (`IndexMap` preserves definition order)
    resolved_enum_names: indexmap::IndexMap<String, HeapPtr>,
    /// System operations provider.
    sys_ops: std::sync::Arc<sys_ops::SysOps>,
    runtime_compiler: Option<Arc<dyn RuntimeCompiler>>,
    /// Context passed to `sys_ops` that need engine-level information.
    sys_op_ctx: sys_types::EngineSysOpContext,
    /// Compiled test cases from the BAML program.
    test_cases: Vec<bex_vm_types::TestCase>,
    /// Process argv passed in at engine creation. Exposed to BAML via
    /// `baml.sys.argv()`. Shared (cheap to clone) with each spawned VM.
    argv: Arc<[String]>,

    // --- GC coordination ---
    heap_permit_manager: Arc<HeapPermitManager>,
    /// Used to prevent multiple threads from trying to run GC at the same time.
    /// Only one should run it, the rest should wait for it to complete.
    checking_gc: AtomicBool,
    /// Used to notify long-running threads that they should park the VM even if they aren't at a typical yield point.
    #[cfg(not(target_arch = "wasm32"))]
    park_requested: Arc<AtomicBool>,

    /// Map of active function calls by ID.
    active_calls: Mutex<HashMap<CallId, ActiveCall>>,
    lifecycle: Mutex<EngineLifecycle>,
    lifecycle_changed: tokio::sync::Notify,
    shutdown_required: AtomicBool,

    futures: FutureManager,

    rooted_unhandled_spawn_errors: Mutex<VecDeque<RootedUnhandledSpawnError>>,
    unhandled_spawn_state: Mutex<UnhandledSpawnState>,
    unhandled_spawn_delivery: tokio::sync::Mutex<()>,

    /// Loaded packages plus the program-wide interface → impl-rules index, shared
    /// with every VM so spawned workers see the same index. The source of truth
    /// for interface dispatch, recursive aliases, and named-item lookup.
    packages: Arc<bex_vm::package_load::PackageIndex>,

    /// Value-scoped runtime class/interface dispatch table.
    dynamic_dispatch: Arc<bex_vm::package_load::DynDispatchTables>,
    /// Weak-table forwarding/sweep participant registered for every GC.
    _dynamic_dispatch_permit: bex_heap::InactiveHeapPermit<bex_vm::package_load::DynDispatchRoot>,

    /// Builtin `baml.errors.*` / `baml.panics.*` class pointers, resolved once
    /// from `packages` and shared with every spawned VM (each `BexVm` would
    /// otherwise re-resolve them from `packages` on construction).
    error_class_ptrs: Arc<[bex_vm_types::HeapPtr]>,
    panic_class_ptrs: Arc<[bex_vm_types::HeapPtr]>,

    /// Shared process/store profiler session. `Off` owns no profiler resource;
    /// engines never reread the environment or lazily activate it.
    profiler_session: Arc<ProfilerSession>,

    /// Whether this engine's profiling lifecycle has been activated
    /// (metadata registered with the profiling consumer). Engines built via
    /// [`BexEngine::new`] activate at construction. Candidate engines built
    /// via [`BexEngine::new_with_deferred_profiling`] stay inactive until a
    /// winning conditional commit calls [`BexEngine::activate_profiling`];
    /// a superseded candidate therefore drops without registering metadata
    /// or emitting an `engine_closed` tombstone.
    prof_activated: AtomicBool,
}

impl Drop for BexEngine {
    /// Closes the engine's profiling lifecycle: a non-blocking notification;
    /// the direct consumer drains remaining events, seals backend state, and
    /// frees metadata. Every commit happened-before the last `Arc` release.
    ///
    /// An engine whose profiling lifecycle was never activated (a discarded
    /// candidate) drops quietly: it registered no metadata, so it must not
    /// emit a close notification or leave a closed-engine tombstone.
    fn drop(&mut self) {
        let rooted = self
            .rooted_unhandled_spawn_errors
            .get_mut()
            .map_or(0, |errors| errors.len());
        let queued = self
            .unhandled_spawn_state
            .get_mut()
            .map_or(0, |state| state.queued.len());
        let count = rooted + queued;
        let shutdown_called = self
            .lifecycle
            .get_mut()
            .is_ok_and(|state| *state == EngineLifecycle::Closed);
        let shutdown_required = self.shutdown_required.load(Ordering::Acquire);
        if count != 0 || (shutdown_required && !shutdown_called) {
            tracing::warn!(
                count,
                shutdown_called,
                "dropping engine without a complete unhandled-spawn-error drain"
            );
        }
        if self.profiler_session.is_on() && self.prof_activated.load(Ordering::Acquire) {
            bex_events::prof::engine_closed(self.engine_id.0);
        }
    }
}

/// Maps M0 [`ProgramMetadata`] into the direct consumer's function registry.
fn prof_engine_metadata(meta: &ProgramMetadata) -> bex_events::prof::EngineProfileMetadata {
    bex_events::prof::EngineProfileMetadata {
        program_id: hex_bytes(&meta.program_id.0),
        source_snapshot_id: meta.source_snapshot_id.as_ref().map(|id| hex_bytes(&id.0)),
        revision_id: meta.revision_id.as_ref().map(|id| id.0.clone()),
        functions: meta
            .function_table
            .functions
            .iter()
            .map(|f| bex_events::prof::FunctionMetaEntry {
                function_id: f.function_id.0,
                fqn: f.fqn.clone(),
                source_file: f.source_file.clone().unwrap_or_default(),
                span_start: f.source_span.as_ref().map_or(0, |sp| sp.start),
                span_end: f.source_span.as_ref().map_or(0, |sp| sp.end),
                kind: match &f.kind {
                    bex_events::RuntimeFunctionKind::Bytecode => "bytecode".to_string(),
                    bex_events::RuntimeFunctionKind::SysOp(_) => "sysop".to_string(),
                    bex_events::RuntimeFunctionKind::Native
                    | bex_events::RuntimeFunctionKind::NativeUnresolved => "native".to_string(),
                },
                definition_key: f.definition_key.as_ref().map(|key| key.0.clone()),
                owner_type: f.owner_type.as_ref().map(|key| key.0.clone()),
                parent_function: f.parent_function.as_ref().map(|key| key.0.clone()),
                lambda_path: f.lambda_path.clone(),
                package_name: f.package_name.clone(),
                namespace: f.namespace.clone(),
            })
            .collect(),
    }
}

fn hex_bytes(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

#[cfg(target_arch = "wasm32")]
fn _default_round_robin_start() -> usize {
    // Keep wasm deterministic for tooling (matches legacy behavior).
    0
}

#[cfg(not(target_arch = "wasm32"))]
fn _default_round_robin_start() -> usize {
    use web_time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    #[allow(clippy::cast_possible_truncation)]
    {
        nanos as usize
    }
}

fn epoch_ms() -> u64 {
    use web_time::{SystemTime, UNIX_EPOCH};
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    u64::try_from(millis).unwrap_or(u64::MAX)
}

fn truncate_preview(mut value: String, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value;
    }
    let cut = value
        .char_indices()
        .nth(max_chars)
        .map_or(value.len(), |(idx, _)| idx);
    value.truncate(cut);
    value.push_str("...");
    value
}

/// Extract an owned `RuntimeTy` from a `SysOp::BamlHostCallHostValue` type-arg operand
/// (an `Object::Type(Box<TypeValue>)`).
///
/// The VM packs the sys-op args as `[handle, args_array, ret_ty, throws_ty]`
/// (see `bex_vm::vm`'s `CallIndirect`-`HostClosure` path): `ret_ty` is
/// `type_arg_0` (`T`, the declared return type) and `throws_ty` is `type_arg_1`
/// (`E`, the declared error contract). Returns `Err` if the slot is absent or
/// not a `Type` object — for `BamlHostCallHostValue` that's an engine/compiler
/// ABI bug and a missing slot would silently skip both
/// `validate_host_return_schema` and `enforce_host_throw_contract`, so the
/// caller surfaces it as a fatal internal error rather than soft-failing the
/// contract checks. `slot_name` is used only for the diagnostic message.
fn host_call_type_arg(
    ty_arg: Option<Value>,
    slot_index: usize,
    slot_name: &'static str,
) -> Result<baml_type::RuntimeTy, bex_vm::errors::VmInternalError> {
    let bad_slot = || bex_vm::errors::VmInternalError::BridgeFailure {
        message: format!(
            "call_host_value: missing or non-Type {slot_name} (sysop arg slot \
             {slot_index}); this is an engine/compiler ABI bug — expected the \
             VM to pack args as [handle, args_array, ret_ty, throws_ty]"
        ),
    };
    let ptr = ty_arg
        .as_ref()
        .and_then(Value::as_object_ptr)
        .ok_or_else(bad_slot)?;
    // SAFETY: the pointer was just allocated by the VM for this sys-op call.
    // This MUST be read while the heap permit is held (i.e. *before* the sys-op
    // await releases it): the engine-local `args` Vec is not a GC root, so after
    // the await a moving GC may relocate/collect this `Object::Type` and the raw
    // pointer would dangle. The caller clones the `RuntimeTy` out before awaiting.
    match unsafe { ptr.get() } {
        // `Object::Type` stores a realized type; widen it to the boundary `RuntimeTy`.
        Object::Type(type_value) => Ok(type_value.ty.clone().into()),
        _ => Err(bad_slot()),
    }
}

/// Clone the realized parameter contract captured by a host closure while the
/// VM heap permit is held. These types drive the operation-specific outbound
/// conversion for callback arguments; in particular, a union-typed parameter
/// must become `BexExternalValue::Union` before the shared wire encoder runs.
fn host_call_params(
    handle: Option<Value>,
) -> Result<Vec<baml_type::RealizedFunctionParamTy>, bex_vm::errors::VmInternalError> {
    let malformed = || bex_vm::errors::VmInternalError::BridgeFailure {
        message: "call_host_value: missing or non-HostClosure handle in sysop arg slot 0"
            .to_string(),
    };
    let ptr = handle
        .as_ref()
        .and_then(Value::as_object_ptr)
        .ok_or_else(malformed)?;
    // SAFETY: the caller invokes this while holding the active VM heap permit,
    // before the sys-op await can allow a moving collection.
    match unsafe { ptr.get() } {
        Object::HostClosure(closure) => Ok(closure.params.as_ref().clone()),
        _ => Err(malformed()),
    }
}

/// Convert a sysop `OpError`'s inner `VmRustFnError` into a heap-allocated
/// VM exception `Value` using the VM's own conversion helpers — the same
/// path a `$rust_function` error or `throw` opcode takes:
///   * `VmBamlError`  → [`BexVm::error_to_exception_value`] (a `baml.errors.*` instance)
///   * `VmPanic`      → [`BexVm::panic_to_exception_value`] (a `baml.panics.*` instance)
///   * pre-built `Thrown(value)` → returned as-is
///   * `InternalError` is fatal (no catchable value)
///
/// The caller injects the returned value via
/// [`BexVm::try_handle_external_exception`] so it unwinds through the
/// standard VM exception machinery; an in-BAML `catch` matches it like any
/// other throw.
pub(crate) fn op_error_to_throw_value(
    vm: &mut bex_vm::BexVm,
    kind: bex_vm::errors::VmRustFnError,
) -> Result<(Value, bex_vm::errors::ProfilerErrorKind), bex_vm::errors::VmInternalError> {
    use bex_vm::errors::{ProfilerErrorKind, VmRustFnError};
    match kind {
        VmRustFnError::BamlError(err) => {
            Ok((vm.error_to_exception_value(err), ProfilerErrorKind::Fresh))
        }
        VmRustFnError::Panic(panic) => {
            Ok((vm.panic_to_exception_value(panic), ProfilerErrorKind::Fresh))
        }
        VmRustFnError::Thrown {
            value,
            profiler_kind,
        } => Ok((value, profiler_kind)),
        VmRustFnError::InternalError(err) => Err(err),
    }
}

/// Decoded `baml.spawn.SpawnParams` fields (BEP-034 middleware) — see
/// [`BexEngine::read_spawn_params`].
struct SpawnParamsData {
    body: HeapPtr,
    name: Option<String>,
    group: Option<Arc<TaskGroupInner>>,
    cancel: Option<CancellationToken>,
    detach: bool,
}

/// Enforce the host callable's declared throws contract `E` on a
/// materialized thrown `Value`. Returns the value unchanged on a contract
/// match; on mismatch returns a fresh `baml.panics.HostContractViolation`
/// panic `Value` carrying the host class identity for diagnostics.
///
/// Operates on the post-materialization `Value` (not the source
/// `BexExternalValue` or `VmBamlError`) so the check applies uniformly to
/// every shape a host-callable throw can take —
/// `OpErrorPayload::HostThrown` (wire-routed) and
/// `OpErrorPayload::Vm(BamlError(HostCallable))` (engine-internal) both
/// materialize into the same `Object::Instance` of
/// `baml.errors.HostCallable`.
///
/// The check reads the value's runtime BAML type via
/// [`value_runtime_baml_ty`] and tests `value_ty ⊑ contract` via the canonical,
/// program-aware type algebra ([`baml_type::normalize::is_subtype`] over the VM
/// as its [`TypeContext`](baml_type::normalize::TypeContext)), so a thrown
/// concrete that *implements* a declared interface contract is on-contract (the
/// context-free `RuntimeTy::is_subtype_of` fork could not see that membership).
/// `BuiltinUnknown` accepts everything (the "throws unknown" fallback for
/// undeclared host contracts); concrete classes reject anything not in their
/// subtype lattice.
///
/// Panic-class values (`baml.panics.*`) bypass the contract entirely:
/// panics are not catchable errors and a fn's `throws E` clause never
/// includes them. This also avoids re-wrapping an engine-generated
/// `HostContractViolation` (from a wrong-return-type check upstream)
/// into a second `HostContractViolation` with a corrupted message.
fn enforce_host_throw_contract(
    thread: &mut ActiveHeapPermit<BexThread>,
    value: Value,
    contract: &RuntimeTy,
) -> Value {
    // `BuiltinUnknown` is the top type — short-circuit before any heap
    // walking.
    if matches!(contract, RuntimeTy::BuiltinUnknown { .. }) {
        return value;
    }
    let runtime_ty = value_runtime_baml_ty(value, thread.proof());
    // Panics propagate as panics regardless of `E` — they're an
    // engine-level failure mode, not something the user's callable opts
    // into via `throws`.
    if let Some(RuntimeTy::Class(name, _, _)) = runtime_ty.as_ref()
        && name.is_panic_type()
    {
        return value;
    }
    let on_contract = runtime_ty.as_ref().is_some_and(|rt: &RuntimeTy| {
        // The VM is the runtime `TypeContext`; operands upcast to `Ty` by a
        // zero-cost borrow.
        baml_type::normalize::is_subtype(rt.as_ty(), contract.as_ty(), &thread.vm)
    });
    if on_contract {
        return value;
    }
    // Off-contract: build the `HostContractViolation` panic. Echo the
    // host's `class_name` / `language` from the `HostCallable` wrapper's
    // fields when present — the common case where a bridge SDK wraps a
    // native exception with its host-class identity in those fields.
    let (host_class, host_language) = extract_host_diagnostics_from_value(value, thread.proof());
    let runtime_ty_str = runtime_ty
        .as_ref()
        .map_or_else(|| "<unknown>".to_string(), std::string::ToString::to_string);
    let panic = bex_vm::errors::VmPanic::HostContractViolation {
        message: format!(
            "host callable threw a value of type `{runtime_ty_str}` that is not in its \
             declared throws contract (`{contract}`)",
        ),
        class_name: host_class,
        language: host_language,
    };
    thread.vm.panic_to_exception_value(panic)
}

/// Extract a materialized `Value`'s runtime BAML type for the purpose of
/// host-throw contract checking — the class FQN + type args for
/// `Object::Instance`, the scalar tag for primitives. Returns `None` for
/// shapes that can't reasonably inhabit a thrown-value position (e.g.
/// `Object::HostClosure`, `Object::FunctionRef`); the caller treats
/// `None` as off-contract.
///
/// `_proof` ensures the caller holds an active heap permit so the
/// `HeapPtr` derefs in this function are sound.
fn value_runtime_baml_ty(value: Value, _proof: bex_heap::PermitProof<'_>) -> Option<RuntimeTy> {
    use baml_type::TyAttr;
    use bex_vm_types::ValueKind;
    match value.kind() {
        ValueKind::OmittedArg => None,
        ValueKind::Null => Some(RuntimeTy::Null {
            attr: TyAttr::default(),
        }),
        ValueKind::Int(_) => Some(RuntimeTy::Int {
            attr: TyAttr::default(),
        }),
        ValueKind::Bool(_) => Some(RuntimeTy::Bool {
            attr: TyAttr::default(),
        }),
        ValueKind::Object(ptr) => {
            // SAFETY: caller holds an active heap permit (`_proof`), so
            // deref'ing this `HeapPtr` is sound.
            match unsafe { ptr.get() } {
                Object::Instance(instance) => {
                    // SAFETY: `instance.class` is a heap-rooted Class
                    // pointer; valid under the same permit.
                    let class_obj = unsafe { instance.class.get() };
                    let Object::Class(class) = class_obj else {
                        return None;
                    };
                    Some(RuntimeTy::Class(
                        class.name.clone(),
                        instance
                            .class_type_args
                            .iter()
                            .map(baml_type::RuntimeTy::from)
                            .collect(),
                        TyAttr::default(),
                    ))
                }
                Object::String(_) => Some(RuntimeTy::String {
                    attr: TyAttr::default(),
                }),
                Object::Float(_) => Some(RuntimeTy::Float {
                    attr: TyAttr::default(),
                }),
                Object::Variant(variant) => {
                    // SAFETY: the variant's `enm` is a heap-rooted Enum
                    // class, valid under the same permit.
                    let enum_obj = unsafe { variant.enm.get() };
                    let Object::Enum(enum_def) = enum_obj else {
                        return None;
                    };
                    Some(RuntimeTy::Enum(enum_def.name.clone(), TyAttr::default()))
                }
                // Other Object shapes (HostClosure, FunctionRef, Array,
                // Map, etc.) are not meaningful in a thrown position;
                // treat as no runtime type → off-contract.
                _ => None,
            }
        }
    }
}

/// Pull `class_name` / `language` String fields out of a thrown
/// `Object::Instance` (the common `baml.errors.HostCallable` wrapper
/// shape) so a `HostContractViolation` panic can echo the host's
/// exception identity in its diagnostics. Non-Instance values or
/// missing fields return `(None, None)`.
///
/// `_proof` ensures the caller holds an active heap permit so the
/// `HeapPtr` derefs in this function are sound.
fn extract_host_diagnostics_from_value(
    value: Value,
    _proof: bex_heap::PermitProof<'_>,
) -> (Option<String>, Option<String>) {
    let Some(ptr) = value.as_object_ptr() else {
        return (None, None);
    };
    // SAFETY: caller holds an active heap permit.
    let Object::Instance(instance) = (unsafe { ptr.get() }) else {
        return (None, None);
    };
    let Object::Class(class) = (unsafe { instance.class.get() }) else {
        return (None, None);
    };
    let read_string_field = |field_name: &str| -> Option<String> {
        let idx = class.fields.iter().position(|f| f.name == field_name)?;
        let field_value = instance.try_load_field(idx)?;
        let ptr = field_value.as_object_ptr()?;
        match unsafe { ptr.get() } {
            Object::String(s) if !s.is_empty() => Some(s.as_str().to_string()),
            _ => None,
        }
    };
    (
        read_string_field("class_name"),
        read_string_field("language"),
    )
}

// TODO(bep-053): replace with compiler-owned metadata — capital-letter
// sniffing on FQN segments is an acknowledged-interim heuristic and must not
// become load-bearing.
fn derive_owner_type_definition_key(fqn: &str) -> Option<bex_events::DefinitionKey> {
    let parts = fqn.split('.').collect::<Vec<_>>();
    if parts.len() < 3 {
        return None;
    }

    let owner = parts[parts.len() - 2];
    if !owner
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
    {
        return None;
    }

    Some(bex_events::DefinitionKey(format!(
        "class:{}",
        parts[..parts.len() - 1].join(".")
    )))
}

// TODO(bep-053): replace with compiler-owned lambda identity — the
// `find("<lambda")` name sniff is an acknowledged-interim heuristic.
fn derive_lambda_metadata(fqn: &str) -> (Option<bex_events::DefinitionKey>, Option<String>) {
    let Some(lambda_start) = fqn.find("<lambda") else {
        return (None, None);
    };

    let parent = fqn[..lambda_start].trim_end_matches(['.', ':']).to_string();
    let parent_function =
        (!parent.is_empty()).then(|| bex_events::DefinitionKey(format!("function:{parent}")));
    let lambda_path = Some(fqn[lambda_start..].to_string());
    (parent_function, lambda_path)
}

// ── Friendly generic-inference errors ───────────────────────────────────────
//
// Inbound generic-call failures are surfaced to host callers, so their messages
// must read for someone with no type-theory background. Two distinct shapes:
//
//   - *must-specify* — BAML found no evidence to infer `var` (a return-/body-only
//     var, or a value position that came up empty). The fix is for the caller to
//     name the type, so the message shows how.
//   - *conflict* — the arguments demand mutually incompatible types for `var`;
//     naming a type would not help (the args still wouldn't match), so the
//     message explains the clash instead of suggesting a binding.
//
// Host call syntax is Python for now (subscript `f[int](...)` / `_types=`); a
// per-host renderer is future work (see `03c-impl-guide`).

/// The declared, still-*symbolic* form of a stored signature template: each
/// frame slot becomes the type variable that slot names.
///
/// The host boundary infers a generic call's type arguments by matching the
/// declared types against incoming wire values (`collect_type_var_bindings`),
/// which keys on type-variable *names*. Substituting an empty frame instead
/// would collapse every slot to `unknown` and erase exactly what that inference
/// reads — turning "infer `T` from the argument" into "there is no `T`".
fn declared_symbolic(template: &baml_type::TyTemplate, func: &bex_vm_types::Function) -> RuntimeTy {
    // `display_type_params` is De Bruijn ordered, so a param's position *is* its
    // frame slot — the index a `ParamTy` identity carries.
    let slot_vars: Vec<RuntimeTy> = func
        .display_type_params
        .iter()
        .enumerate()
        .map(|(i, p)| {
            RuntimeTy::TypeVar(
                baml_type::ParamTy::new(
                    u32::try_from(i).unwrap_or(u32::MAX),
                    baml_type::Name::new(p.split_whitespace().next().unwrap_or(p)),
                ),
                baml_type::TyAttr::default(),
            )
        })
        .collect();
    template.substitute_symbolic(&slot_vars)
}

/// The bare name the host caller used (e.g. `one_type_arg`), stripped of the
/// engine's namespace/package qualification (`user.generic_tests.one_type_arg`)
/// so the call examples in an error message match what the user actually typed.
fn host_display_name(function_name: &str) -> &str {
    function_name.rsplit('.').next().unwrap_or(function_name)
}

/// A generic call whose `var` could not be inferred from the arguments — tell the
/// caller to specify it, with the Python subscript and `_types=` forms.
fn friendly_must_specify(function_name: &str, generic_params: &[String], var: &str) -> String {
    let name = host_display_name(function_name);
    let placeholders = if generic_params.is_empty() {
        "int".to_string()
    } else {
        generic_params
            .iter()
            .map(|_| "int")
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "`{name}` is a generic BAML function, and BAML could not infer a type for its type \
         parameter `{var}` from the call arguments. Python callers must specify it explicitly \
         as a subscript — e.g. `{name}[{placeholders}](...)`."
    )
}

/// A generic call whose arguments demand incompatible types for some `TypeVar`.
/// `detail` is the plain-language clash from the unifier; we only add the call
/// frame.
fn friendly_inference_conflict(function_name: &str, detail: &str) -> String {
    let name = host_display_name(function_name);
    format!("`{name}` was called with arguments whose types can't be reconciled. {detail}")
}

/// A generic call whose argument value doesn't inhabit its (now concrete)
/// declared parameter type — e.g. a caller-specified `T=int` contradicted by a
/// `string` actual (03b C4). `detail` is the structural clash from
/// [`crate::conversion::check_generic_arg`]; we add the call frame and the
/// 1-based argument position.
fn friendly_arg_type_mismatch(function_name: &str, arg_index: usize, detail: &str) -> String {
    let name = host_display_name(function_name);
    format!(
        "`{name}` was called with a value that doesn't match its type: argument {} {detail}.",
        arg_index + 1
    )
}

impl BexEngine {
    fn next_engine_id() -> EngineId {
        static NEXT_ENGINE_ID: AtomicU64 = AtomicU64::new(1);
        EngineId(NEXT_ENGINE_ID.fetch_add(1, Ordering::Relaxed))
    }

    fn build_program_metadata(program: &bex_vm_types::Program) -> ProgramMetadata {
        // Ids are 1-based sequential in pool order (`0` = unassigned) — the
        // exact sequence the pre-heap walk in `new()` stamps onto each
        // `Function.function_id`, so metadata and compact producer records
        // agree byte-for-byte. M0 moves this to compile time.
        let mut next_function_id: u32 = 0;
        let mut functions: Vec<bex_events::FunctionMetadata> = program
            .objects
            .iter()
            .filter_map(|obj| {
                let Object::Function(function) = obj else {
                    return None;
                };

                next_function_id += 1;
                let function_id = FunctionId(next_function_id);
                let fqn = function.name.clone();
                let owner_type = derive_owner_type_definition_key(&fqn);
                let (parent_function, lambda_path) = derive_lambda_metadata(&fqn);
                let mut parts = fqn.split('.').map(str::to_string).collect::<Vec<_>>();
                let display_name = parts.last().cloned().unwrap_or_else(|| fqn.clone());
                let package_name = if parts.len() > 1 {
                    Some(parts.remove(0))
                } else {
                    None
                };
                let namespace = if parts.len() > 1 {
                    parts[..parts.len() - 1].to_vec()
                } else {
                    Vec::new()
                };
                let source_file =
                    (!function.source_file.is_empty()).then(|| function.source_file.clone());
                let source_span = Some(bex_events::SourceSpan {
                    file_id: function.span.file_id.as_u32(),
                    start: function.span.range.start().into(),
                    end: function.span.range.end().into(),
                });

                Some(bex_events::FunctionMetadata {
                    function_id,
                    fqn: fqn.clone(),
                    display_name,
                    source_file,
                    source_span,
                    kind: function.kind.into(),
                    origin: function.origin.into(),
                    owner_type,
                    parent_function,
                    lambda_path,
                    definition_key: Some(bex_events::DefinitionKey(format!("function:{fqn}"))),
                    package_name,
                    namespace,
                    source_snapshot_id: None,
                    revision_id: None,
                    semantic_lanes: None,
                })
            })
            .collect();

        // Synthetic rows live just past the pool indices, so they can never
        // collide with a real function's id.
        functions.push(bex_events::FunctionMetadata {
            function_id: FunctionId(next_function_id + 1),
            fqn: SPAWN_CLOSURE_FQN.to_string(),
            display_name: SPAWN_CLOSURE_DISPLAY_NAME.to_string(),
            source_file: None,
            source_span: None,
            kind: bex_events::RuntimeFunctionKind::Bytecode,
            origin: bex_events::RuntimeFunctionOrigin::Internal,
            owner_type: None,
            parent_function: None,
            lambda_path: Some(SPAWN_CLOSURE_DISPLAY_NAME.to_string()),
            definition_key: Some(bex_events::DefinitionKey(format!(
                "function:{SPAWN_CLOSURE_FQN}"
            ))),
            package_name: Some("baml".to_string()),
            namespace: Vec::new(),
            source_snapshot_id: None,
            revision_id: None,
            semantic_lanes: None,
        });

        functions.push(bex_events::FunctionMetadata {
            function_id: FunctionId(next_function_id + 2),
            fqn: UNKNOWN_FUNCTION_FQN.to_string(),
            display_name: UNKNOWN_FUNCTION_DISPLAY_NAME.to_string(),
            source_file: None,
            source_span: None,
            kind: bex_events::RuntimeFunctionKind::Bytecode,
            origin: bex_events::RuntimeFunctionOrigin::Internal,
            owner_type: None,
            parent_function: None,
            lambda_path: None,
            definition_key: Some(bex_events::DefinitionKey(format!(
                "function:{UNKNOWN_FUNCTION_FQN}"
            ))),
            package_name: Some("baml".to_string()),
            namespace: Vec::new(),
            source_snapshot_id: None,
            revision_id: None,
            semantic_lanes: None,
        });

        ProgramMetadata {
            program_id: ProgramId::new_random(),
            source_snapshot_id: None,
            revision_id: None,
            function_table: FunctionMetadataTable { functions },
        }
    }

    /// Create a new engine with the given program.
    ///
    /// The engine creates a unified heap containing compile-time objects
    /// (functions, classes, enums). Each function call creates a VM that
    /// shares this heap and allocates runtime objects into its own TLAB.
    ///
    /// # Arguments
    ///
    /// * `bytecode_program` - The compiled BAML program bytecode
    /// * `sys_ops` - System operations provider (use `sys_types_native::SysOps::native()` for default)
    /// * `argv` - Process-style argv values exposed to BAML via `baml.sys.argv()`.
    ///   Pass `Vec::new()` when argv is not applicable (e.g. tests, IDE, library embedding).
    pub fn new(
        bytecode_program: bex_vm_types::Program,
        sys_ops: std::sync::Arc<sys_ops::SysOps>,
        argv: Vec<String>,
    ) -> Result<Self, EngineError> {
        let engine = Self::new_with_deferred_profiling(bytecode_program, sys_ops, argv)?;
        engine.activate_profiling();
        Ok(engine)
    }

    /// Creates an engine with an explicitly injected immutable profiler
    /// session. Tests and embedders can share one session across engines
    /// without mutating process environment state.
    pub fn new_with_profiler_session(
        bytecode_program: bex_vm_types::Program,
        sys_ops: std::sync::Arc<sys_ops::SysOps>,
        argv: Vec<String>,
        profiler_session: Arc<ProfilerSession>,
    ) -> Result<Self, EngineError> {
        let engine = Self::new_with_deferred_profiling_runtime_compiler_and_session(
            bytecode_program,
            sys_ops,
            argv,
            None,
            profiler_session,
        )?;
        engine.activate_profiling();
        Ok(engine)
    }

    /// Like [`BexEngine::new`], but keeps the profiling lifecycle inactive.
    ///
    /// Used for *candidate* engines that may be discarded before ever
    /// becoming the installed engine (LSP conditional commit): construction —
    /// including `$init` — runs normally, but no profiling metadata is
    /// registered and dropping the candidate emits no `engine_closed`
    /// notification. The winning commit calls
    /// [`BexEngine::activate_profiling`] immediately before making the
    /// engine reachable.
    pub fn new_with_deferred_profiling(
        bytecode_program: bex_vm_types::Program,
        sys_ops: std::sync::Arc<sys_ops::SysOps>,
        argv: Vec<String>,
    ) -> Result<Self, EngineError> {
        Self::new_with_deferred_profiling_and_runtime_compiler(
            bytecode_program,
            sys_ops,
            argv,
            None,
        )
    }

    /// Construct an engine with runtime compilation enabled by an injected
    /// compiler implementation.
    pub fn new_with_runtime_compiler(
        bytecode_program: bex_vm_types::Program,
        sys_ops: std::sync::Arc<sys_ops::SysOps>,
        argv: Vec<String>,
        runtime_compiler: Arc<dyn RuntimeCompiler>,
    ) -> Result<Self, EngineError> {
        let engine = Self::new_with_deferred_profiling_and_runtime_compiler(
            bytecode_program,
            sys_ops,
            argv,
            Some(runtime_compiler),
        )?;
        engine.activate_profiling();
        Ok(engine)
    }

    /// Deferred-profiling variant used by conditional project installation.
    pub fn new_with_deferred_profiling_and_runtime_compiler(
        bytecode_program: bex_vm_types::Program,
        sys_ops: std::sync::Arc<sys_ops::SysOps>,
        argv: Vec<String>,
        runtime_compiler: Option<Arc<dyn RuntimeCompiler>>,
    ) -> Result<Self, EngineError> {
        Self::new_with_deferred_profiling_runtime_compiler_and_session(
            bytecode_program,
            sys_ops,
            argv,
            runtime_compiler,
            Arc::clone(ProfilerSession::global()),
        )
    }

    fn new_with_deferred_profiling_runtime_compiler_and_session(
        bytecode_program: bex_vm_types::Program,
        sys_ops: std::sync::Arc<sys_ops::SysOps>,
        argv: Vec<String>,
        runtime_compiler: Option<Arc<dyn RuntimeCompiler>>,
        profiler_session: Arc<ProfilerSession>,
    ) -> Result<Self, EngineError> {
        let argv: Arc<[String]> = Arc::from(argv);
        let process_euid = ProcessEuid::current();
        let engine_id = Self::next_engine_id();
        let program_metadata = Self::build_program_metadata(&bytecode_program);

        // Extract package_init_order before consuming bytecode_program.
        let package_init_order = bytecode_program.package_init_order.clone();

        // Convert the pure bytecode to a VM-ready program with native functions attached
        let bytecode =
            bex_vm::convert_program(bytecode_program).map_err(EngineError::VmInternalError)?;

        // Extract test cases before consuming other bytecode fields.
        let test_cases = bytecode.test_cases;

        // Extract compile-time objects for the heap
        let mut compile_time_objects: Vec<Object> = bytecode.objects.into_iter().collect();

        // Box every reachable `ConstValue::Float` into a compile-time
        // `Object::Float` (floats can no longer live inline in `Value`).
        // `bytecode.globals` are rewritten further down, during the
        // `ConstValue` → `Value` conversion, using the returned index map.
        let float_indices = bex_vm_types::types::box_compile_time_floats(
            &mut compile_time_objects,
            &bytecode.globals,
        );

        // Encode compact bytecode for all functions before the heap freezes them.
        // This must happen before BexHeap::new() because objects become immutable
        // behind an Arc after that point.
        //
        // The same pre-freeze walk is the profiling interim function-id
        // provider (plan §2.6): per-run sequential ids (1-based; 0 =
        // unassigned), assigned unconditionally so ids are deterministic,
        // with the header metadata table built only when profiling is on.
        // M0 moves assignment to compile time and replaces this seam.
        let mut next_function_id: u32 = 0;
        let mut function_pool_indices: Vec<usize> = Vec::new();
        let compile_time_objects: Vec<Object> = compile_time_objects
            .into_iter()
            .enumerate()
            .map(|(idx, mut obj)| {
                if let Object::Function(ref mut func) = obj {
                    func.bytecode.compact = Some(func.bytecode.lower_to_compact());
                    next_function_id += 1;
                    func.function_id = next_function_id;
                    function_pool_indices.push(idx);
                }
                obj
            })
            .collect();
        // Profiling metadata registration is deferred to
        // `activate_profiling()` so discarded candidate engines never leave
        // consumer-side state (`BexEngine::new` activates immediately).

        // Pre-compute class and enum indices before moving objects to heap.
        // This is used for allocating instances/variants from sys-op results.
        let class_indices: Vec<(String, usize)> = compile_time_objects
            .iter()
            .enumerate()
            .filter_map(|(idx, obj)| {
                if let Object::Class(class) = obj {
                    Some((class.name.to_string(), idx))
                } else {
                    None
                }
            })
            .collect();

        let enum_indices: Vec<(String, usize)> = compile_time_objects
            .iter()
            .enumerate()
            .filter_map(|(idx, obj)| {
                if let Object::Enum(enm) = obj {
                    Some((enm.name.to_string(), idx))
                } else {
                    None
                }
            })
            .collect();

        // Create the unified heap with compile-time objects, additionally
        // allocating the per-package `Object::Package` / `Object::ImplRule`
        // objects and the `vm.packages` index.
        let (heap, mut package_index) = bex_vm::package_load::build_heap_with_packages(
            compile_time_objects,
            &bytecode.packages,
        );
        let image_objects = bytecode
            .resolved_function_names
            .iter()
            .map(|(name, (idx, _))| (name.clone(), heap.compile_time_ptr(idx.into_raw())))
            .chain(
                class_indices
                    .iter()
                    .map(|(name, idx)| (name.clone(), heap.compile_time_ptr(*idx))),
            )
            .chain(
                enum_indices
                    .iter()
                    .map(|(name, idx)| (name.clone(), heap.compile_time_ptr(*idx))),
            )
            .collect();
        let image_globals = bytecode
            .function_global_indices
            .iter()
            .chain(&bytecode.let_global_indices)
            .map(|(name, idx)| (name.clone(), GlobalIndex::from_raw(*idx)))
            .collect();
        package_index.install_image_symbols(image_objects, image_globals);
        // Shared with every VM so spawned workers see the same package index
        // without re-resolving it.
        let packages = Arc::new(package_index);
        let dynamic_dispatch = Arc::new(bex_vm::package_load::DynDispatchTables::default());
        // Resolve the builtin error/panic class pointers once; shared with every
        // spawned VM rather than re-resolved per `BexVm::new`.
        let error_class_ptrs = bex_vm::vm::resolve_error_class_ptrs(&packages);
        let panic_class_ptrs = bex_vm::vm::resolve_panic_class_ptrs(&packages);

        // Convert ObjectIndex -> HeapPtr for function lookup table.
        // Now that the heap exists, we can get stable pointers to compile-time objects.
        let resolved_function_names: HashMap<String, (HeapPtr, bex_vm_types::FunctionKind)> =
            bytecode
                .resolved_function_names
                .into_iter()
                .map(|(name, (idx, kind))| {
                    let ptr = heap.compile_time_ptr(idx.into_raw());
                    (name, (ptr, kind))
                })
                .collect();

        // Function identity by heap address. Ids are 1-based sequential in
        // pool walk order (the same walk that stamped `Function.function_id`
        // and built the metadata table), and compile-time objects never
        // move, so this is built once and valid for the engine's lifetime.

        // Build class name lookup table from pre-computed indices.
        let resolved_class_names: indexmap::IndexMap<String, HeapPtr> = class_indices
            .into_iter()
            .map(|(name, idx)| (name, heap.compile_time_ptr(idx)))
            .collect();

        // Build enum name lookup table from pre-computed indices.
        let resolved_enum_names: indexmap::IndexMap<String, HeapPtr> = enum_indices
            .into_iter()
            .map(|(name, idx)| (name, heap.compile_time_ptr(idx)))
            .collect();

        // Convert compile-time globals (ConstValue) to runtime globals (Value).
        // Object references are converted from ObjectIndex to HeapPtr.
        // Float globals were redirected to compile-time Object::Float entries
        // by the boxing pre-pass above.
        let globals_vec: Vec<Value> = bytecode
            .globals
            .into_iter()
            .map(|cv| match cv {
                bex_vm_types::ConstValue::Float(f) => {
                    let idx = float_indices[&f.to_bits()];
                    Value::object(heap.compile_time_ptr(idx))
                }
                other => other.to_value(|idx| heap.compile_time_ptr(idx.into_raw())),
            })
            .collect();
        // Mutable during `$init` so `StoreGlobal` can populate top-level let
        // bindings; frozen into `Arc<[Value]>` once `$init` finishes (see below).
        let mut globals_pool = GlobalPool::from_vec(globals_vec);

        #[cfg(not(target_arch = "wasm32"))]
        let park_requested = Arc::new(AtomicBool::new(false));

        // Run $init for each package in dependency order.
        // $init evaluates top-level let-binding initializers and stores their
        // results into the global slots via StoreGlobal instructions.
        // This must run before any user code calls LoadGlobal on let-bound names.
        for init_name in &package_init_order {
            if let Some((init_ptr, _kind)) = resolved_function_names.get(init_name.as_str()) {
                let mut vm = BexVm::new(
                    Arc::clone(&heap),
                    VmGlobals::Owned(globals_pool.clone()),
                    #[cfg(not(target_arch = "wasm32"))]
                    Arc::clone(&park_requested),
                    Arc::clone(&argv),
                    Arc::clone(&packages),
                    Arc::clone(&dynamic_dispatch),
                    Arc::clone(&error_class_ptrs),
                    Arc::clone(&panic_class_ptrs),
                );
                vm.set_entry_point(*init_ptr, &[]);
                // Drive the VM to completion. $init only contains synchronous
                // bytecode, but events and GC safepoints may still yield.
                loop {
                    match vm.exec() {
                        Ok(VmExecState::Complete(_)) => {
                            // Extract the (potentially mutated) global pool back
                            // so StoreGlobal writes are visible to subsequent calls.
                            globals_pool = match vm.globals {
                                VmGlobals::Owned(pool) => pool,
                                VmGlobals::Shared(_) => {
                                    unreachable!("$init VM constructed with Owned globals")
                                }
                            };
                            break;
                        }
                        Ok(VmExecState::Event { .. }) => {
                            // Handle events during $init: push null and continue.
                            // No span context exists during init, so the event is dropped,
                            // but we must push a return value to keep the stack balanced.
                            vm.stack.push(Value::NULL);
                            continue;
                        }
                        Ok(other) => {
                            return Err(EngineError::InitFailed(format!(
                                "$init function '{init_name}' yielded unexpectedly: {other:?}"
                            )));
                        }
                        Err(e) => {
                            return Err(EngineError::InitFailed(format!(
                                "$init function '{init_name}' failed: {e}"
                            )));
                        }
                    }
                }
            }
        }

        // Freeze the now-populated globals into a `SharedGlobals` so the GC
        // can trace + forward `Value::object(HeapPtr)` entries. Every
        // post-`$init` VM cloned into a `call_function` invocation reads
        // from this shared instance via `VmGlobals::Shared(globals.clone())`.
        // `StoreGlobal` against the shared view is rejected by the VM as
        // `VmInternalError::StoreGlobalAfterInit`.
        let globals = SharedGlobals::from_vec(globals_pool.0);

        // Build SysOpContext by pre-extracting LLM function metadata from the heap.
        // This avoids passing raw HeapPtrs to sys_ops.
        let llm_functions = Self::extract_llm_function_info(&resolved_function_names);

        // Extract class and enum definitions for output format rendering.
        let class_definitions = Self::extract_class_definitions(&resolved_class_names);
        let enum_definitions = Self::extract_enum_definitions(&resolved_enum_names);

        let heap_permit_manager = Arc::new(HeapPermitManager::new());
        // We just created the permit manager so `new_permit` will not block:
        // the only synchronization inside is the `holders` mutex which is
        // uncontended at this point. `futures::executor::block_on` (rather
        // than `tokio::runtime::Handle::block_on`) is used so that this
        // constructor stays callable from inside a tokio runtime.
        //
        // If `new_permit` ever takes a real lock or schedules async work,
        // this assumption breaks and the constructor would deadlock — at
        // which point we'd have to make `BexEngine::new` async (TODO).
        let futures_permit = futures::executor::block_on(
            heap_permit_manager
                .new_permit(FutureManagerInner::new(Tlab::new_empty(Arc::clone(&heap)))),
        );

        // Register the frozen globals pool as its own permit holder so the
        // GC traces and forwards `Value::object(HeapPtr)` entries (e.g.
        // top-level `let g = [1, 2, 3]` populated during `$init`). The
        // permit is never `acquire()`d — its sole job is to participate in
        // `HeapGuard::collect_roots` / `forward_roots` walks. The
        // `SharedGlobals` is `Clone` (Arc bump), so the holder and the
        // engine's own field point at the same `UnsafeCell<Box<[Value]>>`.
        let globals_permit =
            futures::executor::block_on(heap_permit_manager.new_permit(globals.clone()));
        let dynamic_dispatch_permit = futures::executor::block_on(heap_permit_manager.new_permit(
            bex_vm::package_load::DynDispatchRoot(Arc::clone(&dynamic_dispatch)),
        ));

        // Build a default RuntimeIo from the SysOps table with an empty context.
        // This is replaced per-call in execute_sys_op with a live context that
        // carries the correct cancellation token and spawner.
        let runtime_io = sys_ops::build_runtime_io(
            &sys_ops,
            &heap,
            &heap_permit_manager,
            &sys_types::SysOpContext::empty(),
        );

        let sys_op_ctx = sys_types::EngineSysOpContext {
            llm_functions: Arc::new(llm_functions),
            function_global_indices: Arc::new(bytecode.function_global_indices),
            class_definitions: Arc::new(class_definitions),
            enum_definitions: Arc::new(enum_definitions),
            type_alias_definitions: Arc::new(bex_vm::package_load::all_recursive_type_aliases(
                &packages,
            )),
            runtime_io,
        };

        Ok(Self {
            process_euid,
            engine_id,
            program_metadata,
            next_thread_id: AtomicU64::new(1),
            heap,
            globals,
            _globals_permit: globals_permit,
            resolved_function_names,
            resolved_class_names,
            resolved_enum_names,
            sys_ops,
            runtime_compiler,
            sys_op_ctx,
            test_cases,
            argv,
            heap_permit_manager,
            checking_gc: AtomicBool::new(false),
            #[cfg(not(target_arch = "wasm32"))]
            park_requested,
            active_calls: Mutex::new(HashMap::new()),
            lifecycle: Mutex::new(EngineLifecycle::Running),
            lifecycle_changed: tokio::sync::Notify::new(),
            shutdown_required: AtomicBool::new(false),
            futures: FutureManager::new(futures_permit),
            rooted_unhandled_spawn_errors: Mutex::new(VecDeque::new()),
            unhandled_spawn_state: Mutex::new(UnhandledSpawnState {
                handler: None,
                queued: VecDeque::new(),
                delivering: false,
            }),
            unhandled_spawn_delivery: tokio::sync::Mutex::new(()),
            packages,
            dynamic_dispatch,
            _dynamic_dispatch_permit: dynamic_dispatch_permit,
            error_class_ptrs,
            panic_class_ptrs,
            profiler_session,
            prof_activated: AtomicBool::new(false),
        })
    }

    /// Activate this engine's profiling lifecycle by registering metadata
    /// with the direct consumer. Idempotent; a no-op when the `BAML_PROFILE`
    /// master switch is off.
    ///
    /// The consumer uses the M0 metadata table; its ids match those stamped
    /// on each function during construction.
    pub fn activate_profiling(&self) {
        self.shutdown_required.store(true, Ordering::Release);
        if !self.profiler_session.is_on() {
            return;
        }
        if self
            .prof_activated
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            bex_events::prof::backend::register_engine_session(
                self.engine_id,
                &self.profiler_session,
            );
            bex_events::prof::register_engine_metadata(
                self.engine_id.0,
                prof_engine_metadata(&self.program_metadata),
            );
        }
    }

    #[must_use]
    pub fn process_euid(&self) -> ProcessEuid {
        self.process_euid
    }

    #[must_use]
    pub fn engine_id(&self) -> EngineId {
        self.engine_id
    }

    #[must_use]
    pub fn program_metadata(&self) -> &ProgramMetadata {
        &self.program_metadata
    }

    fn next_bex_thread_id(&self) -> BexThreadId {
        BexThreadId(self.next_thread_id.fetch_add(1, Ordering::Relaxed))
    }

    // ── BEX profiling event stream (bex_events::prof) ──────────────────

    /// Mints the next logical BEX thread id — shared by the profiling
    /// stream and the host-event identity (one id universe).
    fn next_prof_thread_id(&self) -> u64 {
        self.next_bex_thread_id().0
    }

    /// Engine-side profiling emission — thread lifecycle and sys-op
    /// completion, all cold paths. Always resolves the ring through the
    /// calling OS thread's TLS lookup, **never** a stale VM snapshot:
    /// engine arms run after `.await`s, where the task may have migrated
    /// OS threads (the VM snapshot is only valid within one exec resume).
    #[allow(unsafe_code)]
    fn prof_emit(&self, rec: &bex_events::prof::record::RawRecord<'_>) -> bool {
        if !self.profiler_session.is_on() {
            return true;
        }
        let Some(handle) = bex_events::prof::ring_for_engine(self.engine_id.0) else {
            return false;
        };
        let mut buf = [0u8; bex_events::prof::record::MAX_RECORD_LEN];
        let len = rec.encode(&mut buf);
        // SAFETY: the handle was claimed via this live thread's TLS lookup
        // on the line above; engine arms never run from TLS destructors.
        unsafe { handle.push(&buf[..len]) }
    }

    fn prof_record_transport_loss(
        &self,
        handle: Option<bex_events::prof::backend::BoundaryHandle>,
    ) {
        if let Some(handle) = handle {
            bex_events::prof::backend::record_engine_transport_loss(self.engine_id, handle);
        }
    }

    /// Re-snapshots the VM's ring from the *current* OS thread's TLS (D5a).
    /// MUST run after every heap-permit re-acquire (the task may have
    /// migrated OS threads across the await) and before any engine-driven VM
    /// re-entry that can emit — `set_entry_point`, the loop-head exec, and
    /// `inject_sysop_throw`'s unwind all push through this snapshot.
    fn prof_refresh_vm_ring(&self, vm: &mut bex_vm::BexVm) {
        vm.prof_ring = if self.profiler_session.is_on() && !vm.prof_suppressed {
            bex_events::prof::ring_for_engine(self.engine_id.0)
                .map(bex_events::prof::RingHandle::ring)
        } else {
            None
        };
        if self.profiler_session.is_on() && !vm.prof_suppressed && vm.prof_ring.is_none() {
            self.prof_record_transport_loss(vm.prof_boundary_handle);
        }
    }

    fn prof_charge_await(
        &self,
        vm: &mut bex_vm::BexVm,
        call_id: u64,
        start_ticks: u64,
        end_ticks: u64,
    ) {
        let Some(memory) = self.profiler_session.memory() else {
            return;
        };
        let elapsed_ns = self
            .profiler_session
            .elapsed_ns(start_ticks, end_ticks)
            .ok();
        vm.prof_record_await(call_id, elapsed_ns, memory);
    }

    fn prof_emit_call_end(
        &self,
        vm: &mut bex_vm::BexVm,
        call_id: u64,
        status: bex_events::prof::record::FunctionEndStatus,
    ) {
        use bex_events::prof::record::RawRecord;
        let ts_ticks = bex_events::prof::clock::now_ticks();
        let thread_id = BexThreadId(vm.prof_thread_id);
        let committed = match vm.prof_take_await(call_id) {
            Some((await_ns, await_count)) => self.prof_emit(&RawRecord::EndFunctionAwaited {
                status,
                thread_id,
                call_id: BexCallId(call_id),
                ts_ticks,
                await_ns,
                await_count,
            }),
            None => self.prof_emit(&RawRecord::EndFunction {
                status,
                thread_id,
                call_id: BexCallId(call_id),
                ts_ticks,
            }),
        };
        if !committed {
            self.prof_record_transport_loss(vm.prof_boundary_handle);
        }
    }

    /// Closes the sys-op call pair opened at the VM's `VmExecState::SysOp`
    /// yield site (no-op when the VM minted nothing — profiling off or a
    /// non-call sys-op source). Cancellation paths pass `Cancelled` — the
    /// in-flight op was cancelled, not failed (§7 decision 1).
    fn prof_end_sysop(
        &self,
        vm: &mut bex_vm::BexVm,
        status: bex_events::prof::record::FunctionEndStatus,
    ) -> Option<(u64, u32)> {
        let call_id = vm.pending_sysop_call_id.take();
        let function_id = vm.pending_sysop_function_id.take().unwrap_or(0);
        vm.pending_sysop_capture_mask = VmCaptureMask::disabled();
        if vm.prof_ring.is_none() {
            return call_id.map(|call_id| (call_id, function_id));
        }
        if let Some(call_id) = call_id {
            self.prof_emit_call_end(vm, call_id, status);
        }
        call_id.map(|call_id| (call_id, function_id))
    }

    /// §7 decision 2: terminated threads never strand open calls. Closes
    /// every call frame still open in the suspended VM (innermost-first),
    /// plus any armed-but-unclosed sysop pair, with `status` `EndFunction`s.
    /// Called exactly once per terminated thread at cancellation blocks that
    /// end it without unwinding the VM. Threads whose terminal panic unwound
    /// VM-side already closed their frames in the unwinder, so those paths
    /// must not also drain. Emits via the TLS ring lookup, so it is safe on
    /// any OS thread regardless of the VM's ring snapshot (D5a).
    fn prof_drain_open_calls(
        &self,
        vm: &mut bex_vm::BexVm,
        status: bex_events::prof::record::FunctionEndStatus,
    ) {
        let pending_sysop_call_id = vm.pending_sysop_call_id.take();
        vm.pending_sysop_function_id = None;
        vm.pending_sysop_capture_mask = VmCaptureMask::disabled();
        if vm.prof_ring.is_none() {
            return;
        }
        if let Some(call_id) = pending_sysop_call_id {
            self.prof_emit_call_end(vm, call_id, status);
        }
        let open_call_ids: Vec<_> = vm.prof_open_call_ids().collect();
        for call_id in open_call_ids {
            self.prof_emit_call_end(vm, call_id, status);
        }
    }

    /// Status for a sysop pair that ended in an op error, classified by
    /// error class exactly like the VM's inline-native close
    /// (`prof_native_error_status`): cancel-classed → `Cancelled`,
    /// exit-classed → `Exited`, everything else → `Errored`. Needed because
    /// a cancel-classed payload can arrive through the *generic* result
    /// path (host drops the `CompletionHandle` without completing — the
    /// thread's own token never fired), and the frames the injected throw
    /// subsequently unwinds will close `Cancelled` via the unwinder's
    /// class peek — the pair must agree.
    fn prof_sysop_error_status(
        err: &sys_types::OpError,
    ) -> bex_events::prof::record::FunctionEndStatus {
        use bex_events::prof::record::FunctionEndStatus;
        use bex_vm_types::errors::{VmPanic, VmRustFnError};
        match &err.payload {
            sys_types::OpErrorPayload::Vm(VmRustFnError::Panic(VmPanic::Cancelled)) => {
                FunctionEndStatus::Cancelled
            }
            sys_types::OpErrorPayload::Vm(VmRustFnError::Panic(VmPanic::Exit { .. })) => {
                FunctionEndStatus::Exited
            }
            _ => FunctionEndStatus::Errored,
        }
    }
    /// Pre-extract LLM function metadata from heap objects.
    ///
    /// This avoids passing raw `HeapPtr`s to `sys_ops` — instead, we read the
    /// data once during construction and store it in `SysOpContext`.
    fn extract_llm_function_info(
        resolved_function_names: &HashMap<String, (HeapPtr, bex_vm_types::FunctionKind)>,
    ) -> HashMap<String, sys_types::LlmFunctionInfo> {
        let mut llm_functions = HashMap::new();
        for (name, (ptr, _kind)) in resolved_function_names {
            // SAFETY: ptr is from resolved_function_names, a compile-time object
            let obj = unsafe { ptr.get() };
            if let Object::Function(func) = obj {
                if let Some(FunctionMeta::Llm { client }) = &func.body_meta {
                    llm_functions.insert(
                        name.clone(),
                        sys_types::LlmFunctionInfo {
                            client_name: client.clone(),
                            return_type: declared_symbolic(&func.return_type, func),
                        },
                    );
                }
            }
        }
        llm_functions
    }

    /// Extract class definitions from the heap for output format rendering.
    fn extract_class_definitions(
        resolved_class_names: &indexmap::IndexMap<String, HeapPtr>,
    ) -> indexmap::IndexMap<baml_type::TypeName, sys_types::ClassDefinition> {
        let mut defs = indexmap::IndexMap::new();
        for (_name, ptr) in resolved_class_names {
            // SAFETY: ptr is from resolved_class_names, a compile-time object
            let obj = unsafe { ptr.get() };
            if let Object::Class(cls) = obj {
                defs.insert(
                    cls.name.clone(),
                    sys_types::ClassDefinition {
                        name: cls.name.display_name().to_string(),
                        description: cls.description.clone(),
                        alias: cls.alias.clone(),
                        fields: cls
                            .fields
                            .iter()
                            .map(|f| sys_types::ClassFieldDefinition {
                                name: f.name.clone(),
                                field_type: f.field_type.clone(),
                                field_template: Some(f.field_template.clone()),
                                description: f.description.clone(),
                                alias: f.alias.clone(),
                                skip: f.skip,
                            })
                            .collect(),
                    },
                );
            }
        }
        defs
    }

    /// Extract enum definitions from the heap for output format rendering.
    fn extract_enum_definitions(
        resolved_enum_names: &indexmap::IndexMap<String, HeapPtr>,
    ) -> indexmap::IndexMap<baml_type::TypeName, sys_types::EnumDefinition> {
        let mut defs = indexmap::IndexMap::new();
        for (_name, ptr) in resolved_enum_names {
            // SAFETY: ptr is from resolved_enum_names, a compile-time object
            let obj = unsafe { ptr.get() };
            if let Object::Enum(enm) = obj {
                defs.insert(
                    enm.name.clone(),
                    sys_types::EnumDefinition {
                        name: enm.name.display_name().to_string(),
                        description: enm.description.clone(),
                        alias: enm.alias.clone(),
                        variants: enm
                            .variants
                            .iter()
                            .filter(|v| !v.skip)
                            .map(|v| sys_types::EnumVariantDefinition {
                                name: v.name.clone(),
                                description: v.description.clone(),
                                alias: v.alias.clone(),
                            })
                            .collect(),
                    },
                );
            }
        }
        defs
    }

    fn enum_definition(enm: &bex_vm_types::Enum) -> sys_types::EnumDefinition {
        sys_types::EnumDefinition {
            name: enm.name.display_name().to_string(),
            description: enm.description.clone(),
            alias: enm.alias.clone(),
            variants: enm
                .variants
                .iter()
                .filter(|variant| !variant.skip)
                .map(|variant| sys_types::EnumVariantDefinition {
                    name: variant.name.clone(),
                    description: variant.description.clone(),
                    alias: variant.alias.clone(),
                })
                .collect(),
        }
    }

    fn class_definition(class: &bex_vm_types::Class) -> sys_types::ClassDefinition {
        sys_types::ClassDefinition {
            name: class.name.display_name().to_string(),
            description: class.description.clone(),
            alias: class.alias.clone(),
            fields: class
                .fields
                .iter()
                .filter(|field| !field.skip)
                .map(|field| sys_types::ClassFieldDefinition {
                    name: field.name.clone(),
                    field_type: field.field_type.clone(),
                    field_template: Some(field.field_template.clone()),
                    description: field.description.clone(),
                    alias: field.alias.clone(),
                    skip: field.skip,
                })
                .collect(),
        }
    }

    /// Gather runtime definitions from the type descriptors passed directly
    /// to a sys-op. Type arguments are lowered as ordinary `Object::Type`
    /// arguments, so this is the last synchronous chokepoint before the permit
    /// is released for async work.
    fn runtime_type_overlay(
        &self,
        args: &[Value],
        _permit: bex_heap::PermitProof<'_>,
    ) -> RuntimeTypeOverlay {
        let mut overlay = RuntimeTypeOverlay::default();
        for value in args {
            let Some(type_ptr) = value.as_object_ptr() else {
                continue;
            };
            let Object::Type(type_value) = (unsafe { type_ptr.get() }) else {
                continue;
            };
            for (name, class_ptr) in &type_value.defs().classes {
                let Object::Class(class) = (unsafe { class_ptr.get() }) else {
                    debug_assert!(
                        false,
                        "dynamic class definition must point to Object::Class"
                    );
                    continue;
                };
                overlay
                    .class_definitions
                    .entry(name.clone())
                    .or_insert_with(|| Self::class_definition(class));
                overlay
                    .class_handles
                    .entry(name.to_string())
                    .or_insert_with(|| self.heap.create_handle(*class_ptr));
            }
            for (name, enum_ptr) in &type_value.defs().enums {
                let Object::Enum(enm) = (unsafe { enum_ptr.get() }) else {
                    debug_assert!(false, "dynamic enum definition must point to Object::Enum");
                    continue;
                };
                overlay
                    .enum_definitions
                    .entry(name.clone())
                    .or_insert_with(|| Self::enum_definition(enm));
                overlay
                    .enum_handles
                    .entry(name.to_string())
                    .or_insert_with(|| self.heap.create_handle(*enum_ptr));
            }
        }
        overlay
    }

    fn runtime_schema_overlay(&self, vm: &BexVm, args: &[Value]) -> Option<RuntimeSchemaOverlay> {
        let mut pending = args
            .iter()
            .filter_map(Value::as_object_ptr)
            .filter_map(|ptr| match vm.get_object(ptr) {
                Object::Type(value) if !value.owner.is_null() => Some(value.owner),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut seen = std::collections::HashSet::<usize>::new();
        let mut classes = IndexMap::new();
        let mut enums = IndexMap::new();
        let mut named_owners = IndexMap::new();

        while let Some(owner) = pending.pop() {
            if !seen.insert(owner.as_ptr() as usize) {
                continue;
            }
            let Object::Package(package) = vm.get_object(owner) else {
                continue;
            };
            let Some(runtime) = package.runtime.as_ref() else {
                continue;
            };
            pending.extend(runtime.dependencies.iter().copied());
            let handle = self.heap.create_handle(owner);
            for ptr in package.classes.values().copied() {
                let Object::Class(class) = vm.get_object(ptr) else {
                    continue;
                };
                classes.insert(
                    class.name.clone(),
                    sys_types::ClassDefinition {
                        name: class.name.display_name().to_string(),
                        description: class.description.clone(),
                        alias: class.alias.clone(),
                        fields: class
                            .fields
                            .iter()
                            .map(|field| sys_types::ClassFieldDefinition {
                                name: field.name.clone(),
                                field_type: field.field_type.clone(),
                                field_template: Some(field.field_template.clone()),
                                description: field.description.clone(),
                                alias: field.alias.clone(),
                                skip: field.skip,
                            })
                            .collect(),
                    },
                );
                named_owners.insert(class.name.to_string(), handle.clone());
            }
            for ptr in package.enums.values().copied() {
                let Object::Enum(enm) = vm.get_object(ptr) else {
                    continue;
                };
                enums.insert(
                    enm.name.clone(),
                    sys_types::EnumDefinition {
                        name: enm.name.display_name().to_string(),
                        description: enm.description.clone(),
                        alias: enm.alias.clone(),
                        variants: enm
                            .variants
                            .iter()
                            .filter(|variant| !variant.skip)
                            .map(|variant| sys_types::EnumVariantDefinition {
                                name: variant.name.clone(),
                                description: variant.description.clone(),
                                alias: variant.alias.clone(),
                            })
                            .collect(),
                    },
                );
                named_owners.insert(enm.name.to_string(), handle.clone());
            }
        }

        (!named_owners.is_empty()).then_some(RuntimeSchemaOverlay {
            classes,
            enums,
            named_owners,
        })
    }

    /// Get a reference to the shared heap.
    pub fn heap(&self) -> &Arc<BexHeap> {
        &self.heap
    }

    /// Get statistics about heap usage.
    ///
    /// Useful for monitoring concurrent execution and debugging.
    pub fn heap_stats(&self) -> bex_heap::HeapStats {
        self.heap.stats()
    }

    /// Get a reference to the heap permit manager.
    pub fn heap_permit_manager(&self) -> &Arc<HeapPermitManager> {
        &self.heap_permit_manager
    }

    /// Number of currently `Pending` futures tracked by the engine. Useful
    /// for telemetry and tests that verify the future manager cleans up
    /// completed futures.
    pub async fn active_future_count(&self) -> usize {
        self.futures
            .active_future_count(&self.heap_permit_manager)
            .await
    }

    pub fn set_unhandled_spawn_error_handler(&self, handler: Option<UnhandledSpawnErrorHandler>) {
        self.unhandled_spawn_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .handler = handler;
        self.drain_unhandled_spawn_errors();
    }

    pub fn take_unhandled_spawn_errors(&self) -> Vec<UnhandledSpawnError> {
        self.unhandled_spawn_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .queued
            .drain(..)
            .collect()
    }

    async fn begin_shutdown(self: &Arc<Self>) -> Option<ShutdownGuard> {
        loop {
            let notified = self.lifecycle_changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let mut lifecycle = self
                    .lifecycle
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                match *lifecycle {
                    EngineLifecycle::Running => {
                        *lifecycle = EngineLifecycle::Closing;
                        return Some(ShutdownGuard {
                            engine: Arc::clone(self),
                            completed: false,
                        });
                    }
                    EngineLifecycle::Closed => return None,
                    EngineLifecycle::Closing => {}
                }
            }
            notified.await;
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn active_call_count(&self) -> usize {
        self.active_calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .filter(|call| !call.pending)
            .count()
    }

    async fn wait_for_active_calls(&self) {
        loop {
            let notified = self.lifecycle_changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self
                .active_calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .values()
                .all(|call| call.pending)
            {
                return;
            }
            notified.await;
        }
    }

    /// Wait for spawned work to settle, then run the final GC sweep that
    /// surfaces unreachable unobserved errors.
    pub async fn shutdown(self: &Arc<Self>) {
        self.shutdown_with_progress(|count| {
            tracing::warn!(
                count,
                "BAML is still waiting for active futures to finish (press Ctrl+C to cancel now)"
            );
        })
        .await;
    }

    /// Shut down the engine, reporting the number of active BAML futures every
    /// five seconds while shutdown is blocked.
    pub async fn shutdown_with_progress<F>(self: &Arc<Self>, on_wait: F)
    where
        F: FnMut(usize) + Send,
    {
        #[cfg(not(target_arch = "wasm32"))]
        const WAIT_LOG_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);
        #[cfg(not(target_arch = "wasm32"))]
        let mut on_wait = on_wait;
        #[cfg(target_arch = "wasm32")]
        let _ = on_wait;

        let Some(shutdown) = self.begin_shutdown().await else {
            return;
        };
        #[cfg(not(target_arch = "wasm32"))]
        loop {
            if tokio::time::timeout(WAIT_LOG_INTERVAL, self.wait_for_active_calls())
                .await
                .is_ok()
            {
                break;
            }
            let count = self.active_call_count();
            if count != 0 {
                on_wait(count);
            }
        }
        #[cfg(target_arch = "wasm32")]
        self.wait_for_active_calls().await;

        loop {
            let handles = self
                .futures
                .pending_join_handles(&self.heap_permit_manager)
                .await;
            if handles.is_empty() {
                break;
            }
            let wait = async move {
                for handle in handles {
                    let _ = handle.wait().await;
                }
            };
            #[cfg(not(target_arch = "wasm32"))]
            {
                if tokio::time::timeout(WAIT_LOG_INTERVAL, wait).await.is_err() {
                    let count = self.active_future_count().await;
                    if count != 0 {
                        on_wait(count);
                    }
                }
            }
            #[cfg(target_arch = "wasm32")]
            {
                wait.await;
            }
        }

        self.collect_garbage(bex_heap::CollectionLevel::Major).await;
        shutdown.complete();
    }

    fn enqueue_unhandled_spawn_errors(
        &self,
        errors: impl IntoIterator<Item = UnhandledSpawnError>,
    ) {
        self.unhandled_spawn_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .queued
            .extend(errors);
        self.drain_unhandled_spawn_errors();
    }

    fn drain_unhandled_spawn_errors(&self) {
        {
            let mut state = self
                .unhandled_spawn_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.delivering || state.handler.is_none() || state.queued.is_empty() {
                return;
            }
            state.delivering = true;
        }

        loop {
            let next = {
                let mut state = self
                    .unhandled_spawn_state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let Some(handler) = state.handler.clone() else {
                    state.delivering = false;
                    return;
                };
                let Some(error) = state.queued.pop_front() else {
                    state.delivering = false;
                    return;
                };
                (handler, error)
            };

            let (handler, error) = next;
            if catch_unwind(AssertUnwindSafe(|| handler(error.clone()))).is_err() {
                let mut state = self
                    .unhandled_spawn_state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                state.queued.push_front(error);
                state.handler = None;
                state.delivering = false;
                drop(state);
                tracing::error!(
                    "unhandled spawn error handler panicked; handler removed and report requeued"
                );
                return;
            }
        }
    }

    async fn dispatch_unhandled_spawn_errors(&self) {
        let _delivery = self.unhandled_spawn_delivery.lock().await;
        let pending: Vec<_> = self
            .rooted_unhandled_spawn_errors
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .drain(..)
            .collect();
        if pending.is_empty() {
            return;
        }
        let permit = self.heap_permit_manager.new_permit(()).await;
        let active = permit.acquire().await;
        let errors: Vec<_> = pending
            .into_iter()
            .map(|error| {
                let value = match error.value {
                    RootedUnhandledValue::Inline(value) => value,
                    RootedUnhandledValue::Handle(handle) => {
                        let ptr = self
                            .resolve_handle(active.proof(), &handle)
                            .expect("fresh unhandled-error handle must resolve");
                        Value::object(ptr)
                    }
                };
                UnhandledSpawnError {
                    report_id: error.report_id,
                    value: self.vm_value_to_owned(active.proof(), value),
                    trace: error.trace,
                    cancelled: error.cancelled,
                }
            })
            .collect();
        drop(active);
        self.enqueue_unhandled_spawn_errors(errors);
    }

    /// Resolve a [`bex_external_types::Handle`] to its current [`HeapPtr`].
    ///
    /// The permit parameter proves GC cannot run while the caller is using the
    /// returned pointer — holding any `ActiveHeapPermit<T>` keeps one of the
    /// manager's semaphore tokens in scope, so `request_park` cannot proceed.
    ///
    /// Returns `None` if the handle has been invalidated.
    pub fn resolve_handle(
        &self,
        _permit: bex_heap::PermitProof<'_>,
        handle: &bex_external_types::Handle,
    ) -> Option<HeapPtr> {
        use bex_external_types::WeakHeapRef;
        self.heap.resolve_handle_ptr(handle.slab_key())
    }

    /// Explicitly trigger garbage collection.
    ///
    /// Requests and waits for all heap permit holders to park.
    /// Once they are parked, runs the GC.
    ///
    /// # Returns
    ///
    /// Statistics about the collection (live count, collected count, etc.)
    pub async fn collect_garbage(
        self: &Arc<Self>,
        level: bex_heap::CollectionLevel,
    ) -> bex_heap::GcStats {
        #[cfg(not(target_arch = "wasm32"))]
        let park_request_guard = ParkRequestGuard::new(Arc::clone(&self.park_requested));
        let mut heap_guard = self.heap_permit_manager.request_park().await;
        #[cfg(not(target_arch = "wasm32"))]
        drop(park_request_guard);

        // Collect roots from handles (objects returned to external code)
        let mut all_roots = self.heap.collect_handle_roots();

        heap_guard.collect_roots(&mut all_roots);

        tracing::debug!(
            "GC: {} total roots from {} handles and {} parked heap permits",
            all_roots.len(),
            self.heap.stats().active_handles,
            heap_guard.num_permits(),
        );

        // Run GC — always returns the forwarding map so we can update parked VM stacks.
        let (stats, _remapped_roots, forwarding) =
            unsafe { self.heap.collect_garbage_generational(&all_roots, level) };

        // Bug H, check 1 (heap_debug only): every pointer the GC was told
        // about (`all_roots`) must end up in the forwarding map. If a
        // holder's `collect_roots` produces a pointer the GC's BFS does
        // not reach, the subsequent `forward_roots` will leave a stale
        // reference behind. This assertion turns that silent class of
        // bug into an immediate panic during stress tests.
        #[cfg(feature = "heap_debug")]
        for &ptr in &all_roots {
            assert!(
                forwarding.contains_key(&ptr),
                "heap_debug: post-GC integrity sweep — root {ptr:?} was not \
                 reached by the GC BFS (collect_roots produced it but it is \
                 not in the forwarding map)"
            );
        }

        // Update all parked VM stacks with forwarding pointers and invalidate TLABs
        // SAFETY: VMs are still parked (gc_complete not yet notified), we have
        // exclusive access via the parked_vms lock we're still holding
        heap_guard.forward_roots(&forwarding);

        // Bug H, check 3 (heap_debug only): after `forward_roots`, no
        // holder root should still point into the inactive (former
        // active) space. If any does, `forward_roots` missed it — the
        // stale pointer would dereference into freed/poisoned memory.
        #[cfg(feature = "heap_debug")]
        {
            let mut roots_after = self.heap.collect_handle_roots();
            heap_guard.collect_roots(&mut roots_after);
            for &ptr in &roots_after {
                assert!(
                    !self.heap.debug_ptr_in_inactive(ptr),
                    "heap_debug: post-forward_roots — root {ptr:?} still points \
                     into the inactive space (forward_roots missed it)"
                );
            }
        }

        self.heap.verify_quick();

        // Root object-valued errors into handles before releasing the GC
        // guard. The raw queue values are post-copy pointers and must survive
        // the await needed to reacquire an ordinary permit for deep-copying.
        let unhandled_spawn_errors = self
            .heap
            .take_unhandled_spawn_errors()
            .into_iter()
            .map(|error| {
                let value = error
                    .value
                    .as_object_ptr()
                    .map_or(RootedUnhandledValue::Inline(error.value), |ptr| {
                        RootedUnhandledValue::Handle(self.heap.create_handle(ptr))
                    });
                RootedUnhandledSpawnError {
                    report_id: error.future_id.as_usize(),
                    value,
                    trace: error.trace,
                    cancelled: error.cancelled,
                }
            })
            .collect::<VecDeque<_>>();
        self.rooted_unhandled_spawn_errors
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend(unhandled_spawn_errors);

        drop(heap_guard);

        // Flush deferred host-value releases now that the stop-the-world window
        // has closed. Collecting a dead `Object::HostClosure` runs
        // `HostValueArc::drop`, which only *enqueues* the key (it cannot fire
        // the host release callback inline: `Drop` runs while all heap permits
        // are parked, and the callback runs arbitrary host code — e.g. Python
        // re-acquires the GIL — which would be an AB-BA deadlock against the
        // parked permits). The `heap_guard` is dropped above, so all permits
        // are released and we hold none here; this is a safe point to invoke
        // the host callbacks. (`gc_safepoint` re-acquires its permit only
        // *after* `collect_garbage` returns; both `maybe_collect_garbage` call
        // sites release their permit before calling.)
        bex_external_types::host_value::host_release_dispatch::drain();

        self.dispatch_unhandled_spawn_errors().await;

        // BEP-042: run the `cleanup` finalizer for every instance this
        // collection kept alive. The `heap_guard` is dropped (so `call_function`
        // can acquire a permit), the queued pointers are valid (drained at this
        // same safepoint, before any other collection), and a caller that holds
        // `checking_gc` (the engine GC paths) blocks a nested collection from
        // moving them mid-drain.
        self.drain_finalizers().await;

        tracing::debug!(
            "GC completed: {} live, {} collected",
            stats.live_count,
            stats.collected_count
        );

        stats
    }

    /// Execute a function by name.
    ///
    /// # Arguments
    ///
    /// Arguments are passed as `Vec<BexExternalValue>`:
    /// - Primitives and strings are passed directly (e.g. `BexExternalValue::String(...)`)
    /// - `Handle` references existing heap objects
    /// - `Adt(Media | PromptAst)` allocates new builtin ADT objects on the heap
    ///
    /// # Example
    ///
    /// ```ignore
    /// let result = engine.call_function("get_user", vec![
    ///     "Alice".into(),
    ///     42i64.into(),
    /// ], None).await?;
    /// ```
    pub async fn call_function(
        self: &Arc<Self>,
        function_name: &str,
        args: Vec<BexExternalValue>,
        call_ctx: FunctionCallContext,
        copy_objects: bool,
    ) -> Result<BexExternalValue, EngineError> {
        self.call_function_with_trace(function_name, args, call_ctx, copy_objects)
            .await
            .and_then(|result| result.value)
    }

    pub async fn call_function_with_trace(
        self: &Arc<Self>,
        function_name: &str,
        args: Vec<BexExternalValue>,
        FunctionCallContext {
            host_call_id,
            boundary,
            logger,
            cancel,
            profile_intent,
            type_args,
            type_defs,
        }: FunctionCallContext,
        copy_objects: bool,
    ) -> Result<BexCallResult, EngineError> {
        let args = args
            .into_iter()
            .map(|arg| BexCallArg::Provided(Box::new(arg)))
            .collect();
        self.call_function_bound_args_with_trace(
            function_name,
            args,
            FunctionCallContext {
                host_call_id,
                boundary,
                logger,
                cancel,
                profile_intent,
                type_args,
                type_defs,
            },
            copy_objects,
        )
        .await
    }

    pub async fn call_function_bound_args(
        self: &Arc<Self>,
        function_name: &str,
        args: Vec<BexCallArg>,
        call_ctx: FunctionCallContext,
        copy_objects: bool,
    ) -> Result<BexExternalValue, EngineError> {
        self.call_function_bound_args_with_trace(function_name, args, call_ctx, copy_objects)
            .await
            .and_then(|result| result.value)
    }

    pub async fn call_function_bound_args_with_trace(
        self: &Arc<Self>,
        function_name: &str,
        args: Vec<BexCallArg>,
        FunctionCallContext {
            host_call_id,
            boundary,
            logger,
            cancel,
            profile_intent,
            type_args,
            type_defs,
        }: FunctionCallContext,
        copy_objects: bool,
    ) -> Result<BexCallResult, EngineError> {
        // Register this host call so `cancel_function_call(host_call_id)` can
        // target it. The RAII guard removes the entry on drop (including
        // panic unwind). Insertion and guard construction are atomic, so a
        // panic here cannot leak registry entries.
        let (_call_guard, cancel) =
            ActiveCallGuard::register(Arc::clone(self), host_call_id, cancel)?;

        // Fail fast if already cancelled — guarantees pre-cancelled IDs
        // always produce a `baml.panics.Cancelled` panic regardless of
        // function contents.
        if cancel.is_cancelled() {
            return Err(cancelled_unhandled_throw());
        }

        let (function_index, kind) = self.lookup_function(function_name)?;
        if matches!(kind, bex_vm_types::FunctionKind::NativeUnresolved) {
            return Err(EngineError::NotInvokableAsEntry {
                name: function_name.to_string(),
                kind: format!("{kind:?}"),
            });
        }
        self.validate_bound_args(function_name, &args)?;
        let mut return_type = self
            .function_return_type(function_name)
            .unwrap_or(RuntimeTy::Null {
                attr: baml_type::TyAttr::default(),
            });
        let throws_type = self.function_throws_type(function_name);

        // Declared parameter types (TypeVars unsubstituted).
        let params = self.function_params(function_name)?;
        let declared_param_types: Vec<RuntimeTy> =
            params.iter().map(|(_, ty, _)| (*ty).clone()).collect();

        // `type_args` is the unified `TypeVar -> concrete` binding map for a
        // generic call (01pt3). It already holds the host's explicit `_types=`
        // bindings (source (b)); fold in source (a): class type args recovered
        // from a generic `self` receiver (instance methods). A generic instance
        // method called by name leaves its declared types with the class's type
        // vars unsubstituted (e.g. `Stream.next`'s `TStream | Done`);
        // the inbound `self` handle carries them concretely, so zipping the
        // declared `self` against the actual recovers the bindings. See
        // bridge-generics/streaming/04. `collect_type_var_bindings` only fills
        // keys not already bound, so the explicit `_types=` bindings win on
        // conflict.
        // Whether the *caller* specified any explicit type binding (subscript /
        // `_types=`). In explicit/partial mode the wire must be fully bound — an
        // under-specified (argless) generic instance is a host bug and Gate B
        // rejects it. In pure-inference mode (no caller bindings) BAML instead
        // recovers an unbound instance's args from its field values (03b G1), so
        // Gate B is lenient about its missing wire args. Captured before the
        // self-receiver / inference sources mutate `type_args`.
        let caller_specified_types = !type_args.is_empty();
        let mut type_args = type_args;
        if let (Some(self_declared), Some(BexCallArg::Provided(self_value))) =
            (declared_param_types.first(), args.first())
        {
            if let Some(self_actual) = crate::conversion::tagged_handle_runtime_ty(self_value) {
                crate::conversion::collect_type_var_bindings(
                    self_declared,
                    self_actual,
                    &mut type_args,
                );
            }
        }
        // A call is generic iff the callee declares any generic params, OR a
        // TypeVar appears in a parameter / the return type. The declared-params
        // list also catches type params used only in the body (e.g.
        // `one_type_arg<T>()` reflecting `T`), which a signature scan misses.
        // Non-generic calls bypass all type-var work and keep the existing
        // permissive coercion path untouched (no regression, zero extra cost).
        // Computed here (before the `type_args` freeze) because the inference
        // step below mutates `type_args` and is gated on this flag. The
        // (unsubstituted) `return_type` is used; self-receiver substitution does
        // not change whether the callee is generic.
        let declared_generic_params = self.function_generic_params(function_name);
        let callee_is_generic = !declared_generic_params.is_empty()
            || declared_param_types
                .iter()
                .chain(std::iter::once(&return_type))
                .any(crate::conversion::contains_type_var);

        // ── TYPEVAR BINDING POLICY (inbound value-inference 01a/01b +
        // `03c-impl-guide`). For each `TypeVar` `T`, after applying explicit
        // (`_types=` / subscript) bindings:
        //
        //   1. Explicit wins. If the caller specified `T`, use it. (This is also
        //      what satisfies the must-specify cases below.)
        //   2. Closure poison ⇒ must-specify. If `T` occurs *anywhere inside a
        //      closure/lambda-typed parameter's signature* (its param types or
        //      return type, at any nesting depth), `T` MUST be explicitly
        //      specified; unspecified ⇒ the call ERRORS. This OVERRIDES any
        //      value-position evidence `T` might also have — a closure occurrence
        //      poisons `T` globally.
        //   3. No value position ⇒ must-specify. If `T`'s only occurrences are
        //      return-only or body-only (no value position anywhere), `T`
        //      likewise MUST be explicitly specified; errors if not.
        //   4. Value position, no leaf ⇒ `RustType`. If `T` has ≥1 value
        //      position, NO closure occurrence, and value-inference still
        //      produced no concrete leaf (empty collection, `null` actual,
        //      unbound generic under a non-recursing formal), default
        //      `T = RustType` and let it ride opaquely as `RustData`.
        //
        // The normal path (a value position that *did* yield a leaf) is
        // unchanged. Steps 2–3 are detectable from the *signature alone*, so the
        // error is eager, at the call site (Gate A below) — not deferred into the
        // body.
        //
        // Mechanically: inference only ever *adds* bindings — synthesize each
        // provided arg's concrete `RuntimeTy`, unify against the declared param
        // type, and fold in with `or_insert` so explicit (source b) and
        // self-receiver (source a) bindings already in `type_args` WIN.
        if callee_is_generic {
            // Synthesize each provided arg's `RuntimeTy` and pair it with its
            // declared formal, then solve every var across all arguments at once
            // with variance tracking (`02d`/`02e`): a `TypeVar` used at
            // conflicting variances — contravariant function params, invariant
            // container/class args — has no consistent binding and is rejected
            // here rather than fabricated into an unsound union.
            let pairs: Vec<(RuntimeTy, RuntimeTy)> = args
                .iter()
                .enumerate()
                .filter_map(|(idx, arg)| match (declared_param_types.get(idx), arg) {
                    (Some(declared), BexCallArg::Provided(value)) => {
                        // Formal-aware: a forcing generic-class formal recovers an
                        // unbound instance's args from its fields (03b G1); every
                        // other value synthesizes from the value alone.
                        let actual = self.synth_inference_actual(declared, value);
                        Some((declared.clone(), actual))
                    }
                    _ => None,
                })
                .collect();
            let inferred =
                crate::conversion::infer_bindings_runtime_checked(&pairs).map_err(|detail| {
                    EngineError::TypeMismatch {
                        message: friendly_inference_conflict(function_name, &detail),
                    }
                })?;

            // Classify where each TypeVar occurs across the parameter types to
            // drive rules 2 and 4 above.
            let positions = crate::conversion::classify_param_var_positions(&declared_param_types);

            for (name, ty) in inferred {
                // Rule 2 (closure poison): drop any value-inferred binding so the
                // var is required explicitly — even when another argument would
                // otherwise pin it (`apply<T,R>(f: (T)->R, x: T)` — `x` must NOT
                // bind `T`).
                if positions.closure.contains(&name) {
                    continue;
                }
                type_args.entry(name).or_insert(ty);
            }

            // Rule 4 (RustType default): a var with ≥1 value position, no closure
            // occurrence, still unbound after inference defaults to `rust_type` and
            // rides opaquely. Vars with no value position (return/body-only, rule 3)
            // and closure-poisoned vars (rule 2) are left unbound for Gate A's
            // must-specify error.
            for var in &positions.value_position {
                if positions.closure.contains(var) || positions.ambiguous_union.contains(var) {
                    continue;
                }
                type_args
                    .entry(var.clone())
                    .or_insert_with(|| RuntimeTy::RustType {
                        attr: baml_type::TyAttr::default(),
                    });
            }
        }

        let mut type_args = type_args;

        // Materialize definition-carrying host type bindings once per call.
        // Every call receives fresh mints, while the exact `TypeValue`s remain
        // attached to the entry frame so `LoadType<T>` preserves that arrival's
        // identity and definition overlay.
        let mut thread = self.new_root_thread(cancel.clone()).await;
        let mut type_values = IndexMap::new();
        for (name, definition) in type_defs {
            let type_value = thread
                .vm
                .materialize_portable_type_def(definition)
                .map_err(|message| EngineError::TypeMismatch { message })?;
            type_args.insert(name.clone(), RuntimeTy::from(&type_value.ty));
            type_values.insert(name, type_value);
        }

        // Always fold the recovered bindings into the return type. This is the
        // pre-existing streaming fix (a generic `self` method's return type
        // carries the class's type vars; the receiver binds them concretely),
        // now also applying any inferred bindings, and is a no-op when
        // `type_args` is empty.
        return_type = crate::conversion::substitute_type_vars(&return_type, &type_args);

        // Strict generic handling — substitution + full-binding enforcement
        // (Gate A) + per-arg structural check (Gate B) — applies to *every*
        // generic call: free functions, instance methods, and static methods
        // alike. The host is required to fully bind every call: a free
        // function's/static's TypeVars arrive by name through `type_args`, and
        // a generic instance method's class TypeVars arrive on the receiver
        // (sent by the host as the instance's wire class args, or recovered
        // from a generic `self` handle above). If a receiver can't supply its
        // class type args, that's a host/SDK bug to fix at the source — the
        // engine does not paper over it with a runtime-`unknown` fallback.
        // Non-generic calls bypass all of this and keep the existing permissive
        // coercion path untouched.
        let param_types: Vec<RuntimeTy> = if callee_is_generic {
            // Substitute the explicit/recovered bindings into every declared
            // parameter so coercion and validation see concrete types instead
            // of bare TypeVars.
            let substituted: Vec<RuntimeTy> = declared_param_types
                .iter()
                .map(|t| crate::conversion::substitute_type_vars(t, &type_args))
                .collect();

            // ── Gate A — full-binding enforcement. The wire must be fully
            // bound. Two checks:
            //   (1) every declared generic param has a binding — catches
            //       body-only type params (`one_type_arg<T>()`) that never reach
            //       the signature. Scoped to *free functions*: a class method's
            //       `display_type_params` include the enclosing class params,
            //       which a static never binds and an instance method carries on
            //       the receiver — demanding them *by name* would be wrong, so
            //       for class methods this check is skipped and the method's own
            //       params fall to check (2) (they're in the signature).
            //   (2) no TypeVar survives substitution in the params/return —
            //       catches a param/return type var the bindings didn't cover,
            //       including an instance method whose receiver failed to supply
            //       its class type args.
            let missing_declared = if self.is_class_method(function_name) {
                // Demand the method's OWN generic params (the suffix after the
                // class prefix). Inherited class params ride on the receiver (an
                // instance method) or are phantom (a static), so they're never
                // bound by name and must not be demanded. The method's own params
                // ARE demanded — so rule 3 (no value position ⇒ must-specify)
                // fires for a method's body-only own var (`reflect_t<T>()`), which
                // check (2)'s signature scan misses.
                let class_prefix = self.enclosing_class_generic_param_count(function_name);
                declared_generic_params
                    .iter()
                    .skip(class_prefix)
                    .find(|p| !type_args.contains_key(p.as_str()))
                    .cloned()
            } else {
                declared_generic_params
                    .iter()
                    .find(|p| !type_args.contains_key(p.as_str()))
                    .cloned()
            };
            let unbound = missing_declared.or_else(|| {
                substituted
                    .iter()
                    .chain(std::iter::once(&return_type))
                    .find_map(crate::conversion::first_unbound_type_var)
            });
            if let Some(name) = unbound {
                return Err(EngineError::TypeMismatch {
                    message: friendly_must_specify(
                        function_name,
                        &declared_generic_params,
                        name.as_str(),
                    ),
                });
            }
            substituted
        } else {
            declared_param_types
        };

        // Type-directed coercion for each provided arg: lets host SDKs send
        // `int(42)` to a `bigint` slot (and vice versa) without re-encoding,
        // and rewrites the engine-registered class FQN onto incoming
        // `Map`/`Instance`/`Variant` values. Idempotent for already-matching
        // values, so callers that already coerced (e.g. `BexProject::Bex`
        // kwargs entry) aren't double-charged. For a generic call, the now-
        // concrete `param_types` also drive Gate B — a structural check that
        // hard-fails a wire value that doesn't inhabit its expected type
        // (01pt3 item 5).
        let args: Vec<BexCallArg> = args
            .into_iter()
            .enumerate()
            .map(|(idx, arg)| match arg {
                BexCallArg::Provided(value) => {
                    let coerced = self.coerce_inbound_arg(*value, &param_types[idx])?;
                    if callee_is_generic {
                        crate::conversion::check_generic_arg(
                            &coerced,
                            &param_types[idx],
                            caller_specified_types,
                        )
                        .map_err(|detail| EngineError::TypeMismatch {
                            message: friendly_arg_type_mismatch(function_name, idx, &detail),
                        })?;
                    }
                    Ok(BexCallArg::Provided(Box::new(coerced)))
                }
                BexCallArg::OmittedDefault => Ok(BexCallArg::OmittedDefault),
            })
            .collect::<Result<_, EngineError>>()?;

        // Reuse the (substituted) `param_types` to thread the expected
        // `RuntimeTy` into per-arg VM conversion. Binding a `HostValue` to an
        // `Object::HostClosure` needs it: the closure carries the declared
        // `RuntimeTy::Function`'s arity and return type, extracted from the
        // parameter type.
        let vm_args: Vec<Value> = args
            .into_iter()
            .enumerate()
            .map(|(idx, arg)| match arg {
                BexCallArg::Provided(arg) => self.convert_external_to_vm_value_with_ty(
                    &mut thread,
                    *arg,
                    param_types.get(idx),
                ),
                BexCallArg::OmittedDefault => Ok(Value::OMITTED_ARG),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let root_input_values = params
            .iter()
            .zip(vm_args.iter())
            .map(|((name, _, _), value)| ((*name).to_string(), *value))
            .collect::<Vec<_>>();

        // `function_index` is the entry function's `HeapPtr`. The call's named
        // `type_args` are lowered to positional De Bruijn slots against the
        // callee's generic params inside `set_entry_point_with_type_args`.
        self.run_entry_point(
            thread,
            function_index,
            vm_args,
            type_args,
            type_values,
            return_type,
            throws_type,
            host_call_id,
            boundary,
            profile_intent,
            logger,
            Some(root_input_values),
            cancel,
            copy_objects,
        )
        .await
    }

    /// Build a fresh root [`BexThread`] over the shared heap and acquire its
    /// heap permit. Shared by the named-entry (`call_function_bound_args`) and
    /// callable-entry (`call_callable`) paths.
    async fn new_root_thread(
        self: &Arc<Self>,
        cancel: CancellationToken,
    ) -> ActiveHeapPermit<BexThread> {
        // Globals are shared as a frozen `Arc<[Value]>` — cloning is a refcount bump.
        let vm = BexVm::new(
            Arc::clone(&self.heap),
            VmGlobals::Shared(self.globals.clone()),
            #[cfg(not(target_arch = "wasm32"))]
            Arc::clone(&self.park_requested),
            Arc::clone(&self.argv),
            Arc::clone(&self.packages),
            Arc::clone(&self.dynamic_dispatch),
            Arc::clone(&self.error_class_ptrs),
            Arc::clone(&self.panic_class_ptrs),
        );
        // BEP-034: wrap the root VM in a `BexThread` from the outset so the
        // permit's `RootHaver` is the thread (delegating to the inner VM).
        // Spawned children build their own `BexThread`s in `spawn_thread`.
        let mut vm = vm;
        vm.prof_thread_id = self.next_prof_thread_id();
        vm.root_profiler =
            RootProfiler::Inactive(bex_events::prof::backend::InactiveReason::Suppressed);
        vm.prof_suppressed = true;
        vm.prof_boundary_root_pending = false;
        // Identity seed for the `$id` surface (baml.id.*): unconditional —
        // `$id` works with profiling off, and the ids it exposes are the
        // VM-minted ids the event stream records.
        vm.bex_ref_seed = Some((self.process_euid, self.engine_id));
        // No ring snapshot and no StartThread here: the permit acquisition
        // below awaits (the snapshot would go stale across an OS-thread
        // migration), and early-error returns between here and the run loop
        // would leak a StartThread with no EndThread. Both happen at
        // guaranteed-balanced points instead: the StartThread in
        // run_entry_point right before set_entry_point (§7 decision 7 —
        // straight-line into run_thread_event_loop, whose every exit path
        // emits the EndThread), and the snapshot at the same spot plus each
        // loop-head resume.
        let root_thread = BexThread::new_root(vm, cancel);
        let inactive = self.heap_permit_manager.new_permit(root_thread).await;
        inactive.acquire().await
    }

    /// Shared entry-point core: set the VM's entry frame, run the thread event
    /// loop, and extract the root value. Used by both the named-function path
    /// and the closure path; the callers differ only in how they resolve the
    /// entry pointer and its `return`/`throws` types.
    #[expect(clippy::too_many_arguments)]
    async fn run_entry_point(
        self: &Arc<Self>,
        mut thread: ActiveHeapPermit<BexThread>,
        entry_ptr: HeapPtr,
        vm_args: Vec<Value>,
        type_args: indexmap::IndexMap<String, RuntimeTy>,
        type_values: indexmap::IndexMap<String, bex_vm_types::types::TypeValue>,
        return_type: RuntimeTy,
        throws_type: Option<RuntimeTy>,
        host_call_id: CallId,
        boundary: BoundaryContext,
        profile_intent: RootProfileIntent,
        logger: TraceLogger,
        root_input_values: Option<Vec<(String, Value)>>,
        cancel: CancellationToken,
        copy_objects: bool,
    ) -> Result<BexCallResult, EngineError> {
        // Finish all fallible host type narrowing before durable run
        // admission. Once `run.meta` commits, the path below is straight-line
        // into the balanced root event loop.
        let type_args = type_args
            .into_iter()
            .map(|(name, ty)| match baml_type::RealizedTy::try_from(&ty) {
                Ok(realized) => Ok((name, realized)),
                Err(e) => Err(EngineError::VmInternalError(
                    bex_vm::errors::VmInternalError::TypeSubstitution {
                        message: format!(
                            "host entry-point type argument `{name}` is not realized: {e}"
                        ),
                    },
                )),
            })
            .collect::<Result<indexmap::IndexMap<_, _>, _>>()?;
        let root_thread_ref = ThreadRef {
            process_euid: self.process_euid,
            engine_id: self.engine_id,
            thread_id: BexThreadId(thread.vm.prof_thread_id),
        };
        let admission = self.profiler_session.register_root(
            profile_intent,
            root_thread_ref,
            self.program_metadata.program_id,
            self.program_metadata
                .revision_id
                .as_ref()
                .map(|revision| revision.0.clone()),
            self.program_metadata
                .source_snapshot_id
                .as_ref()
                .map(|source| hex_bytes(&source.0)),
        );
        thread.vm.root_profiler = admission.profiler();
        thread.vm.prof_boundary_handle = admission.boundary_handle();
        thread.vm.prof_suppressed = !thread.vm.root_profiler.is_active();
        thread.vm.prof_boundary_root_pending = thread.vm.root_profiler.is_active();
        if thread.vm.root_profiler.is_active() {
            thread.vm.prof_enable_await_accumulator();
        }
        // D5a: the entry-frame CallFunction below pushes into the snapshot;
        // take it on THIS thread, after the last await before the push.
        self.prof_refresh_vm_ring(&mut thread.vm);
        // §7 decision 7: the root StartThread is emitted before the entry
        // frame's CallFunction so that every thread's first record is its
        // StartThread — a uniform invariant for renderers (children get
        // theirs at the Spawn arm, before their entry push). BALANCE: the
        // matching EndThread is emitted by run_thread_event_loop on every
        // exit path; no early return / `?` may be introduced between this
        // emission and the run_thread_event_loop call below, or an error
        // path would leak an unclosed StartThread.
        if thread.vm.prof_ring.is_some() {
            let committed = self.prof_emit(&bex_events::prof::record::RawRecord::StartThread {
                flags: 0,
                thread_id: BexThreadId(thread.vm.prof_thread_id),
                parent_thread_id: BexThreadId(0), // engine-root thread
                parent_call_id: BexCallId(0),
                ts_ticks: bex_events::prof::clock::now_ticks(),
                name: b"",
            });
            if !committed {
                self.prof_record_transport_loss(thread.vm.prof_boundary_handle);
            }
        }
        thread
            .vm
            .set_entry_point_with_type_values(entry_ptr, &vm_args, type_args, type_values);
        thread
            .vm
            .install_boundary_id_for_current_call(boundary.boundary_id);
        let entry_call_ref = CallRef {
            process_euid: self.process_euid,
            engine_id: self.engine_id,
            thread_id: BexThreadId(thread.vm.prof_thread_id),
            call_id: BexCallId(thread.vm.current_call_id()),
        };
        #[cfg(not(target_arch = "wasm32"))]
        let backend_value_capture =
            thread
                .vm
                .prof_boundary_handle
                .map(|boundary| BackendValueCaptureContext {
                    session: Arc::clone(&self.profiler_session),
                    boundary,
                });
        #[cfg(not(target_arch = "wasm32"))]
        let root_capture = backend_value_capture
            .clone()
            .map(|backend| RootValueCaptureContext {
                call_ref: entry_call_ref,
                backend,
            });
        #[cfg(target_arch = "wasm32")]
        let root_capture: Option<RootValueCaptureContext> = None;
        let log_capture = logger.is_enabled().then(|| LogCaptureContext {
            boundary_id: boundary.boundary_id,
            logger: logger.clone(),
        });
        #[cfg(not(target_arch = "wasm32"))]
        let call_capture = backend_value_capture.map(|backend| CallValueCaptureContext { backend });
        #[cfg(target_arch = "wasm32")]
        let call_capture: Option<CallValueCaptureContext> = None;
        thread.vm.set_call_input_capture_hook(
            call_capture
                .as_ref()
                .map(CallValueCaptureContext::input_capture_hook),
        );
        if let (Some(capture), Some(entries)) = (root_capture.as_ref(), root_input_values.as_ref())
        {
            #[cfg(not(target_arch = "wasm32"))]
            capture
                .backend
                .capture_with(entry_call_ref, ValueRole::Input, false, |reservation| {
                    TraceHeap::copy_named_values_bounded(
                        &self.heap,
                        thread.proof(),
                        entries,
                        reservation,
                    )
                });
        }

        // Run the event loop.
        let result = self
            .run_thread_event_loop(
                return_type,
                throws_type,
                thread,
                host_call_id,
                root_capture.clone(),
                call_capture,
                log_capture,
                &cancel,
                copy_objects,
            )
            .await;

        // Flush any host-value releases queued during this call. The root
        // thread's `ActiveHeapPermit` was consumed by `run_thread_event_loop`
        // (taken by value) and has been dropped by the time it returns, so we
        // hold no heap permit here and the host release callbacks run safely
        // off the parked-permit window. This makes release prompt even when a
        // GC never ran during the call.
        bex_external_types::host_value::host_release_dispatch::drain();

        // active_calls cleanup is done by ActiveCallGuard on drop.
        //
        // Keep genuine engine errors intact. Cancellation is surfaced as a
        // `baml.panics.Cancelled` panic — either raised by the VM's `Await`
        // opcode, or synthesized by engine safepoints (see
        // `cancelled_unhandled_throw`).
        let boundary_status = match &result {
            Ok(ThreadOutcome::RootValue(_)) => BoundaryEndStatus::Succeeded,
            Ok(ThreadOutcome::SettledChild(_)) => BoundaryEndStatus::Failed,
            Err(error) if is_cancelled_engine_error(error) => BoundaryEndStatus::Cancelled,
            Err(_) => BoundaryEndStatus::Failed,
        };
        #[cfg(not(target_arch = "wasm32"))]
        if let RootAdmission::Active(active) = admission {
            active.completion.complete(boundary_status);
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (admission, boundary_status);
        match result {
            Ok(ThreadOutcome::RootValue(value)) => Ok(BexCallResult {
                value: Ok(value),
                entry_call_ref,
            }),
            Ok(ThreadOutcome::SettledChild(_)) => Ok(BexCallResult {
                // Root threads should never produce SettledChild; treat as an
                // engine invariant violation rather than silently returning Null.
                value: Err(EngineError::Other(
                    "BEP-034: root thread terminated as SettledChild".to_string(),
                )),
                entry_call_ref,
            }),
            Err(err) => Ok(BexCallResult {
                value: Err(err),
                entry_call_ref,
            }),
        }
    }

    /// Invoke a callable value — a raw function, a BAML closure (with captures),
    /// a bound method, or a host closure — referenced by a host
    /// [`Handle`](bex_external_types::Handle), as a fresh root call,
    /// returning its result. This is the by-value counterpart of
    /// [`Self::call_function`], used to call a BAML callback passed to a sys-op
    /// (e.g. an HTTP server `handler`). The callee's `return`/`throws` types and
    /// type args are read from the heap object rather than a name lookup; a bound
    /// method's receiver is injected as `self` and its instance's class type args
    /// are seeded — mirroring the VM's normal call path
    /// (`execute_call_from_locals_offset`).
    pub async fn call_callable(
        self: &Arc<Self>,
        handle: bex_external_types::Handle,
        args: Vec<BexExternalValue>,
        call_ctx: FunctionCallContext,
        copy_objects: bool,
    ) -> Result<BexExternalValue, EngineError> {
        self.call_callable_with_trace_impl(
            handle,
            CallableArgs::Positional(args),
            call_ctx,
            copy_objects,
        )
        .await
        .and_then(|result| result.value)
    }

    pub async fn call_callable_with_trace(
        self: &Arc<Self>,
        handle: bex_external_types::Handle,
        args: Vec<BexExternalValue>,
        call_ctx: FunctionCallContext,
        copy_objects: bool,
    ) -> Result<BexCallResult, EngineError> {
        self.call_callable_with_trace_impl(
            handle,
            CallableArgs::Positional(args),
            call_ctx,
            copy_objects,
        )
        .await
    }

    /// Invoke a host-returned callable using ordered required arguments and
    /// named supplied optionals, matching the cross-SDK callable convention.
    pub async fn call_callable_named(
        self: &Arc<Self>,
        handle: bex_external_types::Handle,
        required: indexmap::IndexMap<String, BexExternalValue>,
        optional: indexmap::IndexMap<String, BexExternalValue>,
        call_ctx: FunctionCallContext,
        copy_objects: bool,
    ) -> Result<BexExternalValue, EngineError> {
        self.call_callable_with_trace_impl(
            handle,
            CallableArgs::Named { required, optional },
            call_ctx,
            copy_objects,
        )
        .await
        .and_then(|result| result.value)
    }

    async fn call_callable_with_trace_impl(
        self: &Arc<Self>,
        handle: bex_external_types::Handle,
        args: CallableArgs,
        FunctionCallContext {
            host_call_id,
            boundary,
            logger,
            cancel,
            profile_intent,
            type_args: _,
            type_defs: _,
        }: FunctionCallContext,
        copy_objects: bool,
    ) -> Result<BexCallResult, EngineError> {
        let (_call_guard, cancel) =
            ActiveCallGuard::register(Arc::clone(self), host_call_id, cancel)?;
        if cancel.is_cancelled() {
            return Err(cancelled_unhandled_throw());
        }
        let mut thread = self.new_root_thread(cancel.clone()).await;

        // Resolve the handle to the live heap object. The handle keeps it rooted.
        let entry_ptr = self
            .resolve_handle(thread.proof(), &handle)
            .ok_or_else(|| {
                EngineError::Other("call_callable: stale callable handle".to_string())
            })?;

        // Unwrap to the entry pointer to run, the receiver to inject as `self`
        // (bound methods only), the type args to seed the frame, and the inner
        // `Function` pointer to read metadata from. `entry` is the closure value
        // itself (so its captures/upvalues resolve) or, for a bound method, the
        // inner function (its `self` is supplied positionally).
        let (entry, receiver, seed_type_args, func_ptr, host_signature) =
            match thread.vm.get_object(entry_ptr) {
                Object::Function(_) => (entry_ptr, None, Vec::new(), Some(entry_ptr), None),
                // A plain (or `foo<int>`-instantiated) function reference: the
                // pooled wrapper over the function's global slot (see emit's
                // `emit_pooled_function_value`). Resolve to the underlying
                // `Function` object; its `type_args` seed the frame.
                Object::GenericFunction(gf) => {
                    let type_args = gf.type_args.to_vec();
                    let inner = thread.vm.globals.get(thread.proof(), gf.function);
                    let func_ptr = inner.as_object_ptr().ok_or_else(|| {
                        EngineError::Other(
                            "call_callable: function wrapper resolves to no object".to_string(),
                        )
                    })?;
                    (func_ptr, None, type_args, Some(func_ptr), None)
                }
                Object::Closure(closure) => (
                    entry_ptr,
                    None,
                    closure.captured_type_args.to_vec(),
                    Some(closure.function),
                    None,
                ),
                Object::BoundMethod(bm) => {
                    let receiver = bm.receiver;
                    let class_type_args = receiver
                        .as_object_ptr()
                        .and_then(|ptr| match thread.vm.get_object(ptr) {
                            Object::Instance(inst) => Some(inst.class_type_args.to_vec()),
                            _ => None,
                        })
                        .unwrap_or_default();
                    (
                        bm.function,
                        Some(receiver),
                        class_type_args,
                        Some(bm.function),
                        None,
                    )
                }
                Object::HostClosure(host) => {
                    let throws_type = match &*host.throws_ty {
                        baml_type::RealizedTy::Never { .. }
                        | baml_type::RealizedTy::Void { .. } => None,
                        ty => Some(RuntimeTy::from(ty.clone())),
                    };
                    let param_types: Vec<RuntimeTy> = host
                        .params
                        .iter()
                        .map(|param| RuntimeTy::from(param.ty.clone()))
                        .collect::<Vec<_>>();
                    let param_names = host
                        .params
                        .iter()
                        .enumerate()
                        .map(|(index, param)| {
                            param
                                .name
                                .as_ref()
                                .map_or_else(|| format!("arg{index}"), ToString::to_string)
                        })
                        .collect();
                    let param_has_default = host
                        .params
                        .iter()
                        .map(baml_type::RealizedFunctionParamTy::is_optional)
                        .collect();
                    (
                        entry_ptr,
                        None,
                        Vec::new(),
                        None,
                        Some((
                            RuntimeTy::from((*host.ret_ty).clone()),
                            throws_type,
                            host.arity,
                            param_types,
                            param_names,
                            param_has_default,
                            Vec::new(),
                        )),
                    )
                }
                other => {
                    return Err(EngineError::TypeMismatch {
                        message: format!("call_callable: handle is not callable ({other:?})"),
                    });
                }
            };

        // Read the inner function's metadata. `param_types` carries declared
        // parameter types (including `self` for methods) for type-directed
        // coercion; lambdas leave it empty (types inferred, not stored), so the
        // real arity comes from `arity` and coercion is best-effort.
        let (
            mut return_type,
            throws_type,
            arity,
            param_types,
            param_names,
            param_has_default,
            generic_param_names,
        ) = if let Some(signature) = host_signature {
            signature
        } else {
            match thread
                .vm
                .get_object(func_ptr.expect("non-host callable must resolve to a function"))
            {
                Object::Function(func) => {
                    // A value referencing an unresolved native builtin can't be an
                    // entry point (parity with `call_function_bound_args`).
                    if matches!(func.kind, bex_vm_types::FunctionKind::NativeUnresolved) {
                        return Err(EngineError::NotInvokableAsEntry {
                            name: func.name.clone(),
                            kind: format!("{:?}", func.kind),
                        });
                    }
                    // De Bruijn-ordered generic-param names (enclosing class
                    // params first, then the function's own), bounds stripped to
                    // the bare TypeVar — used to lower the positional `seed_type_args`
                    // onto the named `type_args` channel below.
                    let generic_param_names: Vec<String> = func
                        .display_type_params
                        .iter()
                        .map(|p| p.split_whitespace().next().unwrap_or(p).to_string())
                        .collect();
                    // The stored signature is templated over this callee's frame
                    // slots, in the same De Bruijn order as both `seed_type_args`
                    // and `generic_param_names`. Fill each slot with the seeded
                    // type where the caller supplied one, and otherwise with that
                    // slot's own type variable: an unseeded slot must stay *named*
                    // and symbolic, because the host boundary infers it from the
                    // incoming wire values by matching them against these declared
                    // types (see `collect_type_var_bindings`). Collapsing it to
                    // `unknown` would erase what that inference reads.
                    let slot_types: Vec<RuntimeTy> = (0..generic_param_names.len())
                        .map(|i| {
                            seed_type_args.get(i).map_or_else(
                                || {
                                    RuntimeTy::TypeVar(
                                        baml_type::ParamTy::new(
                                            u32::try_from(i).unwrap_or(u32::MAX),
                                            baml_type::Name::new(generic_param_names[i].as_str()),
                                        ),
                                        baml_type::TyAttr::default(),
                                    )
                                },
                                |t| t.as_runtime_ty().clone(),
                            )
                        })
                        .collect();
                    (
                        func.return_type.substitute_symbolic(&slot_types),
                        match &func.throws_type {
                            baml_type::TyTemplate::Never { .. } => None,
                            t => Some(t.substitute_symbolic(&slot_types)),
                        },
                        func.arity,
                        func.param_types
                            .iter()
                            .map(|t| t.substitute_symbolic(&slot_types))
                            .collect(),
                        func.param_names.clone(),
                        func.param_has_default.clone(),
                        generic_param_names,
                    )
                }
                _ => {
                    return Err(EngineError::TypeMismatch {
                        message: "call_callable: value does not wrap a function".to_string(),
                    });
                }
            }
        };

        // For a bound method on a generic class, substitute the declared return
        // type's class type vars from the receiver's concrete type args (seeded
        // above from the instance). Mirrors the named-entry path in
        // `call_function_bound_args`; without it a generic method's `TStream`-like
        // return arm stays an unsubstituted type var and host-return conversion
        // panics on a concrete value. See bridge-generics/streaming/04.
        if receiver.is_some() {
            if let Some(RuntimeTy::Class(_, declared_args, _)) = param_types.first() {
                let mut bindings = indexmap::IndexMap::new();
                for (declared, concrete) in declared_args.iter().zip(seed_type_args.iter()) {
                    crate::conversion::collect_type_var_bindings(
                        declared,
                        concrete.as_runtime_ty(),
                        &mut bindings,
                    );
                }
                return_type = crate::conversion::substitute_type_vars(&return_type, &bindings);
            }
        }

        // A bound method's `arity` counts the implicit `self`; callers don't pass
        // it (the receiver is injected below), so the visible arity drops by one.
        let self_offset = usize::from(receiver.is_some());
        let user_arity = arity.saturating_sub(self_offset);
        let args = match args {
            CallableArgs::Positional(args) => {
                if args.len() != user_arity {
                    return Err(EngineError::TypeMismatch {
                        message: format!(
                            "callable expects {user_arity} argument(s), got {}",
                            args.len()
                        ),
                    });
                }
                args.into_iter()
                    .map(|arg| BexCallArg::Provided(Box::new(arg)))
                    .collect()
            }
            CallableArgs::Named {
                required,
                mut optional,
            } => {
                let required_arity = (self_offset..arity)
                    .filter(|idx| !param_has_default.get(*idx).copied().unwrap_or(false))
                    .count();
                if required.len() != required_arity {
                    return Err(EngineError::TypeMismatch {
                        message: format!(
                            "callable expects {required_arity} required argument(s), got {}",
                            required.len()
                        ),
                    });
                }
                let mut required = required.into_values();
                let mut ordered = Vec::with_capacity(user_arity);
                for idx in self_offset..arity {
                    if !param_has_default.get(idx).copied().unwrap_or(false) {
                        ordered.push(BexCallArg::Provided(Box::new(
                            required
                                .next()
                                .expect("required callable arity was validated"),
                        )));
                        continue;
                    }
                    let name = param_names
                        .get(idx)
                        .ok_or_else(|| EngineError::TypeMismatch {
                            message: format!("callable parameter {idx} has no name"),
                        })?;
                    if let Some(value) = optional.shift_remove(name) {
                        ordered.push(BexCallArg::Provided(Box::new(value)));
                    } else {
                        ordered.push(BexCallArg::OmittedDefault);
                    }
                }
                if !optional.is_empty() {
                    let mut extra = optional.keys().cloned().collect::<Vec<_>>();
                    extra.sort();
                    return Err(EngineError::TypeMismatch {
                        message: format!(
                            "callable got unexpected argument(s): {}",
                            extra.join(", ")
                        ),
                    });
                }
                ordered
            }
        };

        // Coerce each provided arg to its declared param type (offset by `self`
        // for bound methods).
        let coerced: Vec<BexCallArg> = args
            .into_iter()
            .enumerate()
            .map(|(idx, arg)| match param_types.get(idx + self_offset) {
                Some(ty) => match arg {
                    BexCallArg::Provided(value) => self
                        .coerce_inbound_arg(*value, ty)
                        .map(|value| BexCallArg::Provided(Box::new(value))),
                    BexCallArg::OmittedDefault => Ok(BexCallArg::OmittedDefault),
                },
                None => Ok(arg),
            })
            .collect::<Result<_, _>>()?;

        // Build VM args: the receiver as `self` in slot 0 (bound methods), then
        // the converted user args.
        let mut vm_args: Vec<Value> = Vec::with_capacity(coerced.len() + self_offset);
        if let Some(receiver) = receiver {
            vm_args.push(receiver);
        }
        for (idx, arg) in coerced.into_iter().enumerate() {
            vm_args.push(match arg {
                BexCallArg::Provided(arg) => self.convert_external_to_vm_value_with_ty(
                    &mut thread,
                    *arg,
                    param_types.get(idx + self_offset),
                )?,
                BexCallArg::OmittedDefault => Value::OMITTED_ARG,
            });
        }

        // The legacy span label stays "<callable>" (host-facing name for a
        // by-value invocation), but the host-event identity is the real
        // callee: `func_ptr` is the unwrapped `Object::Function`, so it
        // resolves to the actual function's metadata row. The structural
        // `CallFunction` id is stamped independently by the VM from
        // `Function.function_id`.
        // Lower the closure's captured / bound-method's class type args (held
        // positionally in De Bruijn order) onto the named `type_args` channel by
        // pairing each with the callee's generic-param name. A lambda has no
        // declared param names, so fall back to the index as a key; the named
        // lowering then emits the unnamed bindings in order.
        // The seed args are realized (a value's captured/class type args); widen
        // them into the `RuntimeTy` the host `type_args` channel carries. They are
        // re-narrowed to `RealizedTy` at the `set_entry_point_with_type_args`
        // boundary inside `run_entry_point`.
        let seed_type_args: indexmap::IndexMap<String, RuntimeTy> = seed_type_args
            .into_iter()
            .enumerate()
            .map(|(i, ty)| {
                (
                    generic_param_names
                        .get(i)
                        .cloned()
                        .unwrap_or_else(|| i.to_string()),
                    RuntimeTy::from(ty),
                )
            })
            .collect();
        self.run_entry_point(
            thread,
            entry,
            vm_args,
            seed_type_args,
            IndexMap::new(),
            return_type,
            throws_type,
            host_call_id,
            boundary,
            profile_intent,
            logger,
            None,
            cancel,
            copy_objects,
        )
        .await
    }

    /// Cancel a function call by its ID.
    ///
    /// If the call is still running, it will be interrupted at the next
    /// cancellation check point. If the ID is not active yet, reserve it with
    /// an already-cancelled token so the later call starts cancelled.
    pub fn cancel_function_call(&self, call_id: CallId) -> Result<(), EngineError> {
        ActiveCallGuard::reserve_cancelled(self, call_id);
        Ok(())
    }

    fn validate_bound_args(
        &self,
        function_name: &str,
        args: &[BexCallArg],
    ) -> Result<(), EngineError> {
        let params = self.function_params(function_name)?;
        if args.len() != params.len() {
            return Err(EngineError::TypeMismatch {
                message: format!(
                    "Function `{function_name}` expects {} argument(s), got {}",
                    params.len(),
                    args.len()
                ),
            });
        }

        for (idx, arg) in args.iter().enumerate() {
            if matches!(arg, BexCallArg::OmittedDefault) && !params[idx].2 {
                return Err(EngineError::TypeMismatch {
                    message: format!(
                        "Argument `{}` for function `{function_name}` cannot be omitted; only parameters with defaults may use omitted defaults",
                        params[idx].0
                    ),
                });
            }
        }

        Ok(())
    }

    /// Look up a function by name and return its heap pointer + kind.
    /// Resolution follows the canonical [`sys_types::resolve_name`] rule
    /// (exact → `user.{name}` → unambiguous suffix match), shared with
    /// `baml run`, `baml pack`, and the sysop LLM/`$new` resolvers.
    ///
    /// Returning the kind lets call-site gates (e.g.
    /// `call_function_bound_args` rejecting non-Bytecode entries) avoid a
    /// second heap dereference.
    fn lookup_function(
        &self,
        function_name: &str,
    ) -> Result<(HeapPtr, bex_vm_types::FunctionKind), EngineError> {
        sys_types::resolve_name(&self.resolved_function_names, function_name)
            .found()
            .map(|(_k, (ptr, kind))| (*ptr, *kind))
            .ok_or_else(|| EngineError::FunctionNotFound {
                name: function_name.to_string(),
            })
    }

    /// Resolve a function name to the key actually present in
    /// `resolved_function_names`. Uses [`sys_types::resolve_name`] so the
    /// rule matches `lookup_function` exactly.
    fn resolve_function_name<'a>(&'a self, name: &str) -> Option<&'a str> {
        sys_types::resolve_name(&self.resolved_function_names, name)
            .found()
            .map(|(k, _)| k)
    }

    /// Get the return type for a function by dereferencing its heap object.
    pub fn function_return_type(&self, name: &str) -> Option<RuntimeTy> {
        let resolved = self.resolve_function_name(name)?;
        let (ptr, _kind) = self.resolved_function_names.get(resolved)?;
        // SAFETY: ptr is from resolved_function_names, a compile-time object
        let obj = unsafe { ptr.get() };
        match obj {
            Object::Function(func) => Some(declared_symbolic(&func.return_type, func)),
            _ => None,
        }
    }

    /// Get the inferred throws type for a function by dereferencing its heap object.
    fn function_throws_type(&self, name: &str) -> Option<RuntimeTy> {
        let resolved = self.resolve_function_name(name)?;
        let (ptr, _kind) = self.resolved_function_names.get(resolved)?;
        // SAFETY: ptr is from resolved_function_names, a compile-time object
        let obj = unsafe { ptr.get() };
        match obj {
            Object::Function(func) => match &func.throws_type {
                baml_type::TyTemplate::Never { .. } => None,
                t => Some(declared_symbolic(t, func)),
            },
            _ => None,
        }
    }

    /// Get parameter names and types for a function by dereferencing its heap object.
    pub fn function_params(&self, name: &str) -> Result<Vec<(&str, RuntimeTy, bool)>, EngineError> {
        let resolved = self
            .resolve_function_name(name)
            .ok_or(EngineError::FunctionNotFound {
                name: name.to_string(),
            })?;
        let (ptr, _kind) =
            self.resolved_function_names
                .get(resolved)
                .ok_or(EngineError::FunctionNotFound {
                    name: name.to_string(),
                })?;
        // SAFETY: ptr is from resolved_function_names, a compile-time object
        let obj = unsafe { ptr.get() };
        match obj {
            Object::Function(func) => Ok(func
                .param_names
                .iter()
                .zip(func.param_types.iter())
                .enumerate()
                .map(|(idx, (name, ty))| {
                    (
                        name.as_str(),
                        declared_symbolic(ty, func),
                        func.param_has_default.get(idx).copied().unwrap_or(false),
                    )
                })
                .collect()),
            other => Err(EngineError::TypeMismatch {
                message: format!("Expected Function, got {other:?}"),
            }),
        }
    }

    /// The callee's declared generic-param names (bare, bounds stripped), in
    /// declaration order. Empty for a non-generic function. Sourced from the
    /// `Function`'s `display_type_params`, so it includes type params that
    /// appear only in the body (e.g. `one_type_arg<T>()` whose `T` shows up
    /// solely via `type.of<T>()`), which a signature-only scan misses.
    fn function_generic_params(&self, name: &str) -> Vec<String> {
        let Some(resolved) = self.resolve_function_name(name) else {
            return vec![];
        };
        let Some((ptr, _kind)) = self.resolved_function_names.get(resolved) else {
            return vec![];
        };
        // SAFETY: ptr is from resolved_function_names, a compile-time object.
        match unsafe { ptr.get() } {
            Object::Function(func) => func
                .display_type_params
                .iter()
                // `display_type_params` may render bounds ("T extends Foo"); the
                // bare TypeVar name is the leading whitespace-free token.
                .map(|p| p.split_whitespace().next().unwrap_or(p).to_string())
                .collect(),
            _ => vec![],
        }
    }

    /// Whether `function_name` resolves to a method on a class (its FQN's parent
    /// segment names a registered class), as opposed to a free function. Used to
    /// scope Gate A's declared-param completeness check (1): a method's
    /// `display_type_params` are De Bruijn-ordered as *class params first, then
    /// the method's own*, and those leading class params are never bound *by
    /// name* in `type_args` — a static never binds them, and an instance
    /// method's ride on the receiver (the wire instance's class args, or a
    /// generic `self` handle), not the named channel. Demanding each appear in
    /// `type_args` would wrongly reject, so check (1) is skipped for class
    /// methods; their own params still fall to check (2), which scans the
    /// signature (and catches a class param the receiver failed to supply).
    /// Free functions have no such prefix.
    fn is_class_method(&self, function_name: &str) -> bool {
        let Some(resolved) = self.resolve_function_name(function_name) else {
            return false;
        };
        let Some(idx) = resolved.rfind('.') else {
            return false;
        };
        let parent = &resolved[..idx];
        self.resolved_class_names.contains_key(parent)
            || self
                .resolved_class_names
                .contains_key(&format!("user.{parent}"))
            || parent
                .strip_prefix("user.")
                .is_some_and(|p| self.resolved_class_names.contains_key(p))
    }

    /// Number of generic params declared by the class that *encloses*
    /// `function_name` — the De Bruijn class-prefix length on a method's
    /// `display_type_params` (`GenericBox<T>.new` ⇒ 1, `Helper.reflect_t` ⇒ 0).
    /// `0` for free functions or when the parent isn't a registered class. Gate A
    /// uses it to split a method's *own* generic params (which it must demand —
    /// rule 3) from inherited class params (which ride on the receiver / are
    /// phantom on a static, so are never bound by name).
    fn enclosing_class_generic_param_count(&self, function_name: &str) -> usize {
        let Some(resolved) = self.resolve_function_name(function_name) else {
            return 0;
        };
        let Some(idx) = resolved.rfind('.') else {
            return 0;
        };
        let parent = &resolved[..idx];
        let class_ptr = self
            .resolved_class_names
            .get(parent)
            .or_else(|| self.resolved_class_names.get(&format!("user.{parent}")))
            .or_else(|| {
                parent
                    .strip_prefix("user.")
                    .and_then(|p| self.resolved_class_names.get(p))
            });
        let Some(ptr) = class_ptr else {
            return 0;
        };
        // SAFETY: ptr is from resolved_class_names, a compile-time object.
        match unsafe { ptr.get() } {
            Object::Class(class) => class.generic_param_count,
            _ => 0,
        }
    }

    /// Check if a function exists by name (tries exact then "user." prefix).
    pub fn function_exists(&self, name: &str) -> bool {
        self.resolve_function_name(name).is_some()
    }

    /// Replace the process argv exposed to BAML via `baml.sys.argv()`.
    ///
    /// Allowed only before the engine is wrapped in an `Arc` and shared,
    /// because we mutate `self` directly. Used by `baml run` to derive
    /// `argv[1]` from the compiled program (BEP-027 §"`baml.argv`": the
    /// path of the file containing root `main`).
    pub fn set_argv(&mut self, argv: Vec<String>) {
        self.argv = Arc::from(argv);
    }

    /// View of the current process argv. Used by hosts that need to
    /// patch `argv[1]` based on resolution context (e.g. swapping a
    /// script-alias name for the resolved function it expanded to).
    pub fn argv(&self) -> &[String] {
        &self.argv
    }

    /// Find a user-callable function by qualified name (`user.main`),
    /// display name (`main`), or unambiguous namespace-leaf suffix
    /// (`lorem.Func` resolving to `user.ns_lorem.Func`). Uses the
    /// shared [`sys_types::resolve_name`] rule and post-filters to
    /// user-callable bytecode functions.
    pub fn find_user_function(&self, name: &str) -> Option<UserFunctionInfo> {
        let (qualified, _) =
            sys_types::resolve_name(&self.resolved_function_names, name).found()?;
        let qualified = qualified.to_string();
        self.user_functions()
            .into_iter()
            .find(|f| f.qualified_name == qualified)
    }

    /// List all user-callable functions with signature info.
    pub fn user_functions(&self) -> Vec<UserFunctionInfo> {
        self.resolved_function_names
            .iter()
            .filter_map(|(name, (ptr, kind))| {
                if !matches!(kind, bex_vm_types::FunctionKind::Bytecode) {
                    return None;
                }
                let obj = unsafe { ptr.get() };
                match obj {
                    Object::Function(func) if func.origin.is_user_callable() => {
                        let display_name = name.strip_prefix("user.").unwrap_or(name).to_string();
                        let is_llm =
                            matches!(func.body_meta, Some(bex_vm_types::FunctionMeta::Llm { .. }));
                        let display_param_types =
                            if func.display_param_types.len() == func.param_names.len() {
                                func.display_param_types.clone()
                            } else {
                                func.param_types.iter().map(ToString::to_string).collect()
                            };
                        let display_return_type = if func.display_return_type.is_empty() {
                            func.return_type.to_string()
                        } else {
                            func.display_return_type.clone()
                        };
                        Some(UserFunctionInfo {
                            qualified_name: name.clone(),
                            display_name,
                            origin: func.origin,
                            param_names: func.param_names.clone(),
                            param_types: func
                                .param_types
                                .iter()
                                .map(|t| declared_symbolic(t, func))
                                .collect(),
                            param_has_default: func.param_has_default.clone(),
                            return_type: declared_symbolic(&func.return_type, func),
                            display_type_params: func.display_type_params.clone(),
                            display_param_types,
                            display_return_type,
                            source_file: func.source_file.clone(),
                            is_llm,
                        })
                    }
                    _ => None,
                }
            })
            .collect()
    }

    /// Find a test case by name.
    pub fn test_case(
        &self,
        function_name: &str,
        test_name: &str,
    ) -> Option<&bex_vm_types::TestCase> {
        self.test_cases
            .iter()
            .find(|t| t.function_names.iter().any(|n| function_name == n) && t.name == test_name)
    }

    // ========================================================================
    // Test Collection API
    // ========================================================================

    /// Collect all tests for a package by invoking `{package}.$init_test(collector)`.
    ///
    /// Returns a `BexExternalValue::Handle` pointing to a live `testing.TestRegistry`
    /// heap object (GC-rooted), or `BexExternalValue::Null` if the package has no tests.
    ///
    /// If the package has no test blocks, `$init_test` will not exist in the program
    /// and `Null` is returned immediately.
    ///
    /// # Arguments
    ///
    /// - `package`: The package name (e.g. `"user"`).
    /// - `cancel`: A [`CancellationToken`] for caller-controlled cancellation.
    pub async fn collect_tests(
        self: &Arc<Self>,
        package: &str,
        call_id: CallId,
        cancel: CancellationToken,
    ) -> Result<BexExternalValue, EngineError> {
        let init_test_name = if package == "user" {
            "$init_test".to_string()
        } else {
            format!("{package}.$init_test")
        };

        // If no $init_test function exists, this package has no tests.
        if self.lookup_function(&init_test_name).is_err() {
            return Ok(BexExternalValue::Null);
        }

        let ctx = || {
            FunctionCallContextBuilder::new(call_id)
                .with_cancel_token(cancel.clone())
                .suppress_internal_profile()
                .build()
        };

        // Step 1: Create a live TestCollector on the heap
        let collector = self
            .call_function(
                "testing.TestCollector.new",
                vec![BexExternalValue::String("".into())],
                ctx(),
                false, // return Handle, not deep copy
            )
            .await?;

        // Step 2: Populate the collector in-place via $init_test
        self.call_function(
            &init_test_name,
            vec![collector.clone()],
            ctx(),
            true, // return value is null, doesn't matter
        )
        .await?;

        // Step 3: Wrap in a TestRegistry
        let registry = self
            .call_function(
                "testing.TestRegistry.new",
                vec![collector],
                ctx(),
                false, // return Handle to live registry
            )
            .await?;

        Ok(registry)
    }

    /// Run GC if conditions are met (called at safepoints),
    /// or yield if another thread is running GC.
    ///
    /// Uses the adaptive `should_collect()` policy to choose the appropriate
    /// collection level (Minor or Major) based on live object counts and
    /// allocation pressure.
    async fn gc_safepoint<T: RootHaver>(
        self: &Arc<Self>,
        mut permit: ActiveHeapPermit<T>,
    ) -> ActiveHeapPermit<T> {
        let i_am_checking = self
            .checking_gc
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok();
        if i_am_checking {
            // We won the CAS, so we own the GC check.
            if let Some(level) = self.heap.should_collect() {
                let inactive = permit.release();
                self.collect_garbage(level).await;
                permit = inactive.acquire().await;
            }
            self.checking_gc.store(false, Ordering::Release);
            permit
        } else {
            // Another thread is checking; park if they've requested it.
            permit.renew().await
        }
    }

    /// Heuristic-driven GC check that does **not** require the caller to
    /// hold an active heap permit. Use this when the caller is already in a
    /// permit-released state (e.g., the engine's `Await` branch waiting on
    /// a `SetOnce`) — calling [`Self::gc_safepoint`] there would do an
    /// extra release-and-reacquire pair around the heuristic for nothing.
    async fn maybe_collect_garbage(self: &Arc<Self>) {
        let i_am_checking = self
            .checking_gc
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok();
        if i_am_checking {
            if let Some(level) = self.heap.should_collect() {
                self.collect_garbage(level).await;
            }
            self.checking_gc.store(false, Ordering::Release);
        }
        // If we are not the checker, the actual checker (some other VM) is
        // either already waiting in `request_park` or about to. Our caller
        // is permit-released, so they're not blocking that wait.
    }

    /// BEP-042: run the `cleanup` finalizer for each instance the GC kept alive
    /// during the collection that just completed.
    ///
    /// Called immediately after a collection (the GC copied each pending instance
    /// into the survivor space, so it is alive). The queued instances are rooted
    /// into handles before the first `await` (see the body), so they stay valid
    /// across any nested or concurrent collection — `collect_garbage` is `pub` and
    /// can be called directly, where the `checking_gc` guard is NOT held, so we do
    /// not rely on it. Each `cleanup` runs as an ordinary function call on the
    /// real instance (passed by handle, never copied), so it sets the instance's
    /// run-once latch; a `cleanup` racing on another fiber via an explicit call
    /// or `defer` is deduped by that latch.
    ///
    /// A throwing `cleanup` is logged and swallowed — there is no caller to
    /// propagate to on the GC path, and a bad finalizer must not break the GC
    /// (BEP-042; matches Python's `__del__`).
    async fn drain_finalizers(self: &Arc<Self>) {
        // Root the entire queue into handles BEFORE the first `await`. The queued
        // pointers are raw `HeapPtr`s; if a `cleanup` body yields and a nested or
        // concurrent collection runs (possible when `collect_garbage` is called
        // directly, where `checking_gc` is not held), the not-yet-processed
        // pointers would be moved. Handles are GC roots and are fixed up across a
        // collection, so converting up front keeps every pending instance valid.
        let pending: Vec<_> = self
            .heap
            .take_pending_finalizers()
            .into_iter()
            .map(|(ptr, cleanup_fn)| (self.heap.create_handle(ptr), cleanup_fn))
            .collect();
        for (handle, cleanup_fn) in pending {
            let ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
                .suppress_internal_profile()
                .build();
            // `Box::pin` breaks the async-recursion cycle: a `cleanup` body can
            // allocate and trigger another collection, which re-enters this
            // drain — boxing erases the otherwise-infinite future size.
            let call = Box::pin(self.call_function(
                &cleanup_fn,
                vec![BexExternalValue::Handle(handle)],
                ctx,
                /* copy_objects = */ false,
            ));
            if let Err(e) = call.await {
                tracing::warn!("BEP-042 cleanup finalizer `{cleanup_fn}` failed: {e}");
            }
        }
    }

    /// Transition the child future settled by `thread` to `Cancelled` and
    /// fire its cancel token so descendants cascade-cancel.
    async fn settle_child_cancelled(
        &self,
        thread: &mut ActiveHeapPermit<BexThread>,
        future_id: FutureId,
    ) -> Result<(), EngineError> {
        let child_cancel = thread.vm_thread_cancel().clone();
        let mut guard = self.futures.acquire(thread.proof()).await;
        guard.cancel_future(future_id)?;
        drop(guard);
        child_cancel.cancel();
        Ok(())
    }

    /// Transition the child future settled by `thread` to `Error(value)` and
    /// fire its cancel token. GC reports the error if the future becomes
    /// unreachable before an await observes it.
    async fn settle_child_errored(
        &self,
        thread: &mut ActiveHeapPermit<BexThread>,
        future_id: FutureId,
        value: Value,
        trace: Vec<bex_vm::StackFrame>,
    ) -> Result<(), EngineError> {
        let child_cancel = thread.vm_thread_cancel().clone();
        let mut guard = self.futures.acquire(thread.proof()).await;
        guard.err_future(future_id, value, trace)?;
        drop(guard);
        child_cancel.cancel();
        Ok(())
    }

    fn capture_root_value(
        &self,
        thread: &ActiveHeapPermit<BexThread>,
        capture: &RootValueCaptureContext,
        value: Value,
    ) {
        #[cfg(not(target_arch = "wasm32"))]
        capture
            .backend
            .capture_with(capture.call_ref, ValueRole::Output, false, |reservation| {
                TraceHeap::copy_value_bounded(&self.heap, thread.proof(), value, reservation)
            });
    }

    fn drain_vm_call_captures(
        &self,
        thread: &mut ActiveHeapPermit<BexThread>,
        capture: Option<&CallValueCaptureContext>,
    ) {
        let events = thread.vm.drain_call_capture_events();
        if let Some(capture) = capture {
            for event in events {
                let call_ref = CallRef {
                    process_euid: self.process_euid,
                    engine_id: self.engine_id,
                    thread_id: BexThreadId(event.thread_id),
                    call_id: BexCallId(event.call_id),
                };
                #[cfg(not(target_arch = "wasm32"))]
                if event.kind == VmCallCaptureKind::Output {
                    capture.backend.capture_with(
                        call_ref,
                        ValueRole::Output,
                        event.manual,
                        |reservation| {
                            TraceHeap::copy_value_bounded(
                                &self.heap,
                                thread.proof(),
                                event.value,
                                reservation,
                            )
                        },
                    );
                }
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let error_events = thread.vm.drain_error_capture_events();
            if let Some(backend) = capture.map(|capture| &capture.backend) {
                for event in error_events {
                    backend.capture_error(event, |reservation, value| {
                        TraceHeap::copy_value_bounded(
                            &self.heap,
                            thread.proof(),
                            value,
                            reservation,
                        )
                    });
                }
            } else {
                for event in error_events {
                    bex_events::prof::backend::complete_engine_error_value(
                        self.engine_id,
                        event.id,
                        bex_events::prof::backend::ValueState::Lost(
                            ValueLossReason::StoreUnavailable,
                        ),
                    );
                }
            }
        }
    }

    fn capture_baml_log_event(
        &self,
        thread: &ActiveHeapPermit<BexThread>,
        capture: Option<&LogCaptureContext>,
        data: Value,
        source_location: Option<VmEventSourceLocation>,
    ) {
        let Some(capture) = capture else {
            return;
        };
        let call = bex_events::run::TraceCallKey {
            process_euid: self.process_euid,
            engine_id: self.engine_id,
            thread_id: BexThreadId(thread.vm.prof_thread_id),
            call_id: BexCallId(thread.vm.current_call_id()),
        };
        capture
            .logger
            .capture_with(capture.boundary_id, call, |trace_heap| {
                let (level, body) = Self::extract_baml_log_payload(data);
                let metadata = TraceLogMetadata {
                    level,
                    source: Self::source_location_from_event(source_location),
                    timestamp_ms: epoch_ms(),
                    message_preview: Self::log_message_preview(body),
                };
                let snapshot =
                    trace_heap.copy_value_from_bex_heap(&self.heap, thread.proof(), body);
                (metadata, snapshot)
            });
    }

    fn extract_baml_log_payload(data: Value) -> (Option<String>, Value) {
        let Some(ptr) = data.as_object_ptr() else {
            return (None, data);
        };
        let Object::Map(map) = (unsafe { ptr.get() }) else {
            return (None, data);
        };
        let mut level = None;
        let mut body = None;
        for (key, value) in map.to_index_map() {
            match key.to_string().as_str() {
                "level" => {
                    if let Some(ptr) = value.as_object_ptr()
                        && let Object::String(level_value) = unsafe { ptr.get() }
                    {
                        level = Some(level_value.to_string());
                    }
                }
                "data" => body = Some(value),
                _ => {}
            }
        }
        (level, body.unwrap_or(data))
    }

    fn source_location_from_event(
        source_location: Option<VmEventSourceLocation>,
    ) -> Option<bex_events::run::SourceLocation> {
        let VmEventSourceLocation {
            file_id,
            line,
            column,
            start_offset,
            end_offset,
        } = source_location?;
        Some(bex_events::run::SourceLocation {
            file_path: None,
            file_id: Some(u64::from(file_id)),
            line,
            column,
            end_line: None,
            end_column: None,
            start_offset: Some(start_offset),
            end_offset: Some(end_offset),
        })
    }

    fn log_message_preview(value: Value) -> Option<String> {
        let preview = match value.kind() {
            ValueKind::Null => "null".to_string(),
            ValueKind::Bool(value) => value.to_string(),
            ValueKind::Int(value) => value.to_string(),
            ValueKind::OmittedArg => "<omitted argument>".to_string(),
            ValueKind::Object(ptr) => match unsafe { ptr.get() } {
                Object::String(value) => value.to_string(),
                Object::Bigint(value) => value.to_string(),
                Object::Float(value) => value.to_string(),
                _ => return None,
            },
        };
        Some(truncate_preview(preview, 160))
    }

    /// Route an unhandled VM throw value through the embedder's terminal
    /// paths: settle a spawned child errored (or cancelled), surface a clean
    /// `baml.panics.Exit`, or surface as `EngineError::UnhandledThrow`.
    ///
    /// Shared tail between [`Self::run_thread_event_loop`]'s
    /// `VmError::ThrownUnhandled` arm and the sysop-injected-throw path in
    /// [`Self::inject_sysop_throw`] — both are "the VM tried to unwind
    /// and no handler matched, what now."
    async fn route_unhandled_vm_throw(
        self: &Arc<Self>,
        thread: &mut ActiveHeapPermit<BexThread>,
        _call_id: CallId,
        value: Value,
        trace: Vec<bex_vm::StackFrame>,
        throws_type: Option<&RuntimeTy>,
    ) -> Result<ThreadOutcome, EngineError> {
        if let Some(future_id) = thread.vm_thread_settles_future() {
            let is_cancel_panic =
                thread.vm_thread_cancel().is_cancelled() && self.is_cancelled_panic(value);
            if is_cancel_panic {
                self.settle_child_cancelled(thread, future_id).await?;
                // Trace info for the child is dropped on the floor — engine-local
                // since v1 spans for spawned bodies are TODO.
                let _ = trace;
                return Ok(ThreadOutcome::SettledChild(ChildSettleKind::Cancelled));
            }
            self.settle_child_errored(thread, future_id, value, trace)
                .await?;
            return Ok(ThreadOutcome::SettledChild(ChildSettleKind::Errored));
        }
        // A panic escaping all in-BAML catches to the host is an
        // engine-level failure mode, not a value the function opted into
        // via `throws` — so it must bypass the declared-throws re-typing and
        // route through `vm_value_to_owned` (the same branch used when there
        // is no `throws` clause). Re-typing it against a 2+-member throws
        // union would call `find_matching_member`, which never matches a
        // `baml.panics.*` instance and would surface an internal
        // `TypeMismatch` leak instead of the clean panic. This mirrors the
        // panic bypass in [`enforce_host_throw_contract`].
        let runtime_ty = value_runtime_baml_ty(value, thread.proof());
        let value_is_panic = runtime_ty
            .as_ref()
            .is_some_and(|rt| matches!(rt, RuntimeTy::Class(name, _, _) if name.is_panic_type()));
        let external = match throws_type {
            Some(ty) if !value_is_panic => {
                self.convert_vm_value_to_external_with_type(value, ty, &thread.vm, thread.proof())?
            }
            _ => self.vm_value_to_owned(thread.proof(), value),
        };
        // `baml.panics.Exit { code }` escaping all handlers is the clean-
        // termination path — surface as Exit so the host maps it to a
        // process exit code.
        if let Some(code) = extract_exit_code(&external) {
            return Err(EngineError::Exit { code });
        }
        Err(EngineError::UnhandledThrow {
            value: Box::new(external),
            trace,
        })
    }

    /// Deliver a sysop error at a `SysOp` terminal by **injecting it into the
    /// VM's exception unwinder** — the same path a `throw` opcode takes. An
    /// in-BAML `try { f(x) } catch (e: …) { … }` therefore catches sysop
    /// throws like any other throw.
    ///
    /// Returns `Ok(None)` when the VM caught the throw and execution should
    /// continue at the catch body; `Ok(Some(outcome))` when the throw escaped
    /// all handlers and the thread terminated (settled child, root unhandled,
    /// or process-exit path); `Err` only for engine-internal failures during
    /// value materialization or unwinding (every catchable `VmRustFnError`
    /// variant produces a throw value via [`op_error_to_throw_value`]).
    ///
    /// Materialize the sysop error into a thrown VM `Value` regardless of
    /// payload shape, then — for host-callable throws — enforce the
    /// declared-throws contract on the materialized Value.
    ///
    /// **Why materialize-then-validate** (rather than validate the
    /// pre-materialization shape): a host-callable throw can arrive
    /// through two paths that produce structurally identical materialized
    /// Values but different `OpErrorPayload` variants:
    ///
    /// - `HostThrown(BexExternalValue::Instance{class_name="baml.errors.
    ///   HostCallable", ...})` — the wire-routed path that all production
    ///   bridges (cffi, wasm, python, node) emit.
    /// - `Vm(VmRustFnError::BamlError(VmBamlError::HostCallable{...}))`
    ///   — the engine-internal path that synthesizes a host-callable
    ///   error from inside Rust without going through the wire.
    ///
    /// Both materialize into an `Object::Instance` of
    /// `baml.errors.HostCallable`, so applying the contract check
    /// post-materialization handles both paths with a single code path.
    ///
    /// `host_callable_throws_contract` is the host callable's declared
    /// `E` (the second generic of `call_host_value<T, E>`), specifically
    /// for the throws-contract check. Distinct from `throws_type` (the
    /// *outer* BAML function's declared throws) — that one drives the
    /// `route_unhandled_vm_throw` re-typing when the throw escapes all
    /// in-BAML catches. For non-host-callable sysops (which never produce
    /// `HostThrown` payloads), pass `None` for the throws contract.
    #[expect(
        clippy::too_many_arguments,
        reason = "The unwind bridge keeps capture, contract, and thread state explicit."
    )]
    async fn inject_sysop_throw(
        self: &Arc<Self>,
        thread: &mut ActiveHeapPermit<BexThread>,
        call_id: CallId,
        op_err: OpError,
        throws_type: Option<&RuntimeTy>,
        host_callable_throws_contract: Option<&RuntimeTy>,
        call_capture: Option<&CallValueCaptureContext>,
        origin_call_capture: Option<(u64, u32, VmCaptureMask)>,
    ) -> Result<Option<ThreadOutcome>, EngineError> {
        let (materialized, mut profiler_kind) = match op_err.payload {
            sys_types::OpErrorPayload::HostThrown(thrown) => (
                self.convert_external_to_vm_value(thread, *thrown)?,
                bex_vm::errors::ProfilerErrorKind::Fresh,
            ),
            sys_types::OpErrorPayload::Vm(kind) => op_error_to_throw_value(&mut thread.vm, kind)
                .map_err(EngineError::VmInternalError)?,
        };
        let vm_value = if let Some(contract) = host_callable_throws_contract {
            enforce_host_throw_contract(thread, materialized, contract)
        } else {
            materialized
        };
        if vm_value != materialized {
            profiler_kind = bex_vm::errors::ProfilerErrorKind::Fresh;
        }
        let thrown = bex_vm::errors::VmThrown {
            value: vm_value,
            profiler_kind,
            language_is_rethrow: false,
            origin: if let Some((call_id, function_id, mask)) = origin_call_capture {
                bex_vm::errors::VmUnwindOrigin {
                    throw_call_id: call_id,
                    throw_function_id: function_id,
                    throw_site: None,
                    source: bex_vm::errors::VmUnwindSource::EngineCall,
                    selected_error: mask.selected && mask.error,
                    manual_eligible: mask.manual,
                    origin_span_already_terminated: true,
                }
            } else {
                bex_vm::errors::VmUnwindOrigin::unresolved(
                    bex_vm::errors::VmUnwindSource::EngineCall,
                )
            },
        };
        let unwind_result = thread.vm.try_handle_external_thrown(thrown);
        self.drain_vm_call_captures(thread, call_capture);
        match unwind_result {
            // A handler caught the injected exception.
            Ok(()) => Ok(None),
            Err(bex_vm::errors::VmError::ThrownUnhandled { value, trace }) => Ok(Some(
                self.route_unhandled_vm_throw(thread, call_id, value, trace, throws_type)
                    .await?,
            )),
            Err(bex_vm::errors::VmError::Thrown(thrown)) => Ok(Some(
                self.route_unhandled_vm_throw(thread, call_id, thrown.value, Vec::new(), None)
                    .await?,
            )),
            Err(bex_vm::errors::VmError::InternalError(err)) => {
                Err(EngineError::VmInternalError(err))
            }
            Err(bex_vm::errors::VmError::TracedInternalError { source, trace }) => {
                Err(EngineError::TracedVmInternalError { source, trace })
            }
        }
    }

    /// True if `value` is an `Object::Instance` whose class is
    /// `baml.panics.Cancelled`. Used to differentiate cancellation panics
    /// from regular errors when settling a spawned thread that unwound.
    fn is_cancelled_panic(&self, value: Value) -> bool {
        let Some(ptr) = value.as_object_ptr() else {
            return false;
        };
        let Some(&cancelled_class_ptr) = self.resolved_class_names.get(CANCELLED_PANIC_CLASS)
        else {
            return false;
        };
        // SAFETY: a Value::Object that escaped a VM exec step is alive on
        // the heap; this read is gated by the caller's active heap permit.
        match unsafe { ptr.get() } {
            Object::Instance(instance) => instance.class == cancelled_class_ptr,
            _ => false,
        }
    }

    /// Read the spawn parameters out of a `baml.spawn.SpawnParams` instance
    /// (BEP-034 middleware: the value a `spawn ... with` transformer pipeline
    /// produced). Fields are read BY INDEX in declaration order — body=0,
    /// name=1, group=2, cancel=3, detach=4 — keep in sync with
    /// `ns_spawn/spawn.baml`. Returns `None` when the value is not a
    /// well-formed `SpawnParams` (caller falls back to the spawn operands).
    ///
    /// Safe to deref the pointers because the caller holds the active heap
    /// permit and `params` is rooted by the `UnscheduledFuture` being handled.
    fn read_spawn_params(params: HeapPtr) -> Option<SpawnParamsData> {
        // Fields 2/3 helper: `group` / `cancel` are `TaskGroup` / `CancelToken`
        // instances whose `_handle` field (index 0) is `Object::RustData`.
        fn handle_object(value: Value) -> Option<&'static Object> {
            let inst_ptr = value.as_object_ptr()?;
            let Object::Instance(inst) = (unsafe { inst_ptr.get() }) else {
                return None;
            };
            let handle_ptr = inst.load_field(0).as_object_ptr()?;
            Some(unsafe { handle_ptr.get() })
        }

        let Object::Instance(instance) = (unsafe { params.get() }) else {
            return None;
        };
        // Field 0: `body` — the closure the spawned thread runs. A middleware
        // transformer may have wrapped or replaced the original spawn body.
        let body = instance.load_field(0).as_object_ptr()?;

        // Field 1: `name` — optional human-readable label.
        let name =
            instance
                .load_field(1)
                .as_object_ptr()
                .and_then(|ptr| match unsafe { ptr.get() } {
                    Object::String(s) => Some(s.to_string()),
                    _ => None,
                });

        let group = handle_object(instance.load_field(2)).and_then(|obj| match obj {
            Object::RustData(data) => data.clone().downcast::<TaskGroupInner>().ok(),
            _ => None,
        });
        let cancel = handle_object(instance.load_field(3)).and_then(|obj| match obj {
            Object::RustData(data) => data.downcast_ref::<CancellationToken>().cloned(),
            _ => None,
        });

        // Field 4: `detach`.
        let detach = instance.load_field(4).as_bool().unwrap_or(false);

        Some(SpawnParamsData {
            body,
            name,
            group,
            cancel,
            detach,
        })
    }

    /// Spawn a fresh `BexThread` that runs the body packaged in `closure`
    /// and settles a newly-allocated future when the body terminates.
    ///
    /// Allocates the future under the calling thread's heap permit (a
    /// short-lived `()` permit so the new future and child VM are heap
    /// objects gated against GC for the construction window), then
    /// constructs a child `BexThread` whose cancel token is a child of
    /// `parent_cancel` so a parent cancellation cascades down the spawn
    /// tree. The child thread runs its own `run_thread_event_loop` on the
    /// tokio runtime (or `wasm_bindgen_futures::spawn_local` on wasm) and
    /// holds its own permit while executing.
    ///
    /// The future the child settles is pre-allocated by the caller (under the
    /// parent's permit) and identified by `future_id`.
    #[expect(
        clippy::too_many_arguments,
        reason = "spawn edge carries the full context"
    )]
    fn spawn_thread(
        self: Arc<Self>,
        child_cancel: CancellationToken,
        closure: HeapPtr,
        name: Option<String>,
        user_cancel: Option<CancellationToken>,
        group: Option<Arc<TaskGroupInner>>,
        call_id: CallId,
        future_id: FutureId,
        prof_thread_id: u64,
        prof_suppressed: bool,
        root_profiler: RootProfiler,
        boundary_lease: Option<ThreadBoundaryLeaseGuard>,
        call_capture: Option<CallValueCaptureContext>,
        log_capture: Option<LogCaptureContext>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), EngineError>> + Send + 'static>,
    > {
        Box::pin(self.spawn_thread_inner(
            child_cancel,
            closure,
            name,
            user_cancel,
            group,
            call_id,
            future_id,
            prof_thread_id,
            prof_suppressed,
            root_profiler,
            boundary_lease,
            call_capture,
            log_capture,
        ))
    }

    /// Dispatch a spawned body on a fresh `BexThread`.
    ///
    /// The child's heap `Future` (identified by `future_id`) is allocated by
    /// the caller under the *parent's* permit — see the `VmExecState::Spawn`
    /// dispatch site. This function only builds the child VM, registers a new
    /// (initially inactive) permit for it, and fires the task. Critically it
    /// acquires **no** heap permit on the calling task — the child's permit is
    /// acquired on the spawned task — so the parent never holds two permits at
    /// once (which would deadlock GC's `acquire_many`).
    #[allow(clippy::too_many_arguments)]
    async fn spawn_thread_inner(
        self: Arc<Self>,
        child_cancel: CancellationToken,
        closure: HeapPtr,
        name: Option<String>,
        user_cancel: Option<CancellationToken>,
        group: Option<Arc<TaskGroupInner>>,
        call_id: CallId,
        future_id: FutureId,
        prof_thread_id: u64,
        prof_suppressed: bool,
        root_profiler: RootProfiler,
        boundary_lease: Option<ThreadBoundaryLeaseGuard>,
        call_capture: Option<CallValueCaptureContext>,
        log_capture: Option<LogCaptureContext>,
    ) -> Result<(), EngineError> {
        // BEP-034 spawn options: link a user-provided `CancelToken`
        // (`with baml.spawn.options(cancel = ...)`) into this spawn's effective
        // token. Firing the user token cancels the child; the watcher
        // self-terminates when `child_cancel` fires (body done / cancelled
        // / parent cascade) so it never outlives the spawn. (The token itself
        // — fresh for `detach = true`, a child of the parent's otherwise — is
        // derived at the dispatch site, where the future is also allocated.)
        if let Some(user) = user_cancel {
            let linked = child_cancel.clone();
            let watcher = async move {
                tokio::select! {
                    biased;
                    () = user.cancelled() => linked.cancel(),
                    () = linked.cancelled() => {}
                }
            };
            #[cfg(not(target_arch = "wasm32"))]
            tokio::spawn(watcher);
            #[cfg(target_arch = "wasm32")]
            wasm_bindgen_futures::spawn_local(watcher);
        }

        // BEP-034 rate limiting: register with the TaskGroup *synchronously*,
        // here (before the body task is polled), so a `group.cancel()` /
        // `active_count()` issued right after `spawn` already observes this
        // member. The returned ticket is `acquire`d (parked on) inside the body
        // task, so a queued task does not hold the heap permit while waiting.
        let group_ticket = group.map(|group| group.register(child_cancel.clone()));

        // Build the child VM up-front (synchronously) so the await on the
        // permit only holds Send values across yield points.
        let mut child_vm = BexVm::new(
            Arc::clone(&self.heap),
            VmGlobals::Shared(self.globals.clone()),
            #[cfg(not(target_arch = "wasm32"))]
            Arc::clone(&self.park_requested),
            Arc::clone(&self.argv),
            Arc::clone(&self.packages),
            Arc::clone(&self.dynamic_dispatch),
            Arc::clone(&self.error_class_ptrs),
            Arc::clone(&self.panic_class_ptrs),
        );
        child_vm.prof_thread_id = prof_thread_id;
        child_vm.prof_suppressed = prof_suppressed;
        child_vm.root_profiler = root_profiler;
        child_vm.prof_boundary_handle = boundary_lease
            .as_ref()
            .and_then(ThreadBoundaryLeaseGuard::handle);
        child_vm.prof_boundary_root_pending = false;
        if child_vm.root_profiler.is_active() {
            child_vm.prof_enable_await_accumulator();
        }
        child_vm.bex_ref_seed = Some((self.process_euid, self.engine_id));
        child_vm.set_call_input_capture_hook(
            call_capture
                .as_ref()
                .map(CallValueCaptureContext::input_capture_hook),
        );
        // Snapshot on the spawning thread, immediately before the entry
        // frame's CallFunction lands (no await in between); the loop-head
        // refresh re-snapshots on the child task's own thread later. The
        // StartThread (with the spawn edge) was already emitted into the
        // ring at the Spawn arm.
        self.prof_refresh_vm_ring(&mut child_vm);
        child_vm.set_entry_point(closure, &[]);
        let child_entry_call_id = BexCallId(child_vm.current_call_id());
        let child_profile_enabled = child_vm.prof_ring.is_some();

        // Register a new (inactive) permit for the child. `new_permit` only
        // takes the holders mutex — it does NOT acquire a semaphore permit —
        // so this is safe to call while the parent still holds its own permit.
        // The child's permit is acquired below, on the spawned task.
        let child_thread = BexThread::new_child(child_vm, child_cancel.clone(), name, future_id);
        let inactive = self.heap_permit_manager.new_permit(child_thread).await;

        // Return type / throws type are approximated; the future's value is
        // converted on the awaiter side.
        let engine = self;
        let return_type = RuntimeTy::Null {
            attr: baml_type::TyAttr::default(),
        };
        let task = async move {
            // Declared first so it drops last: if this future is dropped at
            // any await below (before the event loop takes over), the closer
            // emits the child's EndFunction/EndThread (follow-up 11).
            let mut prof_closer = SpawnProfCloser {
                engine: Arc::clone(&engine),
                prof_thread_id,
                entry_call_id: child_entry_call_id,
                awaited: None,
                armed: child_profile_enabled,
                boundary_lease,
            };
            // BEP-034 rate limiting: if this spawn joined a `TaskGroup`, park
            // here — WITHOUT the heap permit, so a queued task doesn't block GC
            // — until a slot frees. A task cancelled while queued (group/user/
            // parent) settles its future `Cancelled` and never runs its body.
            // The permit is held for the body's lifetime and releases the slot
            // (waking the next FIFO waiter) on drop.
            let entry_wait_start = child_profile_enabled.then(bex_events::prof::clock::now_ticks);
            let _group_permit = match group_ticket {
                Some(ticket) => match ticket.acquire().await {
                    Some(permit) => Some(permit),
                    None => {
                        let mut permit = inactive.acquire().await;
                        if let Some(start_ticks) = entry_wait_start {
                            let end_ticks = bex_events::prof::clock::now_ticks();
                            engine.prof_charge_await(
                                &mut permit.vm,
                                child_entry_call_id.0,
                                start_ticks,
                                end_ticks,
                            );
                            prof_closer.awaited = permit.vm.prof_take_await(child_entry_call_id.0);
                        }
                        if let Err(err) =
                            engine.settle_child_cancelled(&mut permit, future_id).await
                        {
                            tracing::error!(
                                ?err,
                                ?future_id,
                                "failed to settle queued-then-cancelled spawn"
                            );
                        }
                        // The armed `prof_closer` emits the profiling
                        // closes (EndFunction{Cancelled} + EndThread{
                        // Cancelled}) when it drops at this return — the
                        // event loop that would otherwise close them never
                        // runs for a queued-then-cancelled spawn.
                        return;
                    }
                },
                None => None,
            };
            let mut permit = inactive.acquire().await;
            if let Some(start_ticks) = entry_wait_start {
                let end_ticks = bex_events::prof::clock::now_ticks();
                engine.prof_charge_await(
                    &mut permit.vm,
                    child_entry_call_id.0,
                    start_ticks,
                    end_ticks,
                );
            }
            // The entry call's id was minted by set_entry_point on the
            // spawning thread; its CallFunction is already in the ring.
            // EndThread (ring) is emitted by the run_thread_event_loop
            // wrapper on every exit path from here on.
            prof_closer.defuse();
            match engine
                .run_thread_event_loop(
                    return_type,
                    None,
                    permit,
                    call_id,
                    None,
                    call_capture,
                    log_capture,
                    &child_cancel,
                    true,
                )
                .await
            {
                Ok(ThreadOutcome::SettledChild(_)) => {}
                // The abnormal arms close any spans the event loop left open
                // before the thread ends — the ring EndThread (emitted by the
                // run_thread_event_loop wrapper) must never strand open spans.
                Ok(ThreadOutcome::RootValue(_)) => {
                    tracing::error!(
                        ?future_id,
                        "spawn thread returned a root value instead of settling its future"
                    );
                }
                Err(err) => {
                    tracing::error!(
                        ?err,
                        ?future_id,
                        "spawn thread terminated with engine error"
                    );
                    // The event loop settles the future itself on the VM-error
                    // paths (`InternalError` / `TracedInternalError` /
                    // unhandled-throw arms), but an `EngineError` escaping
                    // through any other `?` in the loop arrives here with the
                    // future still `Pending` — and an unsettled future parks
                    // every awaiter forever (the parent, its parent, …). This
                    // arm is the single choke point that restores the
                    // propagation chain: settle the future with the engine
                    // error so the awaiter re-raises it, *its* terminal path
                    // settles the next future up, and the root surfaces the
                    // original error to the host. The thread's own permit died
                    // with the event loop, so settle under a fresh rootless
                    // permit (`()` roots nothing; the future entry itself
                    // roots the heap object).
                    let admin = engine.heap_permit_manager.new_permit(()).await;
                    let active = admin.acquire().await;
                    engine
                        .futures
                        .acquire(active.proof())
                        .await
                        .settle_spawn_engine_error(future_id, err);
                    child_cancel.cancel();
                }
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(task);
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(task);

        Ok(())
    }

    /// Runs a thread's event loop (see [`Self::run_thread_event_loop_inner`])
    /// and closes its profiling lifecycle: one `EndThread` per `StartThread`,
    /// on every exit path (the inner loop has many early returns; thread
    /// join at shutdown makes the final commits visible to the consumer per
    /// plan §1).
    #[expect(
        clippy::too_many_arguments,
        reason = "The thread wrapper forwards lifecycle, capture, and cancellation state explicitly."
    )]
    async fn run_thread_event_loop(
        self: &Arc<Self>,
        return_type: RuntimeTy,
        throws_type: Option<RuntimeTy>,
        thread: ActiveHeapPermit<BexThread>,
        call_id: CallId,
        root_capture: Option<RootValueCaptureContext>,
        call_capture: Option<CallValueCaptureContext>,
        log_capture: Option<LogCaptureContext>,
        cancel: &CancellationToken,
        copy_objects: bool,
    ) -> Result<ThreadOutcome, EngineError> {
        let profile_thread = thread.vm.prof_ring.is_some();
        let prof_thread_id = thread.vm.prof_thread_id;
        let prof_boundary_handle = thread.vm.prof_boundary_handle;
        // StartThread was already emitted before this function: roots in
        // run_entry_point (right before `set_entry_point`, §7 decision 7 —
        // StartThread-first is a wire invariant), children at the Spawn
        // arm. This wrapper owns the matching EndThread on every exit path;
        // queued-then-cancelled spawns (which never reach this loop) close
        // their lifecycle in spawn_thread_inner's task body.
        let result = self
            .run_thread_event_loop_inner(
                return_type,
                throws_type,
                thread,
                call_id,
                root_capture,
                call_capture,
                log_capture,
                cancel,
                copy_objects,
            )
            .await;
        if profile_thread {
            let status = match &result {
                Ok(ThreadOutcome::SettledChild(ChildSettleKind::Cancelled)) => {
                    bex_events::prof::record::ThreadEndStatus::Cancelled
                }
                Ok(ThreadOutcome::SettledChild(ChildSettleKind::Errored)) => {
                    bex_events::prof::record::ThreadEndStatus::Errored
                }
                // `baml.sys.exit` is a clean termination request: exit
                // code 0 closes as Completed, non-zero as Errored.
                Ok(_) | Err(EngineError::Exit { code: 0 }) => {
                    bex_events::prof::record::ThreadEndStatus::Completed
                }
                Err(EngineError::Exit { .. }) => bex_events::prof::record::ThreadEndStatus::Errored,
                Err(_) if cancel.is_cancelled() => {
                    bex_events::prof::record::ThreadEndStatus::Cancelled
                }
                Err(_) => bex_events::prof::record::ThreadEndStatus::Errored,
            };
            let committed = self.prof_emit(&bex_events::prof::record::RawRecord::EndThread {
                status,
                thread_id: BexThreadId(prof_thread_id),
                ts_ticks: bex_events::prof::clock::now_ticks(),
            });
            if !committed {
                self.prof_record_transport_loss(prof_boundary_handle);
            }
        }
        result
    }

    /// Drive a `BexThread` to completion, dispatching sys-ops, awaits, span
    /// notifications, and early-yield events.
    ///
    /// The `thread` parameter wraps the executing `BexVm` plus metadata
    /// (cancel token, optional name, optional `settles_future`) that
    /// distinguishes a root call from a spawned child. The permit is
    /// released at async safepoints (via `gc_safepoint`) and re-acquired
    /// after any concurrent GC finishes. Each re-acquisition invalidates
    /// the VM's TLAB through the post-GC `forward_roots` hook.
    ///
    /// Returns a [`ThreadOutcome`] describing how the thread terminated:
    /// either a host-visible root value, a root that has already written
    /// to the host, or a settled child future. Children route their
    /// `Complete` / `ThrownUnhandled` / `InternalError` transitions
    /// through [`FutureManager`] so the awaiter resumes correctly.
    #[allow(clippy::too_many_arguments)]
    async fn run_thread_event_loop_inner(
        self: &Arc<Self>,
        return_type: RuntimeTy,
        throws_type: Option<RuntimeTy>,
        mut thread: ActiveHeapPermit<BexThread>,
        call_id: CallId,
        root_capture: Option<RootValueCaptureContext>,
        call_capture: Option<CallValueCaptureContext>,
        log_capture: Option<LogCaptureContext>,
        cancel: &CancellationToken,
        copy_objects: bool,
    ) -> Result<ThreadOutcome, EngineError> {
        loop {
            // D5a: refresh the profiling ring snapshot once per exec resume
            // — a TLS lookup here, never per push. Sound because exec()
            // never crosses an .await and tokio migrates tasks only at
            // .await points, so one refresh covers OS-thread migration
            // (plan §6, invariant 4: if exec ever yields mid-step, this
            // model must be revisited).
            self.prof_refresh_vm_ring(&mut thread.vm);

            let vm_exec_result = thread.vm.exec();
            self.drain_vm_call_captures(&mut thread, call_capture.as_ref());

            let exec_result = match vm_exec_result {
                Ok(state) => state,
                Err(bex_vm::errors::VmError::ThrownUnhandled { value, trace }) => {
                    return self
                        .route_unhandled_vm_throw(
                            &mut thread,
                            call_id,
                            value,
                            trace,
                            throws_type.as_ref(),
                        )
                        .await;
                }
                Err(bex_vm::errors::VmError::Thrown(thrown)) => {
                    return self
                        .route_unhandled_vm_throw(
                            &mut thread,
                            call_id,
                            thrown.value,
                            Vec::new(),
                            throws_type.as_ref(),
                        )
                        .await;
                }
                Err(bex_vm::errors::VmError::InternalError(err)) => {
                    if let Some(future_id) = thread.vm_thread_settles_future() {
                        let mut guard = self.futures.acquire(thread.proof()).await;
                        guard
                            .internal_error_future(future_id, EngineError::VmInternalError(err))?;
                        drop(guard);
                        thread.vm_thread_cancel().cancel();
                        return Ok(ThreadOutcome::SettledChild(ChildSettleKind::Errored));
                    }
                    return Err(EngineError::VmInternalError(err));
                }
                Err(bex_vm::errors::VmError::TracedInternalError { source, trace }) => {
                    if let Some(future_id) = thread.vm_thread_settles_future() {
                        let mut guard = self.futures.acquire(thread.proof()).await;
                        guard.internal_error_future(
                            future_id,
                            EngineError::TracedVmInternalError { source, trace },
                        )?;
                        drop(guard);
                        thread.vm_thread_cancel().cancel();
                        return Ok(ThreadOutcome::SettledChild(ChildSettleKind::Errored));
                    }
                    return Err(EngineError::TracedVmInternalError { source, trace });
                }
            };
            match exec_result {
                VmExecState::Complete(value) => {
                    // Spawned children: write the value into the future
                    // registry and return SettledChild. The awaiter's
                    // next `Await` instruction picks up `FutureRead::Ready`.
                    if let Some(future_id) = thread.vm_thread_settles_future() {
                        let mut guard = self.futures.acquire(thread.proof()).await;
                        guard.fulfill_future(future_id, value)?;
                        return Ok(ThreadOutcome::SettledChild(ChildSettleKind::Fulfilled));
                    }
                    // "Cancel wins" semantics: if cancellation races with a
                    // completed VM step, report a cancellation panic rather
                    // than returning a success value.
                    let cancelled = cancel.is_cancelled();

                    if cancelled {
                        return Err(cancelled_unhandled_throw());
                    }

                    if let Some(capture) = root_capture.as_ref() {
                        self.capture_root_value(&thread, capture, value);
                    }

                    let (return_value, _event_result) = if !copy_objects {
                        if let Some(ptr) = value.as_object_ptr() {
                            // SAFETY: the active thread holds the heap permit
                            // through `thread.proof()`.
                            //
                            // Heap-boxed floats can't be handle-wrapped — a
                            // function declared `-> float` (or
                            // `-> Union<float, ...>` / `-> float?`) should
                            // surface as an inline `BexExternalValue::Float`,
                            // not an opaque `Handle`. Route them through the
                            // typed converter so declared-type metadata
                            // (e.g. Union wrapping) is preserved; the bare
                            // unboxing fast-path stripped that.
                            if matches!(unsafe { ptr.get() }, Object::Float(_)) {
                                let external = self.convert_vm_value_to_external_with_type(
                                    value,
                                    &return_type,
                                    &thread.vm,
                                    thread.proof(),
                                )?;
                                (external.clone(), external)
                            } else {
                                let handle = self.heap.create_handle(ptr);
                                (
                                    BexExternalValue::Handle(handle),
                                    self.vm_value_to_owned(thread.proof(), value),
                                )
                            }
                        } else {
                            let external = self.convert_vm_value_to_external_with_type(
                                value,
                                &return_type,
                                &thread.vm,
                                thread.proof(),
                            )?;
                            let external = crate::conversion::coerce_return_to_declared_type(
                                external,
                                &return_type,
                            )?;
                            (external.clone(), external)
                        }
                    } else {
                        let external = self.convert_vm_value_to_external_with_type(
                            value,
                            &return_type,
                            &thread.vm,
                            thread.proof(),
                        )?;
                        let external = crate::conversion::coerce_return_to_declared_type(
                            external,
                            &return_type,
                        )?;
                        (external.clone(), external)
                    };

                    return Ok(ThreadOutcome::RootValue(return_value));
                }

                VmExecState::SysOp { operation, args } => {
                    // Single round-trip sys-op call. Convert args, race
                    // the op against the active cancel token, and push the
                    // resulting value back on the VM stack. No
                    // `Object::Future` is allocated and no `FutureManager`
                    // entry is created — the schedule/await dance would be
                    // pure overhead because the user never sees the future.
                    #[allow(clippy::large_enum_variant)]
                    enum SysOpOutcome {
                        Cancelled,
                        Result(Result<BexExternalValue, OpError>),
                    }

                    // Honor an already-cancelled call before traversing and
                    // externalizing sys-op arguments. Host-call argument packs
                    // can contain arbitrarily large value trees; cancellation
                    // must remain an O(1) pre-check rather than paying that
                    // conversion cost for an operation that will never run.
                    if cancel.is_cancelled() {
                        // Cancel-at-yield: spawned children settle as
                        // Cancelled so the heap Future no longer hangs
                        // at Pending; root threads surface the cancel
                        // to the host.
                        self.prof_end_sysop(
                            &mut thread.vm,
                            bex_events::prof::record::FunctionEndStatus::Cancelled,
                        );
                        self.prof_drain_open_calls(
                            &mut thread.vm,
                            bex_events::prof::record::FunctionEndStatus::Cancelled,
                        );
                        if let Some(future_id) = thread.vm_thread_settles_future() {
                            self.settle_child_cancelled(&mut thread, future_id).await?;
                            return Ok(ThreadOutcome::SettledChild(ChildSettleKind::Cancelled));
                        }
                        return Err(cancelled_unhandled_throw());
                    }

                    let runtime_type_overlay = self.runtime_type_overlay(&args, thread.proof());
                    let runtime_compile_request = match operation {
                        SysOp::BamlReflectPackageCompile => {
                            Some(Ok(Self::runtime_compile_request(&thread.vm, &args)?))
                        }
                        SysOp::BamlReflectSessionCompile => {
                            Some(Self::runtime_session_compile_request(&mut thread.vm, &args))
                        }
                        _ => None,
                    };
                    if let Some(Ok(request)) = runtime_compile_request.as_ref()
                        && let bex_vm_types::RuntimeCompileMode::Session(session) = &request.mode
                        && let Some(future_id) = thread.vm_thread_settles_future()
                    {
                        let mut guard = self.futures.acquire(thread.proof()).await;
                        guard.register_session_lease(future_id, &session.lease)?;
                    }
                    let runtime_schema_overlay = self.runtime_schema_overlay(&thread.vm, &args);

                    let bex_args: Vec<BexExternalValue> =
                        if operation == SysOp::BamlHostCallHostValue {
                            let params = host_call_params(args.first().copied())
                                .map_err(EngineError::VmInternalError)?;
                            if args.len() != 4 {
                                return Err(EngineError::VmInternalError(
                                    bex_vm::errors::VmInternalError::BridgeFailure {
                                        message: format!(
                                            "call_host_value: got {} sysop arguments, expected 4",
                                            args.len()
                                        ),
                                    },
                                ));
                            }
                            vec![
                                self.vm_arg_to_bex_value(args[0]),
                                self.convert_host_call_args_pack(
                                    args[1],
                                    &params,
                                    &thread.vm,
                                    thread.proof(),
                                )?,
                                self.vm_arg_to_bex_value(args[2]),
                                self.vm_arg_to_bex_value(args[3]),
                            ]
                        } else {
                            args.iter()
                                .map(|value| self.vm_arg_to_bex_value(*value))
                                .collect()
                        };

                    // Capture the host-call type args (`type_arg_0`/`args[2]`
                    // = return type `T`; `type_arg_1`/`args[3]` = throws
                    // contract `E`) as OWNED `RuntimeTy` values now, while the heap
                    // permit is still held and the packed `Object::Type`
                    // pointers are live. The async wait below releases the
                    // permit, and a moving GC can then relocate/collect the
                    // object; the engine-local `args` Vec is not a GC root
                    // and is never forwarded, so re-reading the raw pointer
                    // post-await would be a use-after-free. Cloning the
                    // `RuntimeTy`s here sidesteps that.
                    //
                    // `host_throws_ty` drives the throws-contract check at
                    // the host-throw injection site below: a host throw
                    // that doesn't match `E` becomes a
                    // `baml.panics.HostContractViolation` panic instead of
                    // a catchable throw.
                    let host_ret_ty: Option<baml_type::RuntimeTy> =
                        if operation == SysOp::BamlHostCallHostValue {
                            Some(
                                host_call_type_arg(args.get(2).copied(), 2, "ret_ty")
                                    .map_err(EngineError::VmInternalError)?,
                            )
                        } else {
                            None
                        };
                    let host_throws_ty: Option<baml_type::RuntimeTy> =
                        if operation == SysOp::BamlHostCallHostValue {
                            Some(
                                host_call_type_arg(args.get(3).copied(), 3, "throws_ty")
                                    .map_err(EngineError::VmInternalError)?,
                            )
                        } else {
                            None
                        };

                    let sys_op_result = if let Some(request) = runtime_compile_request {
                        match request {
                            Ok(request) => self.execute_runtime_compile(request, operation),
                            Err(error) => SysOpResult::Ready(Err(error)),
                        }
                    } else {
                        self.execute_sys_op(
                            operation,
                            &bex_args,
                            &runtime_type_overlay,
                            call_id,
                            cancel,
                            thread.proof(),
                            runtime_schema_overlay.as_ref(),
                        )
                    };

                    let outcome = match sys_op_result {
                        SysOpResult::Ready(r) => r,
                        SysOpResult::Async(fut) => {
                            // Release the heap permit so concurrent GC
                            // can run during the wait. Re-acquire
                            // before touching VM state.
                            let prof_await = if thread.vm.root_profiler.is_active() {
                                thread
                                    .vm
                                    .pending_sysop_call_id
                                    .map(|call_id| (call_id, bex_events::prof::clock::now_ticks()))
                            } else {
                                None
                            };
                            let inactive = thread.release();
                            self.maybe_collect_garbage().await;
                            let outcome = tokio::select! {
                                biased;
                                () = cancel.cancelled() => SysOpOutcome::Cancelled,
                                r = fut                  => SysOpOutcome::Result(r),
                            };
                            thread = inactive.acquire().await;
                            if let Some((call_id, start_ticks)) = prof_await {
                                let end_ticks = bex_events::prof::clock::now_ticks();
                                self.prof_charge_await(
                                    &mut thread.vm,
                                    call_id,
                                    start_ticks,
                                    end_ticks,
                                );
                            }
                            // Re-snapshot post-await (D5a): the task may be on a different
                            // OS thread now, and engine-driven VM re-entries before the
                            // next loop-head refresh (inject_sysop_throw's unwind) push
                            // through this snapshot.
                            self.prof_refresh_vm_ring(&mut thread.vm);
                            match outcome {
                                SysOpOutcome::Cancelled => {
                                    self.prof_end_sysop(
                                        &mut thread.vm,
                                        bex_events::prof::record::FunctionEndStatus::Cancelled,
                                    );
                                    self.prof_drain_open_calls(
                                        &mut thread.vm,
                                        bex_events::prof::record::FunctionEndStatus::Cancelled,
                                    );
                                    if let Some(future_id) = thread.vm_thread_settles_future() {
                                        self.settle_child_cancelled(&mut thread, future_id).await?;
                                        return Ok(ThreadOutcome::SettledChild(
                                            ChildSettleKind::Cancelled,
                                        ));
                                    }
                                    return Err(cancelled_unhandled_throw());
                                }
                                SysOpOutcome::Result(r) => r,
                            }
                        }
                    };

                    // PR4b: close the sys-op call pair opened at the VM's
                    // yield site. Post-await this task may sit on a
                    // different OS thread — prof_end_sysop goes through the
                    // TLS ring lookup, so that is fine. Errors classify by
                    // class like the inline-native close: a host-abandoned
                    // op (CompletionHandle dropped → cancel-classed payload,
                    // no token fired) closes Cancelled, matching the
                    // Cancelled frames its injected throw then unwinds.
                    let sysop_capture_mask = thread.vm.pending_sysop_capture_mask;
                    let sysop_capture_call = self.prof_end_sysop(
                        &mut thread.vm,
                        match &outcome {
                            Ok(_) => bex_events::prof::record::FunctionEndStatus::Ok,
                            Err(op_err) => Self::prof_sysop_error_status(op_err),
                        },
                    );
                    let sysop_origin_capture = sysop_capture_call
                        .map(|(call_id, function_id)| (call_id, function_id, sysop_capture_mask));

                    match outcome {
                        Ok(external) => {
                            // Schema-aware return-type validation for host
                            // callables. The bridge's shared
                            // `validate_host_return` guard already rejected
                            // scalar / enum-identity / class-name mismatches at
                            // the FFI boundary; here — where the compiled class
                            // schema is reachable — we additionally validate
                            // class *field types* against the declared return
                            // type (`host_ret_ty`, captured from `args[2]` before
                            // the await). A mismatch is injected into the VM's
                            // exception unwinder so an in-BAML `catch` can
                            // catch it exactly like a host-raised error.
                            if operation == SysOp::BamlHostCallHostValue
                                && let Some(ret_ty) = host_ret_ty.as_ref()
                                && let Err(message) =
                                    self.validate_host_return_schema(&external, ret_ty)
                            {
                                // A wrong-return-type at the engine-level
                                // schema check is the same kind of contract
                                // breach as the FFI-boundary guard catches:
                                // the host returned a value that doesn't
                                // inhabit `T`. Surface as
                                // `baml.panics.HostContractViolation`
                                // (panic, not catchable).
                                let op_err = OpError::new(
                                    SysOp::BamlHostCallHostValue,
                                    sys_types::VmPanic::HostContractViolation {
                                        message,
                                        class_name: None,
                                        language: None,
                                    },
                                );
                                if let Some(outcome) = self
                                    .inject_sysop_throw(
                                        &mut thread,
                                        call_id,
                                        op_err,
                                        throws_type.as_ref(),
                                        host_throws_ty.as_ref(),
                                        call_capture.as_ref(),
                                        sysop_origin_capture,
                                    )
                                    .await?
                                {
                                    return Ok(outcome);
                                }
                                // VM caught the throw; the unwinder truncated
                                // the eval stack back to the catching frame's
                                // locals region, so we must NOT push the
                                // would-be return value (there's no slot
                                // expecting it any more). Fall through to the
                                // top of the outer loop.
                            } else {
                                // Convert the external value back into a VM
                                // Value (allocating string / list / instance
                                // heap objects as needed) and push it onto
                                // the eval stack. The bytecode that follows
                                // this sys-op call is a normal `store_var`
                                // / projection / whatever the surrounding
                                // expression expected — no implicit await.
                                let value = if operation == SysOp::BamlHostCallHostValue {
                                    self.convert_external_to_vm_value_with_ty(
                                        &mut thread,
                                        external,
                                        host_ret_ty.as_ref(),
                                    )?
                                } else if let Some(overlay) = runtime_schema_overlay.as_ref() {
                                    self.convert_external_to_vm_value_with_runtime_schema(
                                        &mut thread,
                                        external,
                                        overlay,
                                        &runtime_type_overlay.class_handles,
                                        &runtime_type_overlay.enum_handles,
                                    )?
                                } else {
                                    self.convert_external_to_vm_value_with_dynamic_types(
                                        &mut thread,
                                        external,
                                        None,
                                        &runtime_type_overlay.class_handles,
                                        &runtime_type_overlay.enum_handles,
                                    )?
                                };
                                if let Some((call_id, _, mask)) = sysop_origin_capture {
                                    thread
                                        .vm
                                        .queue_engine_call_output_capture(call_id, mask, value);
                                    self.drain_vm_call_captures(&mut thread, call_capture.as_ref());
                                }
                                thread.vm.stack.push(value);
                            }
                        }
                        Err(op_err) => {
                            // A sysop error. The throw value (built via
                            // [`op_error_to_throw_value`]) is **injected into
                            // the VM's exception unwinder** via
                            // [`Self::inject_sysop_throw`], so an in-BAML
                            // `try { f(x) } catch (e: …) { … }` catches sysop
                            // throws like any other throw. If no handler
                            // matches, the throw propagates exactly like a
                            // VM-internal `ThrownUnhandled` — settling a
                            // spawned child errored or surfacing as
                            // `EngineError::UnhandledThrow` at the root.
                            if let Some(outcome) = self
                                .inject_sysop_throw(
                                    &mut thread,
                                    call_id,
                                    op_err,
                                    throws_type.as_ref(),
                                    host_throws_ty.as_ref(),
                                    call_capture.as_ref(),
                                    sysop_origin_capture,
                                )
                                .await?
                            {
                                return Ok(outcome);
                            }
                            // VM caught the throw; fall through to the top of
                            // the outer loop for the next `vm.exec()`.
                        }
                    }
                }

                VmExecState::Spawn {
                    future: unscheduled,
                    source_span: spawn_source_span,
                } => {
                    // BEP-034: pull the closure + name off the
                    // `UnscheduledFuture` heap object and hand them to
                    // `spawn_thread`, which allocates the future and
                    // dispatches the body on a fresh `BexThread`.
                    let unscheduled = thread
                        .vm
                        .unscheduled_future(unscheduled)
                        .map_err(EngineError::VmInternalError)?;
                    let UnscheduledFuture {
                        closure,
                        name: name_ptr,
                        config: config_ptr,
                        returns,
                        throws,
                    } = unscheduled.clone();
                    let spawn_name: Option<String> =
                        name_ptr.and_then(|ptr| match unsafe { ptr.get() } {
                            Object::String(s) => Some(s.to_string()),
                            _ => None,
                        });
                    // BEP-034 middleware: a `spawn ... with` lowers its final
                    // transformed `baml.spawn.SpawnParams` into the config
                    // operand. The params override the spawn operands — a
                    // transformer may have wrapped/replaced the body or set a
                    // name — and carry the options: `cancel` links into the
                    // child's effective token; `detach` decouples it from the
                    // parent; `group` rate-limits it.
                    let params = config_ptr.and_then(Self::read_spawn_params);
                    let (closure, spawn_name) = match &params {
                        Some(p) => (p.body, p.name.clone().or(spawn_name)),
                        None => (closure, spawn_name),
                    };
                    let (user_cancel, group, detach) = match params {
                        Some(p) => (p.cancel, p.group, p.detach),
                        None => (None, None, false),
                    };
                    // Each spawned thread gets a child cancel token so parent →
                    // child cascade falls out of the token tree without bespoke
                    // tracking. A `detach = true` spawn instead gets a fresh,
                    // independent token so the parent's cancellation (and
                    // unhandled-throw cascade) does NOT reach it — it behaves
                    // like a top-level task.
                    let child_cancel = if detach {
                        CancellationToken::new()
                    } else {
                        cancel.child_token()
                    };
                    // Allocate the child's future under the parent's
                    // already-held permit, then hand the id to `spawn_thread`.
                    //
                    // Acquiring a *fresh* heap permit inside the spawn path
                    // (while this task still holds its own) deadlocks against
                    // GC: `HeapPermitManager::request_park` drains the entire
                    // semaphore via `acquire_many(MAX_PERMITS)`, and tokio's
                    // semaphore is fair — so a nested 1-permit acquire queues
                    // *behind* a pending park request, which in turn cannot
                    // proceed until *this* task's permit is released. The task
                    // can't release until the spawn completes → cycle, with all
                    // workers idle. Keeping the spawn path to a single permit
                    // per task (this `new_future` runs under `thread`) avoids
                    // it. The guard is dropped before the `spawn_thread` await
                    // so no non-`Send` guard crosses a yield point.
                    // BEX profiling: the spawn edge. This is the one place
                    // the parent thread id, the spawning call id, and the
                    // child's name are all in hand (plan §2.2).
                    let child_prof_thread_id = self.next_prof_thread_id();
                    let child_boundary_lease = thread.vm.prof_boundary_handle.and_then(|handle| {
                        self.profiler_session
                            .boundary_registry()
                            .and_then(|registry| registry.try_acquire_child_handle(handle).ok())
                            .map(|lease| {
                                ThreadBoundaryLeaseGuard::new(
                                    Arc::clone(&self.profiler_session),
                                    lease,
                                )
                            })
                    });
                    let child_profile_active = child_boundary_lease.is_some();
                    let child_root_profiler =
                        if child_profile_active || !thread.vm.root_profiler.is_active() {
                            thread.vm.root_profiler
                        } else {
                            RootProfiler::Inactive(
                                bex_events::prof::backend::InactiveReason::ThreadLeaseUnavailable,
                            )
                        };
                    if child_profile_active {
                        let name = spawn_name.as_deref().unwrap_or("");
                        let committed = self.prof_emit(
                            &bex_events::prof::record::RawRecord::StartThreadSpawn {
                                flags: 0,
                                thread_id: BexThreadId(child_prof_thread_id),
                                parent_thread_id: BexThreadId(thread.vm.prof_thread_id),
                                parent_call_id: BexCallId(thread.vm.current_call_id()),
                                ts_ticks: bex_events::prof::clock::now_ticks(),
                                spawn_site: spawn_source_span,
                                name: bex_events::prof::record::capped_name_bytes(name),
                            },
                        );
                        if !committed {
                            self.prof_record_transport_loss(
                                child_boundary_lease
                                    .as_ref()
                                    .and_then(ThreadBoundaryLeaseGuard::handle),
                            );
                        }
                    }

                    let future_ptr = {
                        let mut guard = self.futures.acquire(thread.proof()).await;
                        let (future_id, future_ptr) =
                            guard.new_future(returns, throws, child_cancel.clone());
                        drop(guard);
                        Arc::clone(self)
                            .spawn_thread(
                                child_cancel,
                                closure,
                                spawn_name,
                                user_cancel,
                                group,
                                call_id,
                                future_id,
                                child_prof_thread_id,
                                !child_profile_active,
                                child_root_profiler,
                                child_boundary_lease,
                                call_capture.clone(),
                                log_capture.clone(),
                            )
                            .await?;
                        future_ptr
                    };
                    thread.vm.stack.push(Value::object(future_ptr));
                }

                VmExecState::Await(future_id) => {
                    // Fail-fast if the thread's own cancel token is
                    // already fired (e.g. parent cascaded into us
                    // between the previous yield and this await). The
                    // BEP guarantees that the next `await` after the
                    // token fires throws `Cancelled`; we honor that
                    // even when the awaited future is unrelated to our
                    // cancel chain (and would otherwise never settle
                    // via cascade).
                    //
                    // For spawned children: route through `cancel_future`
                    // so the heap Future settles instead of leaking as
                    // Pending. Mirrors the `VmError::ThrownUnhandled`
                    // arm above for a Cancelled panic from the VM side.
                    if cancel.is_cancelled() {
                        self.prof_drain_open_calls(
                            &mut thread.vm,
                            bex_events::prof::record::FunctionEndStatus::Cancelled,
                        );
                        if let Some(future_id) = thread.vm_thread_settles_future() {
                            self.settle_child_cancelled(&mut thread, future_id).await?;
                            return Ok(ThreadOutcome::SettledChild(ChildSettleKind::Cancelled));
                        }
                        return Err(cancelled_unhandled_throw());
                    }
                    #[allow(clippy::items_after_statements)]
                    // Outcome of the SetOnce-vs-cancel race below. Inline
                    // here because moving it to module scope just for
                    // clippy's preference would split the readers'
                    // attention across two files.
                    enum AwaitOutcome {
                        Cancelled,
                        Done(Result<(), EngineError>),
                    }
                    // Tightly-scoped guard so the proof's borrow on `vm`
                    // ends before we release the VM permit below.
                    let future = {
                        let mut g = self.futures.acquire(thread.proof()).await;
                        g.future_ready(future_id)?
                    };
                    // Release the VM permit before the SetOnce wait — the
                    // wait is the safepoint. Holding a permit through the
                    // wait would deadlock concurrent GC park (which needs
                    // every permit) against the spawned task that fulfils
                    // this future (which needs a permit to write the heap).
                    let prof_await = thread.vm.root_profiler.is_active().then(|| {
                        (
                            thread.vm.prof_current_call_id(),
                            bex_events::prof::clock::now_ticks(),
                        )
                    });
                    let inactive = thread.release();
                    // While parked, run a heuristic-driven GC check (no
                    // permit dance needed since we're already released).
                    self.maybe_collect_garbage().await;
                    // Race the SetOnce wait against the thread's own
                    // cancel token so an unrelated-future await
                    // doesn't hang when the thread itself is cancelled
                    // (cascade only saves us when the awaited future is
                    // a descendant whose token derives from ours).
                    let outcome = tokio::select! {
                        biased;
                        () = cancel.cancelled() => AwaitOutcome::Cancelled,
                        r = future              => AwaitOutcome::Done(r),
                    };
                    thread = inactive.acquire().await;
                    if let Some((call_id, start_ticks)) = prof_await {
                        let end_ticks = bex_events::prof::clock::now_ticks();
                        self.prof_charge_await(&mut thread.vm, call_id, start_ticks, end_ticks);
                    }
                    // Re-snapshot post-await (D5a): the task may be on a different
                    // OS thread now, and engine-driven VM re-entries before the
                    // next loop-head refresh (inject_sysop_throw's unwind) push
                    // through this snapshot.
                    self.prof_refresh_vm_ring(&mut thread.vm);
                    match outcome {
                        AwaitOutcome::Cancelled => {
                            self.prof_drain_open_calls(
                                &mut thread.vm,
                                bex_events::prof::record::FunctionEndStatus::Cancelled,
                            );
                            if let Some(future_id) = thread.vm_thread_settles_future() {
                                self.settle_child_cancelled(&mut thread, future_id).await?;
                                return Ok(ThreadOutcome::SettledChild(ChildSettleKind::Cancelled));
                            }
                            return Err(cancelled_unhandled_throw());
                        }
                        AwaitOutcome::Done(r) => r?,
                    }
                }

                // BEP-034 `baml.future.__await_any`: park until the FIRST of
                // several input futures settles, then resume so the VM
                // re-executes the `AwaitAny` opcode and pushes the winner's
                // index. Mirrors the `Await` arm's permit/cancel dance but
                // races all the inputs' SetOnce wakeups at once. The opcode
                // already filtered out futures that were settled going in, so
                // an empty `future_ids` means every input had a wakeup pending
                // or the array was empty.
                VmExecState::AwaitAny(future_ids) => {
                    // Fail-fast on our own cancellation, exactly as `Await`
                    // does: the next suspension point after the token fires
                    // must surface `Cancelled`.
                    if cancel.is_cancelled() {
                        self.prof_drain_open_calls(
                            &mut thread.vm,
                            bex_events::prof::record::FunctionEndStatus::Cancelled,
                        );
                        if let Some(future_id) = thread.vm_thread_settles_future() {
                            self.settle_child_cancelled(&mut thread, future_id).await?;
                            return Ok(ThreadOutcome::SettledChild(ChildSettleKind::Cancelled));
                        }
                        return Err(cancelled_unhandled_throw());
                    }
                    #[allow(clippy::items_after_statements)]
                    enum AwaitAnyOutcome {
                        Cancelled,
                        Done(Result<(), EngineError>),
                    }
                    // Build one SetOnce waiter per pending input. Each waiter
                    // is self-contained (clones the future's `Arc<SetOnce>`),
                    // so we can drop the guard and release the permit before
                    // parking — same safepoint discipline as `Await`.
                    let waiters = {
                        let mut g = self.futures.acquire(thread.proof()).await;
                        let mut ws = Vec::with_capacity(future_ids.len());
                        for future_id in &future_ids {
                            ws.push(g.future_ready(*future_id)?);
                        }
                        ws
                    };
                    let prof_await = thread.vm.root_profiler.is_active().then(|| {
                        (
                            thread.vm.prof_current_call_id(),
                            bex_events::prof::clock::now_ticks(),
                        )
                    });
                    let inactive = thread.release();
                    self.maybe_collect_garbage().await;
                    let outcome = if waiters.is_empty() {
                        // Empty INPUT array: `future_ready` returns a waiter
                        // for every id — already-settled inputs yield
                        // immediately-ready waiters — so empty waiters ⟺
                        // empty `future_ids`. Per BEP-034 (matching JS
                        // `Promise.race([])`), racing an empty array never
                        // settles: park on cancellation rather than busy-spin
                        // or panic in `select_all` (which rejects an empty
                        // iterator).
                        cancel.cancelled().await;
                        AwaitAnyOutcome::Cancelled
                    } else {
                        let first_settled =
                            futures::future::select_all(waiters.into_iter().map(Box::pin));
                        tokio::select! {
                            biased;
                            () = cancel.cancelled() => AwaitAnyOutcome::Cancelled,
                            (r, _idx, _rest) = first_settled => AwaitAnyOutcome::Done(r),
                        }
                    };
                    thread = inactive.acquire().await;
                    if let Some((call_id, start_ticks)) = prof_await {
                        let end_ticks = bex_events::prof::clock::now_ticks();
                        self.prof_charge_await(&mut thread.vm, call_id, start_ticks, end_ticks);
                    }
                    // Re-snapshot post-await (D5a): the task may be on a different
                    // OS thread now, and engine-driven VM re-entries before the
                    // next loop-head refresh (inject_sysop_throw's unwind) push
                    // through this snapshot.
                    self.prof_refresh_vm_ring(&mut thread.vm);
                    match outcome {
                        AwaitAnyOutcome::Cancelled => {
                            self.prof_drain_open_calls(
                                &mut thread.vm,
                                bex_events::prof::record::FunctionEndStatus::Cancelled,
                            );
                            if let Some(future_id) = thread.vm_thread_settles_future() {
                                self.settle_child_cancelled(&mut thread, future_id).await?;
                                return Ok(ThreadOutcome::SettledChild(ChildSettleKind::Cancelled));
                            }
                            return Err(cancelled_unhandled_throw());
                        }
                        // Only an internal-error future surfaces here; normal
                        // BAML success/throw/cancel settles resolve the SetOnce
                        // with `Ok(())` and are observed when the VM re-reads
                        // the future (and the stdlib `await futures[i]` it).
                        AwaitAnyOutcome::Done(r) => r?,
                    }
                }

                VmExecState::Event {
                    event_name,
                    data,
                    source_location,
                } => {
                    if event_name == "$baml_log" {
                        self.capture_baml_log_event(
                            &thread,
                            log_capture.as_ref(),
                            data,
                            source_location,
                        );
                    }
                    // Only reserved `$baml_log` events are currently produced
                    // by the standard library. `SendEvent` pops its two
                    // arguments but does not push a return value, so push null
                    // before the VM resumes at the next instruction.
                    thread.vm.stack.push(Value::NULL);
                }

                VmExecState::EarlyYield => {
                    let prof_await = thread.vm.root_profiler.is_active().then(|| {
                        (
                            thread.vm.prof_current_call_id(),
                            bex_events::prof::clock::now_ticks(),
                        )
                    });
                    thread = self.gc_safepoint(thread).await;
                    if let Some((call_id, start_ticks)) = prof_await {
                        let end_ticks = bex_events::prof::clock::now_ticks();
                        self.prof_charge_await(&mut thread.vm, call_id, start_ticks, end_ticks);
                    }
                }
            }
        }
    }

    /// Execute a system operation via uniform dispatch through function pointers.
    ///
    /// All `sys_ops` (including LLM ops) go through the `SysOps` function pointer table.
    /// No more special-case matching — adding a new `#[sys_op]` in the DSL automatically
    /// gets dispatched here via the generated `SysOps::get()`.
    ///
    /// A per-call context is created by cloning the shared `sys_op_ctx` with the
    /// call's cancellation token. This is O(1) since all fields are `Arc`-wrapped.
    #[allow(clippy::too_many_arguments)] // Interpreter state is explicit at this dispatch boundary.
    fn execute_sys_op(
        self: &Arc<Self>,
        op: SysOp,
        args: &[BexExternalValue],
        runtime_type_overlay: &RuntimeTypeOverlay,
        call_id: CallId,
        cancel: &CancellationToken,
        permit: bex_heap::PermitProof<'_>,
        runtime_schema: Option<&RuntimeSchemaOverlay>,
    ) -> SysOpResult {
        fn check(op: SysOp, err: &OpError) {
            if let sys_types::OpErrorPayload::Vm(kind) = &err.payload {
                if let Err(violation) = sys_types::validate_sys_op_error(op, kind) {
                    tracing::warn!("{violation}");
                }
            }
        }

        if op == SysOp::BamlSysCollectGarbage {
            // A collection cannot run while this VM holds its active heap
            // permit. Returning an async operation makes the event loop release
            // that permit before polling us; the engine can then park every VM,
            // collect, drain `cleanup()` finalizers, and only afterward resume
            // the caller.
            let engine = self.clone();
            return SysOpResult::Async(Box::pin(async move {
                engine
                    .collect_garbage(bex_heap::CollectionLevel::Major)
                    .await;
                Ok(BexExternalValue::Null)
            }));
        }
        let args = args.iter().map(std::convert::Into::into).collect();
        let fn_ptr = self.sys_ops.get(op);
        let mut ctx = self.sys_op_ctx.to_op_context(cancel.clone(), self.clone());
        if let Some(runtime_schema) = runtime_schema {
            let mut classes = ctx.class_definitions.as_ref().clone();
            classes.extend(runtime_schema.classes.clone());
            ctx.class_definitions = Arc::new(classes);
            let mut enums = ctx.enum_definitions.as_ref().clone();
            enums.extend(runtime_schema.enums.clone());
            ctx.enum_definitions = Arc::new(enums);
        }
        if !runtime_type_overlay.class_definitions.is_empty() {
            let mut classes = ctx.class_definitions.as_ref().clone();
            classes.extend(runtime_type_overlay.class_definitions.clone());
            ctx.class_definitions = Arc::new(classes);
        }
        if !runtime_type_overlay.enum_definitions.is_empty() {
            let mut enums = ctx.enum_definitions.as_ref().clone();
            enums.extend(runtime_type_overlay.enum_definitions.clone());
            ctx.enum_definitions = Arc::new(enums);
        }
        // Rebuild RuntimeIo with the live per-call context so IO calls
        // (media resolution, auth) use the correct cancellation token.
        ctx.runtime_io =
            sys_ops::build_runtime_io(&self.sys_ops, &self.heap, &self.heap_permit_manager, &ctx);
        let result = fn_ptr(&self.heap, permit, args, &ctx, call_id);

        // `validate_sys_op_error` only applies to the `Vm` payload variant —
        // a `HostThrown` payload is checked engine-side against the
        // surrounding callable's declared `E`, not against the sysop's
        // category contract, so it short-circuits here.
        match result {
            SysOpResult::Ready(Ok(v)) => SysOpResult::Ready(Ok(v)),
            SysOpResult::Ready(Err(err)) => {
                check(op, &err);
                SysOpResult::Ready(Err(err))
            }
            SysOpResult::Async(fut) => {
                let boxed = Box::pin(async move {
                    let res = fut.await;
                    if let Err(err) = &res {
                        check(op, err);
                    }
                    res
                });
                SysOpResult::Async(boxed)
            }
        }
    }

    fn runtime_compile_request(
        vm: &BexVm,
        args: &[Value],
    ) -> Result<bex_vm_types::RuntimeCompileRequest, EngineError> {
        fn type_mount(
            vm: &BexVm,
            export_name: &str,
            ptr: bex_vm_types::HeapPtr,
        ) -> Result<bex_vm_types::RuntimeTypeMount, EngineError> {
            let Object::Type(value) = vm.get_object(ptr) else {
                return Err(EngineError::TypeMismatch {
                    message: format!("with_types entry `{export_name}` is not a type"),
                });
            };
            let identity_name = match &value.ty {
                baml_type::RealizedTy::Class(qtn, _, _) | baml_type::RealizedTy::Enum(qtn, _) => {
                    qtn.clone()
                }
                _ => {
                    // A hand-rolled name in the hidden runtime namespace, for a
                    // mounted type that has no qualified name of its own (a
                    // list, a union, …). `baml_type::RUNTIME_MINT_NAMESPACE` is
                    // the convention's source of truth — the segment below is
                    // it, spelled out because this name is not one this crate
                    // may hand to `to_runtime_local`.
                    //
                    // The discriminator here is `r-<n>`/`s-<n>`, not the bare
                    // `<n>` a minted declaration carries, so that a mounted
                    // type can never collide with one. `has_runtime_mint`
                    // parses that segment as a `u64` and therefore never
                    // matches these — which is correct: this name is a mount
                    // key, not a declaration's identity.
                    let suffix = match value.mint() {
                        bex_vm_types::types::MintId::Runtime(n) => format!("r-{n}"),
                        bex_vm_types::types::MintId::Static(n) => format!("s-{n}"),
                    };
                    baml_type::QualifiedTypeName::new(
                        baml_type::Name::new("user"),
                        vec![
                            baml_type::Name::new(baml_type::RUNTIME_MINT_NAMESPACE),
                            baml_type::Name::new(suffix),
                        ],
                        baml_type::Name::new(export_name),
                    )
                }
            };
            let classes = value
                .defs()
                .classes
                .iter()
                .filter_map(|(qtn, ptr)| {
                    let Object::Class(class) = vm.get_object(*ptr) else {
                        return None;
                    };
                    Some(bex_vm_types::RuntimeMountedClass {
                        qtn: qtn.clone(),
                        fields: class
                            .fields
                            .iter()
                            .map(|field| {
                                (
                                    baml_type::Name::new(&field.name),
                                    baml_type::Ty::from(&field.field_type),
                                    bex_vm_types::RuntimeMountedFieldAttrs {
                                        alias: field.alias.clone(),
                                        description: field.description.clone(),
                                    },
                                )
                            })
                            .collect(),
                    })
                })
                .collect();
            let enums = value
                .defs()
                .enums
                .iter()
                .filter_map(|(qtn, ptr)| {
                    let Object::Enum(enm) = vm.get_object(*ptr) else {
                        return None;
                    };
                    Some(bex_vm_types::RuntimeMountedEnum {
                        qtn: qtn.clone(),
                        variants: enm
                            .variants
                            .iter()
                            .map(|variant| baml_type::Name::new(&variant.name))
                            .collect(),
                    })
                })
                .collect();
            let witnesses = value
                .defs()
                .witnesses
                .iter()
                .map(|witness| {
                    (
                        baml_type::Interface::new(
                            witness.interface.clone(),
                            witness
                                .interface_args
                                .iter()
                                .map(baml_type::Ty::from)
                                .collect(),
                            witness
                                .associated_types
                                .iter()
                                .map(|(name, ty)| (name.clone(), baml_type::Ty::from(ty)))
                                .collect(),
                        ),
                        witness.field_links.clone(),
                    )
                })
                .collect();
            Ok(bex_vm_types::RuntimeTypeMount {
                export_name: baml_type::Name::new(export_name),
                identity_name,
                ty: value.ty.clone(),
                classes,
                enums,
                witnesses,
            })
        }

        fn map_entries(
            vm: &BexVm,
            value: Value,
        ) -> Result<IndexMap<bex_str::BexStr, Value>, EngineError> {
            let Some(ptr) = value.as_object_ptr() else {
                return Err(EngineError::TypeMismatch {
                    message: "Package.compile expected a map".to_string(),
                });
            };
            let Object::Map(map) = vm.get_object(ptr) else {
                return Err(EngineError::TypeMismatch {
                    message: "Package.compile expected a map".to_string(),
                });
            };
            Ok(map.to_index_map())
        }

        let files_value = args
            .first()
            .copied()
            .ok_or_else(|| EngineError::TypeMismatch {
                message: "Package.compile is missing files".to_string(),
            })?;
        let packages_value = args
            .get(1)
            .copied()
            .ok_or_else(|| EngineError::TypeMismatch {
                message: "Package.compile is missing packages".to_string(),
            })?;
        let mut files = IndexMap::new();
        for (path, value) in map_entries(vm, files_value)? {
            let Some(ptr) = value.as_object_ptr() else {
                return Err(EngineError::TypeMismatch {
                    message: format!("Package.compile file `{path}` must be a string"),
                });
            };
            let Object::String(source) = vm.get_object(ptr) else {
                return Err(EngineError::TypeMismatch {
                    message: format!("Package.compile file `{path}` must be a string"),
                });
            };
            files.insert(path.to_string(), source.to_string());
        }

        let mut packages = IndexMap::new();
        for (alias, value) in map_entries(vm, packages_value)? {
            let Some(wrapper_ptr) = value.as_object_ptr() else {
                return Err(EngineError::TypeMismatch {
                    message: format!("Package.compile dependency `{alias}` must be a Package"),
                });
            };
            let package_ptr = match vm.get_object(wrapper_ptr) {
                Object::Package(_) => wrapper_ptr,
                Object::Instance(wrapper) => {
                    wrapper.load_field(0).as_object_ptr().ok_or_else(|| {
                        EngineError::TypeMismatch {
                            message: format!(
                                "Package.compile dependency `{alias}` is not initialized"
                            ),
                        }
                    })?
                }
                _ => {
                    return Err(EngineError::TypeMismatch {
                        message: format!("Package.compile dependency `{alias}` must be a Package"),
                    });
                }
            };
            let Object::Package(package) = vm.get_object(package_ptr) else {
                return Err(EngineError::TypeMismatch {
                    message: format!("Package.compile dependency `{alias}` is not initialized"),
                });
            };
            let types = package
                .mounted_types
                .iter()
                .map(|(name, ptr)| type_mount(vm, name, *ptr))
                .collect::<Result<Vec<_>, _>>()?;
            packages.insert(
                alias.to_string(),
                bex_vm_types::RuntimePackageMount {
                    interface_blob: package.interface_blob.clone(),
                    types,
                },
            );
        }
        Ok(bex_vm_types::RuntimeCompileRequest {
            files,
            packages,
            mode: bex_vm_types::RuntimeCompileMode::Package,
        })
    }

    fn runtime_session_compile_request(
        vm: &mut BexVm,
        args: &[Value],
    ) -> Result<bex_vm_types::RuntimeCompileRequest, OpError> {
        let operation = SysOp::BamlReflectSessionCompile;
        let invalid = |message: String| {
            OpError::new(
                operation,
                bex_vm_types::errors::VmBamlError::InvalidArgument { message },
            )
        };
        let receiver = args
            .first()
            .copied()
            .ok_or_else(|| invalid("Session.eval is missing its receiver".to_string()))?;
        let wrapper_ptr = receiver
            .as_object_ptr()
            .ok_or_else(|| invalid("Session.eval receiver is not an instance".to_string()))?;
        let package_ptr = match vm.get_object(wrapper_ptr) {
            Object::Package(_) => wrapper_ptr,
            Object::Instance(wrapper) => wrapper
                .load_field(0)
                .as_object_ptr()
                .ok_or_else(|| invalid("Session is not initialized".to_string()))?,
            _ => {
                return Err(invalid(
                    "Session.eval receiver is not an instance".to_string(),
                ));
            }
        };
        let source_ptr = args
            .get(1)
            .and_then(Value::as_object_ptr)
            .ok_or_else(|| invalid("Session.eval source is not a string".to_string()))?;
        let Object::String(source) = vm.get_object(source_ptr) else {
            return Err(invalid("Session.eval source is not a string".to_string()));
        };
        let source = source.to_string();
        let expected = host_call_type_arg(args.get(2).copied(), 2, "eval type contract")
            .map_err(|error| OpError::new(operation, error))?;

        let busy = {
            let Object::Package(package) = vm.get_object(package_ptr) else {
                return Err(invalid(
                    "Session has an invalid runtime payload".to_string(),
                ));
            };
            package
                .session
                .as_ref()
                .map(|state| state.busy.clone())
                .ok_or_else(|| invalid("Session has an invalid runtime payload".to_string()))?
        };
        let Some(lease) = bex_vm_types::SessionEvalLease::acquire(busy) else {
            return Err(OpError::host_thrown_value(
                operation,
                BexExternalValue::Instance {
                    class_name: "baml.reflect.errors.SessionBusy".to_string(),
                    type_args: Vec::new(),
                    fields: indexmap::indexmap! {
                        "message".to_string() => BexExternalValue::String(
                            "a Session permits only one active eval".into(),
                        ),
                    },
                },
            ));
        };
        let (history, visible, dependency_names, sequence) = {
            let Object::Package(package) = vm.get_object_mut(package_ptr) else {
                return Err(invalid(
                    "Session has an invalid runtime payload".to_string(),
                ));
            };
            let state = package
                .session
                .as_mut()
                .ok_or_else(|| invalid("Session has an invalid runtime payload".to_string()))?;
            let sequence = state.submission_counter;
            state.submission_counter = state.submission_counter.saturating_add(1);
            let dependencies = package
                .runtime
                .as_ref()
                .map(|runtime| runtime.dependency_names.clone())
                .unwrap_or_default();
            (
                state.history.clone(),
                state.visible.clone(),
                dependencies,
                sequence,
            )
        };
        let mut packages = IndexMap::new();
        for (alias, ptr) in dependency_names {
            let Object::Package(package) = vm.get_object(ptr) else {
                return Err(invalid(format!(
                    "Session dependency `{alias}` has an invalid runtime payload"
                )));
            };
            packages.insert(
                alias,
                bex_vm_types::RuntimePackageMount {
                    interface_blob: package.interface_blob.clone(),
                    types: Vec::new(),
                },
            );
        }
        let submission_name = format!("$submission_{sequence}.baml");
        let session = bex_vm_types::RuntimeSessionCompileRequest {
            submission_name,
            source,
            history,
            visible,
            expected,
            lease,
        };
        Ok(bex_vm_types::RuntimeCompileRequest {
            files: IndexMap::new(),
            packages,
            mode: bex_vm_types::RuntimeCompileMode::Session(Box::new(session)),
        })
    }

    fn execute_runtime_compile(
        &self,
        request: bex_vm_types::RuntimeCompileRequest,
        operation: SysOp,
    ) -> SysOpResult {
        fn string(value: impl Into<String>) -> BexExternalValue {
            BexExternalValue::String(value.into().into())
        }
        fn diagnostic(value: bex_vm_types::RuntimeCompileDiagnostic) -> BexExternalValue {
            let span =
                value
                    .span
                    .map_or(BexExternalValue::Null, |span| BexExternalValue::Instance {
                        class_name: "baml.reflect.Span".to_string(),
                        type_args: Vec::new(),
                        fields: indexmap::indexmap! {
                            "file".to_string() => string(span.file),
                            "start".to_string() => BexExternalValue::Int(i64::try_from(span.start).expect("source offsets fit BAML int")),
                            "end".to_string() => BexExternalValue::Int(i64::try_from(span.end).expect("source offsets fit BAML int")),
                        },
                    });
            BexExternalValue::Instance {
                class_name: "baml.reflect.Diagnostic".to_string(),
                type_args: Vec::new(),
                fields: indexmap::indexmap! {
                    "code".to_string() => string(value.code),
                    "message".to_string() => string(value.message),
                    "span".to_string() => span,
                },
            }
        }

        let compiler = self.runtime_compiler.clone();
        SysOpResult::Async(Box::pin(async move {
            let Some(compiler) = compiler else {
                return Err(OpError::new(
                    operation,
                    bex_vm_types::errors::VmBamlError::Unsupported {
                        message: "runtime compiler was not installed by the host".to_string(),
                    },
                ));
            };
            match compiler.compile(request) {
                Ok(artifact) => Ok(BexExternalValue::Instance {
                    class_name: "baml.reflect.Package".to_string(),
                    type_args: Vec::new(),
                    fields: indexmap::indexmap! {
                        "_inner".to_string() => BexExternalValue::RustData(Arc::new(artifact)),
                    },
                }),
                Err(diagnostics) => {
                    let message = diagnostics
                        .iter()
                        .find(|diagnostic| {
                            diagnostic.severity == bex_vm_types::RuntimeDiagnosticSeverity::Error
                        })
                        .map_or_else(
                            || "runtime compilation failed".to_string(),
                            |diagnostic| diagnostic.message.clone(),
                        );
                    let items = diagnostics.into_iter().map(diagnostic).collect();
                    Err(OpError::host_thrown_value(
                        operation,
                        BexExternalValue::Instance {
                            class_name: "baml.reflect.errors.CompilationError".to_string(),
                            type_args: Vec::new(),
                            fields: indexmap::indexmap! {
                                "message".to_string() => string(message),
                                "diagnostics".to_string() => BexExternalValue::Array {
                                    element_type: baml_type::RuntimeTy::unknown(),
                                    items,
                                },
                            },
                        },
                    ))
                }
            }
        }))
    }
}

#[async_trait]
impl sys_types::VmSpawner for BexEngine {
    async fn spawn_with_function(
        self: Arc<Self>,
        function_name: String,
        args: Vec<BexExternalValue>,
        cancel: CancellationToken,
    ) -> Result<BexExternalValue, Box<dyn Send + Sync + 'static>> {
        self.call_function(
            &function_name,
            args,
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_cancel_token(cancel)
                .build(),
            true,
        )
        .await
        .map_err(|e| Box::new(e) as Box<dyn Send + Sync + 'static>)
    }

    async fn spawn_with_callable(
        self: Arc<Self>,
        callable: bex_external_types::Handle,
        args: Vec<BexExternalValue>,
        cancel: CancellationToken,
    ) -> Result<BexExternalValue, Box<dyn Send + Sync + 'static>> {
        self.call_callable(
            callable,
            args,
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_cancel_token(cancel)
                .build(),
            true,
        )
        .await
        .map_err(|e| Box::new(e) as Box<dyn Send + Sync + 'static>)
    }
}

// Coverage for sysop error → catchable throw routing now lives in
// the `host_value_callable` engine integration test (end-to-end via the VM),
// which exercises every `VmRustFnError` variant routed through
// `op_error_to_throw_value` + `inject_sysop_throw`. The
// `sys_types::tests::contract_*` unit tests pin the contract-category
// mapping; `error_to_exception_value` / `panic_to_exception_value` in
// `bex_vm::vm` are exercised transitively by the same integration test
// (no dedicated `bex_vm` unit test today — a follow-up could add one if
// the integration-only coverage proves slow to diagnose regressions).
//
// The former `call_host_value_tests` module asserted the same shape against
// the now-deleted `op_error_to_catchable_throw` helper; removing it avoids
// duplicating the same expectations across crates.

#[cfg(test)]
mod concurrent_tests {
    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn cancelling_pending_gc_park_request_clears_vm_flag() {
        use std::{
            sync::{
                Arc,
                atomic::{AtomicBool, Ordering},
            },
            time::Duration,
        };

        use super::ParkRequestGuard;
        use crate::HeapPermitManager;

        let permit_manager = Arc::new(HeapPermitManager::new());
        let active_permit = permit_manager.new_permit(()).await.acquire().await;
        let park_requested = Arc::new(AtomicBool::new(false));

        let request = {
            let permit_manager = Arc::clone(&permit_manager);
            let park_requested = Arc::clone(&park_requested);
            tokio::spawn(async move {
                let park_request_guard = ParkRequestGuard::new(park_requested);
                let _heap_guard = permit_manager.request_park().await;
                drop(park_request_guard);
            })
        };

        tokio::time::timeout(Duration::from_secs(1), async {
            while !park_requested.load(Ordering::Relaxed) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("GC request should set the VM park flag");
        assert!(
            !request.is_finished(),
            "the active heap permit should keep the GC request pending"
        );

        request.abort();
        let join_error = request
            .await
            .expect_err("aborted GC request should not complete normally");
        assert!(join_error.is_cancelled());
        assert!(
            !park_requested.load(Ordering::Relaxed),
            "cancelling the GC request must clear the VM park flag"
        );

        drop(active_permit);
    }

    /// Test that demonstrates concurrent `call_function` is safe.
    /// This test verifies that:
    /// 1. Multiple concurrent calls complete successfully
    /// 2. Each call gets its own VM with its own TLAB
    /// 3. No data races occur during parallel execution
    #[tokio::test]
    async fn test_concurrent_calls_safe() {
        // Note: This requires a test BAML program to be available
        // Skip if test infrastructure not set up
        if std::env::var("BAML_TEST_CONCURRENT").is_err() {
            return;
        }

        // This test is a placeholder demonstrating the concurrent execution pattern.
        // In a real implementation, you would:
        // 1. Compile a test BAML program to bytecode
        // 2. Create a BexEngine from the bytecode
        // 3. Wrap it in Arc and spawn concurrent calls
        // 4. Verify all calls complete successfully
        //
        // Example (when test infrastructure is ready):
        // ```
        // let engine = /* create test engine */;
        // let engine = Arc::new(engine);
        //
        // // Spawn 10 concurrent calls
        // let mut handles = vec![];
        // for i in 0..10 {
        //     let engine = Arc::clone(&engine);
        //     handles.push(tokio::spawn(async move {
        //         // Each call should succeed independently
        //         let args = vec![ExternalValue::Int(i)];
        //         engine.call_function("identity", &args).await
        //     }));
        // }
        //
        // // All should complete successfully
        // for handle in handles {
        //     let result = handle.await.expect("task panicked");
        //     assert!(result.is_ok(), "concurrent call failed: {:?}", result);
        // }
        // ```
    }
}

#[cfg(test)]
mod mint_identity_tests {
    use std::sync::Arc;

    use baml_project::testing::compile_source;
    use bex_vm_types::{Object, types::MintId};
    use sys_native::SysOpsExt;
    use tokio_util::sync::CancellationToken;

    use super::BexEngine;

    fn engine() -> Arc<BexEngine> {
        let program = compile_source("function main() -> null { null }");
        Arc::new(
            BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new())
                .expect("engine construction should succeed"),
        )
    }

    async fn mint_in_engine(engine: &Arc<BexEngine>, ty: baml_type::RealizedTy) -> MintId {
        let mut thread = engine.new_root_thread(CancellationToken::new()).await;
        let ptr = thread.vm.alloc_static_type(ty);
        let Object::Type(type_value) = thread.vm.get_object(ptr) else {
            panic!("alloc_static_type must allocate Object::Type")
        };
        type_value.mint()
    }

    #[tokio::test]
    async fn static_digest_is_canonical_and_deterministic_across_engines() {
        let left = baml_type::RealizedTy::Union(
            vec![
                baml_type::RealizedTy::int(),
                baml_type::RealizedTy::string(),
            ],
            baml_type::TyAttr::default(),
        );
        let right = baml_type::RealizedTy::Union(
            vec![
                baml_type::RealizedTy::string(),
                baml_type::RealizedTy::int(),
            ],
            baml_type::TyAttr::default(),
        );

        let first = mint_in_engine(&engine(), left).await;
        let second = mint_in_engine(&engine(), right).await;
        assert_eq!(first, second);
        assert!(matches!(first, MintId::Static(_)));
    }
}
