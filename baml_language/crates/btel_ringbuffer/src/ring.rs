use std::mem::{MaybeUninit, size_of};

use crate::sync::{AtomicUsize, Ordering, Padded, UnsafeCell};

/// Private SPSC storage. The registry supplies exactly one writer and reader.
pub(crate) struct Ring<T> {
    write: Padded<AtomicUsize>,
    read: Padded<AtomicUsize>,
    slots: Box<[UnsafeCell<MaybeUninit<T>>]>,
    mask: usize,
}

// SAFETY: Only the sole writer initializes free slots; only the sole reader
// moves out published slots. Release/acquire cursors transfer exclusive slot
// ownership. Endpoints use &mut self and cannot be cloned. T crosses threads
// by ownership transfer, so Send is sufficient; shared &T is never exposed.
unsafe impl<T: Send> Sync for Ring<T> {}

#[derive(Default)]
pub(crate) struct Cursor {
    pub(crate) next: usize,
    peer: usize,
}

impl<T> Ring<T> {
    pub(crate) fn valid_capacity(capacity: usize) -> bool {
        capacity.is_power_of_two()
            && isize::try_from(capacity).is_ok()
            && capacity
                .checked_mul(size_of::<UnsafeCell<MaybeUninit<T>>>())
                .is_some_and(|bytes| isize::try_from(bytes).is_ok())
    }

    pub(crate) fn new(capacity: usize) -> Self {
        debug_assert!(Self::valid_capacity(capacity));
        Self {
            write: Padded(AtomicUsize::new(0)),
            read: Padded(AtomicUsize::new(0)),
            slots: (0..capacity)
                .map(|_| UnsafeCell::new(MaybeUninit::uninit()))
                .collect(),
            mask: capacity - 1,
        }
    }

    /// Only after acquiring a reusable pair (or allocating it).
    pub(crate) fn writer(&self) -> Cursor {
        let next = self.write.0.load(Ordering::Relaxed);
        Cursor { next, peer: next }
    }

    #[inline(always)]
    pub(crate) fn has_space(&self, cursor: &mut Cursor) -> bool {
        if cursor.next.wrapping_sub(cursor.peer) == self.slots.len() {
            cursor.peer = self.read.0.load(Ordering::Acquire);
            cursor.next.wrapping_sub(cursor.peer) != self.slots.len()
        } else {
            true
        }
    }

    /// Caller owns the writer and has just established space with `has_space`.
    #[inline(always)]
    pub(crate) fn write(&self, cursor: &mut Cursor, value: T) {
        self.slots[cursor.next & self.mask].with_mut(|slot| {
            // SAFETY: This slot is free, and no reader can access it until
            // publication below. Writing MaybeUninit does not drop stale bits.
            unsafe {
                (*slot).write(value);
            }
        });
        cursor.next = cursor.next.wrapping_add(1);
        self.write.0.store(cursor.next, Ordering::Release);
    }

    #[inline(always)]
    pub(crate) fn read(&self, cursor: &mut Cursor) -> Option<T> {
        if cursor.next == cursor.peer {
            cursor.peer = self.write.0.load(Ordering::Acquire);
            if cursor.next == cursor.peer {
                return None;
            }
        }
        let value = self.slots[cursor.next & self.mask].with_mut(|slot| {
            // SAFETY: The acquired publication initialized this slot. The
            // reader is exclusive and does not release it until after moving.
            unsafe { (*slot).assume_init_read() }
        });
        cursor.next = cursor.next.wrapping_add(1);
        self.read.0.store(cursor.next, Ordering::Release);
        Some(value)
    }

    pub(crate) fn is_empty(&self, cursor: &Cursor) -> bool {
        cursor.next == self.write.0.load(Ordering::Acquire)
    }

    #[cfg(test)]
    pub(crate) fn seed_empty(&self, position: usize) {
        self.read.0.store(position, Ordering::Relaxed);
        self.write.0.store(position, Ordering::Relaxed);
    }
}

impl<T> Drop for Ring<T> {
    fn drop(&mut self) {
        // No endpoint can survive its shared allocation. Destruction therefore
        // runs with exclusive storage ownership, never concurrently with a writer.
        let mut read = self.read.0.load(Ordering::Relaxed);
        let write = self.write.0.load(Ordering::Relaxed);
        while read != write {
            self.slots[read & self.mask].with_mut(|slot| {
                // SAFETY: Exactly this unread range remains initialized.
                unsafe {
                    (*slot).assume_init_drop();
                }
            });
            read = read.wrapping_add(1);
        }
    }
}
