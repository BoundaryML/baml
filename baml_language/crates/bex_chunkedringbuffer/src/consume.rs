//! Initialized-slot ownership for in-place record consumption.
#![allow(unsafe_code)]

/// Visit each live slot, then destroy it in place. A callback may take ownership
/// of fields, but cannot retain the slot borrow. Panic destroys only live slots.
pub(crate) fn consume<T>(records: &mut Vec<T>, mut visit: impl FnMut(&mut T)) {
    struct Remaining<'a, T> {
        records: &'a mut Vec<T>,
        next: usize,
        len: usize,
    }
    impl<T> Drop for Remaining<'_, T> {
        fn drop(&mut self) {
            // SAFETY: [next,len) is exactly the still initialized suffix. Vec's
            // length is zero, so it cannot destroy these slots a second time.
            unsafe {
                std::ptr::drop_in_place(std::ptr::slice_from_raw_parts_mut(
                    self.records.as_mut_ptr().add(self.next),
                    self.len - self.next,
                ));
            }
        }
    }
    let len = records.len();
    // SAFETY: Remaining owns destruction from now on; allocation remains in Vec.
    unsafe {
        records.set_len(0);
    }
    let mut remaining = Remaining {
        records,
        next: 0,
        len,
    };
    while remaining.next < len {
        // SAFETY: this index is in the live initialized suffix; exclusive Vec borrow.
        let slot = unsafe { remaining.records.as_mut_ptr().add(remaining.next) };
        visit(unsafe { &mut *slot });
        // Advance before destruction: if Drop panics, this slot must not be retried.
        remaining.next += 1;
        unsafe {
            std::ptr::drop_in_place(slot);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::Cell,
        panic::{AssertUnwindSafe, catch_unwind},
        rc::Rc,
    };

    use super::*;
    struct Value(Rc<Cell<usize>>);
    impl Drop for Value {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    #[test]
    fn partial_panic_and_reuse_destroy_only_initialized_slots() {
        let drops = Rc::new(Cell::new(0));
        let mut records = Vec::with_capacity(8);
        for _ in 0..5 {
            records.push(Some(Value(drops.clone())));
        }
        let ptr = records.as_ptr();
        let mut taken = None;
        let mut count = 0;
        assert!(
            catch_unwind(AssertUnwindSafe(|| consume(&mut records, |slot| {
                count += 1;
                if count == 1 {
                    taken = slot.take();
                }
                assert!(count != 3, "partial");
            })))
            .is_err()
        );
        assert_eq!(drops.get(), 4);
        assert!(records.is_empty());
        drop(taken);
        assert_eq!(drops.get(), 5);
        records.push(Some(Value(drops.clone())));
        consume(&mut records, |_| {});
        assert_eq!(ptr, records.as_ptr());
        assert_eq!(drops.get(), 6);
    }
}
