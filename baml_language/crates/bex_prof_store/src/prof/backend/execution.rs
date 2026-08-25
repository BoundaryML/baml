//! Fixed-capacity generation-tagged execution ownership.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering, fence},
};

use super::{MemoryDenied, Owner, ProfilerMemoryGovernor, Reservation, ReservationClass};
use crate::ids::{BoundaryId, ThreadRef};

const PHASE_FREE: u8 = u8::MAX;
const STATUS_NONE: u8 = u8::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExecutionHandle {
    pub slot: u32,
    pub generation: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ExecutionPhase {
    Open = 0,
    RootReturned = 1,
    Closing = 2,
    Released = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ExecutionEndStatus {
    Succeeded = 0,
    Failed = 1,
    Cancelled = 2,
    Panicked = 3,
    Abandoned = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaseUnavailable {
    StaleGeneration,
    ExecutionNotOpen,
    CounterSaturated,
    ParentDisarmed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionSlotUnavailable {
    NoStableSlot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutionMetadata {
    pub root_thread_ref: ThreadRef,
    /// Host runtime token (`baml_id_1_…`), opaque to the profiler.
    pub runtime_id: BoundaryId,
    /// `now_ticks()` sampled at the top of `register_root`; the durable
    /// `RootStarted.started_ns` source (a lost `StartThread` record must not
    /// lose the start time).
    pub admitted_ticks: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExecutionProducerHealthSnapshot {
    pub structural_transport_exceeded: u64,
    pub value_attempt_transport_exceeded: u64,
    pub error_capture_attempt_transport_exceeded: u64,
    pub terminal_error_link_transport_exceeded: u64,
}

#[derive(Debug)]
struct ExecutionSlot {
    generation: AtomicU32,
    phase: AtomicU8,
    active_threads: AtomicU64,
    root_status: AtomicU8,
    finish_ready: AtomicBool,
    consumer_drain_armed: AtomicBool,
    /// Set (with `metadata`) under the `metadata` mutex by `reserve_root`;
    /// cleared by `take_admitted` on the consumer thread.
    admitted_pending: AtomicBool,
    /// `now_ticks()` recorded at the one-to-zero lease release.
    closing_ticks: AtomicU64,
    structural_transport_exceeded: AtomicU64,
    value_attempt_transport_exceeded: AtomicU64,
    error_capture_attempt_transport_exceeded: AtomicU64,
    terminal_error_link_transport_exceeded: AtomicU64,
    metadata: Mutex<Option<ExecutionMetadata>>,
}

impl ExecutionSlot {
    fn new() -> Self {
        Self {
            generation: AtomicU32::new(1),
            phase: AtomicU8::new(PHASE_FREE),
            active_threads: AtomicU64::new(0),
            root_status: AtomicU8::new(STATUS_NONE),
            finish_ready: AtomicBool::new(false),
            consumer_drain_armed: AtomicBool::new(false),
            admitted_pending: AtomicBool::new(false),
            closing_ticks: AtomicU64::new(0),
            structural_transport_exceeded: AtomicU64::new(0),
            value_attempt_transport_exceeded: AtomicU64::new(0),
            error_capture_attempt_transport_exceeded: AtomicU64::new(0),
            terminal_error_link_transport_exceeded: AtomicU64::new(0),
            metadata: Mutex::new(None),
        }
    }
}

#[derive(Debug)]
pub struct ExecutionRegistry {
    slots: Box<[ExecutionSlot]>,
    free: Mutex<Vec<u32>>,
    /// `EngineStarted` records pushed by `engine_started` and drained by
    /// `take_admitted` AFTER the slot scan, ordered before same-engine
    /// `RootStarted`s (never the lossy producer lane).
    #[cfg(not(target_arch = "wasm32"))]
    engines_started: Mutex<Vec<super::MetaRecord>>,
    _control_reservation: Reservation,
}

#[derive(Debug)]
pub struct ExecutionThreadLease {
    handle: ExecutionHandle,
    armed: bool,
}

#[derive(Debug)]
pub struct RootExecutionCompletionGuard {
    registry: Arc<ExecutionRegistry>,
    lease: ExecutionThreadLease,
    completed: bool,
}

impl ExecutionRegistry {
    pub fn new(
        slot_count: u32,
        execution_slot_bytes: u64,
        memory: &ProfilerMemoryGovernor,
    ) -> Result<Arc<Self>, MemoryDenied> {
        let accounted_bytes = u64::from(slot_count).saturating_mul(execution_slot_bytes);
        let control_reservation = memory.try_reserve(
            ReservationClass::Control,
            Owner::Population,
            accounted_bytes,
        )?;
        let slots: Box<[ExecutionSlot]> = (0..slot_count)
            .map(|_| ExecutionSlot::new())
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let free = (0..slot_count).rev().collect();
        Ok(Arc::new(Self {
            slots,
            free: Mutex::new(free),
            #[cfg(not(target_arch = "wasm32"))]
            engines_started: Mutex::new(Vec::new()),
            _control_reservation: control_reservation,
        }))
    }

    /// Provisional runtime registration. Phase 3 places durable `run.meta`
    /// admission before this guard is exposed to engine execution.
    pub fn reserve_root(
        self: &Arc<Self>,
        metadata: ExecutionMetadata,
    ) -> Result<RootExecutionCompletionGuard, ExecutionSlotUnavailable> {
        let slot_index = self
            .free
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pop()
            .ok_or(ExecutionSlotUnavailable::NoStableSlot)?;
        let slot = &self.slots[slot_index as usize];
        debug_assert_eq!(slot.phase.load(Ordering::Acquire), PHASE_FREE);
        {
            let mut guard = slot
                .metadata
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *guard = Some(metadata);
            // The pair (metadata, admitted_pending) is written under the
            // mutex so `take_admitted` reads it atomically.
            slot.admitted_pending.store(true, Ordering::Release);
        }
        slot.closing_ticks.store(0, Ordering::Relaxed);
        slot.root_status.store(STATUS_NONE, Ordering::Relaxed);
        slot.finish_ready.store(false, Ordering::Relaxed);
        slot.consumer_drain_armed.store(false, Ordering::Relaxed);
        slot.structural_transport_exceeded
            .store(0, Ordering::Relaxed);
        slot.value_attempt_transport_exceeded
            .store(0, Ordering::Relaxed);
        slot.error_capture_attempt_transport_exceeded
            .store(0, Ordering::Relaxed);
        slot.terminal_error_link_transport_exceeded
            .store(0, Ordering::Relaxed);
        slot.active_threads.store(1, Ordering::Relaxed);
        slot.phase
            .store(ExecutionPhase::Open as u8, Ordering::Release);
        let handle = ExecutionHandle {
            slot: slot_index,
            generation: slot.generation.load(Ordering::Acquire),
        };
        Ok(RootExecutionCompletionGuard {
            registry: Arc::clone(self),
            lease: ExecutionThreadLease {
                handle,
                armed: true,
            },
            completed: false,
        })
    }

    pub fn try_acquire_child(
        &self,
        parent: &ExecutionThreadLease,
    ) -> Result<ExecutionThreadLease, LeaseUnavailable> {
        if !parent.armed {
            return Err(LeaseUnavailable::ParentDisarmed);
        }
        self.try_acquire_child_handle(parent.handle)
    }

    pub fn record_structural_transport_loss(&self, handle: ExecutionHandle) {
        let Ok(slot) = self.validate(handle) else {
            return;
        };
        let _ = slot.structural_transport_exceeded.fetch_update(
            Ordering::Relaxed,
            Ordering::Relaxed,
            |value| value.checked_add(1),
        );
    }

    pub fn record_value_attempt_transport_loss(&self, handle: ExecutionHandle) {
        let Ok(slot) = self.validate(handle) else {
            return;
        };
        saturating_increment(&slot.value_attempt_transport_exceeded);
    }

    pub fn record_error_attempt_transport_loss(&self, handle: ExecutionHandle) {
        let Ok(slot) = self.validate(handle) else {
            return;
        };
        saturating_increment(&slot.error_capture_attempt_transport_exceeded);
    }

    pub fn record_terminal_error_transport_loss(&self, handle: ExecutionHandle) {
        let Ok(slot) = self.validate(handle) else {
            return;
        };
        saturating_increment(&slot.terminal_error_link_transport_exceeded);
    }

    #[must_use]
    pub fn producer_health(&self, handle: ExecutionHandle) -> ExecutionProducerHealthSnapshot {
        self.validate(handle)
            .ok()
            .map_or_else(ExecutionProducerHealthSnapshot::default, |slot| {
                ExecutionProducerHealthSnapshot {
                    structural_transport_exceeded: slot
                        .structural_transport_exceeded
                        .load(Ordering::Acquire),
                    value_attempt_transport_exceeded: slot
                        .value_attempt_transport_exceeded
                        .load(Ordering::Acquire),
                    error_capture_attempt_transport_exceeded: slot
                        .error_capture_attempt_transport_exceeded
                        .load(Ordering::Acquire),
                    terminal_error_link_transport_exceeded: slot
                        .terminal_error_link_transport_exceeded
                        .load(Ordering::Acquire),
                }
            })
    }

    #[must_use]
    pub fn accepts_producer(&self, handle: ExecutionHandle) -> bool {
        self.validate(handle).is_ok_and(|slot| {
            matches!(
                slot.phase.load(Ordering::Acquire),
                phase if phase == ExecutionPhase::Open as u8
                    || phase == ExecutionPhase::RootReturned as u8
            )
        })
    }

    /// Acquires from the handle carried by a currently executing VM. The
    /// outer thread-completion guard is the authoritative armed owner; the VM
    /// holds only this non-owning generation-tagged projection so it can hand
    /// a fresh lease to a child before scheduling.
    pub fn try_acquire_child_handle(
        &self,
        parent: ExecutionHandle,
    ) -> Result<ExecutionThreadLease, LeaseUnavailable> {
        let slot = self.validate(parent)?;
        let phase = slot.phase.load(Ordering::Acquire);
        if phase != ExecutionPhase::Open as u8 && phase != ExecutionPhase::RootReturned as u8 {
            return Err(LeaseUnavailable::ExecutionNotOpen);
        }
        let mut active = slot.active_threads.load(Ordering::Acquire);
        loop {
            if active == u64::MAX {
                return Err(LeaseUnavailable::CounterSaturated);
            }
            match slot.active_threads.compare_exchange_weak(
                active,
                active + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    if slot.generation.load(Ordering::Acquire) != parent.generation {
                        let previous = slot.active_threads.fetch_sub(1, Ordering::Release);
                        debug_assert!(previous > 1);
                        return Err(LeaseUnavailable::StaleGeneration);
                    }
                    return Ok(ExecutionThreadLease {
                        handle: parent,
                        armed: true,
                    });
                }
                Err(actual) => active = actual,
            }
        }
    }

    pub fn finish_thread(&self, lease: &mut ExecutionThreadLease) {
        if !lease.armed {
            return;
        }
        lease.armed = false;
        let Ok(slot) = self.validate(lease.handle) else {
            return;
        };
        let previous = slot.active_threads.fetch_sub(1, Ordering::Release);
        debug_assert!(previous > 0);
        if previous != 1 {
            return;
        }
        fence(Ordering::Acquire);
        slot.closing_ticks
            .store(crate::prof::clock::now_ticks(), Ordering::Release);
        if slot.root_status.load(Ordering::Acquire) == STATUS_NONE {
            debug_assert!(false, "last boundary owner released before root status");
            return;
        }
        if slot
            .phase
            .compare_exchange(
                ExecutionPhase::RootReturned as u8,
                ExecutionPhase::Closing as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        {
            slot.finish_ready.store(true, Ordering::Release);
            #[cfg(all(not(target_arch = "wasm32"), not(baml_loom)))]
            super::hooks::wake_for_backend_terminal();
        }
    }

    #[must_use]
    pub fn ready_handles(&self) -> Vec<ExecutionHandle> {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.finish_ready.load(Ordering::Acquire))
            .map(|(index, slot)| ExecutionHandle {
                slot: u32::try_from(index).expect("boundary slot count is u32-bounded"),
                generation: slot.generation.load(Ordering::Acquire),
            })
            .collect()
    }

    /// Returns `true` only after a previous consumer pass observed this
    /// terminal candidate. The pass between the first `false` and the next
    /// `true` performs a complete ring sweep, so structural commits that
    /// happen-before the last thread lease release cannot be overtaken by
    /// boundary finalization.
    pub fn consumer_drain_completed(&self, handle: ExecutionHandle) -> bool {
        let Ok(slot) = self.validate(handle) else {
            return false;
        };
        if slot.phase.load(Ordering::Acquire) != ExecutionPhase::Closing as u8
            || !slot.finish_ready.load(Ordering::Acquire)
        {
            return false;
        }
        slot.consumer_drain_armed.swap(true, Ordering::AcqRel)
    }

    pub fn closing_facts(
        &self,
        handle: ExecutionHandle,
    ) -> Option<(ExecutionMetadata, ExecutionEndStatus, u64)> {
        let slot = self.validate(handle).ok()?;
        if slot.phase.load(Ordering::Acquire) != ExecutionPhase::Closing as u8
            || !slot.finish_ready.load(Ordering::Acquire)
        {
            return None;
        }
        let metadata = (*slot
            .metadata
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner))?;
        let status = decode_status(slot.root_status.load(Ordering::Acquire))?;
        let closing_ticks = slot.closing_ticks.load(Ordering::Acquire);
        Some((metadata, status, closing_ticks))
    }

    /// Pushes an `EngineStarted` index record; drained by `take_admitted`.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn engine_started(&self, record: super::MetaRecord) -> usize {
        let mut engines = self
            .engines_started
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        engines.push(record);
        engines.len()
    }

    /// Consumer-thread admission drain (streams spec §5.5): (1) scan slots
    /// for `admitted_pending` and clear it, (2) THEN drain the registry-side
    /// `EngineStarted` vector, returning records ordered so every
    /// `EngineStarted` precedes any `RootStarted` of the same engine.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn take_admitted(
        &self,
        clock: &crate::prof::clock::TickConverter,
    ) -> Vec<super::MetaRecord> {
        let mut roots = Vec::new();
        for slot in &self.slots {
            if !slot.admitted_pending.load(Ordering::Acquire) {
                continue;
            }
            let guard = slot
                .metadata
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !slot.admitted_pending.swap(false, Ordering::AcqRel) {
                continue;
            }
            let Some(metadata) = *guard else { continue };
            roots.push(super::MetaRecord::RootStarted {
                root: metadata.root_thread_ref,
                started_ns: clock.to_ns(metadata.admitted_ticks),
                runtime_id: metadata.runtime_id,
            });
        }
        let mut records = std::mem::take(
            &mut *self
                .engines_started
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        records.append(&mut roots);
        records
    }

    pub fn acknowledge_terminal(&self, handle: ExecutionHandle, terminal: ExecutionPhase) -> bool {
        if terminal != ExecutionPhase::Released {
            return false;
        }
        let Ok(slot) = self.validate(handle) else {
            return false;
        };
        if slot
            .phase
            .compare_exchange(
                ExecutionPhase::Closing as u8,
                terminal as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            return false;
        }
        debug_assert!(
            !slot.admitted_pending.load(Ordering::Acquire),
            "take_admitted runs before finalization on the consumer thread"
        );
        slot.admitted_pending.store(false, Ordering::Release);
        slot.finish_ready.store(false, Ordering::Release);
        slot.consumer_drain_armed.store(false, Ordering::Release);
        *slot
            .metadata
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        slot.root_status.store(STATUS_NONE, Ordering::Relaxed);
        slot.active_threads.store(0, Ordering::Relaxed);
        let current_generation = slot.generation.load(Ordering::Relaxed);
        if current_generation == u32::MAX {
            // A wrapped generation could make a centuries-old stale lease
            // valid again. Retire this fixed slot instead; capacity loss is
            // preferable to violating ownership.
            return true;
        }
        let next_generation = current_generation + 1;
        slot.generation.store(next_generation, Ordering::Release);
        slot.phase.store(PHASE_FREE, Ordering::Release);
        self.free
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(handle.slot);
        true
    }

    fn return_root(&self, lease: &mut ExecutionThreadLease, status: ExecutionEndStatus) {
        if !lease.armed {
            return;
        }
        let Ok(slot) = self.validate(lease.handle) else {
            lease.armed = false;
            return;
        };
        let _ = slot.root_status.compare_exchange(
            STATUS_NONE,
            status as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        let _ = slot.phase.compare_exchange(
            ExecutionPhase::Open as u8,
            ExecutionPhase::RootReturned as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        self.finish_thread(lease);
    }

    fn cancel_provisional(&self, lease: &mut ExecutionThreadLease) {
        if !lease.armed {
            return;
        }
        lease.armed = false;
        let Ok(slot) = self.validate(lease.handle) else {
            return;
        };
        if slot.phase.load(Ordering::Acquire) != ExecutionPhase::Open as u8
            || slot.active_threads.load(Ordering::Acquire) != 1
            || slot.root_status.load(Ordering::Acquire) != STATUS_NONE
        {
            debug_assert!(
                false,
                "activated boundary cannot be provisionally cancelled"
            );
            return;
        }
        *slot
            .metadata
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        slot.active_threads.store(0, Ordering::Relaxed);
        let generation = slot.generation.load(Ordering::Relaxed);
        if generation == u32::MAX {
            return;
        }
        slot.generation.store(generation + 1, Ordering::Release);
        slot.phase.store(PHASE_FREE, Ordering::Release);
        self.free
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(lease.handle.slot);
    }

    fn validate(&self, handle: ExecutionHandle) -> Result<&ExecutionSlot, LeaseUnavailable> {
        let Some(slot) = self.slots.get(handle.slot as usize) else {
            return Err(LeaseUnavailable::StaleGeneration);
        };
        if slot.generation.load(Ordering::Acquire) != handle.generation {
            return Err(LeaseUnavailable::StaleGeneration);
        }
        Ok(slot)
    }
}

fn saturating_increment(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        value.checked_add(1)
    });
}

impl ExecutionThreadLease {
    #[must_use]
    pub const fn handle(&self) -> ExecutionHandle {
        self.handle
    }

    #[must_use]
    pub const fn is_armed(&self) -> bool {
        self.armed
    }
}

impl RootExecutionCompletionGuard {
    #[must_use]
    pub const fn lease(&self) -> &ExecutionThreadLease {
        &self.lease
    }

    pub fn acquire_child(&self) -> Result<ExecutionThreadLease, LeaseUnavailable> {
        self.registry.try_acquire_child(&self.lease)
    }

    pub fn complete(mut self, status: ExecutionEndStatus) {
        self.registry.return_root(&mut self.lease, status);
        self.completed = true;
    }

    /// Releases the runtime reservation when durable root admission did not
    /// commit. No active profiler token has escaped at this point.
    pub fn cancel_provisional(mut self) {
        self.registry.cancel_provisional(&mut self.lease);
        self.completed = true;
    }
}

impl Drop for RootExecutionCompletionGuard {
    fn drop(&mut self) {
        if self.completed || !self.lease.armed {
            return;
        }
        let status = if std::thread::panicking() {
            ExecutionEndStatus::Panicked
        } else {
            ExecutionEndStatus::Abandoned
        };
        self.registry.return_root(&mut self.lease, status);
    }
}

fn decode_status(raw: u8) -> Option<ExecutionEndStatus> {
    match raw {
        0 => Some(ExecutionEndStatus::Succeeded),
        1 => Some(ExecutionEndStatus::Failed),
        2 => Some(ExecutionEndStatus::Cancelled),
        3 => Some(ExecutionEndStatus::Panicked),
        4 => Some(ExecutionEndStatus::Abandoned),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ids::{BexThreadId, EngineId, ProcessEuid},
        prof::backend::{MeasuredLayouts, ProfilerSizingPolicy},
    };

    #[cfg(not(target_arch = "wasm32"))]
    fn drain_admissions(registry: &ExecutionRegistry) {
        let clock = crate::prof::clock::TickConverter::from_clock();
        let _ = registry.take_admitted(&clock);
    }

    fn registry(slots: u32) -> Arc<ExecutionRegistry> {
        let sizing = ProfilerSizingPolicy::derive(32 * 1024 * 1024, MeasuredLayouts::V1).unwrap();
        let memory = ProfilerMemoryGovernor::new(sizing, MeasuredLayouts::V1);
        ExecutionRegistry::new(slots, MeasuredLayouts::V1.execution_slot_bytes, &memory).unwrap()
    }

    fn metadata(byte: u8) -> ExecutionMetadata {
        ExecutionMetadata {
            root_thread_ref: ThreadRef {
                process_euid: ProcessEuid([byte; 16]),
                engine_id: EngineId(1),
                thread_id: BexThreadId(1),
            },
            runtime_id: BoundaryId::from_bytes([byte; 16]),
            admitted_ticks: 0,
        }
    }

    #[test]
    fn root_without_descendants_uses_the_same_last_owner_path() {
        let registry = registry(1);
        let root = registry.reserve_root(metadata(1)).unwrap();
        let handle = root.lease().handle();
        drain_admissions(&registry);
        root.complete(ExecutionEndStatus::Succeeded);
        assert_eq!(registry.ready_handles(), vec![handle]);
        assert!(matches!(
            registry.closing_facts(handle),
            Some((metadata, ExecutionEndStatus::Succeeded, _)) if metadata == self::metadata(1)
        ));
        assert!(registry.acknowledge_terminal(handle, ExecutionPhase::Released));
        assert!(registry.ready_handles().is_empty());
    }

    #[test]
    fn child_can_acquire_grandchild_after_root_return() {
        let registry = registry(1);
        let root = registry.reserve_root(metadata(2)).unwrap();
        let handle = root.lease().handle();
        let mut child = root.acquire_child().unwrap();
        root.complete(ExecutionEndStatus::Succeeded);
        assert!(registry.ready_handles().is_empty());
        let mut grandchild = registry.try_acquire_child(&child).unwrap();
        registry.finish_thread(&mut child);
        assert!(registry.ready_handles().is_empty());
        registry.finish_thread(&mut grandchild);
        assert_eq!(registry.ready_handles(), vec![handle]);
    }

    #[test]
    fn stale_generation_cannot_attach_to_reused_slot() {
        let registry = registry(1);
        let root = registry.reserve_root(metadata(3)).unwrap();
        let old_handle = root.lease().handle();
        drain_admissions(&registry);
        root.complete(ExecutionEndStatus::Succeeded);
        assert!(registry.acknowledge_terminal(old_handle, ExecutionPhase::Released));
        let next = registry.reserve_root(metadata(4)).unwrap();
        assert_ne!(next.lease().handle().generation, old_handle.generation);
        let stale = ExecutionThreadLease {
            handle: old_handle,
            armed: true,
        };
        assert!(matches!(
            registry.try_acquire_child(&stale),
            Err(LeaseUnavailable::StaleGeneration)
        ));
    }

    #[test]
    fn dropped_root_is_abandoned_and_closes_once() {
        let registry = registry(1);
        let root = registry.reserve_root(metadata(5)).unwrap();
        let handle = root.lease().handle();
        drop(root);
        assert!(matches!(
            registry.closing_facts(handle),
            Some((metadata, ExecutionEndStatus::Abandoned, _)) if metadata == self::metadata(5)
        ));
        assert_eq!(registry.ready_handles(), vec![handle]);
    }

    #[test]
    fn capacity_is_fixed_until_terminal_acknowledgement() {
        let registry = registry(1);
        let root = registry.reserve_root(metadata(6)).unwrap();
        let handle = root.lease().handle();
        drain_admissions(&registry);
        assert!(matches!(
            registry.reserve_root(metadata(7)),
            Err(ExecutionSlotUnavailable::NoStableSlot)
        ));
        root.complete(ExecutionEndStatus::Failed);
        assert!(matches!(
            registry.reserve_root(metadata(7)),
            Err(ExecutionSlotUnavailable::NoStableSlot)
        ));
        assert!(registry.acknowledge_terminal(handle, ExecutionPhase::Released));
        assert!(registry.reserve_root(metadata(7)).is_ok());
    }

    #[test]
    fn generation_exhaustion_retires_the_slot_instead_of_wrapping() {
        let registry = registry(1);
        registry.slots[0]
            .generation
            .store(u32::MAX, Ordering::Relaxed);
        let root = registry.reserve_root(metadata(8)).unwrap();
        let exhausted = root.lease().handle();
        assert_eq!(exhausted.generation, u32::MAX);
        drain_admissions(&registry);
        root.complete(ExecutionEndStatus::Succeeded);
        assert!(registry.acknowledge_terminal(exhausted, ExecutionPhase::Released));
        assert!(matches!(
            registry.reserve_root(metadata(9)),
            Err(ExecutionSlotUnavailable::NoStableSlot)
        ));
        let stale = ExecutionThreadLease {
            handle: exhausted,
            armed: true,
        };
        assert!(matches!(
            registry.try_acquire_child(&stale),
            Err(LeaseUnavailable::ExecutionNotOpen)
        ));
    }

    #[test]
    fn provisional_cancellation_emits_no_terminal_barrier_and_reuses_safely() {
        let registry = registry(1);
        let root = registry.reserve_root(metadata(10)).unwrap();
        let old = root.lease().handle();
        root.cancel_provisional();
        assert!(registry.ready_handles().is_empty());
        let next = registry.reserve_root(metadata(11)).unwrap();
        assert_ne!(next.lease().handle().generation, old.generation);
    }
}
