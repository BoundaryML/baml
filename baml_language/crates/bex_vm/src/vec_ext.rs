//! Unchecked stack operations with explicit element-lifetime contracts.

#![allow(unsafe_code)]

pub(crate) trait VecExt {
    /// Drop the last element without moving it out of the vector.
    ///
    /// # Safety
    ///
    /// The vector must be nonempty.
    unsafe fn no_return_pop(&mut self);
}

impl<T> VecExt for Vec<T> {
    #[inline]
    unsafe fn no_return_pop(&mut self) {
        debug_assert!(!self.is_empty());
        let new_len = self.len() - 1;
        // SAFETY: the caller guarantees a last initialized element. Shorten the
        // vector before dropping it so a panicking destructor cannot cause that
        // element to be dropped again. The allocation and capacity stay intact.
        unsafe {
            self.set_len(new_len);
            std::ptr::drop_in_place(self.as_mut_ptr().add(new_len));
        }
    }
}

/// Stack transitions for values that need no destructor. Frames must instead
/// use `VecExt::no_return_pop`, which runs the removed element's destructor.
pub(crate) trait CopyVecExt<T: Copy> {
    /// Replace the top `N` elements with `result` in their lowest slot.
    ///
    /// # Safety
    /// Requires `1 <= N <= self.len()`.
    unsafe fn replace_top_n<const N: usize>(&mut self, result: T);

    /// The runtime-count form of `replace_top_n`.
    ///
    /// # Safety
    /// Requires `1 <= n <= self.len()`.
    unsafe fn replace_top_n_dynamic(&mut self, n: usize, result: T);

    /// Discard the top `n` elements without writing a result.
    ///
    /// # Safety
    /// Requires `n <= self.len()`. Zero is allowed.
    unsafe fn truncate_suffix(&mut self, n: usize);
}

#[allow(clippy::inline_always)]
impl<T: Copy> CopyVecExt<T> for Vec<T> {
    #[inline(always)]
    unsafe fn replace_top_n<const N: usize>(&mut self, result: T) {
        if N == 1 {
            debug_assert!(!self.is_empty());
            // Avoid relying on alias analysis to remove a redundant length
            // store after writing through the vector's element pointer.
            // SAFETY: the caller guarantees an initialized last element.
            unsafe { self.as_mut_ptr().add(self.len() - 1).write(result) };
        } else {
            // SAFETY: the caller supplies the same contract with a constant count.
            unsafe { self.replace_top_n_dynamic(N, result) };
        }
    }

    #[inline(always)]
    unsafe fn replace_top_n_dynamic(&mut self, n: usize, result: T) {
        debug_assert!(n > 0 && n <= self.len());
        let destination = self.len() - n;
        // SAFETY: destination is initialized and the new length cannot exceed
        // the old length. Copy values need no drop when overwritten/discarded.
        unsafe {
            self.as_mut_ptr().add(destination).write(result);
            self.set_len(destination + 1);
        }
    }

    #[inline(always)]
    unsafe fn truncate_suffix(&mut self, n: usize) {
        debug_assert!(n <= self.len());
        // SAFETY: the new length is no larger; discarded Copy values need no drop.
        unsafe { self.set_len(self.len() - n) };
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::RefCell,
        panic::{AssertUnwindSafe, catch_unwind},
        rc::Rc,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use super::{CopyVecExt, VecExt};

    #[test]
    fn replacements_preserve_prefix_and_allocation() {
        let mut values = vec![10, 20, 30, 40];
        let ptr = values.as_ptr();
        let capacity = values.capacity();
        // SAFETY: every replacement consumes between one and all live elements.
        unsafe {
            values.replace_top_n::<1>(41);
            assert_eq!(values, [10, 20, 30, 41]);
            values.replace_top_n::<2>(50);
            assert_eq!(values, [10, 20, 50]);
            values.replace_top_n_dynamic(3, 80);
            assert_eq!(values, [80]);
        }
        assert_eq!(values.as_ptr(), ptr);
        assert_eq!(values.capacity(), capacity);
        values.push(90);
        assert_eq!(values, [80, 90]);
    }

    #[test]
    fn copy_suffix_truncation_allows_zero_and_full_length() {
        let mut values = vec![1, 2, 3];
        // SAFETY: each count is within the current length.
        unsafe {
            values.truncate_suffix(0);
            assert_eq!(values, [1, 2, 3]);
            values.truncate_suffix(2);
            assert_eq!(values, [1]);
            values.truncate_suffix(1);
            assert!(values.is_empty());
            values.truncate_suffix(0);
        }
    }

    #[test]
    fn replacements_support_zero_sized_copy_values() {
        let mut values = vec![(); 4];
        // SAFETY: each replacement consumes between one and all live elements.
        unsafe {
            values.replace_top_n::<2>(());
            assert_eq!(values.len(), 3);
            values.replace_top_n_dynamic(3, ());
            assert_eq!(values.len(), 1);
        }
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "n > 0 && n <= self.len()")]
    fn replacement_rejects_zero_count_in_debug() {
        // SAFETY: deliberately invalid only in a debug build, where the
        // assertion panics before any unchecked operation can execute.
        unsafe { vec![1].replace_top_n::<0>(2) };
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "n > 0 && n <= self.len()")]
    fn replacement_rejects_underflow_in_debug() {
        // SAFETY: the debug assertion rejects the invalid count before access.
        unsafe { vec![1].replace_top_n_dynamic(2, 3) };
    }

    struct Record {
        id: usize,
        drops: Rc<RefCell<Vec<usize>>>,
        panic_on_drop: bool,
    }

    impl Drop for Record {
        fn drop(&mut self) {
            self.drops.borrow_mut().push(self.id);
            assert!(!self.panic_on_drop, "test destructor panic");
        }
    }

    #[test]
    fn drops_only_last_and_preserves_allocation() {
        let drops = Rc::new(RefCell::new(Vec::new()));
        let mut values = Vec::with_capacity(4);
        for id in 0..3 {
            values.push(Record {
                id,
                drops: Rc::clone(&drops),
                panic_on_drop: false,
            });
        }
        let ptr = values.as_ptr();
        let capacity = values.capacity();
        // SAFETY: three elements were just pushed.
        unsafe { values.no_return_pop() };
        assert_eq!(values.len(), 2);
        assert_eq!(values.as_ptr(), ptr);
        assert_eq!(values.capacity(), capacity);
        assert_eq!(*drops.borrow(), [2]);
        values.push(Record {
            id: 3,
            drops: Rc::clone(&drops),
            panic_on_drop: false,
        });
        drop(values);
        drops.borrow_mut().sort_unstable();
        assert_eq!(*drops.borrow(), [0, 1, 2, 3]);
    }

    #[test]
    fn panicking_destructor_is_not_dropped_again() {
        let drops = Rc::new(RefCell::new(Vec::new()));
        let mut values = vec![
            Record {
                id: 0,
                drops: Rc::clone(&drops),
                panic_on_drop: false,
            },
            Record {
                id: 1,
                drops: Rc::clone(&drops),
                panic_on_drop: true,
            },
        ];
        let result = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: the vector contains two elements.
            unsafe { values.no_return_pop() };
        }));
        assert!(result.is_err());
        assert_eq!(values.len(), 1);
        assert_eq!(*drops.borrow(), [1]);
        drop(values);
        assert_eq!(*drops.borrow(), [1, 0]);
    }

    #[test]
    fn drops_zero_sized_elements_once() {
        static DROPS: AtomicUsize = AtomicUsize::new(0);
        struct ZeroSized;
        impl Drop for ZeroSized {
            fn drop(&mut self) {
                DROPS.fetch_add(1, Ordering::Relaxed);
            }
        }
        let mut values = vec![ZeroSized, ZeroSized, ZeroSized];
        // SAFETY: the vector contains three zero-sized elements.
        unsafe { values.no_return_pop() };
        assert_eq!(values.len(), 2);
        assert_eq!(DROPS.load(Ordering::Relaxed), 1);
        drop(values);
        assert_eq!(DROPS.load(Ordering::Relaxed), 3);
    }
}
