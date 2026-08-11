//! loom/std shims: the ring compiles against these so the exact same code is
//! model-checked under `RUSTFLAGS="--cfg baml_loom"` and shipped against std.
#![allow(unsafe_code)]
// On wasm32 there is no background consumer thread, but shared ring and
// cooperative-drain code still compile through these std shims.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

#[cfg(not(baml_loom))]
pub(crate) use std::sync::atomic::{
    AtomicBool, AtomicPtr, AtomicU8, AtomicU32, AtomicU64, AtomicUsize, Ordering,
};
#[cfg(all(not(baml_loom), test))]
pub(crate) use std::thread;

#[cfg(baml_loom)]
pub(crate) use loom::cell::UnsafeCell;
#[cfg(baml_loom)]
pub(crate) use loom::sync::atomic::{
    AtomicBool, AtomicPtr, AtomicU8, AtomicU32, AtomicU64, AtomicUsize, Ordering,
};
#[cfg(baml_loom)]
pub(crate) use loom::thread;

/// std stand-in for [`loom::cell::UnsafeCell`]'s closure-based API.
#[cfg(not(baml_loom))]
#[derive(Debug)]
pub(crate) struct UnsafeCell<T>(std::cell::UnsafeCell<T>);

#[cfg(not(baml_loom))]
impl<T> UnsafeCell<T> {
    pub(crate) const fn new(value: T) -> Self {
        Self(std::cell::UnsafeCell::new(value))
    }

    /// Immutable access. Caller upholds the no-concurrent-mutation contract
    /// the loom build model-checks.
    #[inline]
    pub(crate) fn with<R>(&self, f: impl FnOnce(*const T) -> R) -> R {
        f(self.0.get())
    }

    /// Mutable access. Caller upholds the exclusive-access contract the loom
    /// build model-checks.
    #[inline]
    pub(crate) fn with_mut<R>(&self, f: impl FnOnce(*mut T) -> R) -> R {
        f(self.0.get())
    }
}

/// A segment's byte buffer.
///
/// The producer writes a record's bytes and then publishes them with a
/// `Release` store of `commit_len`; the consumer only ever reads ranges below
/// a `commit_len` it `Acquire`-loaded. Producer and consumer therefore touch
/// disjoint byte ranges at all times. The per-byte cells exist so the loom
/// build verifies exactly that claim; in the std build they compile away
/// (`UnsafeCell<u8>` has `u8`'s layout) and writes are a straight `memcpy`.
pub(crate) struct Buf {
    #[cfg(not(baml_loom))]
    bytes: Box<[std::cell::UnsafeCell<u8>]>,
    #[cfg(baml_loom)]
    bytes: Box<[loom::cell::UnsafeCell<u8>]>,
}

impl Buf {
    pub(crate) fn new(len: usize) -> Self {
        #[cfg(not(baml_loom))]
        let bytes = (0..len).map(|_| std::cell::UnsafeCell::new(0u8)).collect();
        #[cfg(baml_loom)]
        let bytes = (0..len).map(|_| loom::cell::UnsafeCell::new(0u8)).collect();
        Self { bytes }
    }

    pub(crate) fn capacity(&self) -> usize {
        self.bytes.len()
    }

    /// Calls `f` with a mutable view of `[offset, offset + len)` so the
    /// producer can serialize a record *directly into the ring*, skipping the
    /// intermediate stack buffer and the extra copy that encoding into a
    /// scratch buffer and copying it in would require.
    ///
    /// # Safety
    /// Caller is the ring's unique producer, the range is in bounds, and no
    /// byte of it has been published (the consumer never reads it
    /// concurrently). `f` must initialize every byte of the slice before it
    /// is committed.
    #[cfg(not(baml_loom))]
    #[inline]
    pub(crate) unsafe fn with_slice_mut(
        &self,
        offset: usize,
        len: usize,
        f: impl FnOnce(&mut [u8]),
    ) {
        debug_assert!(offset + len <= self.bytes.len());
        let base = std::cell::UnsafeCell::raw_get(self.bytes.as_ptr());
        let slice = unsafe { std::slice::from_raw_parts_mut(base.add(offset), len) };
        f(slice);
    }

    /// See the std-build `with_slice_mut`. Fills a temp then stores per byte so
    /// loom still models the producer's writes (the in-place path is a std-only
    /// optimization; the byte-level write set is identical).
    #[cfg(baml_loom)]
    pub(crate) unsafe fn with_slice_mut(
        &self,
        offset: usize,
        len: usize,
        f: impl FnOnce(&mut [u8]),
    ) {
        let mut tmp = vec![0u8; len];
        f(&mut tmp);
        for (i, byte) in tmp.iter().enumerate() {
            self.bytes[offset + i].with_mut(|p| unsafe { *p = *byte });
        }
    }

    /// Calls `f` with the bytes in `[offset, offset + len)`.
    ///
    /// # Safety
    /// Caller is the ring's single consumer and the whole range was published
    /// by a `commit_len` Release→Acquire pair (committed bytes are never
    /// rewritten while reachable by the consumer).
    #[cfg(not(baml_loom))]
    #[inline]
    pub(crate) unsafe fn with_slice<R>(
        &self,
        offset: usize,
        len: usize,
        f: impl FnOnce(&[u8]) -> R,
    ) -> R {
        debug_assert!(offset + len <= self.bytes.len());
        let base = std::cell::UnsafeCell::raw_get(self.bytes.as_ptr()).cast_const();
        let slice = unsafe { std::slice::from_raw_parts(base.add(offset), len) };
        f(slice)
    }

    /// See the std-build `with_slice`. Copies out per byte so loom tracks the
    /// reads.
    #[cfg(baml_loom)]
    pub(crate) unsafe fn with_slice<R>(
        &self,
        offset: usize,
        len: usize,
        f: impl FnOnce(&[u8]) -> R,
    ) -> R {
        let mut copy = Vec::with_capacity(len);
        for i in 0..len {
            copy.push(self.bytes[offset + i].with(|p| unsafe { *p }));
        }
        f(&copy)
    }
}
