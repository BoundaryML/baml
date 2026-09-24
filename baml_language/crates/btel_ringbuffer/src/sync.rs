#[cfg(not(baml_loom))]
pub(crate) use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
};

#[cfg(baml_loom)]
pub(crate) use loom::{
    cell::UnsafeCell,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
};

#[cfg(not(baml_loom))]
pub(crate) struct UnsafeCell<T>(std::cell::UnsafeCell<T>);
#[cfg(not(baml_loom))]
impl<T> UnsafeCell<T> {
    pub(crate) fn new(value: T) -> Self {
        Self(std::cell::UnsafeCell::new(value))
    }
    pub(crate) fn with_mut<R>(&self, f: impl FnOnce(*mut T) -> R) -> R {
        f(self.0.get())
    }
}

#[inline]
pub(crate) fn spin() {
    #[cfg(not(baml_loom))]
    std::hint::spin_loop();
    // Loom must explore the other participant, not exhaust its branch budget
    // modeling a hardware spin hint. Production NEVER yields here.
    #[cfg(baml_loom)]
    thread::yield_now();
}

pub(crate) use btel_settings::layout::Padded;
