//! Bounded direct raw-record join and CCT fold.

use std::sync::Arc;

use rustc_hash::FxHashMap as HashMap;

use super::{
    ActiveCctEpoch, AdmittedBoundary, BoundaryEndStatus, BoundaryHandle, BoundaryPhase,
    BoundaryRef, CapturePlan, ContextAdmission, ContextRef, DerivedSizing, EdgeKind, ErrorCapture,
    ErrorCaptureAttempt, ErrorCaptureId, ErrorCaptureLossReason, EvidenceFact,
    FinishBoundaryResult, MeasuredLayouts, Owner, ParentContextRef, ProfilerMemoryGovernor,
    PublishBatchResult, Reservation, ReservationClass, ResolveIndeterminateResult, RunEnd, SpanEnd,
    SpanStart, TerminalErrorRef, TerminalErrorTarget, ValueState,
};
use crate::{
    ids::{BexCallId, CallRef, EngineId, ProcessEuid, ThreadRef},
    prof::{
        clock::TickConverter,
        record::{CallSiteSourceSpan, FunctionEndStatus, RawRecord},
    },
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BoundaryHealthSnapshot {
    pub corrupt_records: u64,
    pub active_thread_capacity_exceeded: u64,
    pub active_call_capacity_exceeded: u64,
    pub join_capacity_exceeded: u64,
    pub unmatched_call_facts: u64,
    pub unmatched_thread_facts: u64,
    pub clock_invalid: u64,
    pub cct_segment_publish_failed: u64,
    pub evidence_queue_full: u64,
    pub evidence_segment_publish_failed: u64,
    pub structural_transport_exceeded: u64,
    pub value_attempt_transport_exceeded: u64,
    pub applicable_error_unwinds: u64,
    pub error_captures_queued: u64,
    pub error_captures_committed: u64,
    pub error_capture_attempt_transport_exceeded: u64,
    pub error_capture_missing_structural_join: u64,
    pub error_capture_start_uncommitted: u64,
    pub error_capture_evidence_queue_full: u64,
    pub error_capture_evidence_publish_failed: u64,
    pub terminal_error_links_observed: u64,
    pub terminal_error_links_queued: u64,
    pub terminal_error_links_committed: u64,
    pub terminal_error_link_transport_exceeded: u64,
    pub terminal_error_link_start_uncommitted: u64,
    pub terminal_error_link_evidence_publish_failed: u64,
}

impl BoundaryHealthSnapshot {
    const ENCODED_LEN: usize = 8 * 26;

    fn encode(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::ENCODED_LEN);
        for value in [
            self.corrupt_records,
            self.active_thread_capacity_exceeded,
            self.active_call_capacity_exceeded,
            self.join_capacity_exceeded,
            self.unmatched_call_facts,
            self.unmatched_thread_facts,
            self.clock_invalid,
            self.cct_segment_publish_failed,
            self.evidence_queue_full,
            self.evidence_segment_publish_failed,
            self.structural_transport_exceeded,
            self.value_attempt_transport_exceeded,
            self.applicable_error_unwinds,
            self.error_captures_queued,
            self.error_captures_committed,
            self.error_capture_attempt_transport_exceeded,
            self.error_capture_missing_structural_join,
            self.error_capture_start_uncommitted,
            self.error_capture_evidence_queue_full,
            self.error_capture_evidence_publish_failed,
            self.terminal_error_links_observed,
            self.terminal_error_links_queued,
            self.terminal_error_links_committed,
            self.terminal_error_link_transport_exceeded,
            self.terminal_error_link_start_uncommitted,
            self.terminal_error_link_evidence_publish_failed,
        ] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        bytes
    }

    #[must_use]
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != Self::ENCODED_LEN {
            return None;
        }
        let mut chunks = bytes.chunks_exact(8);
        let mut next = || {
            let chunk: [u8; 8] = chunks.next()?.try_into().ok()?;
            Some(u64::from_be_bytes(chunk))
        };
        Some(Self {
            corrupt_records: next()?,
            active_thread_capacity_exceeded: next()?,
            active_call_capacity_exceeded: next()?,
            join_capacity_exceeded: next()?,
            unmatched_call_facts: next()?,
            unmatched_thread_facts: next()?,
            clock_invalid: next()?,
            cct_segment_publish_failed: next()?,
            evidence_queue_full: next()?,
            evidence_segment_publish_failed: next()?,
            structural_transport_exceeded: next()?,
            value_attempt_transport_exceeded: next()?,
            applicable_error_unwinds: next()?,
            error_captures_queued: next()?,
            error_captures_committed: next()?,
            error_capture_attempt_transport_exceeded: next()?,
            error_capture_missing_structural_join: next()?,
            error_capture_start_uncommitted: next()?,
            error_capture_evidence_queue_full: next()?,
            error_capture_evidence_publish_failed: next()?,
            terminal_error_links_observed: next()?,
            terminal_error_links_queued: next()?,
            terminal_error_links_committed: next()?,
            terminal_error_link_transport_exceeded: next()?,
            terminal_error_link_start_uncommitted: next()?,
            terminal_error_link_evidence_publish_failed: next()?,
        })
    }

    fn record_evidence_committed(&mut self, stats: EvidenceBatchStats) {
        self.error_captures_committed = self
            .error_captures_committed
            .saturating_add(stats.error_captures);
        self.terminal_error_links_committed = self
            .terminal_error_links_committed
            .saturating_add(stats.terminal_error_links);
    }

    fn record_evidence_publish_failed(&mut self, stats: EvidenceBatchStats) {
        self.error_capture_evidence_publish_failed = self
            .error_capture_evidence_publish_failed
            .saturating_add(stats.error_captures);
        self.terminal_error_link_evidence_publish_failed = self
            .terminal_error_link_evidence_publish_failed
            .saturating_add(stats.terminal_error_links);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueueHealthSnapshot {
    pub evidence_fact_count: u64,
    pub evidence_accounted_bytes: u64,
    pub publication_inflight: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfilerCheckpoint {
    pub boundary_id: crate::ids::BoundaryId,
    pub committed_cct_sequence: u64,
    pub committed_evidence_sequence: u64,
    pub health: BoundaryHealthSnapshot,
    pub queued_and_inflight: QueueHealthSnapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FinalizationState {
    Open,
    CctIndeterminate,
    EvidenceIndeterminate {
        batch_id: u64,
        stats: EvidenceBatchStats,
    },
    FinishIndeterminate,
}

pub(super) struct BoundaryRuntime {
    pub generation: u32,
    pub publisher: Arc<AdmittedBoundary>,
    cct: Option<ActiveCctEpoch>,
    evidence: EvidenceBatch,
    health: BoundaryHealthSnapshot,
    finalization: FinalizationState,
}

impl std::fmt::Debug for BoundaryRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BoundaryRuntime")
            .field("generation", &self.generation)
            .field("health", &self.health)
            .field("finalization", &self.finalization)
            .finish_non_exhaustive()
    }
}

impl BoundaryRuntime {
    pub(super) fn new(
        generation: u32,
        publisher: Arc<AdmittedBoundary>,
        process_euid: ProcessEuid,
        engine_id: EngineId,
    ) -> Self {
        let meta = publisher.meta();
        Self {
            generation,
            cct: Some(ActiveCctEpoch::new(
                meta.program_id,
                BoundaryRef {
                    process_euid,
                    engine_id,
                    boundary_id: meta.boundary_id,
                },
                MeasuredLayouts::V1.population_item_min_bytes,
            )),
            publisher,
            evidence: EvidenceBatch::new(),
            health: BoundaryHealthSnapshot::default(),
            finalization: FinalizationState::Open,
        }
    }

    fn fresh_epoch(&self, process_euid: ProcessEuid, engine_id: EngineId) -> ActiveCctEpoch {
        ActiveCctEpoch::new(
            self.publisher.meta().program_id,
            BoundaryRef {
                process_euid,
                engine_id,
                boundary_id: self.publisher.meta().boundary_id,
            },
            MeasuredLayouts::V1.population_item_min_bytes,
        )
    }
}

#[derive(Debug)]
struct EvidenceBatch {
    id: u64,
    facts: Vec<EvidenceFact>,
    general: Option<Reservation>,
    manual: Option<Reservation>,
}

impl EvidenceBatch {
    fn new() -> Self {
        Self {
            id: 1,
            facts: Vec::new(),
            general: None,
            manual: None,
        }
    }

    fn push(
        &mut self,
        fact: EvidenceFact,
        manual_eligible: bool,
        memory: &ProfilerMemoryGovernor,
    ) -> Result<u64, ()> {
        let charge = MeasuredLayouts::V1.evidence_item_min_bytes;
        let reservation = memory
            .try_reserve(ReservationClass::General, Owner::Evidence, charge)
            .or_else(|general_error| {
                if manual_eligible {
                    memory.try_reserve(ReservationClass::Manual, Owner::Evidence, charge)
                } else {
                    Err(general_error)
                }
            })
            .map_err(|_| ())?;
        self.push_reserved(fact, reservation)
    }

    fn push_reserved(&mut self, fact: EvidenceFact, reservation: Reservation) -> Result<u64, ()> {
        if reservation.owner() != Owner::Evidence || self.facts.try_reserve(1).is_err() {
            return Err(());
        }
        let slot = match reservation.class() {
            ReservationClass::General => &mut self.general,
            ReservationClass::Manual => &mut self.manual,
            ReservationClass::Control => return Err(()),
        };
        match slot {
            Some(aggregate) => aggregate.absorb(reservation).map_err(|_| ())?,
            None => *slot = Some(reservation),
        }
        self.facts.push(fact);
        Ok(self.id)
    }

    fn target_reached(&self, target: u64) -> bool {
        u64::try_from(self.facts.len())
            .unwrap_or(u64::MAX)
            .saturating_mul(MeasuredLayouts::V1.evidence_item_min_bytes)
            >= target
    }

    fn take(&mut self) -> Self {
        let next_id = self.id.saturating_add(1);
        std::mem::replace(
            self,
            Self {
                id: next_id,
                facts: Vec::new(),
                general: None,
                manual: None,
            },
        )
    }
}

#[derive(Debug)]
struct ThreadState {
    boundary: BoundaryHandle,
    spawn_parent: Option<ContextKeyProjection>,
    spawn_site: Option<CallSiteSourceSpan>,
    _reservation: Reservation,
}

#[derive(Clone, Copy, Debug)]
struct ContextKeyProjection(super::ContextKey);

#[derive(Debug)]
struct CallState {
    boundary: BoundaryHandle,
    context: ContextAdmission,
    context_key: Option<ContextKeyProjection>,
    parent_key: Option<ContextKeyProjection>,
    edge_kind: EdgeKind,
    roles: super::RoleMask,
    span_state: SpanState,
    manual_selected: bool,
    next_runtime_id_ordinal: u32,
    start_ticks: u64,
    values_observed: u8,
    pending_end: Option<OwnedCallEnd>,
    _reservation: Reservation,
}

impl CallState {
    fn observe_value(&mut self, role: super::ValueRole) {
        self.values_observed |= match role {
            super::ValueRole::Input => super::RoleMask::INPUT,
            super::ValueRole::Output => super::RoleMask::OUTPUT,
        };
    }

    fn waits_for_value(&self, status: FunctionEndStatus) -> bool {
        let mut required = 0;
        if self.roles.inputs() {
            required |= super::RoleMask::INPUT;
        }
        if self.roles.output() && status == FunctionEndStatus::Ok {
            required |= super::RoleMask::OUTPUT;
        }
        self.values_observed & required != required
    }

    fn next_missing_value(&self, status: FunctionEndStatus) -> Option<super::ValueRole> {
        if self.roles.inputs() && self.values_observed & super::RoleMask::INPUT == 0 {
            return Some(super::ValueRole::Input);
        }
        (self.roles.output()
            && status == FunctionEndStatus::Ok
            && self.values_observed & super::RoleMask::OUTPUT == 0)
            .then_some(super::ValueRole::Output)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpanState {
    NotSelected,
    Queued(u64),
    Durable,
    Lost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EvidenceBatchOutcome {
    Noop,
    Committed(u64),
    Lost(u64),
    Indeterminate,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct EvidenceBatchStats {
    error_captures: u64,
    terminal_error_links: u64,
}

impl EvidenceBatchStats {
    fn from_facts(facts: &[EvidenceFact]) -> Self {
        let mut stats = Self::default();
        for fact in facts {
            match fact {
                EvidenceFact::ErrorCapture(_) => {
                    stats.error_captures = stats.error_captures.saturating_add(1);
                }
                EvidenceFact::TerminalErrorRef(_) => {
                    stats.terminal_error_links = stats.terminal_error_links.saturating_add(1);
                }
                _ => {}
            }
        }
        stats
    }
}

#[derive(Clone, Copy, Debug)]
struct OwnedCallStart {
    flags: u8,
    call_ref: CallRef,
    parent_call_id: BexCallId,
    function_id: crate::ids::FunctionId,
    call_site: Option<CallSiteSourceSpan>,
    ts_ticks: u64,
}

#[derive(Debug)]
struct PendingCallStart {
    fact: OwnedCallStart,
    boundary: Option<BoundaryHandle>,
    _reservation: Reservation,
}

#[derive(Clone, Copy, Debug)]
struct OwnedCallEnd {
    call_ref: CallRef,
    status: FunctionEndStatus,
    ts_ticks: u64,
    await_ns: u64,
    await_count: u32,
}

#[derive(Debug)]
struct PendingCallEnd {
    fact: OwnedCallEnd,
    boundary: Option<BoundaryHandle>,
    _reservation: Reservation,
}

#[derive(Clone, Copy, Debug)]
struct OwnedThreadStart {
    thread_ref: ThreadRef,
    parent_call: CallRef,
    spawn_site: Option<CallSiteSourceSpan>,
}

#[derive(Debug)]
struct PendingThreadStart {
    fact: OwnedThreadStart,
    boundary: Option<BoundaryHandle>,
    _reservation: Reservation,
}

#[derive(Debug)]
struct PendingThreadEnd {
    boundary: Option<BoundaryHandle>,
    _reservation: Reservation,
}

#[derive(Debug)]
struct PendingRuntimeId {
    id: [u8; 16],
    ts_ticks: u64,
    boundary: Option<BoundaryHandle>,
    _reservation: Reservation,
}

#[derive(Debug)]
struct PendingValueOccurrence {
    handle: BoundaryHandle,
    role: super::ValueRole,
    state: super::ValueState,
    manual_eligible: bool,
    reservation: Reservation,
}

#[derive(Debug)]
struct PendingErrorAttempt {
    handle: BoundaryHandle,
    attempt: ErrorCaptureAttempt,
    value: Option<ValueState>,
    throw_context_ref: Option<ContextRef>,
    selected_dependency: Option<Result<(), ErrorCaptureLossReason>>,
    reservation: Reservation,
}

#[derive(Debug)]
struct ErrorTargetState {
    handle: BoundaryHandle,
    target: TerminalErrorTarget,
    batch_id: Option<u64>,
    _reservation: Reservation,
}

#[derive(Debug)]
struct PendingTerminalError {
    handle: BoundaryHandle,
    call_ref: CallRef,
    target: TerminalErrorTarget,
    start_dependency: Option<Result<(), ErrorCaptureLossReason>>,
    reservation: Reservation,
}

#[derive(Debug, Default)]
pub(super) struct DirectDecoder {
    threads: HashMap<ThreadRef, ThreadState>,
    calls: HashMap<CallRef, CallState>,
    pending_starts: HashMap<CallRef, PendingCallStart>,
    pending_ends: HashMap<CallRef, PendingCallEnd>,
    pending_threads: HashMap<ThreadRef, PendingThreadStart>,
    pending_thread_ends: HashMap<ThreadRef, PendingThreadEnd>,
    pending_runtime_ids: HashMap<CallRef, Vec<PendingRuntimeId>>,
    pending_values: HashMap<CallRef, Vec<PendingValueOccurrence>>,
    pending_error_attempts: HashMap<ErrorCaptureId, PendingErrorAttempt>,
    error_targets: HashMap<ErrorCaptureId, ErrorTargetState>,
    pending_terminal_errors: Vec<PendingTerminalError>,
}

pub(super) struct DecoderResources<'a> {
    pub process_euid: ProcessEuid,
    pub engine_id: EngineId,
    pub memory: &'a ProfilerMemoryGovernor,
    pub sizing: DerivedSizing,
    pub clock: &'a TickConverter,
    pub boundaries: &'a [std::sync::Mutex<Option<BoundaryRuntime>>],
}

impl DirectDecoder {
    pub(super) fn pending_thread_end_count(&self) -> usize {
        self.pending_thread_ends.len()
    }

    pub(super) fn checkpoint(
        handle: BoundaryHandle,
        boundaries: &[std::sync::Mutex<Option<BoundaryRuntime>>],
    ) -> Option<ProfilerCheckpoint> {
        let slot = boundaries.get(handle.slot as usize)?;
        let runtime = slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let runtime = runtime.as_ref()?;
        if runtime.generation != handle.generation {
            return None;
        }
        let fence = runtime.publisher.fence();
        let evidence_accounted_bytes = runtime
            .evidence
            .general
            .as_ref()
            .map_or(0, Reservation::accounted_bytes)
            .saturating_add(
                runtime
                    .evidence
                    .manual
                    .as_ref()
                    .map_or(0, Reservation::accounted_bytes),
            );
        Some(ProfilerCheckpoint {
            boundary_id: runtime.publisher.meta().boundary_id,
            committed_cct_sequence: fence.cct.last_sequence,
            committed_evidence_sequence: fence.evidence.last_sequence,
            health: runtime.health,
            queued_and_inflight: QueueHealthSnapshot {
                evidence_fact_count: u64::try_from(runtime.evidence.facts.len())
                    .unwrap_or(u64::MAX),
                evidence_accounted_bytes,
                publication_inflight: runtime.finalization != FinalizationState::Open,
            },
        })
    }

    pub(super) fn consume(&mut self, resources: &DecoderResources<'_>, raw: RawRecord<'_>) {
        match raw {
            RawRecord::StartThread {
                thread_id,
                parent_thread_id,
                ..
            } if parent_thread_id.0 == 0 => {
                let thread_ref = thread_ref(resources, thread_id);
                if let Some(boundary) = find_boundary_by_root(resources.boundaries, thread_ref) {
                    self.insert_thread(resources, thread_ref, boundary, None, None);
                    self.resolve_starts_for_thread(resources, thread_ref);
                }
            }
            RawRecord::StartThreadSpawn {
                thread_id,
                parent_thread_id,
                parent_call_id,
                spawn_site,
                ..
            } => {
                self.consume_child_thread_start(
                    resources,
                    thread_id,
                    parent_thread_id,
                    parent_call_id,
                    spawn_site,
                );
            }
            RawRecord::StartThread {
                thread_id,
                parent_thread_id,
                parent_call_id,
                ..
            } => {
                self.consume_child_thread_start(
                    resources,
                    thread_id,
                    parent_thread_id,
                    parent_call_id,
                    None,
                );
            }
            RawRecord::CallFunction {
                flags,
                thread_id,
                call_id,
                parent_call_id,
                function_id,
                call_site,
                ts_ticks,
            } => self.consume_call_start(
                resources,
                OwnedCallStart {
                    flags,
                    call_ref: CallRef {
                        process_euid: resources.process_euid,
                        engine_id: resources.engine_id,
                        thread_id,
                        call_id,
                    },
                    parent_call_id,
                    function_id,
                    call_site,
                    ts_ticks,
                },
            ),
            RawRecord::EndFunction {
                status,
                thread_id,
                call_id,
                ts_ticks,
            } => self.consume_call_end(
                resources,
                OwnedCallEnd {
                    call_ref: CallRef {
                        process_euid: resources.process_euid,
                        engine_id: resources.engine_id,
                        thread_id,
                        call_id,
                    },
                    status,
                    ts_ticks,
                    await_ns: 0,
                    await_count: 0,
                },
            ),
            RawRecord::EndFunctionAwaited {
                status,
                thread_id,
                call_id,
                ts_ticks,
                await_ns,
                await_count,
            } => self.consume_call_end(
                resources,
                OwnedCallEnd {
                    call_ref: CallRef {
                        process_euid: resources.process_euid,
                        engine_id: resources.engine_id,
                        thread_id,
                        call_id,
                    },
                    status,
                    ts_ticks,
                    await_ns,
                    await_count,
                },
            ),
            RawRecord::EndThread { thread_id, .. } => {
                let thread_ref = thread_ref(resources, thread_id);
                if self.threads.remove(&thread_ref).is_none() {
                    self.insert_pending_thread_end(resources, thread_ref);
                }
            }
            RawRecord::SetFunctionId {
                thread_id,
                call_id,
                id,
                ts_ticks,
            } => self.consume_runtime_id(
                resources,
                CallRef {
                    process_euid: resources.process_euid,
                    engine_id: resources.engine_id,
                    thread_id,
                    call_id,
                },
                id,
                ts_ticks,
            ),
        }
    }

    // Keeping the decoded event fields flat avoids constructing an additional
    // hot-path transport object.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn consume_value_occurrence(
        &mut self,
        resources: &DecoderResources<'_>,
        handle: BoundaryHandle,
        call_ref: CallRef,
        role: super::ValueRole,
        state: super::ValueState,
        manual_eligible: bool,
        reservation: Option<Reservation>,
    ) {
        if self.calls.contains_key(&call_ref) {
            if self.queue_value_occurrence(
                resources,
                handle,
                call_ref,
                role,
                state,
                manual_eligible,
            ) {
                self.record_value_observed(resources, call_ref, role);
            }
            return;
        }
        let reservation = reservation.or_else(|| {
            resources
                .memory
                .try_reserve(
                    ReservationClass::General,
                    Owner::Values,
                    MeasuredLayouts::V1.value_root_min_bytes,
                )
                .or_else(|general_error| {
                    if manual_eligible {
                        resources.memory.try_reserve(
                            ReservationClass::Manual,
                            Owner::Values,
                            MeasuredLayouts::V1.value_root_min_bytes,
                        )
                    } else {
                        Err(general_error)
                    }
                })
                .ok()
        });
        let Some(reservation) = reservation else {
            with_runtime(resources.boundaries, handle, |runtime| {
                runtime.health.evidence_queue_full =
                    runtime.health.evidence_queue_full.saturating_add(1);
            });
            return;
        };
        self.pending_values
            .entry(call_ref)
            .or_default()
            .push(PendingValueOccurrence {
                handle,
                role,
                state,
                manual_eligible,
                reservation,
            });
    }

    pub(super) fn consume_error_attempt(
        &mut self,
        resources: &DecoderResources<'_>,
        handle: BoundaryHandle,
        attempt: ErrorCaptureAttempt,
        reservation: Reservation,
    ) {
        with_runtime(resources.boundaries, handle, |runtime| {
            runtime.health.applicable_error_unwinds =
                runtime.health.applicable_error_unwinds.saturating_add(1);
        });
        if reservation.owner() != Owner::Evidence
            || self.pending_error_attempts.contains_key(&attempt.id)
            || self.error_targets.contains_key(&attempt.id)
        {
            with_runtime(resources.boundaries, handle, |runtime| {
                runtime.health.corrupt_records = runtime.health.corrupt_records.saturating_add(1);
            });
            return;
        }
        self.pending_error_attempts.insert(
            attempt.id,
            PendingErrorAttempt {
                handle,
                attempt,
                value: None,
                throw_context_ref: None,
                selected_dependency: None,
                reservation,
            },
        );
        self.refresh_error_dependencies();
        self.resolve_error_attempts(resources);
    }

    pub(super) fn complete_error_value(
        &mut self,
        resources: &DecoderResources<'_>,
        id: ErrorCaptureId,
        value: ValueState,
    ) {
        let Some(pending) = self.pending_error_attempts.get_mut(&id) else {
            return;
        };
        if pending.value.replace(value).is_some() {
            with_runtime(resources.boundaries, pending.handle, |runtime| {
                runtime.health.corrupt_records = runtime.health.corrupt_records.saturating_add(1);
            });
            return;
        }
        self.refresh_error_dependencies();
        self.resolve_error_attempts(resources);
    }

    pub(super) fn consume_terminal_error(
        &mut self,
        resources: &DecoderResources<'_>,
        handle: BoundaryHandle,
        call_ref: CallRef,
        target: TerminalErrorTarget,
        reservation: Reservation,
    ) {
        let pending = PendingTerminalError {
            handle,
            call_ref,
            target,
            start_dependency: None,
            reservation,
        };
        with_runtime(resources.boundaries, pending.handle, |runtime| {
            runtime.health.terminal_error_links_observed = runtime
                .health
                .terminal_error_links_observed
                .saturating_add(1);
        });
        if self.pending_terminal_errors.try_reserve(1).is_err() {
            with_runtime(resources.boundaries, pending.handle, |runtime| {
                runtime.health.terminal_error_link_transport_exceeded = runtime
                    .health
                    .terminal_error_link_transport_exceeded
                    .saturating_add(1);
            });
            return;
        }
        self.pending_terminal_errors.push(pending);
        self.refresh_terminal_dependencies();
        self.resolve_terminal_errors(resources);
    }

    fn queue_value_occurrence(
        &mut self,
        resources: &DecoderResources<'_>,
        handle: BoundaryHandle,
        call_ref: CallRef,
        role: super::ValueRole,
        state: super::ValueState,
        manual_eligible: bool,
    ) -> bool {
        let Some(call) = self.calls.get(&call_ref) else {
            return false;
        };
        if call.boundary != handle {
            return false;
        }
        let role_enabled = match role {
            super::ValueRole::Input => call.roles.inputs(),
            super::ValueRole::Output => call.roles.output(),
        };
        if !role_enabled {
            return false;
        }
        if !matches!(call.span_state, SpanState::Queued(_) | SpanState::Durable) {
            return true;
        }
        let context_ref = call.context.context_ref();
        with_runtime(resources.boundaries, handle, |runtime| {
            if runtime
                .evidence
                .push(
                    EvidenceFact::ValueOccurrence(super::ValueOccurrence {
                        call_ref,
                        context_ref,
                        role,
                        state,
                    }),
                    manual_eligible,
                    resources.memory,
                )
                .is_err()
            {
                runtime.health.evidence_queue_full =
                    runtime.health.evidence_queue_full.saturating_add(1);
            }
        });
        self.flush_evidence_if_target(resources, handle);
        true
    }

    fn record_value_observed(
        &mut self,
        resources: &DecoderResources<'_>,
        call_ref: CallRef,
        role: super::ValueRole,
    ) {
        let ready_end = self.calls.get_mut(&call_ref).and_then(|call| {
            call.observe_value(role);
            let end = call.pending_end?;
            (!call.waits_for_value(end.status)).then_some(end)
        });
        if let Some(end) = ready_end {
            self.consume_call_end(resources, end);
        }
    }

    fn consume_child_thread_start(
        &mut self,
        resources: &DecoderResources<'_>,
        thread_id: crate::ids::BexThreadId,
        parent_thread_id: crate::ids::BexThreadId,
        parent_call_id: BexCallId,
        spawn_site: Option<CallSiteSourceSpan>,
    ) {
        let thread_ref = thread_ref(resources, thread_id);
        let parent_call = CallRef {
            process_euid: resources.process_euid,
            engine_id: resources.engine_id,
            thread_id: parent_thread_id,
            call_id: parent_call_id,
        };
        if let Some(parent) = self.calls.get(&parent_call) {
            self.insert_thread(
                resources,
                thread_ref,
                parent.boundary,
                parent.context_key,
                spawn_site,
            );
            self.resolve_starts_for_thread(resources, thread_ref);
        } else {
            self.insert_pending_thread(
                resources,
                OwnedThreadStart {
                    thread_ref,
                    parent_call,
                    spawn_site,
                },
            );
        }
    }

    fn consume_call_start(&mut self, resources: &DecoderResources<'_>, fact: OwnedCallStart) {
        let thread_ref = ThreadRef {
            process_euid: fact.call_ref.process_euid,
            engine_id: fact.call_ref.engine_id,
            thread_id: fact.call_ref.thread_id,
        };
        let Some(thread) = self.threads.get(&thread_ref) else {
            self.insert_pending_start(resources, fact);
            return;
        };
        let boundary = thread.boundary;
        let (parent, parent_admission, edge_kind, call_site) = if fact.parent_call_id.0 == 0 {
            let parent = thread.spawn_parent.map_or(ParentContextRef::Root, |key| {
                ParentContextRef::External(key.0)
            });
            let edge = if thread.spawn_parent.is_some() {
                EdgeKind::Spawn
            } else {
                EdgeKind::Root
            };
            let site = if edge == EdgeKind::Spawn {
                thread.spawn_site
            } else {
                fact.call_site
            };
            (parent, None, edge, site)
        } else {
            let parent_ref = CallRef {
                call_id: fact.parent_call_id,
                ..fact.call_ref
            };
            let Some(parent_call) = self.calls.get(&parent_ref) else {
                self.insert_pending_start(resources, fact);
                return;
            };
            let Some(parent_key) = parent_call.context_key else {
                self.insert_pending_start(resources, fact);
                return;
            };
            (
                ParentContextRef::External(parent_key.0),
                Some(parent_call.context),
                EdgeKind::Call,
                fact.call_site,
            )
        };
        let plan = CapturePlan::from_call_flags(fact.flags).unwrap_or_else(|_| {
            with_runtime(resources.boundaries, boundary, |runtime| {
                runtime.health.corrupt_records = runtime.health.corrupt_records.saturating_add(1);
            });
            CapturePlan::default()
        });
        let parent_key = match parent {
            ParentContextRef::External(key) => Some(ContextKeyProjection(key)),
            ParentContextRef::Root | ParentContextRef::Local(_) => None,
        };
        // Fold the decoded start before retaining per-call join state. Under
        // pressure this preserves the every-start CCT invariant while the
        // active-call loss below explicitly explains why no end/evidence join
        // can be retained.
        rollover_if_needed(resources, boundary);
        let context = with_runtime_value(resources.boundaries, boundary, |runtime| {
            let epoch = runtime.cct.as_mut()?;
            let parent = match parent_admission {
                Some(ContextAdmission::Normal {
                    local_id,
                    context_ref: ContextRef::Normal(key),
                }) if epoch.context_key(local_id) == Some(key) => ParentContextRef::Local(local_id),
                _ => parent,
            };
            Some(epoch.record_start(
                parent,
                fact.function_id,
                call_site,
                edge_kind,
                plan.selected,
                resources.memory,
            ))
        });
        let Some(context) = context else {
            return;
        };
        let context_key = match context.context_ref() {
            ContextRef::Normal(key) => Some(ContextKeyProjection(key)),
            ContextRef::Overflow { .. } => None,
        };
        let Ok(reservation) = resources.memory.try_reserve(
            ReservationClass::General,
            Owner::ActiveCalls,
            MeasuredLayouts::V1.active_call_min_bytes,
        ) else {
            with_runtime(resources.boundaries, boundary, |runtime| {
                runtime.health.active_call_capacity_exceeded = runtime
                    .health
                    .active_call_capacity_exceeded
                    .saturating_add(1);
            });
            return;
        };
        let span_state = if plan.selected {
            let started_ns = resources.clock.to_ns(fact.ts_ticks);
            let parent_call_ref = (fact.parent_call_id.0 != 0).then_some(CallRef {
                call_id: fact.parent_call_id,
                ..fact.call_ref
            });
            with_runtime_value(resources.boundaries, boundary, |runtime| {
                let runtime_id = (plan.reasons.root() && !plan.reasons.manual()).then_some(
                    super::RuntimeIdAnnotation {
                        annotation_ordinal: 0,
                        runtime_id: runtime.publisher.meta().boundary_id,
                    },
                );
                match runtime.evidence.push(
                    EvidenceFact::SpanStart(SpanStart {
                        boundary_id: runtime.publisher.meta().boundary_id,
                        call_ref: fact.call_ref,
                        parent_call_ref,
                        thread_ref,
                        context_ref: context.context_ref(),
                        function_id: fact.function_id,
                        call_site,
                        edge_kind,
                        started_ns,
                        selection_reasons: plan.reasons,
                        roles: plan.roles,
                        runtime_id,
                    }),
                    plan.reasons.manual(),
                    resources.memory,
                ) {
                    Ok(batch) => Some(SpanState::Queued(batch)),
                    Err(()) => {
                        runtime.health.evidence_queue_full =
                            runtime.health.evidence_queue_full.saturating_add(1);
                        Some(SpanState::Lost)
                    }
                }
            })
            .unwrap_or(SpanState::Lost)
        } else {
            SpanState::NotSelected
        };
        self.calls.insert(
            fact.call_ref,
            CallState {
                boundary,
                context,
                context_key,
                parent_key,
                edge_kind,
                roles: plan.roles,
                span_state,
                manual_selected: plan.reasons.manual(),
                next_runtime_id_ordinal: u32::from(plan.reasons.root() && !plan.reasons.manual()),
                start_ticks: fact.ts_ticks,
                values_observed: 0,
                pending_end: None,
                _reservation: reservation,
            },
        );
        self.flush_evidence_if_target(resources, boundary);
        self.resolve_threads_for_parent(resources, fact.call_ref);
        self.resolve_runtime_ids(resources, fact.call_ref);
        self.resolve_values(resources, fact.call_ref);
        self.refresh_error_dependencies();
        self.resolve_error_attempts(resources);
        self.refresh_terminal_dependencies();
        self.resolve_terminal_errors(resources);
        if let Some(pending) = self.pending_ends.remove(&fact.call_ref) {
            self.consume_call_end(resources, pending.fact);
        }
    }

    fn consume_call_end(&mut self, resources: &DecoderResources<'_>, fact: OwnedCallEnd) {
        let Some(mut call) = self.calls.remove(&fact.call_ref) else {
            self.insert_pending_end(resources, fact);
            return;
        };
        if call.waits_for_value(fact.status) {
            if call.pending_end.replace(fact).is_some() {
                with_runtime(resources.boundaries, call.boundary, |runtime| {
                    runtime.health.corrupt_records =
                        runtime.health.corrupt_records.saturating_add(1);
                });
            }
            self.calls.insert(fact.call_ref, call);
            return;
        }
        let inclusive_ns = if fact.ts_ticks < call.start_ticks {
            with_runtime(resources.boundaries, call.boundary, |runtime| {
                runtime.health.clock_invalid = runtime.health.clock_invalid.saturating_add(1);
            });
            0
        } else {
            let start = resources.clock.to_ns(call.start_ticks);
            let end = resources.clock.to_ns(fact.ts_ticks);
            end.checked_sub(start).unwrap_or_else(|| {
                with_runtime(resources.boundaries, call.boundary, |runtime| {
                    runtime.health.clock_invalid = runtime.health.clock_invalid.saturating_add(1);
                });
                0
            })
        };
        with_runtime(resources.boundaries, call.boundary, |runtime| {
            if let Some(epoch) = runtime.cct.as_mut() {
                let local_context_is_current = match call.context {
                    super::ContextAdmission::Normal {
                        local_id,
                        context_ref: super::ContextRef::Normal(key),
                    } => epoch.context_key(local_id) == Some(key),
                    _ => false,
                };
                if local_context_is_current {
                    epoch.record_end(
                        call.context,
                        fact.status,
                        inclusive_ns,
                        fact.await_ns,
                        fact.await_count,
                    );
                } else if let Some(key) = call.context_key {
                    if epoch
                        .record_external_end(
                            key.0,
                            fact.status,
                            inclusive_ns,
                            fact.await_ns,
                            fact.await_count,
                            resources.memory,
                        )
                        .is_err()
                    {
                        runtime.health.active_call_capacity_exceeded = runtime
                            .health
                            .active_call_capacity_exceeded
                            .saturating_add(1);
                    }
                    if call.edge_kind == EdgeKind::Call
                        && let Some(parent) = call.parent_key
                        && epoch
                            .record_external_direct_child(parent.0, inclusive_ns, resources.memory)
                            .is_err()
                    {
                        runtime.health.active_call_capacity_exceeded = runtime
                            .health
                            .active_call_capacity_exceeded
                            .saturating_add(1);
                    }
                } else {
                    epoch.record_end(
                        call.context,
                        fact.status,
                        inclusive_ns,
                        fact.await_ns,
                        fact.await_count,
                    );
                }
            }
        });
        if matches!(call.span_state, SpanState::Queued(_) | SpanState::Durable) {
            with_runtime(resources.boundaries, call.boundary, |runtime| {
                if runtime
                    .evidence
                    .push(
                        EvidenceFact::SpanEnd(SpanEnd {
                            call_ref: fact.call_ref,
                            ended_ns: resources.clock.to_ns(fact.ts_ticks),
                            status: fact.status,
                            inclusive_ns,
                        }),
                        call.manual_selected,
                        resources.memory,
                    )
                    .is_err()
                {
                    runtime.health.evidence_queue_full =
                        runtime.health.evidence_queue_full.saturating_add(1);
                }
            });
            self.flush_evidence_if_target(resources, call.boundary);
        }
    }

    fn consume_runtime_id(
        &mut self,
        resources: &DecoderResources<'_>,
        call_ref: CallRef,
        id: [u8; 16],
        ts_ticks: u64,
    ) {
        let Some(call) = self.calls.get_mut(&call_ref) else {
            self.insert_pending_runtime_id(resources, call_ref, id, ts_ticks);
            return;
        };
        if matches!(call.span_state, SpanState::NotSelected | SpanState::Lost) {
            return;
        }
        let boundary = call.boundary;
        let manual_selected = call.manual_selected;
        let ordinal = call.next_runtime_id_ordinal;
        let Some(next_ordinal) = ordinal.checked_add(1) else {
            with_runtime(resources.boundaries, boundary, |runtime| {
                runtime.health.corrupt_records = runtime.health.corrupt_records.saturating_add(1);
            });
            return;
        };
        call.next_runtime_id_ordinal = next_ordinal;
        with_runtime(resources.boundaries, boundary, |runtime| {
            if runtime
                .evidence
                .push(
                    EvidenceFact::SpanRuntimeId(super::SpanRuntimeId {
                        call_ref,
                        annotation_ordinal: ordinal,
                        runtime_id: crate::ids::BoundaryId::from_bytes(id),
                    }),
                    manual_selected,
                    resources.memory,
                )
                .is_err()
            {
                runtime.health.evidence_queue_full =
                    runtime.health.evidence_queue_full.saturating_add(1);
            }
        });
        self.flush_evidence_if_target(resources, boundary);
    }

    fn insert_pending_runtime_id(
        &mut self,
        resources: &DecoderResources<'_>,
        call_ref: CallRef,
        id: [u8; 16],
        ts_ticks: u64,
    ) {
        let thread_ref = ThreadRef {
            process_euid: call_ref.process_euid,
            engine_id: call_ref.engine_id,
            thread_id: call_ref.thread_id,
        };
        let boundary = self.boundary_for_thread(thread_ref);
        match resources.memory.try_reserve(
            ReservationClass::General,
            Owner::UnresolvedJoins,
            MeasuredLayouts::V1.unresolved_fact_min_bytes,
        ) {
            Ok(reservation) => {
                self.pending_runtime_ids
                    .entry(call_ref)
                    .or_default()
                    .push(PendingRuntimeId {
                        id,
                        ts_ticks,
                        boundary,
                        _reservation: reservation,
                    });
            }
            Err(_) => {
                if let Some(boundary) = boundary {
                    with_runtime(resources.boundaries, boundary, |runtime| {
                        runtime.health.join_capacity_exceeded =
                            runtime.health.join_capacity_exceeded.saturating_add(1);
                    });
                }
            }
        }
    }

    fn boundary_for_thread(&self, thread_ref: ThreadRef) -> Option<BoundaryHandle> {
        self.threads
            .get(&thread_ref)
            .map(|thread| thread.boundary)
            .or_else(|| {
                self.pending_threads
                    .get(&thread_ref)
                    .and_then(|thread| thread.boundary)
            })
    }

    fn insert_thread(
        &mut self,
        resources: &DecoderResources<'_>,
        thread_ref: ThreadRef,
        boundary: BoundaryHandle,
        spawn_parent: Option<ContextKeyProjection>,
        spawn_site: Option<CallSiteSourceSpan>,
    ) {
        if self.threads.contains_key(&thread_ref) {
            return;
        }
        let Ok(reservation) = resources.memory.try_reserve(
            ReservationClass::General,
            Owner::Population,
            MeasuredLayouts::V1.active_thread_min_bytes,
        ) else {
            with_runtime(resources.boundaries, boundary, |runtime| {
                runtime.health.active_thread_capacity_exceeded = runtime
                    .health
                    .active_thread_capacity_exceeded
                    .saturating_add(1);
            });
            return;
        };
        self.threads.insert(
            thread_ref,
            ThreadState {
                boundary,
                spawn_parent,
                spawn_site,
                _reservation: reservation,
            },
        );
        self.attribute_pending_joins();
    }

    fn attribute_pending_joins(&mut self) {
        // Facts may have arrived on another ring before this thread's start.
        // Propagate newly known ownership through any pending descendant
        // threads, then latch it on their unresolved facts. This is a cold
        // reorder path; repeated scans avoid another ungoverned work queue.
        loop {
            let next = self
                .pending_threads
                .iter()
                .find_map(|(thread_ref, pending)| {
                    if pending.boundary.is_some() {
                        return None;
                    }
                    self.boundary_for_thread(call_thread_ref(pending.fact.parent_call))
                        .map(|boundary| (*thread_ref, boundary))
                });
            let Some((thread_ref, boundary)) = next else {
                break;
            };
            if let Some(pending) = self.pending_threads.get_mut(&thread_ref) {
                pending.boundary = Some(boundary);
            }
        }

        let threads = &self.threads;
        let pending_threads = &self.pending_threads;
        let boundary_for_thread = |thread_ref: ThreadRef| {
            threads
                .get(&thread_ref)
                .map(|thread| thread.boundary)
                .or_else(|| {
                    pending_threads
                        .get(&thread_ref)
                        .and_then(|thread| thread.boundary)
                })
        };
        for pending in self.pending_starts.values_mut() {
            if pending.boundary.is_none() {
                pending.boundary = boundary_for_thread(call_thread_ref(pending.fact.call_ref));
            }
        }
        for pending in self.pending_ends.values_mut() {
            if pending.boundary.is_none() {
                pending.boundary = boundary_for_thread(call_thread_ref(pending.fact.call_ref));
            }
        }
        for (thread_ref, pending) in &mut self.pending_thread_ends {
            if pending.boundary.is_none() {
                pending.boundary = boundary_for_thread(*thread_ref);
            }
        }
        for (call_ref, pending) in &mut self.pending_runtime_ids {
            if let Some(boundary) = boundary_for_thread(call_thread_ref(*call_ref)) {
                for annotation in pending {
                    if annotation.boundary.is_none() {
                        annotation.boundary = Some(boundary);
                    }
                }
            }
        }
    }

    fn insert_pending_start(&mut self, resources: &DecoderResources<'_>, fact: OwnedCallStart) {
        if self.pending_starts.contains_key(&fact.call_ref) {
            return;
        }
        let thread_ref = ThreadRef {
            process_euid: fact.call_ref.process_euid,
            engine_id: fact.call_ref.engine_id,
            thread_id: fact.call_ref.thread_id,
        };
        let boundary = self.boundary_for_thread(thread_ref);
        match resources.memory.try_reserve(
            ReservationClass::General,
            Owner::UnresolvedJoins,
            MeasuredLayouts::V1.unresolved_fact_min_bytes,
        ) {
            Ok(reservation) => {
                self.pending_starts.insert(
                    fact.call_ref,
                    PendingCallStart {
                        fact,
                        boundary,
                        _reservation: reservation,
                    },
                );
            }
            Err(_) => record_join_capacity_exceeded(resources.boundaries, boundary),
        }
    }

    fn insert_pending_end(&mut self, resources: &DecoderResources<'_>, fact: OwnedCallEnd) {
        if self.pending_ends.contains_key(&fact.call_ref) {
            return;
        }
        let thread_ref = ThreadRef {
            process_euid: fact.call_ref.process_euid,
            engine_id: fact.call_ref.engine_id,
            thread_id: fact.call_ref.thread_id,
        };
        let boundary = self.boundary_for_thread(thread_ref);
        match resources.memory.try_reserve(
            ReservationClass::General,
            Owner::UnresolvedJoins,
            MeasuredLayouts::V1.unresolved_fact_min_bytes,
        ) {
            Ok(reservation) => {
                self.pending_ends.insert(
                    fact.call_ref,
                    PendingCallEnd {
                        fact,
                        boundary,
                        _reservation: reservation,
                    },
                );
            }
            Err(_) => record_join_capacity_exceeded(resources.boundaries, boundary),
        }
    }

    fn insert_pending_thread(&mut self, resources: &DecoderResources<'_>, fact: OwnedThreadStart) {
        if self.pending_threads.contains_key(&fact.thread_ref) {
            return;
        }
        let parent_thread = ThreadRef {
            process_euid: fact.parent_call.process_euid,
            engine_id: fact.parent_call.engine_id,
            thread_id: fact.parent_call.thread_id,
        };
        let boundary = self
            .threads
            .get(&parent_thread)
            .map(|thread| thread.boundary)
            .or_else(|| {
                self.pending_threads
                    .get(&parent_thread)
                    .and_then(|thread| thread.boundary)
            });
        match resources.memory.try_reserve(
            ReservationClass::General,
            Owner::UnresolvedJoins,
            MeasuredLayouts::V1.unresolved_fact_min_bytes,
        ) {
            Ok(reservation) => {
                self.pending_threads.insert(
                    fact.thread_ref,
                    PendingThreadStart {
                        fact,
                        boundary,
                        _reservation: reservation,
                    },
                );
            }
            Err(_) => record_join_capacity_exceeded(resources.boundaries, boundary),
        }
    }

    fn insert_pending_thread_end(
        &mut self,
        resources: &DecoderResources<'_>,
        thread_ref: ThreadRef,
    ) {
        if self.pending_thread_ends.contains_key(&thread_ref) {
            return;
        }
        let boundary = self
            .threads
            .get(&thread_ref)
            .map(|thread| thread.boundary)
            .or_else(|| {
                self.pending_threads
                    .get(&thread_ref)
                    .and_then(|thread| thread.boundary)
            })
            .or_else(|| find_boundary_by_root(resources.boundaries, thread_ref));
        match resources.memory.try_reserve(
            ReservationClass::General,
            Owner::UnresolvedJoins,
            MeasuredLayouts::V1.unresolved_fact_min_bytes,
        ) {
            Ok(reservation) => {
                self.pending_thread_ends.insert(
                    thread_ref,
                    PendingThreadEnd {
                        boundary,
                        _reservation: reservation,
                    },
                );
            }
            Err(_) => {
                if let Some(boundary) = boundary {
                    with_runtime(resources.boundaries, boundary, |runtime| {
                        runtime.health.join_capacity_exceeded =
                            runtime.health.join_capacity_exceeded.saturating_add(1);
                    });
                }
            }
        }
    }

    fn resolve_threads_for_parent(
        &mut self,
        resources: &DecoderResources<'_>,
        parent_call: CallRef,
    ) {
        loop {
            let next = self.pending_threads.iter().find_map(|(key, pending)| {
                (pending.fact.parent_call == parent_call).then_some(*key)
            });
            let Some(key) = next else { break };
            let pending = self
                .pending_threads
                .remove(&key)
                .expect("selected pending thread exists");
            let Some(parent) = self.calls.get(&parent_call) else {
                break;
            };
            self.insert_thread(
                resources,
                pending.fact.thread_ref,
                parent.boundary,
                parent.context_key,
                pending.fact.spawn_site,
            );
            self.resolve_starts_for_thread(resources, pending.fact.thread_ref);
        }
    }

    pub(super) fn resolve_thread_ends_after_sweep(&mut self) {
        let ready = self
            .pending_thread_ends
            .keys()
            .filter(|thread_ref| self.threads.contains_key(thread_ref))
            .copied()
            .collect::<Vec<_>>();
        for thread_ref in ready {
            self.pending_thread_ends.remove(&thread_ref);
            self.threads.remove(&thread_ref);
        }
    }

    fn resolve_starts_for_thread(
        &mut self,
        resources: &DecoderResources<'_>,
        thread_ref: ThreadRef,
    ) {
        loop {
            // A consumer can observe a spawned child's descendants before the
            // spawning thread's ring publishes the child's entry call. Only
            // remove a fact whose parent is already resolvable; removing and
            // immediately reinserting an unready fact can otherwise select the
            // same hash-map entry forever and livelock the sole consumer.
            let next = self
                .pending_starts
                .iter()
                .filter(|(_, pending)| {
                    pending.fact.call_ref.thread_id == thread_ref.thread_id
                        && pending.fact.call_ref.engine_id == thread_ref.engine_id
                        && pending.fact.call_ref.process_euid == thread_ref.process_euid
                        && self.call_start_dependency_ready(&pending.fact)
                })
                .map(|(key, _)| *key)
                .min_by_key(|key| key.call_id.0);
            let Some(key) = next else { break };
            let pending = self
                .pending_starts
                .remove(&key)
                .expect("selected pending call start exists");
            self.consume_call_start(resources, pending.fact);
        }
    }

    fn call_start_dependency_ready(&self, fact: &OwnedCallStart) -> bool {
        if fact.parent_call_id.0 == 0 {
            return true;
        }
        let parent_ref = CallRef {
            call_id: fact.parent_call_id,
            ..fact.call_ref
        };
        self.calls
            .get(&parent_ref)
            .is_some_and(|parent| parent.context_key.is_some())
    }

    fn resolve_runtime_ids(&mut self, resources: &DecoderResources<'_>, call_ref: CallRef) {
        let Some(mut pending) = self.pending_runtime_ids.remove(&call_ref) else {
            return;
        };
        pending.sort_by_key(|annotation| annotation.ts_ticks);
        for annotation in pending {
            self.consume_runtime_id(resources, call_ref, annotation.id, annotation.ts_ticks);
        }
    }

    fn resolve_values(&mut self, resources: &DecoderResources<'_>, call_ref: CallRef) {
        let Some(pending) = self.pending_values.remove(&call_ref) else {
            return;
        };
        for occurrence in pending {
            self.consume_value_occurrence(
                resources,
                occurrence.handle,
                call_ref,
                occurrence.role,
                occurrence.state,
                occurrence.manual_eligible,
                Some(occurrence.reservation),
            );
        }
    }

    fn resolve_error_attempts(&mut self, resources: &DecoderResources<'_>) {
        loop {
            let ready = self
                .pending_error_attempts
                .iter()
                .find_map(|(id, pending)| match pending.selected_dependency? {
                    Err(reason) => Some((*id, Err(reason))),
                    Ok(()) => Some((*id, Ok((pending.throw_context_ref?, pending.value?)))),
                });
            let Some((id, resolution)) = ready else {
                break;
            };
            let pending = self
                .pending_error_attempts
                .remove(&id)
                .expect("ready error attempt exists");
            let (target, batch_id) = match resolution {
                Ok((throw_context_ref, value)) => {
                    let queued =
                        with_runtime_value(resources.boundaries, pending.handle, |runtime| {
                            match runtime.evidence.push(
                                EvidenceFact::ErrorCapture(ErrorCapture {
                                    id,
                                    boundary_id: runtime.publisher.meta().boundary_id,
                                    throw_call_ref: pending.attempt.throw_call_ref,
                                    throw_context_ref,
                                    throw_function_id: pending.attempt.throw_function_id,
                                    throw_site: pending.attempt.throw_site,
                                    kind: pending.attempt.kind,
                                    source: pending.attempt.source,
                                    value,
                                }),
                                pending.attempt.manual_eligible,
                                resources.memory,
                            ) {
                                Ok(batch) => {
                                    runtime.health.error_captures_queued =
                                        runtime.health.error_captures_queued.saturating_add(1);
                                    Some(batch)
                                }
                                Err(()) => {
                                    runtime.health.evidence_queue_full =
                                        runtime.health.evidence_queue_full.saturating_add(1);
                                    runtime.health.error_capture_evidence_queue_full = runtime
                                        .health
                                        .error_capture_evidence_queue_full
                                        .saturating_add(1);
                                    None
                                }
                            }
                        });
                    if let Some(batch) = queued {
                        (TerminalErrorTarget::Capture(id), Some(batch))
                    } else {
                        (
                            TerminalErrorTarget::Lost(ErrorCaptureLossReason::EvidenceQueueFull),
                            None,
                        )
                    }
                }
                Err(reason) => {
                    with_runtime(
                        resources.boundaries,
                        pending.handle,
                        |runtime| match reason {
                            ErrorCaptureLossReason::StartUncommitted => {
                                runtime.health.error_capture_start_uncommitted = runtime
                                    .health
                                    .error_capture_start_uncommitted
                                    .saturating_add(1);
                            }
                            _ => {
                                runtime.health.error_capture_missing_structural_join = runtime
                                    .health
                                    .error_capture_missing_structural_join
                                    .saturating_add(1);
                            }
                        },
                    );
                    (TerminalErrorTarget::Lost(reason), None)
                }
            };
            self.error_targets.insert(
                id,
                ErrorTargetState {
                    handle: pending.handle,
                    target,
                    batch_id,
                    _reservation: pending.reservation,
                },
            );
            self.flush_evidence_if_target(resources, pending.handle);
            self.resolve_terminal_errors(resources);
        }
    }

    fn refresh_error_dependencies(&mut self) {
        for pending in self.pending_error_attempts.values_mut() {
            if pending.throw_context_ref.is_none()
                && let Some(call) = self.calls.get(&pending.attempt.throw_call_ref)
            {
                if call.boundary == pending.handle {
                    pending.throw_context_ref = Some(call.context.context_ref());
                } else {
                    pending.selected_dependency =
                        Some(Err(ErrorCaptureLossReason::MissingStructuralJoin));
                }
            }
            if pending.selected_dependency.is_none()
                && let Some(call) = self.calls.get(&pending.attempt.first_selected_call_ref)
            {
                pending.selected_dependency = Some(if call.boundary != pending.handle {
                    Err(ErrorCaptureLossReason::MissingStructuralJoin)
                } else {
                    match call.span_state {
                        SpanState::Queued(_) | SpanState::Durable => Ok(()),
                        SpanState::Lost | SpanState::NotSelected => {
                            Err(ErrorCaptureLossReason::StartUncommitted)
                        }
                    }
                });
            }
        }
    }

    fn resolve_terminal_errors(&mut self, resources: &DecoderResources<'_>) {
        let mut index = 0;
        while index < self.pending_terminal_errors.len() {
            let pending = &self.pending_terminal_errors[index];
            let Some(start_dependency) = pending.start_dependency else {
                index += 1;
                continue;
            };
            if start_dependency.is_err() {
                let pending = self.pending_terminal_errors.swap_remove(index);
                with_runtime(resources.boundaries, pending.handle, |runtime| {
                    runtime.health.terminal_error_link_start_uncommitted = runtime
                        .health
                        .terminal_error_link_start_uncommitted
                        .saturating_add(1);
                });
                continue;
            }
            let target = match pending.target {
                TerminalErrorTarget::Capture(id) => {
                    let Some(state) = self.error_targets.get(&id) else {
                        index += 1;
                        continue;
                    };
                    state.target
                }
                lost @ TerminalErrorTarget::Lost(_) => lost,
            };
            let pending = self.pending_terminal_errors.swap_remove(index);
            with_runtime(resources.boundaries, pending.handle, |runtime| {
                if runtime
                    .evidence
                    .push_reserved(
                        EvidenceFact::TerminalErrorRef(TerminalErrorRef {
                            call_ref: pending.call_ref,
                            target,
                        }),
                        pending.reservation,
                    )
                    .is_ok()
                {
                    runtime.health.terminal_error_links_queued =
                        runtime.health.terminal_error_links_queued.saturating_add(1);
                } else {
                    runtime.health.terminal_error_link_transport_exceeded = runtime
                        .health
                        .terminal_error_link_transport_exceeded
                        .saturating_add(1);
                }
            });
            self.flush_evidence_if_target(resources, pending.handle);
        }
    }

    fn refresh_terminal_dependencies(&mut self) {
        for pending in &mut self.pending_terminal_errors {
            if pending.start_dependency.is_some() {
                continue;
            }
            let Some(call) = self.calls.get(&pending.call_ref) else {
                continue;
            };
            pending.start_dependency = Some(if call.boundary != pending.handle {
                Err(ErrorCaptureLossReason::MissingStructuralJoin)
            } else {
                match call.span_state {
                    SpanState::Queued(_) | SpanState::Durable => Ok(()),
                    SpanState::Lost | SpanState::NotSelected => {
                        Err(ErrorCaptureLossReason::StartUncommitted)
                    }
                }
            });
        }
    }

    fn flush_evidence_if_target(
        &mut self,
        resources: &DecoderResources<'_>,
        handle: BoundaryHandle,
    ) {
        let reached = with_runtime_value(resources.boundaries, handle, |runtime| {
            Some(
                runtime
                    .evidence
                    .target_reached(resources.sizing.segment_target_bytes),
            )
        })
        .unwrap_or(false);
        if reached {
            self.flush_evidence(resources, handle);
        }
    }

    fn flush_evidence(
        &mut self,
        resources: &DecoderResources<'_>,
        handle: BoundaryHandle,
    ) -> EvidenceBatchOutcome {
        let outcome = with_runtime_value(resources.boundaries, handle, |runtime| {
            if runtime.finalization != FinalizationState::Open || runtime.evidence.facts.is_empty()
            {
                return Some(EvidenceBatchOutcome::Noop);
            }

            // Context definitions are the durable dependency of every exact
            // evidence fact. Seal the current definitions before taking the
            // evidence batch so a failed definition publication cannot leave
            // a dangling ContextRef on disk.
            if let Some(epoch) = runtime.cct.take() {
                let sealed = epoch.seal();
                runtime.cct =
                    Some(runtime.fresh_epoch(resources.process_euid, resources.engine_id));
                if (!sealed.contexts.is_empty() || !sealed.overflow.is_empty())
                    && match runtime.publisher.publish_cct_epoch(&sealed) {
                        PublishBatchResult::Committed { .. } => false,
                        PublishBatchResult::Lost(_) => {
                            runtime.health.cct_segment_publish_failed =
                                runtime.health.cct_segment_publish_failed.saturating_add(1);
                            let lost = runtime.evidence.take();
                            runtime.health.record_evidence_publish_failed(
                                EvidenceBatchStats::from_facts(&lost.facts),
                            );
                            runtime.health.evidence_segment_publish_failed = runtime
                                .health
                                .evidence_segment_publish_failed
                                .saturating_add(1);
                            return Some(EvidenceBatchOutcome::Lost(lost.id));
                        }
                        PublishBatchResult::Indeterminate(_) => {
                            runtime.finalization = FinalizationState::CctIndeterminate;
                            true
                        }
                    }
                {
                    return Some(EvidenceBatchOutcome::Indeterminate);
                }
            }

            let batch = runtime.evidence.take();
            let batch_id = batch.id;
            let stats = EvidenceBatchStats::from_facts(&batch.facts);
            Some(
                match runtime.publisher.publish_evidence_facts(&batch.facts) {
                    PublishBatchResult::Committed { .. } => {
                        runtime.health.record_evidence_committed(stats);
                        EvidenceBatchOutcome::Committed(batch_id)
                    }
                    PublishBatchResult::Lost(_) => {
                        runtime.health.record_evidence_publish_failed(stats);
                        runtime.health.evidence_segment_publish_failed = runtime
                            .health
                            .evidence_segment_publish_failed
                            .saturating_add(1);
                        EvidenceBatchOutcome::Lost(batch_id)
                    }
                    PublishBatchResult::Indeterminate(_) => {
                        runtime.finalization =
                            FinalizationState::EvidenceIndeterminate { batch_id, stats };
                        EvidenceBatchOutcome::Indeterminate
                    }
                },
            )
        })
        .unwrap_or(EvidenceBatchOutcome::Noop);
        self.apply_evidence_batch_outcome(handle, outcome);
        outcome
    }

    fn apply_evidence_batch_outcome(
        &mut self,
        handle: BoundaryHandle,
        outcome: EvidenceBatchOutcome,
    ) {
        let (batch_id, state) = match outcome {
            EvidenceBatchOutcome::Committed(batch_id) => (batch_id, SpanState::Durable),
            EvidenceBatchOutcome::Lost(batch_id) => (batch_id, SpanState::Lost),
            EvidenceBatchOutcome::Noop | EvidenceBatchOutcome::Indeterminate => return,
        };
        for call in self.calls.values_mut() {
            if call.boundary == handle && call.span_state == SpanState::Queued(batch_id) {
                call.span_state = state;
            }
        }
        for error in self.error_targets.values_mut() {
            if error.handle != handle || error.batch_id != Some(batch_id) {
                continue;
            }
            error.batch_id = None;
            if matches!(outcome, EvidenceBatchOutcome::Lost(_)) {
                error.target =
                    TerminalErrorTarget::Lost(ErrorCaptureLossReason::EvidenceSegmentPublishFailed);
            }
        }
    }

    /// Once every producer lease and command has drained, a requested value
    /// that never reached the decoder can only be an admitted-attempt
    /// transport loss. Materialize that per-span loss and then fold the
    /// already-retained structural end; evidence pressure must not turn a
    /// completed CCT invocation into an unmatched call.
    pub(super) fn complete_missing_values(
        &mut self,
        resources: &DecoderResources<'_>,
        handle: BoundaryHandle,
    ) {
        loop {
            let missing = self.calls.iter().find_map(|(call_ref, call)| {
                let end = call.pending_end?;
                if call.boundary != handle {
                    return None;
                }
                call.next_missing_value(end.status)
                    .map(|role| (*call_ref, role, call.manual_selected))
            });
            let Some((call_ref, role, manual_eligible)) = missing else {
                break;
            };
            self.consume_value_occurrence(
                resources,
                handle,
                call_ref,
                role,
                ValueState::Lost(super::ValueLossReason::ValueAttemptTransportExceeded),
                manual_eligible,
                None,
            );
        }
    }

    pub(super) fn discard_boundary(&mut self, handle: BoundaryHandle) -> BoundaryHealthSnapshot {
        let mut unmatched_calls = 0u64;
        let mut unmatched_threads = 0u64;
        self.calls.retain(|_, call| {
            let remove = call.boundary == handle;
            unmatched_calls = unmatched_calls.saturating_add(u64::from(remove));
            !remove
        });
        self.threads.retain(|_, thread| {
            let remove = thread.boundary == handle;
            unmatched_threads = unmatched_threads.saturating_add(u64::from(remove));
            !remove
        });
        self.pending_starts.retain(|_, pending| {
            let remove = pending.boundary == Some(handle);
            unmatched_calls = unmatched_calls.saturating_add(u64::from(remove));
            !remove
        });
        self.pending_ends.retain(|_, pending| {
            let remove = pending.boundary == Some(handle);
            unmatched_calls = unmatched_calls.saturating_add(u64::from(remove));
            !remove
        });
        self.pending_threads.retain(|_, pending| {
            let remove = pending.boundary == Some(handle);
            unmatched_threads = unmatched_threads.saturating_add(u64::from(remove));
            !remove
        });
        self.pending_thread_ends.retain(|_, pending| {
            let remove = pending.boundary == Some(handle);
            unmatched_threads = unmatched_threads.saturating_add(u64::from(remove));
            !remove
        });
        self.pending_runtime_ids.retain(|_, pending| {
            pending.retain(|annotation| {
                let remove = annotation.boundary == Some(handle);
                unmatched_calls = unmatched_calls.saturating_add(u64::from(remove));
                !remove
            });
            !pending.is_empty()
        });
        self.pending_values.retain(|_, pending| {
            pending.retain(|occurrence| {
                let remove = occurrence.handle == handle;
                unmatched_calls = unmatched_calls.saturating_add(u64::from(remove));
                !remove
            });
            !pending.is_empty()
        });
        let mut missing_error_joins = 0u64;
        self.pending_error_attempts.retain(|_, pending| {
            let remove = pending.handle == handle;
            missing_error_joins = missing_error_joins.saturating_add(u64::from(remove));
            !remove
        });
        self.error_targets
            .retain(|_, target| target.handle != handle);
        let mut missing_terminal_joins = 0u64;
        self.pending_terminal_errors.retain(|pending| {
            let remove = pending.handle == handle;
            missing_terminal_joins = missing_terminal_joins.saturating_add(u64::from(remove));
            !remove
        });
        BoundaryHealthSnapshot {
            unmatched_call_facts: unmatched_calls,
            unmatched_thread_facts: unmatched_threads,
            error_capture_missing_structural_join: missing_error_joins,
            terminal_error_link_start_uncommitted: missing_terminal_joins,
            ..BoundaryHealthSnapshot::default()
        }
    }
}

fn rollover_if_needed(resources: &DecoderResources<'_>, handle: BoundaryHandle) {
    with_runtime(resources.boundaries, handle, |runtime| {
        if runtime.finalization != FinalizationState::Open {
            return;
        }
        let Some(epoch) = runtime.cct.as_ref() else {
            return;
        };
        let estimated_bytes = u64::try_from(epoch.cardinality())
            .unwrap_or(u64::MAX)
            .saturating_mul(MeasuredLayouts::V1.population_item_min_bytes);
        if estimated_bytes < resources.sizing.cct_epoch_target_bytes {
            return;
        }
        let next = runtime.fresh_epoch(resources.process_euid, resources.engine_id);
        let sealed = runtime
            .cct
            .replace(next)
            .expect("open boundary has an active CCT epoch")
            .seal();
        match runtime.publisher.publish_cct_epoch(&sealed) {
            PublishBatchResult::Committed { .. } => {}
            PublishBatchResult::Lost(_) => {
                runtime.health.cct_segment_publish_failed =
                    runtime.health.cct_segment_publish_failed.saturating_add(1);
            }
            PublishBatchResult::Indeterminate(_) => {
                runtime.finalization = FinalizationState::CctIndeterminate;
            }
        }
    });
}

trait ContextAdmissionExt {
    fn context_ref(self) -> ContextRef;
}

impl ContextAdmissionExt for ContextAdmission {
    fn context_ref(self) -> ContextRef {
        match self {
            ContextAdmission::Normal { context_ref, .. }
            | ContextAdmission::Overflow { context_ref, .. } => context_ref,
        }
    }
}

pub(super) fn finalize_ready_boundary(
    handle: BoundaryHandle,
    runtime: &mut BoundaryRuntime,
    decoder: &mut DirectDecoder,
    registry: &super::BoundaryRegistry,
) -> bool {
    match runtime.finalization {
        FinalizationState::CctIndeterminate => {
            if runtime.publisher.resolve_pending() != ResolveIndeterminateResult::Committed {
                return false;
            }
            runtime.finalization = FinalizationState::Open;
        }
        FinalizationState::EvidenceIndeterminate { batch_id, stats } => {
            if runtime.publisher.resolve_pending() != ResolveIndeterminateResult::Committed {
                return false;
            }
            runtime.health.record_evidence_committed(stats);
            decoder.apply_evidence_batch_outcome(handle, EvidenceBatchOutcome::Committed(batch_id));
            runtime.finalization = FinalizationState::Open;
        }
        FinalizationState::FinishIndeterminate => {
            if runtime.publisher.resolve_pending() != ResolveIndeterminateResult::Committed {
                return false;
            }
            return registry.acknowledge_terminal(handle, BoundaryPhase::Sealed);
        }
        FinalizationState::Open => {}
    }

    let discarded = decoder.discard_boundary(handle);
    let producer_health = registry.producer_health(handle);
    runtime.health.structural_transport_exceeded = producer_health.structural_transport_exceeded;
    runtime.health.value_attempt_transport_exceeded =
        producer_health.value_attempt_transport_exceeded;
    runtime.health.error_capture_attempt_transport_exceeded = runtime
        .health
        .error_capture_attempt_transport_exceeded
        .saturating_add(producer_health.error_capture_attempt_transport_exceeded);
    runtime.health.applicable_error_unwinds = runtime
        .health
        .applicable_error_unwinds
        .saturating_add(producer_health.error_capture_attempt_transport_exceeded);
    runtime.health.terminal_error_link_transport_exceeded = runtime
        .health
        .terminal_error_link_transport_exceeded
        .saturating_add(producer_health.terminal_error_link_transport_exceeded);
    runtime.health.terminal_error_links_observed = runtime
        .health
        .terminal_error_links_observed
        .saturating_add(producer_health.terminal_error_link_transport_exceeded);
    runtime.health.unmatched_call_facts = runtime
        .health
        .unmatched_call_facts
        .saturating_add(discarded.unmatched_call_facts);
    runtime.health.unmatched_thread_facts = runtime
        .health
        .unmatched_thread_facts
        .saturating_add(discarded.unmatched_thread_facts);
    runtime.health.error_capture_missing_structural_join = runtime
        .health
        .error_capture_missing_structural_join
        .saturating_add(discarded.error_capture_missing_structural_join);
    runtime.health.terminal_error_link_start_uncommitted = runtime
        .health
        .terminal_error_link_start_uncommitted
        .saturating_add(discarded.terminal_error_link_start_uncommitted);
    if let Some(epoch) = runtime.cct.take() {
        let sealed = epoch.seal();
        if !sealed.contexts.is_empty() || !sealed.overflow.is_empty() {
            match runtime.publisher.publish_cct_epoch(&sealed) {
                PublishBatchResult::Committed { .. } => {}
                PublishBatchResult::Lost(_) => {
                    runtime.health.cct_segment_publish_failed =
                        runtime.health.cct_segment_publish_failed.saturating_add(1);
                    if !runtime.evidence.facts.is_empty() {
                        let lost = runtime.evidence.take();
                        runtime.health.record_evidence_publish_failed(
                            EvidenceBatchStats::from_facts(&lost.facts),
                        );
                        runtime.health.evidence_segment_publish_failed = runtime
                            .health
                            .evidence_segment_publish_failed
                            .saturating_add(1);
                    }
                }
                PublishBatchResult::Indeterminate(_) => {
                    runtime.finalization = FinalizationState::CctIndeterminate;
                    return false;
                }
            }
        }
    }
    if !runtime.evidence.facts.is_empty() {
        let batch = runtime.evidence.take();
        let stats = EvidenceBatchStats::from_facts(&batch.facts);
        match runtime.publisher.publish_evidence_facts(&batch.facts) {
            PublishBatchResult::Committed { .. } => {
                runtime.health.record_evidence_committed(stats);
            }
            PublishBatchResult::Lost(_) => {
                runtime.health.record_evidence_publish_failed(stats);
                runtime.health.evidence_segment_publish_failed = runtime
                    .health
                    .evidence_segment_publish_failed
                    .saturating_add(1);
            }
            PublishBatchResult::Indeterminate(_) => {
                runtime.finalization = FinalizationState::EvidenceIndeterminate {
                    batch_id: batch.id,
                    stats,
                };
                return false;
            }
        }
    }
    let status = registry
        .closing_facts(handle)
        .map_or(BoundaryEndStatus::Abandoned, |(_, status)| status);
    match runtime.publisher.finish_boundary(&RunEnd {
        status,
        terminal_health: runtime.health.encode(),
    }) {
        FinishBoundaryResult::Sealed => {
            registry.acknowledge_terminal(handle, BoundaryPhase::Sealed)
        }
        FinishBoundaryResult::ReleasedIncomplete(_) => {
            registry.acknowledge_terminal(handle, BoundaryPhase::ReleasedIncomplete)
        }
        FinishBoundaryResult::Indeterminate(_) => {
            runtime.finalization = FinalizationState::FinishIndeterminate;
            false
        }
    }
}

fn find_boundary_by_root(
    boundaries: &[std::sync::Mutex<Option<BoundaryRuntime>>],
    root: ThreadRef,
) -> Option<BoundaryHandle> {
    boundaries.iter().enumerate().find_map(|(slot, runtime)| {
        let runtime = runtime
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let runtime = runtime.as_ref()?;
        if runtime.publisher.meta().root_thread_ref == root {
            Some(BoundaryHandle {
                slot: u32::try_from(slot).ok()?,
                generation: runtime.generation,
            })
        } else {
            None
        }
    })
}

fn with_runtime(
    boundaries: &[std::sync::Mutex<Option<BoundaryRuntime>>],
    handle: BoundaryHandle,
    operation: impl FnOnce(&mut BoundaryRuntime),
) {
    let Some(slot) = boundaries.get(handle.slot as usize) else {
        return;
    };
    let mut slot = slot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(runtime) = slot.as_mut() else { return };
    if runtime.generation == handle.generation {
        operation(runtime);
    }
}

fn with_runtime_value<T>(
    boundaries: &[std::sync::Mutex<Option<BoundaryRuntime>>],
    handle: BoundaryHandle,
    operation: impl FnOnce(&mut BoundaryRuntime) -> Option<T>,
) -> Option<T> {
    let slot = boundaries.get(handle.slot as usize)?;
    let mut slot = slot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let runtime = slot.as_mut()?;
    (runtime.generation == handle.generation)
        .then(|| operation(runtime))
        .flatten()
}

fn record_join_capacity_exceeded(
    boundaries: &[std::sync::Mutex<Option<BoundaryRuntime>>],
    handle: Option<BoundaryHandle>,
) {
    if let Some(handle) = handle {
        with_runtime(boundaries, handle, |runtime| {
            runtime.health.join_capacity_exceeded =
                runtime.health.join_capacity_exceeded.saturating_add(1);
        });
    }
}

fn thread_ref(resources: &DecoderResources<'_>, thread_id: crate::ids::BexThreadId) -> ThreadRef {
    ThreadRef {
        process_euid: resources.process_euid,
        engine_id: resources.engine_id,
        thread_id,
    }
}

fn call_thread_ref(call_ref: CallRef) -> ThreadRef {
    ThreadRef {
        process_euid: call_ref.process_euid,
        engine_id: call_ref.engine_id,
        thread_id: call_ref.thread_id,
    }
}

#[cfg(test)]
mod tests {
    use super::{BoundaryHealthSnapshot, DirectDecoder, OwnedCallStart};
    use crate::ids::{BexCallId, BexThreadId, CallRef, EngineId, FunctionId, ProcessEuid};

    #[test]
    fn terminal_health_round_trips_and_rejects_wrong_lengths() {
        let health = BoundaryHealthSnapshot {
            corrupt_records: 1,
            active_thread_capacity_exceeded: 2,
            terminal_error_link_evidence_publish_failed: 25,
            ..BoundaryHealthSnapshot::default()
        };
        let encoded = health.encode();
        assert_eq!(BoundaryHealthSnapshot::decode(&encoded), Some(health));
        assert!(BoundaryHealthSnapshot::decode(&encoded[..encoded.len() - 1]).is_none());
        let mut trailing = encoded;
        trailing.push(0);
        assert!(BoundaryHealthSnapshot::decode(&trailing).is_none());
    }

    #[test]
    fn unresolved_child_call_is_not_ready_for_pending_start_resolution() {
        let decoder = DirectDecoder::default();
        let mut fact = OwnedCallStart {
            flags: 0,
            call_ref: CallRef {
                process_euid: ProcessEuid([1; 16]),
                engine_id: EngineId(2),
                thread_id: BexThreadId(3),
                call_id: BexCallId(5),
            },
            parent_call_id: BexCallId(4),
            function_id: FunctionId(6),
            call_site: None,
            ts_ticks: 7,
        };
        assert!(!decoder.call_start_dependency_ready(&fact));
        fact.parent_call_id = BexCallId(0);
        assert!(decoder.call_start_dependency_ready(&fact));
    }
}
