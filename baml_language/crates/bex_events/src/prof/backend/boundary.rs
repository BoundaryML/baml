//! Fixed-capacity generation-tagged profiling boundary ownership.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering, fence},
};

use super::{MemoryDenied, Owner, ProfilerMemoryGovernor, Reservation, ReservationClass};
use crate::ids::{BoundaryId, ThreadRef};

const PHASE_FREE: u8 = u8::MAX;
const STATUS_NONE: u8 = u8::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BoundaryHandle {
    pub slot: u32,
    pub generation: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BoundaryPhase {
    Open = 0,
    RootReturned = 1,
    Closing = 2,
    Sealed = 3,
    ReleasedIncomplete = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BoundaryEndStatus {
    Succeeded = 0,
    Failed = 1,
    Cancelled = 2,
    Panicked = 3,
    Abandoned = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaseUnavailable {
    StaleGeneration,
    BoundaryNotOpen,
    CounterSaturated,
    ParentDisarmed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundarySlotUnavailable {
    NoStableSlot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoundaryMetadata {
    pub boundary_id: BoundaryId,
    pub root_thread_ref: ThreadRef,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BoundaryProducerHealthSnapshot {
    pub structural_transport_exceeded: u64,
}

#[derive(Debug)]
struct BoundarySlot {
    generation: AtomicU32,
    phase: AtomicU8,
    active_threads: AtomicU64,
    root_status: AtomicU8,
    finish_ready: AtomicBool,
    structural_transport_exceeded: AtomicU64,
    metadata: Mutex<Option<BoundaryMetadata>>,
}

impl BoundarySlot {
    fn new() -> Self {
        Self {
            generation: AtomicU32::new(1),
            phase: AtomicU8::new(PHASE_FREE),
            active_threads: AtomicU64::new(0),
            root_status: AtomicU8::new(STATUS_NONE),
            finish_ready: AtomicBool::new(false),
            structural_transport_exceeded: AtomicU64::new(0),
            metadata: Mutex::new(None),
        }
    }
}

#[derive(Debug)]
pub struct BoundaryRegistry {
    slots: Box<[BoundarySlot]>,
    free: Mutex<Vec<u32>>,
    _control_reservation: Reservation,
}

#[derive(Debug)]
pub struct BoundaryThreadLease {
    handle: BoundaryHandle,
    armed: bool,
}

#[derive(Debug)]
pub struct RootBoundaryCompletionGuard {
    registry: Arc<BoundaryRegistry>,
    lease: BoundaryThreadLease,
    completed: bool,
}

impl BoundaryRegistry {
    pub fn new(
        slot_count: u32,
        boundary_slot_bytes: u64,
        memory: &ProfilerMemoryGovernor,
    ) -> Result<Arc<Self>, MemoryDenied> {
        let accounted_bytes = u64::from(slot_count).saturating_mul(boundary_slot_bytes);
        let control_reservation = memory.try_reserve(
            ReservationClass::Control,
            Owner::Population,
            accounted_bytes,
        )?;
        let slots: Box<[BoundarySlot]> = (0..slot_count)
            .map(|_| BoundarySlot::new())
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let free = (0..slot_count).rev().collect();
        Ok(Arc::new(Self {
            slots,
            free: Mutex::new(free),
            _control_reservation: control_reservation,
        }))
    }

    /// Provisional runtime registration. Phase 3 places durable `run.meta`
    /// admission before this guard is exposed to engine execution.
    pub fn reserve_root(
        self: &Arc<Self>,
        metadata: BoundaryMetadata,
    ) -> Result<RootBoundaryCompletionGuard, BoundarySlotUnavailable> {
        let slot_index = self
            .free
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pop()
            .ok_or(BoundarySlotUnavailable::NoStableSlot)?;
        let slot = &self.slots[slot_index as usize];
        debug_assert_eq!(slot.phase.load(Ordering::Acquire), PHASE_FREE);
        *slot
            .metadata
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(metadata);
        slot.root_status.store(STATUS_NONE, Ordering::Relaxed);
        slot.finish_ready.store(false, Ordering::Relaxed);
        slot.structural_transport_exceeded
            .store(0, Ordering::Relaxed);
        slot.active_threads.store(1, Ordering::Relaxed);
        slot.phase
            .store(BoundaryPhase::Open as u8, Ordering::Release);
        let handle = BoundaryHandle {
            slot: slot_index,
            generation: slot.generation.load(Ordering::Acquire),
        };
        Ok(RootBoundaryCompletionGuard {
            registry: Arc::clone(self),
            lease: BoundaryThreadLease {
                handle,
                armed: true,
            },
            completed: false,
        })
    }

    pub fn try_acquire_child(
        &self,
        parent: &BoundaryThreadLease,
    ) -> Result<BoundaryThreadLease, LeaseUnavailable> {
        if !parent.armed {
            return Err(LeaseUnavailable::ParentDisarmed);
        }
        self.try_acquire_child_handle(parent.handle)
    }

    pub fn record_structural_transport_loss(&self, handle: BoundaryHandle) {
        let Ok(slot) = self.validate(handle) else {
            return;
        };
        let _ = slot.structural_transport_exceeded.fetch_update(
            Ordering::Relaxed,
            Ordering::Relaxed,
            |value| value.checked_add(1),
        );
    }

    #[must_use]
    pub fn producer_health(&self, handle: BoundaryHandle) -> BoundaryProducerHealthSnapshot {
        self.validate(handle)
            .ok()
            .map_or_else(BoundaryProducerHealthSnapshot::default, |slot| {
                BoundaryProducerHealthSnapshot {
                    structural_transport_exceeded: slot
                        .structural_transport_exceeded
                        .load(Ordering::Acquire),
                }
            })
    }

    /// Acquires from the handle carried by a currently executing VM. The
    /// outer thread-completion guard is the authoritative armed owner; the VM
    /// holds only this non-owning generation-tagged projection so it can hand
    /// a fresh lease to a child before scheduling.
    pub fn try_acquire_child_handle(
        &self,
        parent: BoundaryHandle,
    ) -> Result<BoundaryThreadLease, LeaseUnavailable> {
        let slot = self.validate(parent)?;
        let phase = slot.phase.load(Ordering::Acquire);
        if phase != BoundaryPhase::Open as u8 && phase != BoundaryPhase::RootReturned as u8 {
            return Err(LeaseUnavailable::BoundaryNotOpen);
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
                    return Ok(BoundaryThreadLease {
                        handle: parent,
                        armed: true,
                    });
                }
                Err(actual) => active = actual,
            }
        }
    }

    pub fn finish_thread(&self, lease: &mut BoundaryThreadLease) {
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
        if slot.root_status.load(Ordering::Acquire) == STATUS_NONE {
            debug_assert!(false, "last boundary owner released before root status");
            return;
        }
        if slot
            .phase
            .compare_exchange(
                BoundaryPhase::RootReturned as u8,
                BoundaryPhase::Closing as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        {
            slot.finish_ready.store(true, Ordering::Release);
            #[cfg(all(not(target_arch = "wasm32"), not(baml_loom)))]
            crate::prof::consumer::wake_for_backend_terminal();
        }
    }

    #[must_use]
    pub fn ready_handles(&self) -> Vec<BoundaryHandle> {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.finish_ready.load(Ordering::Acquire))
            .map(|(index, slot)| BoundaryHandle {
                slot: u32::try_from(index).expect("boundary slot count is u32-bounded"),
                generation: slot.generation.load(Ordering::Acquire),
            })
            .collect()
    }

    pub fn closing_facts(
        &self,
        handle: BoundaryHandle,
    ) -> Option<(BoundaryMetadata, BoundaryEndStatus)> {
        let slot = self.validate(handle).ok()?;
        if slot.phase.load(Ordering::Acquire) != BoundaryPhase::Closing as u8
            || !slot.finish_ready.load(Ordering::Acquire)
        {
            return None;
        }
        let metadata = (*slot
            .metadata
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner))?;
        let status = decode_status(slot.root_status.load(Ordering::Acquire))?;
        Some((metadata, status))
    }

    pub fn acknowledge_terminal(&self, handle: BoundaryHandle, terminal: BoundaryPhase) -> bool {
        if terminal != BoundaryPhase::Sealed && terminal != BoundaryPhase::ReleasedIncomplete {
            return false;
        }
        let Ok(slot) = self.validate(handle) else {
            return false;
        };
        if slot
            .phase
            .compare_exchange(
                BoundaryPhase::Closing as u8,
                terminal as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            return false;
        }
        slot.finish_ready.store(false, Ordering::Release);
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

    fn return_root(&self, lease: &mut BoundaryThreadLease, status: BoundaryEndStatus) {
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
            BoundaryPhase::Open as u8,
            BoundaryPhase::RootReturned as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        self.finish_thread(lease);
    }

    fn cancel_provisional(&self, lease: &mut BoundaryThreadLease) {
        if !lease.armed {
            return;
        }
        lease.armed = false;
        let Ok(slot) = self.validate(lease.handle) else {
            return;
        };
        if slot.phase.load(Ordering::Acquire) != BoundaryPhase::Open as u8
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

    fn validate(&self, handle: BoundaryHandle) -> Result<&BoundarySlot, LeaseUnavailable> {
        let Some(slot) = self.slots.get(handle.slot as usize) else {
            return Err(LeaseUnavailable::StaleGeneration);
        };
        if slot.generation.load(Ordering::Acquire) != handle.generation {
            return Err(LeaseUnavailable::StaleGeneration);
        }
        Ok(slot)
    }
}

impl BoundaryThreadLease {
    #[must_use]
    pub const fn handle(&self) -> BoundaryHandle {
        self.handle
    }

    #[must_use]
    pub const fn is_armed(&self) -> bool {
        self.armed
    }
}

impl RootBoundaryCompletionGuard {
    #[must_use]
    pub const fn lease(&self) -> &BoundaryThreadLease {
        &self.lease
    }

    pub fn acquire_child(&self) -> Result<BoundaryThreadLease, LeaseUnavailable> {
        self.registry.try_acquire_child(&self.lease)
    }

    pub fn complete(mut self, status: BoundaryEndStatus) {
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

impl Drop for RootBoundaryCompletionGuard {
    fn drop(&mut self) {
        if self.completed || !self.lease.armed {
            return;
        }
        let status = if std::thread::panicking() {
            BoundaryEndStatus::Panicked
        } else {
            BoundaryEndStatus::Abandoned
        };
        self.registry.return_root(&mut self.lease, status);
    }
}

fn decode_status(raw: u8) -> Option<BoundaryEndStatus> {
    match raw {
        0 => Some(BoundaryEndStatus::Succeeded),
        1 => Some(BoundaryEndStatus::Failed),
        2 => Some(BoundaryEndStatus::Cancelled),
        3 => Some(BoundaryEndStatus::Panicked),
        4 => Some(BoundaryEndStatus::Abandoned),
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

    fn registry(slots: u32) -> Arc<BoundaryRegistry> {
        let sizing = ProfilerSizingPolicy::derive(32 * 1024 * 1024, MeasuredLayouts::V1).unwrap();
        let memory = ProfilerMemoryGovernor::new(sizing, MeasuredLayouts::V1);
        BoundaryRegistry::new(slots, MeasuredLayouts::V1.boundary_slot_bytes, &memory).unwrap()
    }

    fn metadata(byte: u8) -> BoundaryMetadata {
        BoundaryMetadata {
            boundary_id: BoundaryId::from_bytes([byte; 16]),
            root_thread_ref: ThreadRef {
                process_euid: ProcessEuid([byte; 16]),
                engine_id: EngineId(1),
                thread_id: BexThreadId(1),
            },
        }
    }

    #[test]
    fn root_without_descendants_uses_the_same_last_owner_path() {
        let registry = registry(1);
        let root = registry.reserve_root(metadata(1)).unwrap();
        let handle = root.lease().handle();
        root.complete(BoundaryEndStatus::Succeeded);
        assert_eq!(registry.ready_handles(), vec![handle]);
        assert_eq!(
            registry.closing_facts(handle),
            Some((metadata(1), BoundaryEndStatus::Succeeded))
        );
        assert!(registry.acknowledge_terminal(handle, BoundaryPhase::Sealed));
        assert!(registry.ready_handles().is_empty());
    }

    #[test]
    fn child_can_acquire_grandchild_after_root_return() {
        let registry = registry(1);
        let root = registry.reserve_root(metadata(2)).unwrap();
        let handle = root.lease().handle();
        let mut child = root.acquire_child().unwrap();
        root.complete(BoundaryEndStatus::Succeeded);
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
        root.complete(BoundaryEndStatus::Succeeded);
        assert!(registry.acknowledge_terminal(old_handle, BoundaryPhase::Sealed));
        let next = registry.reserve_root(metadata(4)).unwrap();
        assert_ne!(next.lease().handle().generation, old_handle.generation);
        let stale = BoundaryThreadLease {
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
        assert_eq!(
            registry.closing_facts(handle),
            Some((metadata(5), BoundaryEndStatus::Abandoned))
        );
        assert_eq!(registry.ready_handles(), vec![handle]);
    }

    #[test]
    fn capacity_is_fixed_until_terminal_acknowledgement() {
        let registry = registry(1);
        let root = registry.reserve_root(metadata(6)).unwrap();
        let handle = root.lease().handle();
        assert!(matches!(
            registry.reserve_root(metadata(7)),
            Err(BoundarySlotUnavailable::NoStableSlot)
        ));
        root.complete(BoundaryEndStatus::Failed);
        assert!(matches!(
            registry.reserve_root(metadata(7)),
            Err(BoundarySlotUnavailable::NoStableSlot)
        ));
        assert!(registry.acknowledge_terminal(handle, BoundaryPhase::ReleasedIncomplete));
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
        root.complete(BoundaryEndStatus::Succeeded);
        assert!(registry.acknowledge_terminal(exhausted, BoundaryPhase::Sealed));
        assert!(matches!(
            registry.reserve_root(metadata(9)),
            Err(BoundarySlotUnavailable::NoStableSlot)
        ));
        let stale = BoundaryThreadLease {
            handle: exhausted,
            armed: true,
        };
        assert!(matches!(
            registry.try_acquire_child(&stale),
            Err(LeaseUnavailable::BoundaryNotOpen)
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
