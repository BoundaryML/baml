/// Replaces a Salsa query value when whole-value equality changes.
///
/// # Safety
///
/// The caller must uphold the safety contract of
/// [`salsa::Update::maybe_update`], including that comparing values across
/// revisions is safe and equality makes replacement unnecessary.
#[allow(unsafe_code)]
pub unsafe fn update_by_eq<T: PartialEq>(old_pointer: *mut T, new_value: T) -> bool {
    // SAFETY: The caller upholds Salsa's `Update::maybe_update` contract.
    let old = unsafe { &*old_pointer };
    if old == &new_value {
        false
    } else {
        // SAFETY: The caller guarantees that `old_pointer` is valid for replacement.
        unsafe {
            std::ptr::drop_in_place(old_pointer);
            std::ptr::write(old_pointer, new_value);
        }
        true
    }
}

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    struct Value {
        equality_key: u8,
        instance: u8,
        dropped: Rc<RefCell<Vec<u8>>>,
    }

    impl PartialEq for Value {
        fn eq(&self, other: &Self) -> bool {
            self.equality_key == other.equality_key
        }
    }

    impl Drop for Value {
        fn drop(&mut self) {
            self.dropped.borrow_mut().push(self.instance);
        }
    }

    fn value(equality_key: u8, instance: u8, dropped: &Rc<RefCell<Vec<u8>>>) -> Value {
        Value {
            equality_key,
            instance,
            dropped: Rc::clone(dropped),
        }
    }

    #[test]
    fn retains_old_value_when_equal() {
        let dropped = Rc::new(RefCell::new(Vec::new()));
        let mut old = value(0, 0, &dropped);

        // SAFETY: `old` remains initialized and valid for the duration of the call.
        let changed =
            unsafe { super::update_by_eq(std::ptr::from_mut(&mut old), value(0, 1, &dropped)) };

        assert!(!changed);
        assert_eq!(old.instance, 0);
        assert_eq!(dropped.borrow().as_slice(), &[1]);
    }

    #[test]
    fn replaces_old_value_when_unequal() {
        let dropped = Rc::new(RefCell::new(Vec::new()));
        let mut old = value(0, 0, &dropped);

        // SAFETY: `old` remains initialized and valid for the duration of the call.
        let changed =
            unsafe { super::update_by_eq(std::ptr::from_mut(&mut old), value(1, 1, &dropped)) };

        assert!(changed);
        assert_eq!(old.instance, 1);
        assert_eq!(dropped.borrow().as_slice(), &[0]);
    }
}
