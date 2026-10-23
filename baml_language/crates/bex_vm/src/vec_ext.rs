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

#[cfg(test)]
mod tests {
    use std::{
        cell::RefCell,
        panic::{AssertUnwindSafe, catch_unwind},
        rc::Rc,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use super::VecExt;

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
