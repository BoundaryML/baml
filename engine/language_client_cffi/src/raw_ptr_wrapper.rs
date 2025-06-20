use std::{
    ops::Deref,
    sync::{atomic::AtomicBool, Arc},
};

use baml_runtime::tracingv2::storage::storage::{Collector, Usage};

pub struct RawPtrWrapper<T> {
    inner: Arc<T>,
    persist: AtomicBool,
}

impl<T: std::fmt::Debug> std::fmt::Debug for RawPtrWrapper<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.inner)
    }
}

impl<T> RawPtrWrapper<T> {
    pub fn from_raw(object: *const libc::c_void, persist: bool) -> Self {
        Self {
            inner: unsafe { Arc::from_raw(object as *const T) },
            persist: AtomicBool::new(persist),
        }
    }

    pub fn destroy(self) {
        self.persist
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn from_object(object: T) -> Self {
        Self {
            inner: Arc::new(object),
            persist: AtomicBool::new(true),
        }
    }

    pub fn send(&self) -> *const libc::c_void {
        Arc::into_raw(self.inner.clone()) as *const libc::c_void
    }
}

impl<T> Deref for RawPtrWrapper<T> {
    type Target = Arc<T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> Drop for RawPtrWrapper<T> {
    fn drop(&mut self) {
        if self.persist.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = Arc::into_raw(self.inner.clone());
        }
    }
}

pub type CollectorWrapper = RawPtrWrapper<Collector>;
pub type UsageWrapper = RawPtrWrapper<Usage>;
