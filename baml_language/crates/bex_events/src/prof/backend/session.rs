#[cfg(not(target_arch = "wasm32"))]
use std::sync::Mutex;
use std::sync::{Arc, OnceLock};

#[cfg(not(target_arch = "wasm32"))]
use super::decoder::{BoundaryRuntime, DecoderResources, DirectDecoder, finalize_ready_boundary};
#[cfg(not(target_arch = "wasm32"))]
use super::{BeginBoundaryResult, BoundaryRunMeta, ProfilerStore};
use super::{
    BoundaryHandle, BoundaryMetadata, BoundaryRegistry, CapturePlan, DerivedSizing,
    ErrorCaptureAttempt, ErrorCaptureId, FunctionCaptureClass, LocalIdOverrides, MeasuredLayouts,
    Owner, ProfilerConfig, ProfilerMemoryGovernor, ProfilerSizingPolicy, Reservation,
    ReservationClass, RootBoundaryCompletionGuard, TerminalErrorTarget, ValueLossReason, ValueRole,
    ValueState, resolve_capture_plan,
};
use crate::ids::{BoundaryId, ProgramId, ThreadRef};

#[cfg(not(target_arch = "wasm32"))]
const DECODER_COMMAND_BATCH_RECORDS: u16 = 256;

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
    producer_commands_tx: crossbeam_channel::Sender<DecoderCommand>,
    #[cfg(not(target_arch = "wasm32"))]
    producer_commands_rx: crossbeam_channel::Receiver<DecoderCommand>,
    #[cfg(not(target_arch = "wasm32"))]
    _producer_queue_reservation: Reservation,
    clock: crate::prof::clock::TickConverter,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
enum DecoderCommand {
    ErrorAttempt {
        handle: BoundaryHandle,
        attempt: ErrorCaptureAttempt,
        reservation: Reservation,
    },
    ErrorValue {
        handle: BoundaryHandle,
        id: ErrorCaptureId,
        value: ValueState,
    },
    EncodedErrorValue {
        handle: BoundaryHandle,
        id: ErrorCaptureId,
        encoded_body: Vec<u8>,
        reservation: Reservation,
    },
    TerminalError {
        handle: BoundaryHandle,
        call_ref: crate::ids::CallRef,
        target: TerminalErrorTarget,
        reservation: Reservation,
    },
    ValueOccurrence {
        handle: BoundaryHandle,
        call_ref: crate::ids::CallRef,
        role: ValueRole,
        state: ValueState,
        manual_eligible: bool,
        reservation: Option<Reservation>,
    },
    EncodedValue {
        handle: BoundaryHandle,
        call_ref: crate::ids::CallRef,
        role: ValueRole,
        encoded_body: Vec<u8>,
        manual_eligible: bool,
        reservation: Reservation,
    },
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
                let producer_queue_bytes = u64::from(sizing.producer_queue_slots)
                    .saturating_mul(MeasuredLayouts::V1.evidence_item_min_bytes);
                let producer_queue_reservation = match memory.try_reserve(
                    ReservationClass::General,
                    Owner::Transport,
                    producer_queue_bytes,
                ) {
                    Ok(reservation) => reservation,
                    Err(denied) => {
                        return (
                            Arc::new(Self {
                                kind: SessionKind::Off,
                            }),
                            Some(SetupDiagnostic {
                                message: format!(
                                    "profiling disabled: bounded producer queue requested {} general bytes with {} available",
                                    denied.requested_bytes, denied.available_bytes
                                ),
                            }),
                        );
                    }
                };
                let producer_queue_slots =
                    usize::try_from(sizing.producer_queue_slots).unwrap_or(usize::MAX);
                let (producer_commands_tx, producer_commands_rx) =
                    crossbeam_channel::bounded(producer_queue_slots);
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
                            producer_commands_tx,
                            #[cfg(not(target_arch = "wasm32"))]
                            producer_commands_rx,
                            #[cfg(not(target_arch = "wasm32"))]
                            _producer_queue_reservation: producer_queue_reservation,
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

    #[must_use]
    pub(crate) fn boundary_accepts_producer(&self, handle: BoundaryHandle) -> bool {
        match &self.kind {
            SessionKind::Off => false,
            SessionKind::On(session) => session.boundaries.accepts_producer(handle),
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
    pub fn checkpoint(&self, handle: BoundaryHandle) -> Option<super::ProfilerCheckpoint> {
        let SessionKind::On(session) = &self.kind else {
            return None;
        };
        DirectDecoder::checkpoint(handle, &session.publishers)
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
        // Only the sole consumer enters the decoder. Producer facts use the
        // bounded command lane, so the whole acquired ring slice can fold
        // under one lock without creating a producer wait edge.
        let mut decoder = session
            .decoder
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for raw in crate::prof::record::iter(bytes) {
            let Ok(raw) = raw else {
                // Records have no resync framing, so the rest of this slice
                // is unreadable. Account that before abandoning it.
                super::decoder::record_engine_framing_error(&session.publishers, engine_id);
                return;
            };
            decoder.consume(&resources, raw);
        }
    }

    /// Drain the bounded producer command lane on the sole consumer thread.
    /// Producers only call `try_send`; they never acquire the decoder mutex or
    /// perform CAS/store I/O.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn drain_producer_commands(&self) -> bool {
        let SessionKind::On(session) = &self.kind else {
            return false;
        };
        let Ok(first) = session.producer_commands_rx.try_recv() else {
            return false;
        };
        let mut decoder = session
            .decoder
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Self::consume_producer_command(session, &mut decoder, first);
        let mut consumed = 1_u16;
        while consumed < DECODER_COMMAND_BATCH_RECORDS {
            let Ok(command) = session.producer_commands_rx.try_recv() else {
                break;
            };
            Self::consume_producer_command(session, &mut decoder, command);
            consumed += 1;
        }
        true
    }

    /// Resolve cross-ring `EndThread` facts only after the consumer has swept
    /// every ring. A child start and its entry call are consecutive on the
    /// parent ring, but an earlier per-ring resolution point could still fall
    /// between them at a segment boundary.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn resolve_thread_ends_after_sweep(&self) -> bool {
        let SessionKind::On(session) = &self.kind else {
            return false;
        };
        let mut decoder = session
            .decoder
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = decoder.pending_thread_end_count();
        decoder.resolve_thread_ends_after_sweep();
        decoder.pending_thread_end_count() != before
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn consume_producer_command(
        session: &OnSession,
        decoder: &mut DirectDecoder,
        command: DecoderCommand,
    ) {
        match command {
            DecoderCommand::ErrorAttempt {
                handle,
                attempt,
                reservation,
            } => decoder.consume_error_attempt(
                &Self::decoder_resources(
                    session,
                    attempt.id.thread_ref.process_euid,
                    attempt.id.thread_ref.engine_id,
                ),
                handle,
                attempt,
                reservation,
            ),
            DecoderCommand::ErrorValue { id, value, .. } => decoder.complete_error_value(
                &Self::decoder_resources(
                    session,
                    id.thread_ref.process_euid,
                    id.thread_ref.engine_id,
                ),
                id,
                value,
            ),
            DecoderCommand::EncodedErrorValue {
                id,
                encoded_body,
                mut reservation,
                ..
            } => {
                let state = Self::publish_value_state(session, &encoded_body, &mut reservation);
                drop(reservation);
                decoder.complete_error_value(
                    &Self::decoder_resources(
                        session,
                        id.thread_ref.process_euid,
                        id.thread_ref.engine_id,
                    ),
                    id,
                    state,
                );
            }
            DecoderCommand::TerminalError {
                handle,
                call_ref,
                target,
                reservation,
            } => decoder.consume_terminal_error(
                &Self::decoder_resources(session, call_ref.process_euid, call_ref.engine_id),
                handle,
                call_ref,
                target,
                reservation,
            ),
            DecoderCommand::ValueOccurrence {
                handle,
                call_ref,
                role,
                state,
                manual_eligible,
                reservation,
            } => decoder.consume_value_occurrence(
                &Self::decoder_resources(session, call_ref.process_euid, call_ref.engine_id),
                handle,
                call_ref,
                role,
                state,
                manual_eligible,
                reservation,
            ),
            DecoderCommand::EncodedValue {
                handle,
                call_ref,
                role,
                encoded_body,
                manual_eligible,
                mut reservation,
            } => {
                let state = Self::publish_value_state(session, &encoded_body, &mut reservation);
                decoder.consume_value_occurrence(
                    &Self::decoder_resources(session, call_ref.process_euid, call_ref.engine_id),
                    handle,
                    call_ref,
                    role,
                    state,
                    manual_eligible,
                    Some(reservation),
                );
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn decoder_resources(
        session: &OnSession,
        process_euid: crate::ids::ProcessEuid,
        engine_id: crate::ids::EngineId,
    ) -> DecoderResources<'_> {
        DecoderResources {
            process_euid,
            engine_id,
            memory: &session.memory,
            sizing: session.sizing,
            clock: &session.clock,
            boundaries: &session.publishers,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn publish_value_state(
        session: &OnSession,
        encoded_body: &[u8],
        reservation: &mut Reservation,
    ) -> ValueState {
        let encoded_bytes = u64::try_from(encoded_body.len()).unwrap_or(u64::MAX);
        let publication_bytes = session
            .store
            .cas_publication_allocation_bound(encoded_bytes);
        if encoded_bytes > session.sizing.single_value_bytes {
            return ValueState::Lost(ValueLossReason::ValueTooLarge);
        }
        if reservation.try_grow(publication_bytes).is_err() {
            return ValueState::Lost(ValueLossReason::ValueMemoryExceeded);
        }
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
            super::PublishCasResult::Lost(_) => ValueState::Lost(ValueLossReason::CasWriteFailed),
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
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn enqueue_producer_command(session: &OnSession, command: DecoderCommand) {
        if let Err(error) = session.producer_commands_tx.try_send(command) {
            let command = error.into_inner();
            match command {
                DecoderCommand::ErrorAttempt { handle, .. }
                | DecoderCommand::ErrorValue { handle, .. }
                | DecoderCommand::EncodedErrorValue { handle, .. } => session
                    .boundaries
                    .record_error_attempt_transport_loss(handle),
                DecoderCommand::TerminalError { handle, .. } => session
                    .boundaries
                    .record_terminal_error_transport_loss(handle),
                DecoderCommand::ValueOccurrence { handle, .. }
                | DecoderCommand::EncodedValue { handle, .. } => session
                    .boundaries
                    .record_value_attempt_transport_loss(handle),
            }
        }
        #[cfg(not(baml_loom))]
        crate::prof::registry::global_ctx().wake().force_wake();
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
        if !session.boundaries.accepts_producer(handle) {
            return None;
        }
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
        if !session.boundaries.accepts_producer(handle) {
            return;
        }
        Self::enqueue_producer_command(
            session,
            DecoderCommand::ErrorAttempt {
                handle,
                attempt,
                reservation,
            },
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn complete_error_value(
        &self,
        handle: BoundaryHandle,
        id: ErrorCaptureId,
        value: ValueState,
    ) {
        let SessionKind::On(session) = &self.kind else {
            return;
        };
        if !session.boundaries.accepts_producer(handle) {
            return;
        }
        Self::enqueue_producer_command(session, DecoderCommand::ErrorValue { handle, id, value });
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
        if !session.boundaries.accepts_producer(handle) {
            return;
        }
        Self::enqueue_producer_command(
            session,
            DecoderCommand::TerminalError {
                handle,
                call_ref,
                target,
                reservation,
            },
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn record_error_attempt_transport_loss(&self, handle: BoundaryHandle) {
        let SessionKind::On(session) = &self.kind else {
            return;
        };
        session
            .boundaries
            .record_error_attempt_transport_loss(handle);
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn record_terminal_error_transport_loss(&self, handle: BoundaryHandle) {
        let SessionKind::On(session) = &self.kind else {
            return;
        };
        session
            .boundaries
            .record_terminal_error_transport_loss(handle);
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
        encoded_body: Vec<u8>,
        manual_eligible: bool,
        reservation: Reservation,
    ) {
        let SessionKind::On(session) = &self.kind else {
            return;
        };
        if !session.boundaries.accepts_producer(handle) {
            return;
        }
        Self::enqueue_producer_command(
            session,
            DecoderCommand::EncodedValue {
                handle,
                call_ref,
                role,
                encoded_body,
                manual_eligible,
                reservation,
            },
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn record_encoded_error_value(
        &self,
        handle: BoundaryHandle,
        id: ErrorCaptureId,
        encoded_body: Vec<u8>,
        reservation: Reservation,
    ) {
        let SessionKind::On(session) = &self.kind else {
            return;
        };
        if !session.boundaries.accepts_producer(handle) {
            return;
        }
        Self::enqueue_producer_command(
            session,
            DecoderCommand::EncodedErrorValue {
                handle,
                id,
                encoded_body,
                reservation,
            },
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn record_error_value_loss(
        &self,
        handle: BoundaryHandle,
        id: ErrorCaptureId,
        reason: ValueLossReason,
    ) {
        self.complete_error_value(handle, id, ValueState::Lost(reason));
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
        if !session.boundaries.accepts_producer(handle) {
            return;
        }
        Self::enqueue_producer_command(
            session,
            DecoderCommand::ValueOccurrence {
                handle,
                call_ref,
                role,
                state: ValueState::Lost(reason),
                manual_eligible,
                reservation: None,
            },
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn maintain_ready_boundaries(&self) -> bool {
        let SessionKind::On(session) = &self.kind else {
            return false;
        };
        // A ready boundary has no live producer, but its already-submitted
        // commands may still be behind this sweep's bounded drain batch.
        // Finalization must not discard those exact evidence facts.
        if !session.producer_commands_rx.is_empty() {
            return false;
        }
        let ready = session.boundaries.ready_handles();
        if ready.is_empty() {
            return false;
        }
        let ready = ready
            .into_iter()
            .filter(|handle| session.boundaries.consumer_drain_completed(*handle))
            .collect::<Vec<_>>();
        if ready.is_empty() {
            // Observing a terminal candidate is progress: the next consumer
            // service pass performs the required full structural sweep before
            // this boundary becomes eligible for durable finalization.
            return true;
        }
        let mut decoder = session
            .decoder
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for handle in ready {
            if let Some((metadata, _)) = session.boundaries.closing_facts(handle) {
                let resources = Self::decoder_resources(
                    session,
                    metadata.root_thread_ref.process_euid,
                    metadata.root_thread_ref.engine_id,
                );
                decoder.complete_missing_values(&resources, handle);
            }
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
        true
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
    use crate::prof::backend::{BoundaryEndStatus, DiskBudget};

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn producer_command_lane_never_takes_decoder_lock_and_reports_saturation() {
        assert!(
            std::mem::size_of::<DecoderCommand>()
                <= usize::try_from(MeasuredLayouts::V1.evidence_item_min_bytes).unwrap()
        );
        let temp = tempfile::TempDir::new().unwrap();
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
        let thread_ref = ThreadRef {
            process_euid: crate::ids::ProcessEuid([1; 16]),
            engine_id: crate::ids::EngineId(2),
            thread_id: crate::ids::BexThreadId(3),
        };
        let admission = session.register_root(
            RootProfileIntent::UserBoundary {
                boundary_id: BoundaryId::from_bytes([4; 16]),
            },
            thread_ref,
            ProgramId([5; 16]),
            None,
            None,
        );
        let handle = admission.boundary_handle().expect("active boundary");
        let SessionKind::On(on) = &session.kind else {
            panic!("profiling session must be active");
        };
        let decoder = on
            .decoder
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let call_ref = crate::ids::CallRef {
            process_euid: thread_ref.process_euid,
            engine_id: thread_ref.engine_id,
            thread_id: thread_ref.thread_id,
            call_id: crate::ids::BexCallId(6),
        };

        session.record_value_loss(
            handle,
            call_ref,
            ValueRole::Output,
            ValueLossReason::CopyFailed,
            false,
        );
        assert!(matches!(
            on.producer_commands_rx.try_recv(),
            Ok(DecoderCommand::ValueOccurrence { .. })
        ));

        for ordinal in 0..on.producer_commands_tx.capacity().unwrap() {
            on.producer_commands_tx
                .try_send(DecoderCommand::ErrorValue {
                    handle,
                    id: ErrorCaptureId {
                        thread_ref,
                        unwind_ordinal: u64::try_from(ordinal).unwrap(),
                    },
                    value: ValueState::Lost(ValueLossReason::CopyFailed),
                })
                .unwrap();
        }
        session.record_value_loss(
            handle,
            call_ref,
            ValueRole::Output,
            ValueLossReason::CopyFailed,
            false,
        );
        assert_eq!(
            on.boundaries
                .producer_health(handle)
                .value_attempt_transport_exceeded,
            1
        );
        drop(decoder);
    }

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

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn final_drain_keeps_structural_end_when_value_command_was_lost() {
        use crate::{
            ids::{BexCallId, BexThreadId, CallRef, EngineId, FunctionId, ProcessEuid},
            prof::{
                backend::DurableRunReader,
                record::{FunctionEndStatus, MAX_RECORD_LEN, RawRecord},
            },
        };

        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path().join(".baml/profiles-v1");
        let boundary_id = BoundaryId::from_bytes([0x55; 16]);
        let thread_ref = ThreadRef {
            process_euid: ProcessEuid([1; 16]),
            engine_id: EngineId(2),
            thread_id: BexThreadId(3),
        };
        let (session, diagnostic) = ProfilerSession::from_config(ProfilerConfig {
            enabled: true,
            store_root: root.clone(),
            process_memory_bytes: 32 * 1024 * 1024,
            disk: DiskBudget {
                max_project_bytes: 16 * 1024 * 1024,
                minimum_free_bytes: 0,
            },
        });
        assert!(diagnostic.is_none());
        let RootAdmission::Active(admission) = session.register_root(
            RootProfileIntent::UserBoundary { boundary_id },
            thread_ref,
            ProgramId([4; 16]),
            None,
            None,
        ) else {
            panic!("root must be admitted");
        };
        let handle = admission.completion.lease().handle();
        let call_ref = CallRef {
            process_euid: thread_ref.process_euid,
            engine_id: thread_ref.engine_id,
            thread_id: thread_ref.thread_id,
            call_id: BexCallId(6),
        };
        let emit = |record: RawRecord<'_>| {
            let mut bytes = [0; MAX_RECORD_LEN];
            let len = record.encode(&mut bytes);
            session.consume_raw_bytes(thread_ref.process_euid, thread_ref.engine_id, &bytes[..len]);
        };
        emit(RawRecord::StartThread {
            flags: 0,
            thread_id: thread_ref.thread_id,
            parent_thread_id: BexThreadId(0),
            parent_call_id: BexCallId(0),
            ts_ticks: 10,
            name: b"",
        });
        emit(RawRecord::CallFunction {
            flags: resolve_capture_plan(true, FunctionCaptureClass::Ordinary, None).to_call_flags(),
            thread_id: thread_ref.thread_id,
            call_id: call_ref.call_id,
            parent_call_id: BexCallId(0),
            function_id: FunctionId(7),
            call_site: None,
            ts_ticks: 20,
        });
        emit(RawRecord::EndFunction {
            status: FunctionEndStatus::Errored,
            thread_id: thread_ref.thread_id,
            call_id: call_ref.call_id,
            ts_ticks: 30,
        });

        let SessionKind::On(on) = &session.kind else {
            panic!("profiling session must be active");
        };
        on.boundaries.record_value_attempt_transport_loss(handle);
        admission.completion.complete(BoundaryEndStatus::Failed);
        assert!(session.maintain_ready_boundaries());
        assert!(session.maintain_ready_boundaries());

        let run = DurableRunReader::open(root, boundary_id)
            .unwrap()
            .load()
            .unwrap();
        let span = run.spans.get(&call_ref).expect("selected span");
        assert_eq!(
            span.end.map(|end| end.status),
            Some(FunctionEndStatus::Errored)
        );
        assert_eq!(
            span.input.map(|value| value.state),
            Some(ValueState::Lost(
                ValueLossReason::ValueAttemptTransportExceeded
            ))
        );
        assert_eq!(run.terminal_health.value_attempt_transport_exceeded, 1);
        assert_eq!(
            run.contexts
                .values()
                .map(|context| context.counters.completed_error)
                .sum::<u64>(),
            1
        );
    }

    /// Records from three logical threads arrive in reverse causal order:
    /// the grandchild's calls first, then the child's, then the root. Every
    /// fact is parked before its owner thread is known, so this exercises
    /// pending-join attribution, spawn-parent propagation through two
    /// levels of parked threads, and final-drain loss accounting for a call
    /// whose parent never arrives.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn reordered_descendant_facts_resolve_and_attribute_losses_to_their_run() {
        use crate::{
            ids::{BexCallId, BexThreadId, CallRef, EngineId, FunctionId, ProcessEuid},
            prof::{
                backend::{ContextKey, DurableRunReader, EdgeKind},
                record::{FunctionEndStatus, MAX_RECORD_LEN, RawRecord, ThreadEndStatus},
            },
        };

        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path().join(".baml/profiles-v1");
        let boundary_id = BoundaryId::from_bytes([0x66; 16]);
        let process_euid = ProcessEuid([1; 16]);
        let engine_id = EngineId(2);
        let root_thread = BexThreadId(3);
        let child_thread = BexThreadId(10);
        let grandchild_thread = BexThreadId(20);
        let root_thread_ref = ThreadRef {
            process_euid,
            engine_id,
            thread_id: root_thread,
        };
        let (session, diagnostic) = ProfilerSession::from_config(ProfilerConfig {
            enabled: true,
            store_root: root.clone(),
            process_memory_bytes: 32 * 1024 * 1024,
            disk: DiskBudget {
                max_project_bytes: 16 * 1024 * 1024,
                minimum_free_bytes: 0,
            },
        });
        assert!(diagnostic.is_none());
        let RootAdmission::Active(admission) = session.register_root(
            RootProfileIntent::UserBoundary { boundary_id },
            root_thread_ref,
            ProgramId([4; 16]),
            None,
            None,
        ) else {
            panic!("root must be admitted");
        };
        let emit = |record: RawRecord<'_>| {
            let mut bytes = [0; MAX_RECORD_LEN];
            let len = record.encode(&mut bytes);
            session.consume_raw_bytes(process_euid, engine_id, &bytes[..len]);
        };
        let ordinary =
            resolve_capture_plan(false, FunctionCaptureClass::Ordinary, None).to_call_flags();
        let call = |thread_id, call_id, parent_call_id, function_id, flags, ts_ticks| {
            RawRecord::CallFunction {
                flags,
                thread_id,
                call_id: BexCallId(call_id),
                parent_call_id: BexCallId(parent_call_id),
                function_id: FunctionId(function_id),
                call_site: None,
                ts_ticks,
            }
        };
        let end = |thread_id, call_id, ts_ticks| RawRecord::EndFunction {
            status: FunctionEndStatus::Ok,
            thread_id,
            call_id: BexCallId(call_id),
            ts_ticks,
        };
        let end_thread = |thread_id, ts_ticks| RawRecord::EndThread {
            status: ThreadEndStatus::Completed,
            thread_id,
            ts_ticks,
        };

        // Grandchild thread first: nested call, its end, an orphan whose
        // parent call never exists, then the entry call, then the spawn fact.
        emit(call(grandchild_thread, 2, 1, 30, ordinary, 300));
        emit(end(grandchild_thread, 2, 310));
        emit(call(grandchild_thread, 3, 99, 31, ordinary, 320));
        emit(call(grandchild_thread, 1, 0, 20, ordinary, 290));
        emit(end(grandchild_thread, 1, 330));
        emit(RawRecord::StartThreadSpawn {
            flags: 0,
            thread_id: grandchild_thread,
            parent_thread_id: child_thread,
            parent_call_id: BexCallId(1),
            ts_ticks: 280,
            spawn_site: None,
            name: b"",
        });
        emit(end_thread(grandchild_thread, 340));

        // Child thread: entry call before its own spawn fact.
        emit(call(child_thread, 1, 0, 10, ordinary, 200));
        emit(RawRecord::StartThreadSpawn {
            flags: 0,
            thread_id: child_thread,
            parent_thread_id: root_thread,
            parent_call_id: BexCallId(6),
            ts_ticks: 190,
            spawn_site: None,
            name: b"",
        });
        emit(end(child_thread, 1, 350));
        emit(end_thread(child_thread, 360));

        // Nothing could resolve yet: every parked fact is still unattributed.
        {
            let SessionKind::On(on) = &session.kind else {
                panic!("profiling session must be active");
            };
            let decoder = on.decoder.lock().unwrap();
            assert_eq!(decoder.pending_thread_end_count(), 2);
        }

        // Root last. Its thread start attributes the whole parked tree; its
        // call start resolves the child, which resolves the grandchild.
        emit(RawRecord::StartThread {
            flags: 0,
            thread_id: root_thread,
            parent_thread_id: BexThreadId(0),
            parent_call_id: BexCallId(0),
            ts_ticks: 100,
            name: b"",
        });
        emit(call(
            root_thread,
            6,
            0,
            1,
            resolve_capture_plan(true, FunctionCaptureClass::Ordinary, None).to_call_flags(),
            110,
        ));
        emit(end(root_thread, 6, 400));
        emit(end_thread(root_thread, 410));
        assert!(session.resolve_thread_ends_after_sweep());

        admission.completion.complete(BoundaryEndStatus::Succeeded);
        assert!(session.maintain_ready_boundaries());
        assert!(session.maintain_ready_boundaries());

        let run = DurableRunReader::open(root, boundary_id)
            .unwrap()
            .load()
            .unwrap();
        assert!(run.end.is_some(), "run must seal");

        // Four contexts form one chain: Root -> Spawn -> Spawn -> Call.
        assert_eq!(run.contexts.len(), 4);
        let find = |edge: EdgeKind, parent: Option<ContextKey>| {
            let mut matches = run.contexts.iter().filter(|(_, context)| {
                context.tuple.is_some_and(|tuple| {
                    tuple.edge_kind == edge && tuple.parent_context_key == parent
                })
            });
            let (key, context) = matches.next().expect("context exists");
            assert!(matches.next().is_none(), "context chain is unambiguous");
            assert_eq!(context.counters.invocations_started, 1);
            assert_eq!(context.counters.completed_ok, 1);
            *key
        };
        let root_key = find(EdgeKind::Root, None);
        let child_key = find(EdgeKind::Spawn, Some(root_key));
        let grandchild_key = find(EdgeKind::Spawn, Some(child_key));
        let nested_key = find(EdgeKind::Call, Some(grandchild_key));
        assert_eq!(
            run.contexts[&nested_key]
                .tuple
                .map(|tuple| tuple.function_id),
            Some(FunctionId(30))
        );

        // The orphan call start was parked before its thread was known and
        // its parent never arrived; it must be charged to this run, not leak.
        assert_eq!(run.terminal_health.unmatched_call_facts, 1);
        assert_eq!(run.terminal_health.unmatched_thread_facts, 0);
        assert_eq!(run.terminal_health.join_capacity_exceeded, 0);
        assert_eq!(run.terminal_health.corrupt_records, 0);
        assert_eq!(run.terminal_health.structural_transport_exceeded, 0);
        assert!(run.overflow.is_empty());

        // The root was the only selected span and it closed cleanly.
        let root_call = CallRef {
            process_euid,
            engine_id,
            thread_id: root_thread,
            call_id: BexCallId(6),
        };
        assert_eq!(run.spans.len(), 1);
        assert_eq!(
            run.spans[&root_call].end.map(|end| end.status),
            Some(FunctionEndStatus::Ok)
        );
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
