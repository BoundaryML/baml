#[cfg(not(target_arch = "wasm32"))]
use std::sync::Mutex;
use std::sync::{Arc, OnceLock};

#[cfg(not(target_arch = "wasm32"))]
use super::decoder::{DecoderResources, DirectDecoder, ExecutionRuntime, finalize_ready_execution};
#[cfg(not(target_arch = "wasm32"))]
use super::writer::{ExecutionCheckpoint, StreamCheckpoint, StreamWriter, WriterEnv, counters};
use super::{
    CapturePlan, DerivedSizing, ExecutionHandle, ExecutionRegistry, FunctionCaptureClass,
    LocalIdOverrides, MeasuredLayouts, Owner, ProfilerConfig, ProfilerMemoryGovernor, Reservation,
    ReservationClass, ValueLossReason, resolve_capture_plan,
};
#[cfg(not(target_arch = "wasm32"))]
use super::{
    ErrorCaptureAttempt, ErrorCaptureId, ExecutionMetadata, ProfilerSizingPolicy,
    RootExecutionCompletionGuard, TerminalErrorTarget, ValueRole, ValueState,
};
#[cfg(not(target_arch = "wasm32"))]
use super::{MetaRecord, ProfilerStore, StreamId};
use crate::ids::{BoundaryId, ProgramId, ThreadRef};
#[cfg(not(target_arch = "wasm32"))]
use crate::ids::{EngineId, ProcessEuid};

#[cfg(not(target_arch = "wasm32"))]
const DECODER_COMMAND_BATCH_RECORDS: u16 = 256;

/// The only per-root admission intents. Internal work must be explicit rather
/// than mutating a generic profiling boolean after construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootProfileIntent {
    /// A user root call; `runtime_id` is the host runtime token
    /// (`baml_id_1_…`), opaque to the profiler.
    UserRoot {
        runtime_id: BoundaryId,
    },
    SuppressInternal,
}

/// Why a root has no profiler state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InactiveReason {
    Disabled,
    Suppressed,
    InvalidMemoryBudget,
    ExecutionStateUnavailable,
    ThreadLeaseUnavailable,
    StoreUnavailable,
    /// The session was created in a parent process; children of `fork()`
    /// profile nothing (streams spec §5.8).
    ForkedProcess,
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

/// Small immutable root token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActiveRootProfiler {
    /// The execution's identity: its root thread.
    pub root_thread_ref: ThreadRef,
}

#[derive(Debug)]
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
struct OnSession {
    config: ProfilerConfig,
    sizing: DerivedSizing,
    memory: ProfilerMemoryGovernor,
    boundaries: Arc<ExecutionRegistry>,
    #[cfg(not(target_arch = "wasm32"))]
    store: Arc<ProfilerStore>,
    #[cfg(not(target_arch = "wasm32"))]
    publishers: Box<[Mutex<Option<ExecutionRuntime>>]>,
    #[cfg(not(target_arch = "wasm32"))]
    decoder: Mutex<DirectDecoder>,
    /// Driven only by the consumer thread; checkpoint readers on other
    /// threads only read through the mutex.
    #[cfg(not(target_arch = "wasm32"))]
    writer: Mutex<StreamWriter>,
    #[cfg(not(target_arch = "wasm32"))]
    producer_commands_tx: crossbeam_channel::Sender<DecoderCommand>,
    #[cfg(not(target_arch = "wasm32"))]
    producer_commands_rx: crossbeam_channel::Receiver<DecoderCommand>,
    #[cfg(not(target_arch = "wasm32"))]
    _producer_queue_reservation: Reservation,
    clock: crate::prof::clock::TickConverter,
    /// Fork guard (streams spec §5.8): the pid this session was created in.
    pid: u32,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
enum DecoderCommand {
    ErrorAttempt {
        handle: ExecutionHandle,
        attempt: ErrorCaptureAttempt,
        reservation: Reservation,
    },
    ErrorValue {
        handle: ExecutionHandle,
        id: ErrorCaptureId,
        value: ValueState,
    },
    EncodedErrorValue {
        handle: ExecutionHandle,
        id: ErrorCaptureId,
        encoded_body: Vec<u8>,
        reservation: Reservation,
    },
    TerminalError {
        handle: ExecutionHandle,
        call_ref: crate::ids::CallRef,
        target: TerminalErrorTarget,
        reservation: Reservation,
    },
    ValueOccurrence {
        handle: ExecutionHandle,
        call_ref: crate::ids::CallRef,
        role: ValueRole,
        state: ValueState,
        manual_eligible: bool,
        reservation: Option<Reservation>,
    },
    EncodedValue {
        handle: ExecutionHandle,
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
    pub completion: RootExecutionCompletionGuard,
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
    pub const fn boundary_handle(&self) -> Option<ExecutionHandle> {
        match self {
            Self::Inactive(_) => None,
            #[cfg(not(target_arch = "wasm32"))]
            Self::Active(admission) => Some(admission.completion.lease().handle()),
        }
    }
}

fn configured_store_root() -> &'static OnceLock<std::path::PathBuf> {
    static CONFIGURED: OnceLock<std::path::PathBuf> = OnceLock::new();
    &CONFIGURED
}

fn global_setup_diagnostic_cell() -> &'static OnceLock<SetupDiagnostic> {
    static DIAGNOSTIC: OnceLock<SetupDiagnostic> = OnceLock::new();
    &DIAGNOSTIC
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AwaitClockInvalid;

/// Process/store session selected once and injected into engines.
///
/// `Off` is a unit variant: it has no ring, consumer, heap, lock, path, clock,
/// lazy initializer, or synchronization field.
#[derive(Debug)]
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
enum SessionKind {
    Off,
    On(Box<OnSession>),
}

#[derive(Debug)]
pub struct ProfilerSession {
    kind: SessionKind,
}

impl ProfilerSession {
    /// Constructs a session from the configuration. Invalid memory budgets
    /// fail to the state-free off variant and one diagnostic.
    #[must_use]
    #[cfg_attr(target_arch = "wasm32", allow(clippy::needless_pass_by_value))]
    pub fn from_config(config: ProfilerConfig) -> (Arc<Self>, Option<SetupDiagnostic>) {
        Self::from_config_impl(config, None)
    }

    /// Test seam: construct with an injected store platform (fault
    /// injection for the store's I/O).
    #[doc(hidden)]
    #[must_use]
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_config_with_platform(
        config: ProfilerConfig,
        platform: Arc<dyn super::StorePlatform>,
    ) -> (Arc<Self>, Option<SetupDiagnostic>) {
        Self::from_config_impl(config, Some(platform))
    }

    #[cfg(target_arch = "wasm32")]
    #[allow(
        clippy::needless_pass_by_value,
        reason = "signature parity with the native arm, which consumes the config"
    )]
    fn from_config_impl(
        config: ProfilerConfig,
        _platform: Option<()>,
    ) -> (Arc<Self>, Option<SetupDiagnostic>) {
        if !config.enabled {
            return (
                Arc::new(Self {
                    kind: SessionKind::Off,
                }),
                None,
            );
        }
        (
            Arc::new(Self {
                kind: SessionKind::Off,
            }),
            Some(SetupDiagnostic {
                message: "profiling disabled: the local profiling store is unavailable on wasm32"
                    .to_string(),
            }),
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn from_config_impl(
        config: ProfilerConfig,
        platform: Option<Arc<dyn super::StorePlatform>>,
    ) -> (Arc<Self>, Option<SetupDiagnostic>) {
        if !config.enabled {
            return (
                Arc::new(Self {
                    kind: SessionKind::Off,
                }),
                None,
            );
        }
        match ProfilerSizingPolicy::derive(config.process_memory_bytes, MeasuredLayouts::V1) {
            Ok(sizing) => {
                let memory = ProfilerMemoryGovernor::new(sizing, MeasuredLayouts::V1);
                let boundaries = match ExecutionRegistry::new(
                    sizing.execution_slots,
                    MeasuredLayouts::V1.execution_slot_bytes,
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
                // One fixed reservation covers every pending index-plane
                // record (streams spec §5.4): ≤ 1 StreamStarted + engines +
                // 2 × execution slots. No per-record reservation exists.
                let meta_queue_bytes = (2u64
                    .saturating_mul(u64::from(sizing.execution_slots))
                    .saturating_add(64))
                .saturating_mul(MeasuredLayouts::V1.meta_record_bytes);
                let meta_queue = match memory.try_reserve(
                    ReservationClass::General,
                    Owner::Writer,
                    meta_queue_bytes,
                ) {
                    Ok(reservation) => reservation,
                    Err(denied) => {
                        return (
                            Arc::new(Self {
                                kind: SessionKind::Off,
                            }),
                            Some(SetupDiagnostic {
                                message: format!(
                                    "profiling disabled: stream writer meta queue requested {} general bytes with {} available",
                                    denied.requested_bytes, denied.available_bytes
                                ),
                            }),
                        );
                    }
                };
                let stream = StreamId(config.stream.unwrap_or_else(ProcessEuid::current));
                let store = match match platform {
                    Some(platform) => ProfilerStore::open(
                        config.store_root.clone(),
                        config.disk,
                        platform,
                        stream,
                    ),
                    None => {
                        ProfilerStore::open_native(config.store_root.clone(), config.disk, stream)
                    }
                } {
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
                let publishers = (0..sizing.execution_slots)
                    .map(|_| Mutex::new(None))
                    .collect::<Vec<_>>()
                    .into_boxed_slice();
                let clock = crate::prof::clock::TickConverter::from_clock();
                #[cfg(not(target_arch = "wasm32"))]
                let writer = {
                    let zero_unix_ns = u64::try_from(crate::prof::clock::started_at_epoch_ns())
                        .unwrap_or(u64::MAX);
                    StreamWriter::new(
                        Arc::clone(&store),
                        config.publish_interval,
                        sizing.segment_target_bytes,
                        meta_queue,
                        MetaRecord::StreamStarted {
                            pid: std::process::id(),
                            zero_unix_ns,
                            baml_version: baml_version::CANONICAL_VERSION.to_string(),
                            os_arch: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
                        },
                    )
                };
                #[cfg(target_arch = "wasm32")]
                let _ = meta_queue;
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
                            writer: Mutex::new(writer),
                            #[cfg(not(target_arch = "wasm32"))]
                            producer_commands_tx,
                            #[cfg(not(target_arch = "wasm32"))]
                            producer_commands_rx,
                            #[cfg(not(target_arch = "wasm32"))]
                            _producer_queue_reservation: producer_queue_reservation,
                            clock,
                            pid: std::process::id(),
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
            let mut config = ProfilerConfig {
                enabled: crate::prof::ProfConfig::global().is_enabled(),
                ..ProfilerConfig::default()
            };
            // Host-resolved store root (the CLI resolves the project root,
            // streams spec §7.5); `BAML_PROFILE_DIR` still wins via the
            // config default.
            if std::env::var_os("BAML_PROFILE_DIR").is_none()
                && let Some(root) = configured_store_root().get()
            {
                config.store_root.clone_from(root);
            }
            let (session, diagnostic) = Self::from_config(config);
            if let Some(diagnostic) = diagnostic {
                let _ = global_setup_diagnostic_cell().set(diagnostic);
            }
            session
        })
    }

    /// The setup diagnostic the global session produced when it came up
    /// disabled: the one explanation for "the run succeeded but wrote no
    /// store". `None` while the global session is uninitialized or healthy.
    /// Hosts surface it in their verbose/diagnostic channel — a profiling
    /// failure must never break the program, but it must be discoverable.
    #[must_use]
    pub fn global_setup_diagnostic() -> Option<SetupDiagnostic> {
        global_setup_diagnostic_cell().get().cloned()
    }

    /// One-line state summary for hosts' verbose channels. Unlike
    /// [`Self::global_setup_diagnostic`] it always says something: active
    /// sessions name the resolved store root (the "it wrote, but where?"
    /// question), disabled ones name the reason or the configuration.
    #[must_use]
    pub fn status_line(&self) -> String {
        match &self.kind {
            SessionKind::On(session) => {
                format!("active, store root {}", session.config.store_root.display())
            }
            SessionKind::Off => match global_setup_diagnostic_cell().get() {
                Some(diagnostic) => diagnostic.message.clone(),
                None => "off (disabled by configuration)".to_string(),
            },
        }
    }

    /// Resolves the global session's store root before its first use (the
    /// CLI calls this with `<project root>/.baml/profiles-v1`). Returns
    /// `false` when a root was already configured; a call after the global
    /// session initialized has no effect on it.
    pub fn configure_global_store_root(root: std::path::PathBuf) -> bool {
        configured_store_root().set(root).is_ok()
    }

    /// The store root a READER should open for `project_root` — the same
    /// precedence the producer applies: `BAML_PROFILE_DIR` wins, else
    /// `<project root>/.baml/profiles-v1`. Every consumer resolving a
    /// store location goes through here so the two sides can never
    /// disagree.
    #[must_use]
    pub fn resolve_store_root(project_root: &std::path::Path) -> std::path::PathBuf {
        std::env::var_os("BAML_PROFILE_DIR").map_or_else(
            || project_root.join(".baml/profiles-v1"),
            std::path::PathBuf::from,
        )
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
    pub fn boundary_registry(&self) -> Option<&Arc<ExecutionRegistry>> {
        match &self.kind {
            SessionKind::Off => None,
            SessionKind::On(session) => Some(&session.boundaries),
        }
    }

    #[must_use]
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) fn boundary_accepts_producer(&self, handle: ExecutionHandle) -> bool {
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

    /// Live stream-level counters (streams spec §5.6 checkpoints).
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn stream_checkpoint(&self) -> Option<StreamCheckpoint> {
        let SessionKind::On(session) = &self.kind else {
            return None;
        };
        Some(
            session
                .writer
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .checkpoint(std::time::Instant::now()),
        )
    }

    /// Per-execution counters; `None` after the slot is released.
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn execution_checkpoint(&self, handle: ExecutionHandle) -> Option<ExecutionCheckpoint> {
        let SessionKind::On(session) = &self.kind else {
            return None;
        };
        let (root, health, queued) = DirectDecoder::queue_snapshot(handle, &session.publishers)?;
        let (data_first_seq, data_last_seq) = session
            .writer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .exec_publication_range(root);
        Some(ExecutionCheckpoint {
            root,
            health,
            queued,
            data_first_seq,
            data_last_seq,
        })
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
            writer: &session.writer,
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
        // Thread identity in the pending maps embeds the owning engine; the
        // euid/engine of these resources are only decode attribution defaults
        // and are not consulted on this path.
        let resources =
            Self::decoder_resources(session, crate::ids::ProcessEuid([0; 16]), EngineId(0));
        decoder.resolve_thread_ends_after_sweep(&resources);
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
            writer: &session.writer,
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
        super::hooks::wake_consumer();
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn record_structural_transport_loss(&self, handle: ExecutionHandle) {
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
        handle: ExecutionHandle,
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
        handle: ExecutionHandle,
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
        handle: ExecutionHandle,
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
        handle: ExecutionHandle,
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
    pub(crate) fn record_error_attempt_transport_loss(&self, handle: ExecutionHandle) {
        let SessionKind::On(session) = &self.kind else {
            return;
        };
        session
            .boundaries
            .record_error_attempt_transport_loss(handle);
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn record_terminal_error_transport_loss(&self, handle: ExecutionHandle) {
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
        handle: ExecutionHandle,
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
        handle: ExecutionHandle,
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
        handle: ExecutionHandle,
        id: ErrorCaptureId,
        reason: ValueLossReason,
    ) {
        self.complete_error_value(handle, id, ValueState::Lost(reason));
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn record_value_loss(
        &self,
        handle: ExecutionHandle,
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

    /// One consumer maintenance pass (streams spec §5.5/§5.6): drain
    /// admission facts into the writer, finalize ready executions, then run
    /// the publication cycle.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn maintain_ready_executions(&self) -> bool {
        let SessionKind::On(session) = &self.kind else {
            return false;
        };
        let now = std::time::Instant::now();
        let admitted = session.boundaries.take_admitted(&session.clock);
        let mut progress = !admitted.is_empty();
        if progress {
            session
                .writer
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .enqueue_admitted(admitted, now);
        }

        // A ready execution has no live producer, but its already-submitted
        // commands may still be behind this sweep's bounded drain batch.
        // Finalization must not discard those exact evidence facts.
        if session.producer_commands_rx.is_empty() {
            let ready = session.boundaries.ready_handles();
            if !ready.is_empty() {
                // Observing a terminal candidate is progress: the next
                // consumer service pass performs the required full structural
                // sweep before an execution becomes eligible for durable
                // finalization.
                progress = true;
                let ready = ready
                    .into_iter()
                    .filter(|handle| session.boundaries.consumer_drain_completed(*handle))
                    .collect::<Vec<_>>();
                if !ready.is_empty() {
                    let mut decoder = session
                        .decoder
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    for handle in ready {
                        if let Some((metadata, _, _)) = session.boundaries.closing_facts(handle) {
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
                        if finalize_ready_execution(
                            handle,
                            runtime,
                            &mut decoder,
                            &session.boundaries,
                            &session.writer,
                            &session.clock,
                        ) {
                            *slot = None;
                        }
                    }
                }
            }
        }

        Self::run_publication(session, now, false);
        progress
    }

    /// Drives the writer's publication cycle. `force` publishes everything
    /// publishable regardless of the batching triggers (flush/engine-close).
    #[cfg(not(target_arch = "wasm32"))]
    fn run_publication(session: &OnSession, now: std::time::Instant, force: bool) {
        let mut decoder = session
            .decoder
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut writer = session
            .writer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let env = WriterEnv {
            publishers: &session.publishers,
        };
        writer.publish_if_due(now, force, &mut decoder, &env);
    }

    /// Flush path (`flush_and_join` / `engine_closed`): drain any remaining
    /// admission facts and publish everything publishable now.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn force_publish(&self) {
        let SessionKind::On(session) = &self.kind else {
            return;
        };
        let now = std::time::Instant::now();
        let admitted = session.boundaries.take_admitted(&session.clock);
        if !admitted.is_empty() {
            session
                .writer
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .enqueue_admitted(admitted, now);
        }
        Self::run_publication(session, now, true);
    }

    /// The batching age trigger of this session, for the consumer's park
    /// timeout (`min(WAKE_INTERVAL, publish_interval)`).
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub(crate) fn publish_interval(&self) -> Option<std::time::Duration> {
        match &self.kind {
            SessionKind::Off => None,
            SessionKind::On(session) => Some(session.config.publish_interval),
        }
    }

    /// Engine activation (streams spec §7.2): rides the registry-side
    /// vector, drained by `take_admitted` after the slot scan — never the
    /// lossy producer lane.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn engine_started(
        &self,
        engine_id: EngineId,
        program_id: ProgramId,
        function_table_cid: Option<super::ValueCid>,
        revision_label: Option<String>,
        source_label: Option<String>,
    ) {
        let SessionKind::On(session) = &self.kind else {
            return;
        };
        let queued = session
            .boundaries
            .engine_started(MetaRecord::EngineStarted {
                engine_id,
                program_id,
                function_table_cid,
                revision_label,
                source_label,
            });
        // More than 64 engines: force the meta-pre publication instead of
        // growing the vector without bound (streams spec §5.4).
        if queued >= 64 {
            super::hooks::wake_consumer();
        }
    }

    /// The one deliberate synchronous publication: the engine's
    /// `FunctionTableV1` CAS object, once per engine before any root of that
    /// engine can be admitted.
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn publish_function_table(&self, encoded_body: &[u8]) -> Option<super::ValueCid> {
        let SessionKind::On(session) = &self.kind else {
            return None;
        };
        let (cid, result) = session
            .store
            .publish_cas_object(super::CodecVersion(2), encoded_body);
        match result {
            super::PublishCasResult::Published | super::PublishCasResult::Reused => Some(cid),
            super::PublishCasResult::Lost(_) | super::PublishCasResult::Conflict => {
                counters::bump(&counters::FUNCTION_TABLE_PUBLISH_FAILED, 1);
                None
            }
            // The token is parked in the store; the writer's next cycle
            // resolves it (streams spec §5.3 step 1).
            super::PublishCasResult::Indeterminate(_) => {
                counters::bump(&counters::FUNCTION_TABLE_PUBLISH_FAILED, 1);
                None
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

    /// Sole root admission seam (streams spec §5.5): no store call, no
    /// lock, no I/O, no queue send. Admission facts ride the registry slot
    /// and are drained by `take_admitted` on the consumer thread.
    #[cfg_attr(target_arch = "wasm32", allow(clippy::needless_return))]
    pub fn register_root(
        &self,
        intent: RootProfileIntent,
        root_thread_ref: ThreadRef,
        program_id: ProgramId,
    ) -> RootAdmission {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (root_thread_ref, program_id);
            let reason = match (&self.kind, intent) {
                (_, RootProfileIntent::SuppressInternal) => InactiveReason::Suppressed,
                (_, RootProfileIntent::UserRoot { .. }) => InactiveReason::Disabled,
            };
            return RootAdmission::Inactive(RootProfiler::Inactive(reason));
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            // Step 1: the admission timestamp — the durable started_ns source
            // (the root StartThread record is emitted later and can be lost).
            let admitted_ticks = crate::prof::clock::now_ticks();
            let RootProfileIntent::UserRoot { runtime_id } = intent else {
                return RootAdmission::Inactive(RootProfiler::Inactive(InactiveReason::Suppressed));
            };
            let SessionKind::On(session) = &self.kind else {
                return RootAdmission::Inactive(RootProfiler::Inactive(InactiveReason::Disabled));
            };
            // Step 3: fork guard — a child inherits the euid and stream lock
            // and would collide on sequences; children profile nothing.
            if std::process::id() != session.pid {
                return RootAdmission::Inactive(RootProfiler::Inactive(
                    InactiveReason::ForkedProcess,
                ));
            }
            // Step 4: two atomic reads; the indeterminate check is what
            // bounds `pending_meta_*` while the store cannot publish.
            if !session.store.is_normal_admission_open() || session.store.is_indeterminate() {
                return RootAdmission::Inactive(RootProfiler::Inactive(
                    InactiveReason::StoreUnavailable,
                ));
            }
            let Ok(completion) = session.boundaries.reserve_root(ExecutionMetadata {
                root_thread_ref,
                runtime_id,
                admitted_ticks,
            }) else {
                return RootAdmission::Inactive(RootProfiler::Inactive(
                    InactiveReason::ExecutionStateUnavailable,
                ));
            };
            let handle = completion.lease().handle();
            let mut slot = session.publishers[handle.slot as usize]
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            debug_assert!(slot.is_none());
            *slot = Some(ExecutionRuntime::new(
                handle.generation,
                root_thread_ref,
                runtime_id,
                program_id,
            ));
            drop(slot);
            RootAdmission::Active(ActiveRootAdmission {
                profiler: RootProfiler::Active(ActiveRootProfiler { root_thread_ref }),
                completion,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prof::backend::{DiskBudget, ExecutionEndStatus};

    #[cfg(not(target_arch = "wasm32"))]
    fn test_config(root: &std::path::Path, euid: crate::ids::ProcessEuid) -> ProfilerConfig {
        ProfilerConfig {
            enabled: true,
            store_root: root.to_owned(),
            process_memory_bytes: 32 * 1024 * 1024,
            disk: DiskBudget {
                max_project_bytes: 16 * 1024 * 1024,
                minimum_free_bytes: 0,
            },
            // Manual publication: tests drive the cycle with force_publish.
            publish_interval: std::time::Duration::MAX,
            stream: Some(euid),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn load_execution(
        root: &std::path::Path,
        euid: crate::ids::ProcessEuid,
        thread: ThreadRef,
    ) -> crate::prof::backend::ExecutionProfile {
        let reader =
            crate::prof::backend::StreamReader::open(root, crate::prof::backend::StreamId(euid))
                .unwrap();
        reader
            .execution(crate::ids::ExecutionId(thread))
            .unwrap()
            .load()
            .unwrap()
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn producer_command_lane_never_takes_decoder_lock_and_reports_saturation() {
        assert!(
            std::mem::size_of::<DecoderCommand>()
                <= usize::try_from(MeasuredLayouts::V1.evidence_item_min_bytes).unwrap()
        );
        let temp = tempfile::TempDir::new().unwrap();
        let euid = crate::ids::ProcessEuid([40; 16]);
        let (session, diagnostic) =
            ProfilerSession::from_config(test_config(&temp.path().join(".baml/profiles-v1"), euid));
        assert!(diagnostic.is_none());
        let thread_ref = ThreadRef {
            process_euid: euid,
            engine_id: crate::ids::EngineId(2),
            thread_id: crate::ids::BexThreadId(3),
        };
        let admission = session.register_root(
            RootProfileIntent::UserRoot {
                runtime_id: BoundaryId::from_bytes([4; 16]),
            },
            thread_ref,
            ProgramId([5; 16]),
        );
        let handle = admission.boundary_handle().expect("active execution");
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
    fn active_execution_exposes_o1_committed_checkpoints() {
        let temp = tempfile::TempDir::new().unwrap();
        let euid = crate::ids::ProcessEuid([7; 16]);
        let (session, diagnostic) =
            ProfilerSession::from_config(test_config(&temp.path().join(".baml/profiles-v1"), euid));
        assert!(diagnostic.is_none());
        let thread_ref = ThreadRef {
            process_euid: euid,
            engine_id: crate::ids::EngineId(2),
            thread_id: crate::ids::BexThreadId(3),
        };
        let admission = session.register_root(
            RootProfileIntent::UserRoot {
                runtime_id: BoundaryId::from_bytes([0x44; 16]),
            },
            thread_ref,
            ProgramId([4; 16]),
        );
        let RootAdmission::Active(admission) = admission else {
            panic!("root must be admitted");
        };
        let stream = session.stream_checkpoint().expect("live stream checkpoint");
        assert_eq!(stream.high_water.meta, 0);
        assert_eq!(stream.high_water.data, 0);
        assert_eq!(stream.pending_groups, 0);
        // The StreamStarted header is pending from session start.
        assert_eq!(stream.pending_meta, 1);
        assert!(!stream.publication_inflight);
        let execution = session
            .execution_checkpoint(admission.completion.lease().handle())
            .expect("live execution checkpoint");
        assert_eq!(execution.root, thread_ref);
        assert_eq!(execution.queued.evidence_fact_count, 0);
        assert_eq!(execution.data_first_seq, 0);
        assert_eq!(execution.data_last_seq, 0);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn final_drain_keeps_structural_end_when_value_command_was_lost() {
        use crate::{
            ids::{BexCallId, BexThreadId, CallRef, EngineId, FunctionId, ProcessEuid},
            prof::record::{FunctionEndStatus, MAX_RECORD_LEN, RawRecord},
        };

        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path().join(".baml/profiles-v1");
        let euid = ProcessEuid([41; 16]);
        let runtime_id = BoundaryId::from_bytes([0x55; 16]);
        let thread_ref = ThreadRef {
            process_euid: euid,
            engine_id: EngineId(2),
            thread_id: BexThreadId(3),
        };
        let (session, diagnostic) = ProfilerSession::from_config(test_config(&root, euid));
        assert!(diagnostic.is_none());
        let RootAdmission::Active(admission) = session.register_root(
            RootProfileIntent::UserRoot { runtime_id },
            thread_ref,
            ProgramId([4; 16]),
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
        admission.completion.complete(ExecutionEndStatus::Failed);
        assert!(session.maintain_ready_executions());
        assert!(session.maintain_ready_executions());
        session.force_publish();

        let profile = load_execution(&root, euid, thread_ref);
        assert_eq!(
            profile.summary.status,
            crate::prof::backend::ExecutionStatus::Failed
        );
        assert_eq!(profile.summary.runtime_id, Some(runtime_id));
        assert_eq!(
            profile.data_state,
            crate::prof::backend::DataState::Complete
        );
        let span = profile.spans.get(&call_ref).expect("selected span");
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
        let health = profile.summary.health.expect("terminal health");
        assert_eq!(health.value_attempt_transport_exceeded, 1);
        assert_eq!(
            profile
                .contexts
                .values()
                .map(|context| context.counters.completed_error)
                .sum::<u64>(),
            1
        );
        // Thread lifecycle is durable: the root thread has a start fact.
        let thread = profile.threads.get(&thread_ref).expect("root thread");
        assert!(thread.start.is_some());
    }

    /// A long chain of same-thread call starts (and their ends) parks before
    /// the owning thread's start record arrives, then resolves in one
    /// `resolve_starts_for_thread` pass. Runs on a deliberately small stack:
    /// the resolution once recursed one frame per parked sibling and
    /// overflowed the consumer thread under exactly this shape.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn parked_sibling_chain_resolves_iteratively_on_a_small_stack() {
        use crate::{
            ids::{BexCallId, BexThreadId, EngineId, FunctionId, ProcessEuid},
            prof::record::{FunctionEndStatus, MAX_RECORD_LEN, RawRecord},
        };

        std::thread::Builder::new()
            .stack_size(512 * 1024)
            .spawn(|| {
                const CHAIN: u64 = 6_000;
                let temp = tempfile::TempDir::new().unwrap();
                let root = temp.path().join(".baml/profiles-v1");
                let euid = ProcessEuid([57; 16]);
                let thread_ref = ThreadRef {
                    process_euid: euid,
                    engine_id: EngineId(2),
                    thread_id: BexThreadId(3),
                };
                let (session, diagnostic) = ProfilerSession::from_config(test_config(&root, euid));
                assert!(diagnostic.is_none());
                let RootAdmission::Active(admission) = session.register_root(
                    RootProfileIntent::UserRoot {
                        runtime_id: BoundaryId::from_bytes([0x66; 16]),
                    },
                    thread_ref,
                    ProgramId([5; 16]),
                ) else {
                    panic!("root must be admitted");
                };
                let emit = |record: RawRecord<'_>| {
                    let mut bytes = [0; MAX_RECORD_LEN];
                    let len = record.encode(&mut bytes);
                    session.consume_raw_bytes(
                        thread_ref.process_euid,
                        thread_ref.engine_id,
                        &bytes[..len],
                    );
                };
                let flags = resolve_capture_plan(false, FunctionCaptureClass::Ordinary, None)
                    .to_call_flags();
                // Every start and end parks: the thread is not yet known.
                for i in 0..CHAIN {
                    emit(RawRecord::CallFunction {
                        flags,
                        thread_id: thread_ref.thread_id,
                        call_id: BexCallId(10 + i),
                        parent_call_id: BexCallId(0),
                        function_id: FunctionId(7),
                        call_site: None,
                        ts_ticks: 20 + 2 * i,
                    });
                    emit(RawRecord::EndFunction {
                        status: FunctionEndStatus::Ok,
                        thread_id: thread_ref.thread_id,
                        call_id: BexCallId(10 + i),
                        ts_ticks: 21 + 2 * i,
                    });
                }
                // The thread start resolves the whole parked chain at once.
                emit(RawRecord::StartThread {
                    flags: 0,
                    thread_id: thread_ref.thread_id,
                    parent_thread_id: BexThreadId(0),
                    parent_call_id: BexCallId(0),
                    ts_ticks: 10,
                    name: b"",
                });
                admission.completion.complete(ExecutionEndStatus::Succeeded);
                assert!(session.maintain_ready_executions());
                assert!(session.maintain_ready_executions());
                session.force_publish();

                let profile = load_execution(&root, euid, thread_ref);
                assert_eq!(
                    profile.summary.status,
                    crate::prof::backend::ExecutionStatus::Succeeded
                );
                assert_eq!(
                    profile.data_state,
                    crate::prof::backend::DataState::Complete
                );
                assert_eq!(
                    profile
                        .contexts
                        .values()
                        .map(|context| context.counters.completed_ok)
                        .sum::<u64>(),
                    CHAIN
                );
            })
            .expect("spawn small-stack resolver thread")
            .join()
            .expect("parked-chain resolution must not overflow the stack");
    }

    /// Canary #4548's A.2 ordering regression, ported to the streams API.
    /// `children` child calls (and their ends) park before the thread is
    /// known, then the parent start parks, and -- when `park_parent_end` --
    /// the parent's own end parks too. The thread start then resolves the
    /// whole burst in one sweep.
    ///
    /// The invariant under test: a call's parked end is consumed only after
    /// every same-thread descendant has opened, because consuming it strips
    /// the context key those children's starts need. If the order inverts,
    /// the children lose their parent context and the child counters below
    /// come up short. The 256 KiB stack keeps the other half of the
    /// guarantee honest -- no stack frame per parked sibling.
    #[cfg(not(target_arch = "wasm32"))]
    fn resolve_parked_burst(children: u64, park_parent_end: bool, euid_byte: u8) {
        use crate::{
            ids::{BexCallId, BexThreadId, EngineId, FunctionId, ProcessEuid},
            prof::{
                backend::{DataState, EdgeKind, ExecutionStatus},
                record::{FunctionEndStatus, MAX_RECORD_LEN, RawRecord, ThreadEndStatus},
            },
        };

        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path().join(".baml/profiles-v1");
        let euid = ProcessEuid([euid_byte; 16]);
        let engine_id = EngineId(2);
        let root_thread = BexThreadId(3);
        let root_thread_ref = ThreadRef {
            process_euid: euid,
            engine_id,
            thread_id: root_thread,
        };
        let (session, diagnostic) = ProfilerSession::from_config(test_config(&root, euid));
        assert!(diagnostic.is_none());
        let RootAdmission::Active(admission) = session.register_root(
            RootProfileIntent::UserRoot {
                runtime_id: BoundaryId::from_bytes([0x77; 16]),
            },
            root_thread_ref,
            ProgramId([4; 16]),
        ) else {
            panic!("root must be admitted");
        };
        let emit = |record: RawRecord<'_>| {
            let mut bytes = [0; MAX_RECORD_LEN];
            let len = record.encode(&mut bytes);
            session.consume_raw_bytes(euid, engine_id, &bytes[..len]);
        };
        let ordinary =
            resolve_capture_plan(false, FunctionCaptureClass::Ordinary, None).to_call_flags();
        let call =
            |call_id, parent_call_id, function_id, flags, ts_ticks| RawRecord::CallFunction {
                flags,
                thread_id: root_thread,
                call_id: BexCallId(call_id),
                parent_call_id: BexCallId(parent_call_id),
                function_id: FunctionId(function_id),
                call_site: None,
                ts_ticks,
            };
        let end = |call_id, ts_ticks| RawRecord::EndFunction {
            status: FunctionEndStatus::Ok,
            thread_id: root_thread,
            call_id: BexCallId(call_id),
            ts_ticks,
        };

        // The whole burst arrives before the thread is known: children (and
        // their ends) first, then the parent start, optionally its end.
        for child in 0..children {
            let call_id = 2 + child;
            let ts = 1_000 + call_id * 4;
            emit(call(call_id, 1, 20, ordinary, ts));
            emit(end(call_id, ts + 1));
        }
        // The parent captures no values: its end must therefore retire the
        // call outright rather than deferring via `waits_for_value`, so the
        // ordering below is load-bearing -- retiring it early really would
        // strip the context key the parked children's starts need.
        emit(call(1, 0, 10, ordinary, 100));
        let parent_end_ts = 1_000 + (2 + children) * 4 + 10;
        if park_parent_end {
            emit(end(1, parent_end_ts));
        }

        // The root thread start resolves everything parked above in one sweep.
        emit(RawRecord::StartThread {
            flags: 0,
            thread_id: root_thread,
            parent_thread_id: BexThreadId(0),
            parent_call_id: BexCallId(0),
            ts_ticks: 50,
            name: b"",
        });
        if !park_parent_end {
            emit(end(1, parent_end_ts));
        }
        emit(RawRecord::EndThread {
            status: ThreadEndStatus::Completed,
            thread_id: root_thread,
            ts_ticks: parent_end_ts + 10,
        });
        // The thread end arrived after its start, so nothing is parked here.
        let _ = session.resolve_thread_ends_after_sweep();

        admission.completion.complete(ExecutionEndStatus::Succeeded);
        assert!(session.maintain_ready_executions());
        assert!(session.maintain_ready_executions());
        session.force_publish();

        let profile = load_execution(&root, euid, root_thread_ref);
        assert_eq!(profile.summary.status, ExecutionStatus::Succeeded);
        assert_eq!(profile.data_state, DataState::Complete);

        // Root context plus one shared child context (same function, no site).
        assert_eq!(profile.contexts.len(), 2, "{:?}", profile.contexts);
        let (root_key, root_context) = profile
            .contexts
            .iter()
            .find(|(_, context)| {
                context
                    .tuple
                    .is_some_and(|tuple| tuple.edge_kind == EdgeKind::Root)
            })
            .expect("root context");
        assert_eq!(root_context.counters.invocations_started, 1);
        assert_eq!(root_context.counters.completed_ok, 1);
        // Every child kept the parent's context key, so all of them folded
        // under it instead of being dropped when the parent end was consumed.
        let child_context = profile
            .contexts
            .values()
            .find(|context| {
                context.tuple.is_some_and(|tuple| {
                    tuple.edge_kind == EdgeKind::Call && tuple.parent_context_key == Some(*root_key)
                })
            })
            .expect("child context under root");
        assert_eq!(child_context.counters.invocations_started, children);
        assert_eq!(child_context.counters.completed_ok, children);

        let health = profile.summary.health.expect("terminal health");
        assert_eq!(health.unmatched_call_facts, 0);
        assert_eq!(health.unmatched_thread_facts, 0);
        assert_eq!(health.join_capacity_exceeded, 0);
        assert_eq!(health.corrupt_records, 0);
        assert_eq!(health.structural_transport_exceeded, 0);
        assert!(profile.overflow.is_empty());
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn on_small_stack(body: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new()
            .name("profiler-consumer-256k".to_string())
            .stack_size(256 * 1024)
            .spawn(body)
            .unwrap()
            .join()
            .unwrap();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn burst_of_parked_children_resolves_without_recursion() {
        on_small_stack(|| resolve_parked_burst(5_000, false, 58));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn burst_of_parked_children_resolves_before_parked_parent_end() {
        on_small_stack(|| resolve_parked_burst(5_000, true, 59));
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
                backend::{ContextKey, EdgeKind},
                record::{FunctionEndStatus, MAX_RECORD_LEN, RawRecord, ThreadEndStatus},
            },
        };

        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path().join(".baml/profiles-v1");
        let euid = ProcessEuid([42; 16]);
        let runtime_id = BoundaryId::from_bytes([0x66; 16]);
        let engine_id = EngineId(2);
        let root_thread = BexThreadId(3);
        let child_thread = BexThreadId(10);
        let grandchild_thread = BexThreadId(20);
        let root_thread_ref = ThreadRef {
            process_euid: euid,
            engine_id,
            thread_id: root_thread,
        };
        let (session, diagnostic) = ProfilerSession::from_config(test_config(&root, euid));
        assert!(diagnostic.is_none());
        let RootAdmission::Active(admission) = session.register_root(
            RootProfileIntent::UserRoot { runtime_id },
            root_thread_ref,
            ProgramId([4; 16]),
        ) else {
            panic!("root must be admitted");
        };
        let emit = |record: RawRecord<'_>| {
            let mut bytes = [0; MAX_RECORD_LEN];
            let len = record.encode(&mut bytes);
            session.consume_raw_bytes(euid, engine_id, &bytes[..len]);
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

        admission.completion.complete(ExecutionEndStatus::Succeeded);
        assert!(session.maintain_ready_executions());
        assert!(session.maintain_ready_executions());
        session.force_publish();

        let profile = load_execution(&root, euid, root_thread_ref);
        assert_eq!(
            profile.summary.status,
            crate::prof::backend::ExecutionStatus::Succeeded
        );
        assert_eq!(
            profile.summary.index_state,
            crate::prof::backend::IndexState::Complete
        );
        assert_eq!(
            profile.data_state,
            crate::prof::backend::DataState::Complete
        );

        // Four contexts form one chain: Root -> Spawn -> Spawn -> Call.
        assert_eq!(profile.contexts.len(), 4);
        let find = |edge: EdgeKind, parent: Option<ContextKey>| {
            let mut matches = profile.contexts.iter().filter(|(_, context)| {
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
            profile.contexts[&nested_key]
                .tuple
                .map(|tuple| tuple.function_id),
            Some(FunctionId(30))
        );

        // The orphan call start was parked before its thread was known and
        // its parent never arrived; it must be charged to this run, not leak.
        let health = profile.summary.health.expect("terminal health");
        assert_eq!(health.unmatched_call_facts, 1);
        assert_eq!(health.unmatched_thread_facts, 0);
        assert_eq!(health.join_capacity_exceeded, 0);
        assert_eq!(health.corrupt_records, 0);
        assert_eq!(health.structural_transport_exceeded, 0);
        assert!(profile.overflow.is_empty());

        // Thread lifecycle is durable: all three threads have start AND end
        // facts, with spawn lineage.
        assert_eq!(profile.threads.len(), 3);
        let child_ref = ThreadRef {
            thread_id: child_thread,
            ..root_thread_ref
        };
        let grandchild_ref = ThreadRef {
            thread_id: grandchild_thread,
            ..root_thread_ref
        };
        for thread_ref in [root_thread_ref, child_ref, grandchild_ref] {
            let thread = profile.threads.get(&thread_ref).expect("thread evidence");
            assert!(thread.start.is_some(), "start for {thread_ref:?}");
            assert!(thread.end.is_some(), "end for {thread_ref:?}");
        }
        assert_eq!(
            profile.threads[&child_ref]
                .start
                .as_ref()
                .and_then(|start| start.parent),
            Some(root_thread_ref)
        );
        assert_eq!(
            profile.threads[&grandchild_ref]
                .start
                .as_ref()
                .and_then(|start| start.parent),
            Some(child_ref)
        );
        assert!(profile.thread_issues.is_empty());

        // The root was the only selected span and it closed cleanly.
        let root_call = CallRef {
            process_euid: euid,
            engine_id,
            thread_id: root_thread,
            call_id: BexCallId(6),
        };
        assert_eq!(profile.spans.len(), 1);
        assert_eq!(
            profile.spans[&root_call].end.map(|end| end.status),
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
        let admission = session.register_root(
            RootProfileIntent::UserRoot {
                runtime_id: BoundaryId::from_bytes([1; 16]),
            },
            ThreadRef {
                process_euid: crate::ids::ProcessEuid([1; 16]),
                engine_id: crate::ids::EngineId(1),
                thread_id: crate::ids::BexThreadId(1),
            },
            ProgramId([1; 16]),
        );
        let root = admission.profiler();
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
        let temp = tempfile::TempDir::new().unwrap();
        let (session, diagnostic) = ProfilerSession::from_config(ProfilerConfig {
            store_root: temp.path().join(".baml/profiles-v1"),
            ..ProfilerConfig::default()
        });
        assert!(diagnostic.is_none());
        let admission = session.register_root(
            RootProfileIntent::SuppressInternal,
            ThreadRef {
                process_euid: crate::ids::ProcessEuid([1; 16]),
                engine_id: crate::ids::EngineId(1),
                thread_id: crate::ids::BexThreadId(1),
            },
            ProgramId([1; 16]),
        );
        assert_eq!(
            admission.profiler(),
            RootProfiler::Inactive(InactiveReason::Suppressed)
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn on_session_owns_policy_and_activates_user_root() {
        let temp = tempfile::TempDir::new().unwrap();
        let euid = crate::ids::ProcessEuid([9; 16]);
        let (session, diagnostic) =
            ProfilerSession::from_config(test_config(&temp.path().join(".baml/profiles-v1"), euid));
        assert!(diagnostic.is_none());
        assert!(session.is_on());
        assert!(session.sizing().is_some());
        assert!(session.memory().is_some());
        assert!(session.boundary_registry().is_some());
        let root_thread_ref = ThreadRef {
            process_euid: euid,
            engine_id: crate::ids::EngineId(1),
            thread_id: crate::ids::BexThreadId(1),
        };
        let admission = session.register_root(
            RootProfileIntent::UserRoot {
                runtime_id: BoundaryId::from_bytes([3; 16]),
            },
            root_thread_ref,
            ProgramId([1; 16]),
        );
        assert_eq!(
            admission.profiler(),
            RootProfiler::Active(ActiveRootProfiler { root_thread_ref })
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

    /// Admission performs no filesystem call: a platform that panics on any
    /// I/O once armed; `register_root` stays `Active` with p99 < 20 µs over
    /// 10k roots (streams spec §9).
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn admission_performs_no_store_io_and_meets_the_latency_gate() {
        use std::sync::atomic::{AtomicBool, Ordering};

        #[derive(Debug)]
        struct PanicOnIoPlatform {
            armed: AtomicBool,
        }

        impl PanicOnIoPlatform {
            fn check(&self, what: &str) {
                assert!(
                    !self.armed.load(Ordering::Relaxed),
                    "root admission must perform no store I/O ({what})"
                );
            }
        }

        impl crate::prof::backend::StorePlatform for PanicOnIoPlatform {
            fn available_space(&self, _path: &std::path::Path) -> std::io::Result<u64> {
                self.check("available_space");
                Ok(u64::MAX)
            }

            fn sync_dir(&self, _path: &std::path::Path) -> std::io::Result<()> {
                self.check("sync_dir");
                Ok(())
            }

            fn sync_file(&self, _file: &std::fs::File) -> std::io::Result<()> {
                self.check("sync_file");
                Ok(())
            }

            fn before_rename(
                &self,
                _kind: crate::prof::backend::StoreFileKind,
                _temporary: &std::path::Path,
            ) -> std::io::Result<()> {
                self.check("before_rename");
                Ok(())
            }
        }

        let temp = tempfile::TempDir::new().unwrap();
        let euid = crate::ids::ProcessEuid([43; 16]);
        let platform = Arc::new(PanicOnIoPlatform {
            armed: AtomicBool::new(false),
        });
        let (session, diagnostic) = ProfilerSession::from_config_with_platform(
            ProfilerConfig {
                // 256 MiB sizing: 2016 slots (the §9 gate baseline).
                process_memory_bytes: 256 * 1024 * 1024,
                ..test_config(&temp.path().join(".baml/profiles-v1"), euid)
            },
            Arc::clone(&platform) as Arc<dyn crate::prof::backend::StorePlatform>,
        );
        assert!(diagnostic.is_none(), "{diagnostic:?}");
        platform.armed.store(true, Ordering::Relaxed);

        let mut latencies_ns: Vec<u64> = Vec::with_capacity(10_000);
        let mut admitted = Vec::new();
        for index in 0..10_000u64 {
            let root = ThreadRef {
                process_euid: euid,
                engine_id: crate::ids::EngineId(1),
                thread_id: crate::ids::BexThreadId(index + 1),
            };
            let start = std::time::Instant::now();
            let admission = session.register_root(
                RootProfileIntent::UserRoot {
                    runtime_id: BoundaryId::from_bytes([7; 16]),
                },
                root,
                ProgramId([5; 16]),
            );
            latencies_ns.push(u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX));
            let RootAdmission::Active(admission) = admission else {
                panic!("root {index} must be admitted");
            };
            admitted.push(admission);
            // Recycle slots outside the timed region (no publication happens:
            // manual interval, no force).
            if admitted.len() == 1024 {
                for admission in admitted.drain(..) {
                    admission.completion.complete(ExecutionEndStatus::Succeeded);
                }
                assert!(session.maintain_ready_executions());
                session.maintain_ready_executions();
            }
        }
        for admission in admitted.drain(..) {
            admission.completion.complete(ExecutionEndStatus::Succeeded);
        }
        session.maintain_ready_executions();
        session.maintain_ready_executions();
        platform.armed.store(false, Ordering::Relaxed);

        latencies_ns.sort_unstable();
        let p99 = latencies_ns[latencies_ns.len() * 99 / 100];
        assert!(
            p99 < 20_000,
            "admission p99 must stay under 20 µs, was {p99} ns"
        );
    }

    /// While the store is indeterminate, `register_root` returns
    /// `Inactive(StoreUnavailable)` and `pending_meta_*` does not grow
    /// (streams spec §9).
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn indeterminate_store_rejects_admission_without_growing_pending_meta() {
        use std::sync::atomic::{AtomicBool, Ordering};

        #[derive(Debug, Default)]
        struct FailNextDirSync {
            armed: AtomicBool,
        }

        impl crate::prof::backend::StorePlatform for FailNextDirSync {
            fn available_space(&self, _path: &std::path::Path) -> std::io::Result<u64> {
                Ok(u64::MAX)
            }

            fn sync_dir(&self, _path: &std::path::Path) -> std::io::Result<()> {
                if self.armed.swap(false, Ordering::Relaxed) {
                    return Err(std::io::Error::other("injected"));
                }
                Ok(())
            }
        }

        let temp = tempfile::TempDir::new().unwrap();
        let euid = crate::ids::ProcessEuid([44; 16]);
        let platform = Arc::new(FailNextDirSync::default());
        let (session, diagnostic) = ProfilerSession::from_config_with_platform(
            test_config(&temp.path().join(".baml/profiles-v1"), euid),
            Arc::clone(&platform) as Arc<dyn crate::prof::backend::StorePlatform>,
        );
        assert!(diagnostic.is_none());
        platform.armed.store(true, Ordering::Relaxed);
        // The function-table CAS publication goes post-rename indeterminate;
        // its token parks in the store.
        assert!(session.publish_function_table(b"table-bytes").is_none());

        let pending_before = session.stream_checkpoint().unwrap().pending_meta;
        let admission = session.register_root(
            RootProfileIntent::UserRoot {
                runtime_id: BoundaryId::from_bytes([7; 16]),
            },
            ThreadRef {
                process_euid: euid,
                engine_id: crate::ids::EngineId(1),
                thread_id: crate::ids::BexThreadId(1),
            },
            ProgramId([5; 16]),
        );
        assert!(matches!(
            admission.profiler(),
            RootProfiler::Inactive(InactiveReason::StoreUnavailable)
        ));
        assert_eq!(
            session.stream_checkpoint().unwrap().pending_meta,
            pending_before
        );

        // The writer's next cycle picks the parked token up and resolves it;
        // admission reopens.
        session.force_publish();
        let admission = session.register_root(
            RootProfileIntent::UserRoot {
                runtime_id: BoundaryId::from_bytes([7; 16]),
            },
            ThreadRef {
                process_euid: euid,
                engine_id: crate::ids::EngineId(1),
                thread_id: crate::ids::BexThreadId(2),
            },
            ProgramId([5; 16]),
        );
        assert!(admission.profiler().is_active());
    }

    /// A second session in one process (same euid) resumes sequences and
    /// emits no second `StreamStarted` (streams spec §9).
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn sequential_reopen_emits_no_second_stream_started() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path().join(".baml/profiles-v1");
        let euid = crate::ids::ProcessEuid([45; 16]);
        let thread = |thread_id| ThreadRef {
            process_euid: euid,
            engine_id: crate::ids::EngineId(1),
            thread_id: crate::ids::BexThreadId(thread_id),
        };
        for round in 0..2u64 {
            let (session, diagnostic) = ProfilerSession::from_config(test_config(&root, euid));
            assert!(diagnostic.is_none(), "{diagnostic:?}");
            let RootAdmission::Active(admission) = session.register_root(
                RootProfileIntent::UserRoot {
                    runtime_id: BoundaryId::from_bytes([7; 16]),
                },
                thread(round + 1),
                ProgramId([5; 16]),
            ) else {
                panic!("root must be admitted");
            };
            admission.completion.complete(ExecutionEndStatus::Succeeded);
            session.maintain_ready_executions();
            session.maintain_ready_executions();
            session.force_publish();
        }
        let stream = crate::prof::backend::StreamId(euid);
        let mut stream_started_count = 0;
        for sequence in 1..=8 {
            let path = crate::prof::backend::segment_path(
                &root,
                stream,
                crate::prof::backend::Plane::Meta,
                sequence,
            );
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let decoded = crate::prof::backend::decode_meta_segment(&bytes, euid).unwrap();
            stream_started_count += decoded
                .records
                .iter()
                .filter(|record| matches!(record, MetaRecord::StreamStarted { .. }))
                .count();
        }
        assert_eq!(stream_started_count, 1);
        // Both executions listed from the resumed stream.
        assert_eq!(
            crate::prof::backend::list_executions(&root).unwrap().len(),
            2
        );
    }

    /// With `publish_interval` set, a completed root is on disk without any
    /// flush once the age trigger fires (streams spec §9).
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn age_trigger_publishes_without_a_flush() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path().join(".baml/profiles-v1");
        let euid = crate::ids::ProcessEuid([46; 16]);
        let (session, diagnostic) = ProfilerSession::from_config(ProfilerConfig {
            publish_interval: std::time::Duration::from_millis(150),
            ..test_config(&root, euid)
        });
        assert!(diagnostic.is_none());
        let RootAdmission::Active(admission) = session.register_root(
            RootProfileIntent::UserRoot {
                runtime_id: BoundaryId::from_bytes([7; 16]),
            },
            ThreadRef {
                process_euid: euid,
                engine_id: crate::ids::EngineId(1),
                thread_id: crate::ids::BexThreadId(1),
            },
            ProgramId([5; 16]),
        ) else {
            panic!("root must be admitted");
        };
        admission.completion.complete(ExecutionEndStatus::Succeeded);
        session.maintain_ready_executions();
        session.maintain_ready_executions();
        // Not yet due.
        assert_eq!(
            crate::prof::backend::list_executions(&root).unwrap().len(),
            0
        );
        std::thread::sleep(std::time::Duration::from_millis(200));
        session.maintain_ready_executions();
        // May need a second cycle: the RootEnded becomes eligible once its
        // (empty) group set has drained; this execution has no data at all,
        // so one due cycle publishes pre and post together.
        session.maintain_ready_executions();
        let executions = crate::prof::backend::list_executions(&root).unwrap();
        assert_eq!(executions.len(), 1);
        assert_eq!(
            executions[0].status,
            crate::prof::backend::ExecutionStatus::Succeeded
        );
    }

    /// Slot release is immediate at finalization, before any publication
    /// (streams spec §9): `ready_handles` empties and the slot is reusable.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn slots_release_at_finalization_before_any_publication() {
        let temp = tempfile::TempDir::new().unwrap();
        let euid = crate::ids::ProcessEuid([47; 16]);
        let (session, diagnostic) = ProfilerSession::from_config(ProfilerConfig {
            // The smallest valid budget: few slots, so reuse is observable.
            process_memory_bytes: 32 * 1024 * 1024,
            ..test_config(&temp.path().join(".baml/profiles-v1"), euid)
        });
        assert!(diagnostic.is_none());
        let slots = session.sizing().unwrap().execution_slots;
        // Churn through 3× the slot capacity without ever publishing.
        for index in 0..u64::from(slots) * 3 {
            let RootAdmission::Active(admission) = session.register_root(
                RootProfileIntent::UserRoot {
                    runtime_id: BoundaryId::from_bytes([7; 16]),
                },
                ThreadRef {
                    process_euid: euid,
                    engine_id: crate::ids::EngineId(1),
                    thread_id: crate::ids::BexThreadId(index + 1),
                },
                ProgramId([5; 16]),
            ) else {
                panic!("slot must be reusable at iteration {index}");
            };
            admission.completion.complete(ExecutionEndStatus::Succeeded);
            session.maintain_ready_executions();
            session.maintain_ready_executions();
            assert!(
                session
                    .boundary_registry()
                    .unwrap()
                    .ready_handles()
                    .is_empty()
            );
        }
        // Nothing was published: publication is not part of release.
        assert_eq!(
            session.stream_checkpoint().unwrap().high_water,
            crate::prof::backend::StreamHighWater::default()
        );
    }

    /// After `fork()`, `register_root` in the child returns
    /// `Inactive(ForkedProcess)`; the parent's stream is intact (streams
    /// spec §5.8/§9).
    #[cfg(unix)]
    #[allow(unsafe_code, clippy::borrow_as_ptr)]
    #[test]
    fn forked_child_profiles_nothing() {
        let temp = tempfile::TempDir::new().unwrap();
        let euid = crate::ids::ProcessEuid([48; 16]);
        let (session, diagnostic) =
            ProfilerSession::from_config(test_config(&temp.path().join(".baml/profiles-v1"), euid));
        assert!(diagnostic.is_none());
        let root = |thread_id| ThreadRef {
            process_euid: euid,
            engine_id: crate::ids::EngineId(1),
            thread_id: crate::ids::BexThreadId(thread_id),
        };

        // SAFETY: the child only calls register_root (no locks it could have
        // inherited mid-acquisition are taken on that path) and `_exit`s.
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0, "fork failed");
        if pid == 0 {
            let admission = session.register_root(
                RootProfileIntent::UserRoot {
                    runtime_id: BoundaryId::from_bytes([7; 16]),
                },
                root(1),
                ProgramId([5; 16]),
            );
            let code = i32::from(!matches!(
                admission.profiler(),
                RootProfiler::Inactive(InactiveReason::ForkedProcess)
            ));
            // SAFETY: _exit is the only safe way out of a forked test child.
            unsafe { libc::_exit(code) };
        }
        let mut status = 0;
        // SAFETY: plain waitpid on the child we just forked.
        let waited = unsafe { libc::waitpid(pid, std::ptr::addr_of_mut!(status), 0) };
        assert_eq!(waited, pid);
        assert!(libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0);

        // The parent still admits normally.
        let admission = session.register_root(
            RootProfileIntent::UserRoot {
                runtime_id: BoundaryId::from_bytes([7; 16]),
            },
            root(2),
            ProgramId([5; 16]),
        );
        assert!(admission.profiler().is_active());
    }

    /// 1,000 sequential roots on one engine, then flush: meta segments = 2
    /// and the data plane is O(bytes), not O(executions) (streams spec §9).
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn thousand_roots_publish_two_meta_segments_and_bounded_data() {
        use crate::prof::record::{MAX_RECORD_LEN, RawRecord};

        let temp = tempfile::TempDir::new().unwrap();
        let root_path = temp.path().join(".baml/profiles-v1");
        let euid = crate::ids::ProcessEuid([49; 16]);
        let (session, diagnostic) = ProfilerSession::from_config(ProfilerConfig {
            // 256 MiB sizing: 2016 slots, so 1,000 pending index records fit
            // the writer's fixed meta-queue bound.
            process_memory_bytes: 256 * 1024 * 1024,
            ..test_config(&root_path, euid)
        });
        assert!(diagnostic.is_none());
        let emit = |record: RawRecord<'_>| {
            let mut bytes = [0; MAX_RECORD_LEN];
            let len = record.encode(&mut bytes);
            session.consume_raw_bytes(euid, crate::ids::EngineId(1), &bytes[..len]);
        };
        for index in 0..1_000u64 {
            let thread_id = crate::ids::BexThreadId(index + 1);
            let root = ThreadRef {
                process_euid: euid,
                engine_id: crate::ids::EngineId(1),
                thread_id,
            };
            let RootAdmission::Active(admission) = session.register_root(
                RootProfileIntent::UserRoot {
                    runtime_id: BoundaryId::from_bytes([7; 16]),
                },
                root,
                ProgramId([5; 16]),
            ) else {
                panic!("root {index} must be admitted");
            };
            emit(RawRecord::StartThread {
                flags: 0,
                thread_id,
                parent_thread_id: crate::ids::BexThreadId(0),
                parent_call_id: crate::ids::BexCallId(0),
                ts_ticks: 10,
                name: b"",
            });
            emit(RawRecord::CallFunction {
                flags: resolve_capture_plan(true, FunctionCaptureClass::Ordinary, None)
                    .to_call_flags(),
                thread_id,
                call_id: crate::ids::BexCallId(1),
                parent_call_id: crate::ids::BexCallId(0),
                function_id: crate::ids::FunctionId(7),
                call_site: None,
                ts_ticks: 20,
            });
            emit(RawRecord::EndFunction {
                status: crate::prof::record::FunctionEndStatus::Ok,
                thread_id,
                call_id: crate::ids::BexCallId(1),
                ts_ticks: 30,
            });
            emit(RawRecord::EndThread {
                status: crate::prof::record::ThreadEndStatus::Completed,
                thread_id,
                ts_ticks: 40,
            });
            admission.completion.complete(ExecutionEndStatus::Succeeded);
            session.maintain_ready_executions();
            session.maintain_ready_executions();
        }
        session.force_publish();

        let stream = crate::prof::backend::StreamId(euid);
        let directory = crate::prof::backend::stream_directory(&root_path, stream);
        let count = |plane: &str| {
            std::fs::read_dir(directory.join(plane))
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry.file_name().to_string_lossy().ends_with(&format!(
                        ".baml{}",
                        if plane == "meta" { "meta" } else { "data" }
                    ))
                })
                .count()
        };
        assert_eq!(count("meta"), 2, "one pre batch, one post batch");
        let data_files = count("data");
        assert!(
            data_files <= 2,
            "1,000 executions must share a handful of data segments, found {data_files}"
        );
        let executions = crate::prof::backend::list_executions(&root_path).unwrap();
        assert_eq!(executions.len(), 1_000);
        assert!(executions.iter().all(|execution| {
            execution.index_state == crate::prof::backend::IndexState::Complete
                && execution.status == crate::prof::backend::ExecutionStatus::Succeeded
        }));
    }

    /// Reader gates (streams spec §9): listing never opens `data/`;
    /// wall-clock projection; liveness; `orphan_groups`.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn reader_lists_without_data_and_projects_wall_clock_and_liveness() {
        let temp = tempfile::TempDir::new().unwrap();
        let root_path = temp.path().join(".baml/profiles-v1");
        let euid = crate::ids::ProcessEuid([50; 16]);
        let stream = crate::prof::backend::StreamId(euid);
        let thread = |thread_id| ThreadRef {
            process_euid: euid,
            engine_id: crate::ids::EngineId(1),
            thread_id: crate::ids::BexThreadId(thread_id),
        };
        {
            let (session, diagnostic) = ProfilerSession::from_config(test_config(&root_path, euid));
            assert!(diagnostic.is_none());
            // One completed execution with data.
            let RootAdmission::Active(admission) = session.register_root(
                RootProfileIntent::UserRoot {
                    runtime_id: BoundaryId::from_bytes([7; 16]),
                },
                thread(1),
                ProgramId([5; 16]),
            ) else {
                panic!("root must be admitted");
            };
            {
                use crate::prof::record::{MAX_RECORD_LEN, RawRecord};
                let emit = |record: RawRecord<'_>| {
                    let mut bytes = [0; MAX_RECORD_LEN];
                    let len = record.encode(&mut bytes);
                    session.consume_raw_bytes(euid, crate::ids::EngineId(1), &bytes[..len]);
                };
                emit(RawRecord::StartThread {
                    flags: 0,
                    thread_id: crate::ids::BexThreadId(1),
                    parent_thread_id: crate::ids::BexThreadId(0),
                    parent_call_id: crate::ids::BexCallId(0),
                    ts_ticks: 10,
                    name: b"",
                });
            }
            admission.completion.complete(ExecutionEndStatus::Succeeded);
            session.maintain_ready_executions();
            session.maintain_ready_executions();
            session.force_publish();

            // A second, unended execution: Running while the stream is alive.
            let RootAdmission::Active(open_admission) = session.register_root(
                RootProfileIntent::UserRoot {
                    runtime_id: BoundaryId::from_bytes([8; 16]),
                },
                thread(2),
                ProgramId([5; 16]),
            ) else {
                panic!("root must be admitted");
            };
            session.maintain_ready_executions();
            session.force_publish();

            let reader = crate::prof::backend::StreamReader::open(&root_path, stream).unwrap();
            assert!(reader.alive, "in-process store must short-circuit alive");
            let executions = reader.executions();
            let open = executions
                .iter()
                .find(|execution| execution.id.0 == thread(2))
                .unwrap();
            assert_eq!(open.status, crate::prof::backend::ExecutionStatus::Running);
            assert_eq!(
                open.index_state,
                crate::prof::backend::IndexState::NoRootEnded
            );

            // Wall clock: started_unix_ns - zero_unix_ns == started_ns.
            let header = reader.header.as_ref().unwrap();
            let ended = executions
                .iter()
                .find(|execution| execution.id.0 == thread(1))
                .unwrap();
            assert_eq!(
                ended.started_unix_ns.unwrap() - header.zero_unix_ns,
                ended.started_ns.unwrap()
            );
            drop(open_admission);
        }
        // Store dropped: dead stream; unended execution reads Abandoned.
        let reader = crate::prof::backend::StreamReader::open(&root_path, stream).unwrap();
        assert!(!reader.alive);
        let executions = reader.executions();
        assert_eq!(
            executions
                .iter()
                .find(|execution| execution.id.0 == thread(2))
                .unwrap()
                .status,
            crate::prof::backend::ExecutionStatus::Abandoned
        );

        // Corrupting a data segment in range is a typed `DataIssue`.
        let data_one = crate::prof::backend::segment_path(
            &root_path,
            stream,
            crate::prof::backend::Plane::Data,
            1,
        );
        let mut bytes = std::fs::read(&data_one).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        std::fs::write(&data_one, bytes).unwrap();
        let damaged = reader
            .execution(crate::ids::ExecutionId(thread(1)))
            .unwrap()
            .load()
            .unwrap();
        let crate::prof::backend::DataState::Incomplete(issues) = damaged.data_state else {
            panic!("corrupt segment must mark the fold incomplete");
        };
        assert!(issues.iter().any(|issue| matches!(
            issue,
            crate::prof::backend::DataIssue::CorruptDataSegment(1)
        )));

        // Listing never opens data/: delete the whole plane and list again.
        std::fs::remove_dir_all(
            crate::prof::backend::stream_directory(&root_path, stream).join("data"),
        )
        .unwrap();
        assert_eq!(
            crate::prof::backend::list_executions(&root_path)
                .unwrap()
                .len(),
            2
        );
    }

    /// `orphan_groups()` finds an execution whose meta batch was lost
    /// (streams spec §9).
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn orphan_groups_find_an_execution_with_a_lost_meta_batch() {
        use std::sync::atomic::{AtomicBool, Ordering};

        #[derive(Debug, Default)]
        struct FailMetaRenames {
            armed: AtomicBool,
        }

        impl crate::prof::backend::StorePlatform for FailMetaRenames {
            fn available_space(&self, _path: &std::path::Path) -> std::io::Result<u64> {
                Ok(u64::MAX)
            }

            fn sync_dir(&self, _path: &std::path::Path) -> std::io::Result<()> {
                Ok(())
            }

            fn before_rename(
                &self,
                kind: crate::prof::backend::StoreFileKind,
                _temporary: &std::path::Path,
            ) -> std::io::Result<()> {
                if kind == crate::prof::backend::StoreFileKind::MetaSegment
                    && self.armed.load(Ordering::Relaxed)
                {
                    return Err(std::io::Error::other("injected"));
                }
                Ok(())
            }
        }

        let temp = tempfile::TempDir::new().unwrap();
        let root_path = temp.path().join(".baml/profiles-v1");
        let euid = crate::ids::ProcessEuid([55; 16]);
        let platform = Arc::new(FailMetaRenames::default());
        let (session, diagnostic) = ProfilerSession::from_config_with_platform(
            test_config(&root_path, euid),
            Arc::clone(&platform) as Arc<dyn crate::prof::backend::StorePlatform>,
        );
        assert!(diagnostic.is_none());
        let root = ThreadRef {
            process_euid: euid,
            engine_id: crate::ids::EngineId(1),
            thread_id: crate::ids::BexThreadId(1),
        };
        let RootAdmission::Active(admission) = session.register_root(
            RootProfileIntent::UserRoot {
                runtime_id: BoundaryId::from_bytes([7; 16]),
            },
            root,
            ProgramId([5; 16]),
        ) else {
            panic!("root must be admitted");
        };
        {
            use crate::prof::record::{MAX_RECORD_LEN, RawRecord};
            let emit = |record: RawRecord<'_>| {
                let mut bytes = [0; MAX_RECORD_LEN];
                let len = record.encode(&mut bytes);
                session.consume_raw_bytes(euid, crate::ids::EngineId(1), &bytes[..len]);
            };
            emit(RawRecord::StartThread {
                flags: 0,
                thread_id: crate::ids::BexThreadId(1),
                parent_thread_id: crate::ids::BexThreadId(0),
                parent_call_id: crate::ids::BexCallId(0),
                ts_ticks: 10,
                name: b"",
            });
        }
        // Lose every meta batch (StreamStarted + RootStarted, then the
        // terminal RootEnded): only the data group survives on disk.
        platform.armed.store(true, Ordering::Relaxed);
        admission.completion.complete(ExecutionEndStatus::Succeeded);
        session.maintain_ready_executions();
        session.maintain_ready_executions();
        session.force_publish();
        drop(session);

        let reader = crate::prof::backend::StreamReader::open(&root_path, stream_of(euid)).unwrap();
        assert!(reader.executions().is_empty(), "no index record survived");
        assert_eq!(reader.orphan_groups().unwrap(), vec![root]);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn stream_of(euid: crate::ids::ProcessEuid) -> crate::prof::backend::StreamId {
        crate::prof::backend::StreamId(euid)
    }

    /// Publication cost gate (streams spec §9): one execution flushed as one
    /// cycle costs `sync_file + sync_dir = 2 (open) + 4 × publications`
    /// through the platform (segment tmp file, final dir, usage tmp file,
    /// root dir per publication).
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn publication_sync_cost_is_four_per_segment_plus_open() {
        use std::sync::atomic::{AtomicU64, Ordering};

        #[derive(Debug, Default)]
        struct CountingPlatform {
            sync_files: AtomicU64,
            sync_dirs: AtomicU64,
        }

        impl crate::prof::backend::StorePlatform for CountingPlatform {
            fn available_space(&self, _path: &std::path::Path) -> std::io::Result<u64> {
                Ok(u64::MAX)
            }

            fn sync_dir(&self, _path: &std::path::Path) -> std::io::Result<()> {
                self.sync_dirs.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }

            fn sync_file(&self, _file: &std::fs::File) -> std::io::Result<()> {
                self.sync_files.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
        }

        let temp = tempfile::TempDir::new().unwrap();
        let euid = crate::ids::ProcessEuid([56; 16]);
        let platform = Arc::new(CountingPlatform::default());
        let (session, diagnostic) = ProfilerSession::from_config_with_platform(
            test_config(&temp.path().join(".baml/profiles-v1"), euid),
            Arc::clone(&platform) as Arc<dyn crate::prof::backend::StorePlatform>,
        );
        assert!(diagnostic.is_none());
        // Store open: usage.state tmp sync_file + root dir sync_dir, plus the
        // two plane-directory syncs of the open-scan.
        let open_files = platform.sync_files.load(Ordering::Relaxed);
        let open_dirs = platform.sync_dirs.load(Ordering::Relaxed);
        assert_eq!(open_files, 1);
        assert_eq!(open_dirs, 3);

        let RootAdmission::Active(admission) = session.register_root(
            RootProfileIntent::UserRoot {
                runtime_id: BoundaryId::from_bytes([7; 16]),
            },
            ThreadRef {
                process_euid: euid,
                engine_id: crate::ids::EngineId(1),
                thread_id: crate::ids::BexThreadId(1),
            },
            ProgramId([5; 16]),
        ) else {
            panic!("root must be admitted");
        };
        {
            use crate::prof::record::{MAX_RECORD_LEN, RawRecord};
            let mut bytes = [0; MAX_RECORD_LEN];
            let record = RawRecord::StartThread {
                flags: 0,
                thread_id: crate::ids::BexThreadId(1),
                parent_thread_id: crate::ids::BexThreadId(0),
                parent_call_id: crate::ids::BexCallId(0),
                ts_ticks: 10,
                name: b"",
            };
            let len = record.encode(&mut bytes);
            session.consume_raw_bytes(euid, crate::ids::EngineId(1), &bytes[..len]);
        }
        admission.completion.complete(ExecutionEndStatus::Succeeded);
        session.maintain_ready_executions();
        session.maintain_ready_executions();
        session.force_publish();

        // Three segment publications (meta pre, data, meta post), no CAS:
        // each costs one sync_file (segment tmp + usage tmp = 2 files? No:
        // per publication the segment tmp file AND the usage tmp file are
        // sync_file, the final dir AND the root dir are sync_dir — 2 + 2.
        let publish_files = platform.sync_files.load(Ordering::Relaxed) - open_files;
        let publish_dirs = platform.sync_dirs.load(Ordering::Relaxed) - open_dirs;
        assert_eq!(
            publish_files + publish_dirs,
            4 * 3,
            "three publications, four fsyncs each (got {publish_files} file + {publish_dirs} dir)"
        );
    }
}
