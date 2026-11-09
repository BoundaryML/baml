//! Owner-thread native values with GC lifetime, without shared heap locks.
use crate::R;
use baml_rt_gc::{Descriptor, Kind};
use std::{marker::PhantomData, rc::Rc};

pub const fn descriptor(type_id: u32) -> Descriptor {
    Descriptor {
        kind: Kind::Opaque,
        size: 8,
        ptr_slots: (0, 0),
        tagged_slots: (0, 0),
        elem: None,
        type_id,
        fields: &[],
        finalizer: None,
    }
}
/// Native values must contain no managed pointers. Use typed GC fields for
/// references into the language heap. Interior mutability belongs to the owner.
pub struct Opaque<T: 'static> {
    raw: usize,
    marker: PhantomData<Rc<T>>,
}
impl<T> Copy for Opaque<T> {}
impl<T> Clone for Opaque<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: 'static> Opaque<T> {
    pub fn new(desc: u32, value: T) -> R<Self> {
        let raw = unsafe { crate::try_alloc(desc, 8)? } as usize;
        unsafe {
            (&mut *crate::HEAP.get()).attach_native(raw, Rc::new(value));
        }
        Ok(Self {
            raw,
            marker: PhantomData,
        })
    }
    /// # Safety
    /// The address is a live opaque allocation of T in the current instance.
    pub unsafe fn from_raw(raw: usize) -> Self {
        Self {
            raw,
            marker: PhantomData,
        }
    }
    pub fn as_raw(self) -> usize {
        self.raw
    }
    pub fn with<R>(self, call: impl FnOnce(&T) -> R) -> R {
        // Release the store borrow before callbacks can allocate or collect.
        let owned = unsafe { (&*crate::HEAP.get()).native::<T>(self.raw) };
        call(&owned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    struct Resource(Rc<Cell<usize>>);
    impl Drop for Resource {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    static D: &[Descriptor] = &[descriptor(3)];
    #[test]
    fn native_values_survive_collection_and_drop_exactly_once() {
        let drops = Rc::new(Cell::new(0));
        let counter = drops.clone();
        let mut instance = crate::Instance::new(D, &[false, false, false, true]);
        instance.set_stress(true);
        unsafe {
            instance.run_owned(move || -> R<()> {
                let kept = Opaque::new(0, Resource(counter.clone()))?;
                for _ in 0..64 {
                    let _ = Opaque::new(0, Resource(counter.clone()))?;
                    crate::safepoint().unwrap();
                }
                kept.with(|r| assert!(Rc::ptr_eq(&r.0, &counter)));
                assert!(counter.get() <= 64);
                assert!(
                    counter.get() > 0,
                    "collections must release unreachable native resources"
                );
                Ok(())
            })
        }
        .unwrap();
        drop(instance);
        assert_eq!(drops.get(), 65);
        assert!(crate::HEAP.get().is_null());
    }
}
