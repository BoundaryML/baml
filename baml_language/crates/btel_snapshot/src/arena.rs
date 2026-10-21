//! The growable arrays a capture is stored in.
//!
//! A capture's storage is reused, so an arena keeps its capacity when it is
//! cleared. With the `stats` feature every growth is counted for the pool the
//! storage came from; without it there is nothing to count.
use std::mem::size_of;
#[cfg(any(test, feature = "stats"))]
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

/// What one pool's storages have allocated, as they grow.
#[cfg(any(test, feature = "stats"))]
#[derive(Default)]
pub(crate) struct Counters {
    /// Bytes of structural capacity, across idle and checked-out storages.
    pub(crate) bytes: AtomicUsize,
    /// The most `bytes` has been, counting an arena's old and new allocation
    /// together while it moves.
    pub(crate) peak: AtomicUsize,
    pub(crate) growths: AtomicUsize,
}
#[cfg(any(test, feature = "stats"))]
impl Counters {
    pub(crate) fn add(&self, bytes: usize) {
        let total = self.bytes.fetch_add(bytes, Ordering::Relaxed) + bytes;
        self.peak.fetch_max(total, Ordering::Relaxed);
    }
    pub(crate) fn remove(&self, bytes: usize) {
        self.bytes.fetch_sub(bytes, Ordering::Relaxed);
    }
}

/// Where a storage's arenas report their growth.
#[derive(Default)]
pub(crate) struct Meter {
    #[cfg(any(test, feature = "stats"))]
    pub(crate) counters: Arc<Counters>,
}
impl Meter {
    /// An arena moves from `old` bytes to an allocation of `replacement` or
    /// more: `grow` makes the move and returns the bytes it now holds. The
    /// new allocation is counted before it exists, so the peak covers the
    /// moment both are live.
    #[cfg(any(test, feature = "stats"))]
    fn growing(&self, old: usize, replacement: usize, grow: impl FnOnce() -> usize) {
        struct Pending<'a> {
            counters: &'a Counters,
            bytes: usize,
        }
        impl Drop for Pending<'_> {
            fn drop(&mut self) {
                self.counters.remove(self.bytes);
            }
        }
        self.counters.add(replacement);
        let mut pending = Pending {
            counters: &self.counters,
            bytes: replacement,
        };
        let actual = grow();
        // An allocator may hand out more than was asked for.
        if actual > replacement {
            self.counters.add(actual - replacement);
        }
        pending.bytes = replacement.saturating_sub(actual) + old;
        self.counters.growths.fetch_add(1, Ordering::Relaxed);
    }
    #[cfg(not(any(test, feature = "stats")))]
    #[expect(clippy::unused_self, reason = "one signature with and without `stats`")]
    fn growing(&self, _: usize, _: usize, grow: impl FnOnce() -> usize) {
        grow();
    }
}

/// One typed arena of a capture. Read as a slice.
pub(crate) struct Arena<T>(Vec<T>);
impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}
impl<T> std::ops::Deref for Arena<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.0
    }
}
impl<T> std::ops::DerefMut for Arena<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.0
    }
}
impl<T> Arena<T> {
    /// Make room for `additional` more items, at least doubling when it grows.
    pub(crate) fn reserve(&mut self, additional: usize, meter: &Meter) {
        let required = self
            .0
            .len()
            .checked_add(additional)
            .expect("snapshot capacity overflow");
        if required <= self.0.capacity() {
            return;
        }
        let capacity = required.max(self.0.capacity().saturating_mul(2));
        let replacement = capacity
            .checked_mul(size_of::<T>())
            .expect("snapshot byte capacity overflow");
        meter.growing(self.capacity_bytes(), replacement, || {
            self.0
                .try_reserve_exact(capacity - self.0.len())
                .expect("snapshot allocation failed");
            self.capacity_bytes()
        });
    }
    pub(crate) fn push(&mut self, item: T, meter: &Meter) {
        self.reserve(1, meter);
        self.0.push(item);
    }
    pub(crate) fn extend_from_slice(&mut self, items: &[T], meter: &Meter)
    where
        T: Clone,
    {
        self.reserve(items.len(), meter);
        self.0.extend_from_slice(items);
    }
    /// Grow to `len` items, filling with `item`. Never shrinks.
    pub(crate) fn fill_to(&mut self, len: usize, item: T, meter: &Meter)
    where
        T: Clone,
    {
        self.reserve(len.saturating_sub(self.0.len()), meter);
        if len > self.0.len() {
            self.0.resize(len, item);
        }
    }
    /// Drop the items and keep the capacity.
    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }
    pub(crate) fn capacity_bytes(&self) -> usize {
        self.0.capacity().saturating_mul(size_of::<T>())
    }
}
