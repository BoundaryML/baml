pub(crate) use std::sync::TryLockError;
#[cfg(not(baml_loom))]
pub(crate) use std::{
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
    },
    thread,
};

#[cfg(baml_loom)]
pub(crate) use loom::{
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
    },
    thread,
};

#[inline]
pub(crate) fn spin() {
    #[cfg(not(baml_loom))]
    std::hint::spin_loop();
    // Only the model yields so Loom can explore the other participant.
    #[cfg(baml_loom)]
    thread::yield_now();
}

pub(crate) use btel_settings::layout::Padded;
