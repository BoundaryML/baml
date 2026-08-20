use std::sync::{Arc, OnceLock};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

#[cfg(not(target_arch = "wasm32"))]
use super::decoder::{BoundaryRuntime, DecoderResources, DirectDecoder, finalize_ready_boundary};
#[cfg(not(target_arch = "wasm32"))]
use super::{AdmittedBoundary, BeginBoundaryResult, BoundaryRunMeta, ProfilerStore};
use super::{
    BoundaryHandle, BoundaryMetadata, BoundaryRegistry, CapturePlan, DerivedSizing,
    ErrorCaptureAttempt, ErrorCaptureId, FunctionCaptureClass, LocalIdOverrides, MeasuredLayouts,
    Owner, ProfilerConfig, ProfilerMemoryGovernor, ProfilerSizingPolicy, Reservation,
    ReservationClass, RootBoundaryCompletionGuard, TerminalErrorTarget, ValueLossReason, ValueRole,
    ValueState, resolve_capture_plan,
};
use crate::ids::{BoundaryId, ProgramId, ThreadRef};

const DECODER_LOCK_BATCH_RECORDS: u16 = 256;

/// The only per-root admission intents. Internal work must be explicit rather
/// than mutating a generic profiling boolean after construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootProfileIntent {
    UserBoundary { boundary_id: BoundaryId },
    SuppressInternal,
}

/// Why a root has no profiler state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InactiveReason {
    Disabled,
    Suppressed,
    InvalidMemoryBudget,
    BoundaryStateUnavailable,
    ThreadLeaseUnavailable,
    BoundaryStoreUnavailable,
    BoundaryStoreIndeterminate,
}

/// Host-visible setup diagnostic. Session construction returns at most one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetupDiagnostic {
    pub message: String,
}

/// Immutable, state-free adapter shared by every hook in an inactive root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootProfiler {
    Inactive(InactiveReason),
    Active(ActiveRootProfiler),
}

impl RootProfiler {
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Active(_))
    }

    /// Active-only capture policy. Inactive callers bypass resolution and get
    /// no exact selection; `LocalId` consumption remains a language operation at
    /// the VM call site before this adapter is consulted.
    #[must_use]
    pub fn resolve_capture_plan(
        self,
        is_boundary_root: bool,
        capture_class: FunctionCaptureClass,
        local_id: Option<LocalIdOverrides>,
    ) -> CapturePlan {
        match self {
            Self::Inactive(_) => CapturePlan::default(),
            Self::Active(_) => resolve_capture_plan(is_boundary_root, capture_class, local_id),
        }
    }
}

/// Small immutable root token. Boundary registration/control ownership is
/// added behind this type in Phase 2/3 without widening VM hook interfaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActiveRootProfiler {
    pub boundary_id: BoundaryId,
}

#[derive(Debug)]
struct OnSession {
    config: ProfilerConfig,
    sizing: DerivedSizing,
    memory: ProfilerMemoryGovernor,
    boundaries: Arc<BoundaryRegistry>,
    #[cfg(not(target_arch = "wasm32"))]
    store: Arc<ProfilerStore>,
    #[cfg(not(target_arch = "wasm32"))]
    publishers: Box<[Mutex<Option<BoundaryRuntime>>]>,
    #[cfg(not(target_arch = "wasm32"))]
    decoder: Mutex<DirectDecoder>,
    #[cfg(not(target_arch = "wasm32"))]
    decoder_producer_waiters: AtomicUsize,
    clock: crate::prof::clock::TickConverter,
}

#[derive(Debug)]
pub enum RootAdmission {
    Inactive(RootProfiler),
    #[cfg(not(target_arch = "wasm32"))]
    Active(ActiveRootAdmission),
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
pub struct ActiveRootAdmission {
    pub profiler: RootProfiler,
    pub completion: RootBoundaryCompletionGuard,
}

impl RootAdmission {
    #[must_use]
    pub const fn profiler(&self) -> RootProfiler {
        match self {
            Self::Inactive(profiler) => *profiler,
            #[cfg(not(target_arch = "wasm32"))]
            Self::Active(admission) => admission.profiler,
        }
    }

    #[must_use]
    pub const fn boundary_handle(&self) -> Option<BoundaryHandle> {
        match self {
            Self::Inactive(_) => None,
            #[cfg(not(target_arch = "wasm32"))]
            Self::Active(admission) => Some(admission.completion.lease().handle()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AwaitClockInvalid;

/// Process/store session selected once and injected into engines.
///
/// `Off` is a unit variant: it has no ring, consumer, heap, lock, path, clock,
/// lazy initializer, or synchronization field.
#[derive(Debug)]
enum SessionKind {
    Off,
    On(Box<OnSession>),
}

#[derive(Debug)]
pub struct ProfilerSession {
    kind: SessionKind,
}

impl ProfilerSession {
    /// Constructs a session from the five-input configuration. Invalid memory
    /// budgets fail to the state-free off variant and one diagnostic.
    #[must_use]
    pub fn from_config(config: ProfilerConfig) -> (Arc<Self>, Option<SetupDiagnostic>) {
        if !config.enabled {
            return (
                Arc::new(Self {
                    kind: SessionKind::Off,
                }),
                None,
            );
        }
        #[cfg(target_arch = "wasm32")]
        return (
            Arc::new(Self {
                kind: SessionKind::Off,
            }),
            Some(SetupDiagnostic {
                message: "profiling disabled: the local profiling store is unavailable on wasm32"
                    .to_string(),
            }),
        );
        #[cfg(not(target_arch = "wasm32"))]
        match ProfilerSizingPolicy::derive(config.process_memory_bytes, MeasuredLayouts::V1) {
            Ok(sizing) => {
                let memory = ProfilerMemoryGovernor::new(sizing, MeasuredLayouts::V1);
                let boundaries = match BoundaryRegistry::new(
                    sizing.boundary_slots,
                    MeasuredLayouts::V1.boundary_slot_bytes,
                    &memory,
                ) {
                    Ok(boundaries) => boundaries,
                    Err(denied) => {
                        return (
                            Arc::new(Self {
                                kind: SessionKind::Off,
                            }),
                            Some(SetupDiagnostic {
                                message: format!(
                                    "profiling disabled: stable boundary registry requested {} control bytes with {} available",
                                    denied.requested_bytes, denied.available_bytes
                                ),
                            }),
                        );
                    }
                };
                #[cfg(not(target_arch = "wasm32"))]
                let store = match ProfilerStore::open_native(config.store_root.clone(), config.disk)
                {
                    Ok(store) => store,
                    Err(error) => {
                        return (
                            Arc::new(Self {
                                kind: SessionKind::Off,
                            }),
                            Some(SetupDiagnostic {
                                message: format!("profiling disabled: {error}"),
                            }),
                        );
                    }
                };
                #[cfg(not(target_arch = "wasm32"))]
                let publishers = (0..sizing.boundary_slots)
                    .map(|_| Mutex::new(None))
                    .collect::<Vec<_>>()
                    .into_boxed_slice();
                let clock = crate::prof::clock::TickConverter::from_clock();
                (
                    Arc::new(Self {
                        kind: SessionKind::On(Box::new(OnSession {
                            config,
                            sizing,
                            memory,
                            boundaries,
                            #[cfg(not(target_arch = "wasm32"))]
                            store,
                            #[cfg(not(target_arch = "wasm32"))]
                            publishers,
                            #[cfg(not(target_arch = "wasm32"))]
                            decoder: Mutex::new(DirectDecoder::default()),
                            #[cfg(not(target_arch = "wasm32"))]
                            decoder_producer_waiters: AtomicUsize::new(0),
                            clock,
                        })),
                    }),
                    None,
                )
            }
            Err(error) => (
                Arc::new(Self {
                    kind: SessionKind::Off,
                }),
                Some(SetupDiagnostic {
                    message: format!(
                        "profiling disabled: process memory budget {} is below minimum {}",
                        error.requested_bytes, error.minimum_bytes
                    ),
                }),
            ),
        }
    }

    /// Shared environment-selected session. Resource policy uses the frozen
    /// defaults; embedders needing another store root inject a session.
    #[must_use]
    pub fn global() -> &'static Arc<Self> {
        static GLOBAL: OnceLock<Arc<ProfilerSession>> = OnceLock::new();
        GLOBAL.get_or_init(|| {
            let config = ProfilerConfig {
                enabled: crate::prof::ProfConfig::global().is_enabled(),
                ..ProfilerConfig::default()
            };
            Self::from_config(config).0
        })
    }

    #[must_use]
    pub const fn is_on(&self) -> bool {
        matches!(self.kind, SessionKind::On(_))
    }

    #[must_use]
    pub fn config(&self) -> Option<&ProfilerConfig> {
        match &self.kind {
            SessionKind::Off => None,
            SessionKind::On(session) => Some(&session.config),
        }
    }

    #[must_use]
    pub const fn sizing(&self) -> Option<DerivedSizing> {
        match &self.kind {
            SessionKind::Off => None,
            SessionKind::On(session) => Some(session.sizing),
        }
    }

    #[must_use]
    pub fn memory(&self) -> Option<&ProfilerMemoryGovernor> {
        match &self.kind {
            SessionKind::Off => None,
            SessionKind::On(session) => Some(&session.memory),
        }
    }

    #[must_use]
    pub fn boundary_registry(&self) -> Option<&Arc<BoundaryRegistry>> {
        match &self.kind {
            SessionKind::Off => None,
            SessionKind::On(session) => Some(&session.boundaries),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn store(&self) -> Option<&Arc<ProfilerStore>> {
        match &self.kind {
            SessionKind::Off => None,
            SessionKind::On(session) => Some(&session.store),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn boundary_publisher(&self, handle: BoundaryHandle) -> Option<Arc<AdmittedBoundary>> {
        let SessionKind::On(session) = &self.kind else {
            return None;
        };
        let slot = session.publishers.get(handle.slot as usize)?;
        let guard = slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let runtime = guard.as_ref()?;
        (runtime.generation == handle.generation).then(|| Arc::clone(&runtime.publisher))
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn checkpoint(&self, handle: BoundaryHandle) -> Option<super::ProfilerCheckpoint> {
        let SessionKind::On(session) = &self.kind else {
            return None;
        };
        DirectDecoder::checkpoint(handle, &session.publishers)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn release_boundary_publisher(&self, handle: BoundaryHandle) {
        let SessionKind::On(session) = &self.kind else {
            return;
        };
        let Some(slot) = session.publishers.get(handle.slot as usize) else {
            return;
        };
        let mut guard = slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if guard
            .as_ref()
            .is_some_and(|runtime| runtime.generation == handle.generation)
        {
            *guard = None;
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn consume_raw_bytes(
        &self,
        process_euid: crate::ids::ProcessEuid,
        engine_id: crate::ids::EngineId,
        bytes: &[u8],
    ) {
        let SessionKind::On(session) = &self.kind else {
            return;
        };
        let resources = DecoderResources {
            process_euid,
            engine_id,
            memory: &session.memory,
            sizing: session.sizing,
            clock: &session.clock,
            boundaries: &session.publishers,
        };
        let mut records = crate::prof::record::iter(bytes);
        loop {
            // Value/error occurrence producers share this mutex. Bound each
            // consumer ownership interval so a selected root completing
            // behind a large structural backlog can enqueue its occurrence
            // without waiting for the entire ring snapshot to fold. Pending
            // joins already make that cross-lane ordering explicit.
            let mut decoder = session
                .decoder
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut consumed = 0_u16;
            while consumed < DECODER_LOCK_BATCH_RECORDS {
                let Some(raw) = records.next() else {
                    return;
                };
                let Ok(raw) = raw else {
                    return;
                };
                decoder.consume(&resources, raw);
                consumed += 1;
            }
            drop(decoder);
            if session.decoder_producer_waiters.load(Ordering::Acquire) != 0 {
                std::thread::yield_now();
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn record_structural_transport_loss(&self, handle: BoundaryHandle) {
        let SessionKind::On(session) = &self.kind else {
            return;
        };
        session.boundaries.record_structural_transport_loss(handle);
    }

    pub fn reserve_value_work(
        &self,
        manual_eligible: bool,
    ) -> Result<Reservation, ValueLossReason> {
        let SessionKind::On(session) = &self.kind else {
            return Err(ValueLossReason::StoreUnavailable);
        };
        session
            .memory
            .try_reserve(
                ReservationClass::General,
                Owner::Values,
                MeasuredLayouts::V1.value_root_min_bytes,
            )
            .or_else(|general_error| {
                if manual_eligible {
                    session.memory.try_reserve(
                        ReservationClass::Manual,
                        Owner::Values,
                        MeasuredLayouts::V1.value_root_min_bytes,
                    )
                } else {
                    Err(general_error)
                }
            })
            .map_err(|_| ValueLossReason::ValueMemoryExceeded)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn reserve_error_attempt(
        &self,
        handle: BoundaryHandle,
        manual_eligible: bool,
    ) -> Option<Reservation> {
        let SessionKind::On(session) = &self.kind else {
            return None;
        };
        self.boundary_publisher(handle)?;
        session
            .memory
            .try_reserve(
                ReservationClass::General,
                Owner::Evidence,
                MeasuredLayouts::V1.evidence_item_min_bytes,
            )
            .or_else(|general_error| {
                if manual_eligible {
                    session.memory.try_reserve(
                        ReservationClass::Manual,
                        Owner::Evidence,
                        MeasuredLayouts::V1.evidence_item_min_bytes,
                    )
                } else {
                    Err(general_error)
                }
            })
            .ok()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn submit_error_attempt(
        &self,
        handle: BoundaryHandle,
        attempt: ErrorCaptureAttempt,
        reservation: Reservation,
    ) {
        let SessionKind::On(session) = &self.kind else {
            return;
        };
        let mut decoder = session
            .decoder
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        decoder.consume_error_attempt(
            &DecoderResources {
                process_euid: attempt.id.thread_ref.process_euid,
                engine_id: attempt.id.thread_ref.engine_id,
                memory: &session.memory,
                sizing: session.sizing,
                clock: &session.clock,
                boundaries: &session.publishers,
            },
            handle,
            attempt,
            reservation,
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn complete_error_value(&self, id: ErrorCaptureId, value: ValueState) {
        let SessionKind::On(session) = &self.kind else {
            return;
        };
        let mut decoder = session
            .decoder
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        decoder.complete_error_value(
            &DecoderResources {
                process_euid: id.thread_ref.process_euid,
                engine_id: id.thread_ref.engine_id,
                memory: &session.memory,
                sizing: session.sizing,
                clock: &session.clock,
                boundaries: &session.publishers,
            },
            id,
            value,
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn submit_terminal_error(
        &self,
        handle: BoundaryHandle,
        call_ref: crate::ids::CallRef,
        target: TerminalErrorTarget,
        reservation: Reservation,
    ) {
        let SessionKind::On(session) = &self.kind else {
            return;
        };
        let mut decoder = session
            .decoder
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        decoder.consume_terminal_error(
            &DecoderResources {
                process_euid: call_ref.process_euid,
                engine_id: call_ref.engine_id,
                memory: &session.memory,
                sizing: session.sizing,
                clock: &session.clock,
                boundaries: &session.publishers,
            },
            handle,
            call_ref,
            target,
            reservation,
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn record_error_attempt_transport_loss(&self, handle: BoundaryHandle) {
        let SessionKind::On(session) = &self.kind else {
            return;
        };
        let Some(publisher) = self.boundary_publisher(handle) else {
            return;
        };
        let meta = publisher.meta();
        DirectDecoder::record_error_attempt_transport_loss(
            &DecoderResources {
                process_euid: meta.root_thread_ref.process_euid,
                engine_id: meta.root_thread_ref.engine_id,
                memory: &session.memory,
                sizing: session.sizing,
                clock: &session.clock,
                boundaries: &session.publishers,
            },
            handle,
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn record_terminal_error_transport_loss(&self, handle: BoundaryHandle) {
        let SessionKind::On(session) = &self.kind else {
            return;
        };
        let Some(publisher) = self.boundary_publisher(handle) else {
            return;
        };
        let meta = publisher.meta();
        DirectDecoder::record_terminal_error_transport_loss(
            &DecoderResources {
                process_euid: meta.root_thread_ref.process_euid,
                engine_id: meta.root_thread_ref.engine_id,
                memory: &session.memory,
                sizing: session.sizing,
                clock: &session.clock,
                boundaries: &session.publishers,
            },
            handle,
        );
    }

    #[must_use]
    pub fn single_value_bytes(&self) -> Option<u64> {
        let SessionKind::On(session) = &self.kind else {
            return None;
        };
        Some(session.sizing.single_value_bytes)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn record_encoded_value(
        &self,
        handle: BoundaryHandle,
        call_ref: crate::ids::CallRef,
        role: ValueRole,
        encoded_body: &[u8],
        manual_eligible: bool,
        mut reservation: Reservation,
    ) {
        let SessionKind::On(session) = &self.kind else {
            return;
        };
        let encoded_bytes = u64::try_from(encoded_body.len()).unwrap_or(u64::MAX);
        let publication_bytes = session
            .store
            .cas_publication_allocation_bound(encoded_bytes);
        let state = if encoded_bytes > session.sizing.single_value_bytes {
            ValueState::Lost(ValueLossReason::ValueTooLarge)
        } else if reservation.try_grow(publication_bytes).is_err() {
            ValueState::Lost(ValueLossReason::ValueMemoryExceeded)
        } else {
            let codec = super::CodecVersion(1);
            let (cid, result) = session.store.publish_cas_object(codec, encoded_body);
            match result {
                super::PublishCasResult::Published | super::PublishCasResult::Reused => {
                    ValueState::Available {
                        cid,
                        codec,
                        encoded_bytes,
                    }
                }
                super::PublishCasResult::Conflict => ValueState::Lost(ValueLossReason::CasConflict),
                super::PublishCasResult::Lost(super::StoreFailureReason::DiskGuardExceeded) => {
                    ValueState::Lost(ValueLossReason::DiskGuardExceeded)
                }
                super::PublishCasResult::Lost(_) => {
                    ValueState::Lost(ValueLossReason::CasWriteFailed)
                }
                super::PublishCasResult::Indeterminate(token) => {
                    if session.store.resolve_indeterminate(token)
                        == super::ResolveIndeterminateResult::Committed
                    {
                        ValueState::Available {
                            cid,
                            codec,
                            encoded_bytes,
                        }
                    } else {
                        ValueState::Lost(ValueLossReason::StoreUnavailable)
                    }
                }
            }
        };
        session
            .decoder_producer_waiters
            .fetch_add(1, Ordering::AcqRel);
        let mut decoder = session
            .decoder
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        session
            .decoder_producer_waiters
            .fetch_sub(1, Ordering::Release);
        decoder.consume_value_occurrence(
            &DecoderResources {
                process_euid: call_ref.process_euid,
                engine_id: call_ref.engine_id,
                memory: &session.memory,
                sizing: session.sizing,
                clock: &session.clock,
                boundaries: &session.publishers,
            },
            handle,
            call_ref,
            role,
            state,
            manual_eligible,
            Some(reservation),
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn record_encoded_error_value(
        &self,
        id: ErrorCaptureId,
        encoded_body: &[u8],
        mut reservation: Reservation,
    ) {
        let SessionKind::On(session) = &self.kind else {
            return;
        };
        let encoded_bytes = u64::try_from(encoded_body.len()).unwrap_or(u64::MAX);
        let publication_bytes = session
            .store
            .cas_publication_allocation_bound(encoded_bytes);
        let state = if encoded_bytes > session.sizing.single_value_bytes {
            ValueState::Lost(ValueLossReason::ValueTooLarge)
        } else if reservation.try_grow(publication_bytes).is_err() {
            ValueState::Lost(ValueLossReason::ValueMemoryExceeded)
        } else {
            let codec = super::CodecVersion(1);
            let (cid, result) = session.store.publish_cas_object(codec, encoded_body);
            match result {
                super::PublishCasResult::Published | super::PublishCasResult::Reused => {
                    ValueState::Available {
                        cid,
                        codec,
                        encoded_bytes,
                    }
                }
                super::PublishCasResult::Conflict => ValueState::Lost(ValueLossReason::CasConflict),
                super::PublishCasResult::Lost(super::StoreFailureReason::DiskGuardExceeded) => {
                    ValueState::Lost(ValueLossReason::DiskGuardExceeded)
                }
                super::PublishCasResult::Lost(_) => {
                    ValueState::Lost(ValueLossReason::CasWriteFailed)
                }
                super::PublishCasResult::Indeterminate(token) => {
                    if session.store.resolve_indeterminate(token)
                        == super::ResolveIndeterminateResult::Committed
                    {
                        ValueState::Available {
                            cid,
                            codec,
                            encoded_bytes,
                        }
                    } else {
                        ValueState::Lost(ValueLossReason::StoreUnavailable)
                    }
                }
            }
        };
        drop(reservation);
        self.complete_error_value(id, state);
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn record_error_value_loss(&self, id: ErrorCaptureId, reason: ValueLossReason) {
        self.complete_error_value(id, ValueState::Lost(reason));
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn record_value_loss(
        &self,
        handle: BoundaryHandle,
        call_ref: crate::ids::CallRef,
        role: ValueRole,
        reason: ValueLossReason,
        manual_eligible: bool,
    ) {
        let SessionKind::On(session) = &self.kind else {
            return;
        };
        session
            .decoder_producer_waiters
            .fetch_add(1, Ordering::AcqRel);
        let mut decoder = session
            .decoder
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        session
            .decoder_producer_waiters
            .fetch_sub(1, Ordering::Release);
        decoder.consume_value_occurrence(
            &DecoderResources {
                process_euid: call_ref.process_euid,
                engine_id: call_ref.engine_id,
                memory: &session.memory,
                sizing: session.sizing,
                clock: &session.clock,
                boundaries: &session.publishers,
            },
            handle,
            call_ref,
            role,
            ValueState::Lost(reason),
            manual_eligible,
            None,
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn maintain_ready_boundaries(&self) {
        let SessionKind::On(session) = &self.kind else {
            return;
        };
        let ready = session.boundaries.ready_handles();
        if ready.is_empty() {
            return;
        }
        let mut decoder = session
            .decoder
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for handle in ready {
            let Some(slot) = session.publishers.get(handle.slot as usize) else {
                continue;
            };
            let mut slot = slot
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let Some(runtime) = slot.as_mut() else {
                continue;
            };
            if runtime.generation != handle.generation {
                continue;
            }
            if finalize_ready_boundary(handle, runtime, &mut decoder, &session.boundaries) {
                *slot = None;
            }
        }
    }

    /// Converts one producer tick interval with the session's immutable clock
    /// line. Off sessions do not sample or convert clocks.
    pub fn elapsed_ns(&self, start_ticks: u64, end_ticks: u64) -> Result<u64, AwaitClockInvalid> {
        let SessionKind::On(session) = &self.kind else {
            return Err(AwaitClockInvalid);
        };
        if end_ticks < start_ticks {
            return Err(AwaitClockInvalid);
        }
        let start_ns = session.clock.to_ns(start_ticks);
        let end_ns = session.clock.to_ns(end_ticks);
        end_ns.checked_sub(start_ns).ok_or(AwaitClockInvalid)
    }

    /// Sole root admission seam. Store/registry failures are introduced here
    /// in their implementation phases; callers never construct an active root.
    #[must_use]
    pub fn begin_root(&self, intent: RootProfileIntent) -> RootProfiler {
        match (&self.kind, intent) {
            (_, RootProfileIntent::SuppressInternal) => {
                RootProfiler::Inactive(InactiveReason::Suppressed)
            }
            (SessionKind::Off, RootProfileIntent::UserBoundary { .. }) => {
                RootProfiler::Inactive(InactiveReason::Disabled)
            }
            (SessionKind::On(_), RootProfileIntent::UserBoundary { boundary_id }) => {
                RootProfiler::Active(ActiveRootProfiler { boundary_id })
            }
        }
    }

    /// Two-phase boundary registration: fixed runtime ownership is reserved
    /// before immutable `run.meta` publication. Only the admitted result can
    /// expose an active profiler token.
    pub fn register_root(
        &self,
        intent: RootProfileIntent,
        root_thread_ref: ThreadRef,
        program_id: ProgramId,
        revision_label: Option<String>,
        source_label: Option<String>,
    ) -> RootAdmission {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (root_thread_ref, program_id, revision_label, source_label);
            return RootAdmission::Inactive(self.begin_root(intent));
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let RootProfileIntent::UserBoundary { boundary_id } = intent else {
                return RootAdmission::Inactive(RootProfiler::Inactive(InactiveReason::Suppressed));
            };
            let SessionKind::On(session) = &self.kind else {
                return RootAdmission::Inactive(RootProfiler::Inactive(InactiveReason::Disabled));
            };
            let Ok(completion) = session.boundaries.reserve_root(BoundaryMetadata {
                boundary_id,
                root_thread_ref,
            }) else {
                return RootAdmission::Inactive(RootProfiler::Inactive(
                    InactiveReason::BoundaryStateUnavailable,
                ));
            };
            match session.store.begin_boundary(BoundaryRunMeta {
                boundary_id,
                program_id,
                root_thread_ref,
                revision_label,
                source_label,
            }) {
                BeginBoundaryResult::Admitted(publisher) => {
                    let handle = completion.lease().handle();
                    let mut slot = session.publishers[handle.slot as usize]
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    debug_assert!(slot.is_none());
                    *slot = Some(BoundaryRuntime::new(
                        handle.generation,
                        Arc::new(publisher),
                        root_thread_ref.process_euid,
                        root_thread_ref.engine_id,
                    ));
                    drop(slot);
                    RootAdmission::Active(ActiveRootAdmission {
                        profiler: RootProfiler::Active(ActiveRootProfiler { boundary_id }),
                        completion,
                    })
                }
                BeginBoundaryResult::Rejected(_) => {
                    completion.cancel_provisional();
                    RootAdmission::Inactive(RootProfiler::Inactive(
                        InactiveReason::BoundaryStoreUnavailable,
                    ))
                }
                BeginBoundaryResult::Indeterminate(_) => {
                    completion.cancel_provisional();
                    RootAdmission::Inactive(RootProfiler::Inactive(
                        InactiveReason::BoundaryStoreIndeterminate,
                    ))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prof::backend::DiskBudget;

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn active_boundary_exposes_o1_committed_checkpoint() {
        let temp = tempfile::TempDir::new().unwrap();
        let boundary_id = BoundaryId::from_bytes([0x44; 16]);
        let (session, diagnostic) = ProfilerSession::from_config(ProfilerConfig {
            enabled: true,
            store_root: temp.path().join(".baml/profiles-v1"),
            process_memory_bytes: 32 * 1024 * 1024,
            disk: DiskBudget {
                max_project_bytes: 16 * 1024 * 1024,
                minimum_free_bytes: 0,
            },
        });
        assert!(diagnostic.is_none());
        let admission = session.register_root(
            RootProfileIntent::UserBoundary { boundary_id },
            ThreadRef {
                process_euid: crate::ids::ProcessEuid([1; 16]),
                engine_id: crate::ids::EngineId(2),
                thread_id: crate::ids::BexThreadId(3),
            },
            ProgramId([4; 16]),
            None,
            None,
        );
        let RootAdmission::Active(admission) = admission else {
            panic!("root must be admitted");
        };
        let checkpoint = session
            .checkpoint(admission.completion.lease().handle())
            .expect("active checkpoint");
        assert_eq!(checkpoint.boundary_id, boundary_id);
        assert_eq!(checkpoint.committed_cct_sequence, 0);
        assert_eq!(checkpoint.committed_evidence_sequence, 0);
        assert_eq!(checkpoint.queued_and_inflight.evidence_fact_count, 0);
    }

    #[test]
    fn off_session_is_state_free_and_never_resolves_capture() {
        let (session, diagnostic) = ProfilerSession::from_config(ProfilerConfig {
            enabled: false,
            ..ProfilerConfig::default()
        });
        assert!(diagnostic.is_none());
        assert!(session.config().is_none());
        assert!(session.memory().is_none());
        assert!(session.boundary_registry().is_none());
        let root = session.begin_root(RootProfileIntent::UserBoundary {
            boundary_id: BoundaryId::from_bytes([1; 16]),
        });
        assert_eq!(root, RootProfiler::Inactive(InactiveReason::Disabled));
        assert_eq!(
            root.resolve_capture_plan(
                true,
                FunctionCaptureClass::Llm,
                Some(LocalIdOverrides {
                    inputs: Some(true),
                    output: Some(true),
                    error: Some(true),
                })
            ),
            CapturePlan::default()
        );
    }

    #[test]
    fn suppressed_root_dominates_an_on_session() {
        let (session, diagnostic) = ProfilerSession::from_config(ProfilerConfig::default());
        assert!(diagnostic.is_none());
        assert_eq!(
            session.begin_root(RootProfileIntent::SuppressInternal),
            RootProfiler::Inactive(InactiveReason::Suppressed)
        );
    }

    #[test]
    fn on_session_owns_policy_and_activates_user_root() {
        let boundary_id = BoundaryId::from_bytes([3; 16]);
        let (session, diagnostic) = ProfilerSession::from_config(ProfilerConfig::default());
        assert!(diagnostic.is_none());
        assert!(session.is_on());
        assert!(session.sizing().is_some());
        assert!(session.memory().is_some());
        assert!(session.boundary_registry().is_some());
        assert_eq!(
            session.begin_root(RootProfileIntent::UserBoundary { boundary_id }),
            RootProfiler::Active(ActiveRootProfiler { boundary_id })
        );
    }

    #[test]
    fn invalid_budget_returns_off_and_one_diagnostic() {
        let (session, diagnostic) = ProfilerSession::from_config(ProfilerConfig {
            enabled: true,
            process_memory_bytes: 1,
            ..ProfilerConfig::default()
        });
        assert!(!session.is_on());
        assert!(diagnostic.is_some());
    }
}
