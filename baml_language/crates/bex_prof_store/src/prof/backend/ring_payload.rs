//! Owned payloads referenced by fixed-size ring records.

use std::sync::{Mutex, MutexGuard};

use super::{Owner, ProfilerMemoryGovernor, Reservation, ReservationClass};

#[derive(Debug)]
struct Slot<T> {
    generation: u32,
    value: Option<T>,
    taken_early: bool,
}

#[derive(Debug)]
pub(super) struct PayloadSlots<T> {
    slots: Box<[Mutex<Slot<T>>]>,
    free_tx: crossbeam_channel::Sender<u32>,
    free_rx: crossbeam_channel::Receiver<u32>,
    _reservation: Reservation,
}

impl<T> PayloadSlots<T> {
    pub(super) fn new(
        capacity: u32,
        memory: &ProfilerMemoryGovernor,
    ) -> Result<Self, super::MemoryDenied> {
        // Include the bounded channel's stamps, indices, and fixed bookkeeping.
        let bytes = u64::from(capacity)
            .saturating_mul((std::mem::size_of::<Mutex<Slot<T>>>() + 32) as u64)
            .saturating_add(1024);
        let reservation = memory.try_reserve(ReservationClass::General, Owner::Transport, bytes)?;
        let (free_tx, free_rx) = crossbeam_channel::bounded(capacity as usize);
        let slots = (0..capacity)
            .map(|index| {
                free_tx
                    .try_send(index)
                    .expect("channel has one entry per slot");
                Mutex::new(Slot {
                    generation: 0,
                    value: None,
                    taken_early: false,
                })
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Ok(Self {
            slots,
            free_tx,
            free_rx,
            _reservation: reservation,
        })
    }

    pub(super) fn reserve(&self) -> Option<PayloadReservation<'_, T>> {
        let index = self.free_rx.try_recv().ok()?;
        let Ok(mut slot) = self.slots[index as usize].try_lock() else {
            let _ = self.free_tx.try_send(index);
            return None;
        };
        // Exhausted generations retire the slot rather than revive an old handle.
        let generation = slot.generation.checked_add(1)?;
        slot.generation = generation;
        Some(PayloadReservation {
            slots: self,
            index,
            slot: Some(slot),
            published: false,
        })
    }

    pub(super) fn take(&self, id: u64) -> Option<T> {
        self.take_or_acknowledge(id).ok().flatten()
    }

    pub(super) fn take_or_acknowledge(&self, id: u64) -> Result<Option<T>, ()> {
        let index = u32::try_from(id & u64::from(u32::MAX)).expect("masked slot index");
        let generation = (id >> 32) as u32;
        let mut slot = self
            .slots
            .get(index as usize)
            .ok_or(())?
            .lock()
            .map_err(|_| ())?;
        if slot.generation != generation {
            return Err(());
        }
        if slot.value.is_none() && !slot.taken_early {
            return Err(());
        }
        let value = slot.value.take();
        slot.taken_early = false;
        drop(slot);
        self.free_tx
            .try_send(index)
            .expect("consumed slot is not free");
        Ok(value)
    }

    #[cfg(any(test, not(target_arch = "wasm32")))]
    pub(super) fn take_matching(&self, matches: impl Fn(&T) -> bool) -> Option<T> {
        for slot in &self.slots {
            let mut slot = slot
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if slot.value.as_ref().is_some_and(&matches) {
                // Keep the slot until its ring record acknowledges the claim.
                // This bounds tombstones and distinguishes invalid handles.
                let value = slot.value.take();
                slot.taken_early = true;
                return value;
            }
        }
        None
    }
}

pub(super) struct PayloadReservation<'a, T> {
    slots: &'a PayloadSlots<T>,
    index: u32,
    slot: Option<MutexGuard<'a, Slot<T>>>,
    published: bool,
}

impl<T> PayloadReservation<'_, T> {
    pub(super) fn publish(mut self, value: T, push: impl FnOnce(u64) -> bool) -> bool {
        let slot = self.slot.as_mut().expect("reservation owns its slot");
        let id = (u64::from(slot.generation) << 32) | u64::from(self.index);
        slot.value = Some(value);
        self.published = push(id);
        self.published
    }
}

impl<T> Drop for PayloadReservation<'_, T> {
    fn drop(&mut self) {
        if !self.published {
            self.slot
                .as_mut()
                .expect("reservation owns its slot")
                .value
                .take();
        }
        // Publish availability only after releasing the slot's lock.
        self.slot.take();
        if !self.published {
            self.slots
                .free_tx
                .try_send(self.index)
                .expect("reserved slot is not free");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::*;
    use crate::prof::backend::{MeasuredLayouts, ProfilerSizingPolicy};

    fn memory() -> ProfilerMemoryGovernor {
        let layouts = MeasuredLayouts::V1;
        ProfilerMemoryGovernor::new(
            ProfilerSizingPolicy::derive(32 * 1024 * 1024, layouts).unwrap(),
            layouts,
        )
    }

    #[test]
    fn slots_are_bounded_and_stale_handles_cannot_take_reused_payloads() {
        let slots = PayloadSlots::new(1, &memory()).unwrap();
        let mut first = 0;
        assert!(slots.reserve().unwrap().publish(10, |id| {
            first = id;
            true
        }));
        assert!(slots.reserve().is_none());
        assert_eq!(slots.take(first), Some(10));
        assert_eq!(slots.take(first), None);
        let mut second = 0;
        assert!(slots.reserve().unwrap().publish(20, |id| {
            second = id;
            true
        }));
        assert_ne!(first, second);
        assert_eq!(slots.take(first), None);
        assert_eq!(slots.take(second), Some(20));
    }

    #[derive(Debug)]
    struct Tracked(Arc<AtomicUsize>);

    #[test]
    fn early_claim_keeps_a_bounded_receipt_until_the_ring_acknowledges_it() {
        let slots = PayloadSlots::new(1, &memory()).unwrap();
        let mut first = 0;
        assert!(slots.reserve().unwrap().publish(10, |id| {
            first = id;
            true
        }));
        assert_eq!(slots.take_matching(|value| *value == 20), None);
        assert_eq!(slots.take_matching(|value| *value == 10), Some(10));
        assert!(slots.reserve().is_none());
        assert_eq!(slots.take_or_acknowledge(first + (1 << 32)), Err(()));
        assert!(slots.reserve().is_none());
        assert_eq!(slots.take_or_acknowledge(first), Ok(None));
        assert_eq!(slots.take_or_acknowledge(first), Err(()));
        let mut second = 0;
        assert!(slots.reserve().unwrap().publish(20, |id| {
            second = id;
            true
        }));
        assert_eq!(slots.take_or_acknowledge(first), Err(()));
        assert_eq!(slots.take_or_acknowledge(second), Ok(Some(20)));
    }

    impl Drop for Tracked {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn rejected_writes_and_teardown_release_payloads_and_budget() {
        let memory = memory();
        let drops = Arc::new(AtomicUsize::new(0));
        let slots = PayloadSlots::new(1, &memory).unwrap();
        assert!(
            !slots
                .reserve()
                .unwrap()
                .publish(Tracked(drops.clone()), |_| false)
        );
        assert_eq!(drops.load(Ordering::Relaxed), 1);
        drop(slots.reserve().unwrap());
        assert!(
            slots
                .reserve()
                .unwrap()
                .publish(Tracked(drops.clone()), |_| true)
        );
        drop(slots);
        assert_eq!(drops.load(Ordering::Relaxed), 2);
        assert_eq!(memory.used_bytes(ReservationClass::General), 0);
    }

    #[test]
    fn consumer_can_take_a_payload_as_soon_as_the_record_is_published() {
        let slots = PayloadSlots::new(1, &memory()).unwrap();
        let (tx, rx) = crossbeam_channel::bounded(1);
        std::thread::scope(|scope| {
            let consumer = scope.spawn(|| slots.take(rx.recv().unwrap()));
            assert!(slots.reserve().unwrap().publish(42, |id| {
                tx.try_send(id).unwrap();
                true
            }));
            assert_eq!(consumer.join().unwrap(), Some(42));
        });
    }
}
