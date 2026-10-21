//! Rust values held on the BAML heap.
//!
//! A `$rust_type` field holds an [`Object::RustData`](crate::Object::RustData):
//! an `Arc` to a Rust value the VM cannot look inside. The collector treats it
//! as a leaf, so the only thing it needs from the value is how much memory it
//! keeps alive, and every payload type says so by implementing
//! [`BexRustData`]. There is no default: a type that cannot be sized cannot be
//! stored.

use std::{
    any::{Any, TypeId},
    fmt,
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicUsize, Ordering},
    },
};

use crate::{AllocDebt, Meter};

/// A Rust value that can be stored on the heap.
///
/// # Measuring
///
/// [`measure`](Self::measure) reports the memory the value keeps alive beyond
/// its own allocation, which the heap reports itself: buffers it owns go to
/// [`Meter::bytes`], strings to [`Meter::string`], and allocations other values
/// may also hold to [`Meter::shared`]. It runs on the allocating VM's thread
/// when the value is stored, and on the collector's thread during a census,
/// while every VM is parked but detached tasks still run. So it must:
///
/// - never block: no lock, no await, no channel;
/// - never `try_lock` a lock that asynchronous code shares, because callers
///   read meaning into a failed `try_lock` (a value that grows off the VM
///   thread keeps a [`RetainedBytes`] its mutators maintain instead);
/// - never drop a last reference, since a `Drop` may take a lock;
/// - never follow a heap reference, allocate on the heap, or call back into
///   the VM, the engine, or the host;
/// - never panic.
///
/// Memory the heap, the engine, or the host owns is not reported here: a heap
/// handle, a type head, the engine itself, or a host object.
pub trait BexRustData: Any + Send + Sync {
    fn measure(&self, meter: &mut Meter);

    /// The name of the concrete type, for diagnostics.
    fn type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}

impl dyn BexRustData {
    /// The value as `Any`, for the downcasts below.
    pub fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }

    /// The `TypeId` of the concrete value. Named so it cannot be confused
    /// with `Any::type_id` called on the `Arc` holding the value, which
    /// reports the `Arc`'s type.
    pub fn payload_type_id(&self) -> TypeId {
        self.as_any().type_id()
    }

    pub fn is<T: BexRustData>(&self) -> bool {
        self.as_any().is::<T>()
    }

    pub fn downcast_ref<T: BexRustData>(&self) -> Option<&T> {
        self.as_any().downcast_ref::<T>()
    }
}

impl fmt::Debug for dyn BexRustData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<rust data: {}>", self.type_name())
    }
}

/// Downcasting a shared payload to its concrete type.
pub trait RustDataArc: Sized {
    /// The payload as `Arc<T>`, sharing the allocation, or the original back.
    fn downcast_payload<T: BexRustData>(self) -> Result<Arc<T>, Self>;

    /// The `TypeId` of the concrete value held.
    fn payload_type_id(&self) -> TypeId;
}

impl RustDataArc for Arc<dyn BexRustData> {
    fn downcast_payload<T: BexRustData>(self) -> Result<Arc<T>, Self> {
        if !self.is::<T>() {
            return Err(self);
        }
        let any: Arc<dyn Any + Send + Sync> = self;
        any.downcast::<T>()
            .map_err(|_| unreachable!("the type was checked just above"))
    }

    fn payload_type_id(&self) -> TypeId {
        (**self).payload_type_id()
    }
}

pub use bex_resource_types::RetainedBytes;

/// What a value keeps alive beyond its own allocation, for a value that is
/// mutated behind a [`MeteredMutex`].
pub trait RetainedFootprint {
    fn retained_bytes(&self) -> usize;
}

/// A mutex around a payload that grows: every unlock publishes the payload's
/// retained bytes, and every [`lock`](Self::lock) charges the growth since the
/// last charge to the account it is given. Growth is therefore charged one
/// lock late, which the VM's settle points make the only delay, and the
/// census reads the published size with no lock at all.
///
/// A mutator with no account, running off the VM thread, locks with
/// [`lock_uncharged`](Self::lock_uncharged): its growth is published, seen by
/// the census, and charged by the next lock that has an account.
#[derive(Debug)]
pub struct MeteredMutex<T> {
    state: Mutex<T>,
    retained: RetainedBytes,
    /// What the budget has been charged for; the allocation charge covers the
    /// initial size.
    charged: AtomicUsize,
}

impl<T: RetainedFootprint> MeteredMutex<T> {
    pub fn new(value: T) -> Self {
        let initial = value.retained_bytes();
        Self {
            state: Mutex::new(value),
            retained: RetainedBytes::new(initial),
            charged: AtomicUsize::new(initial),
        }
    }

    /// Lock, charging `debt` the growth published since the last charge.
    pub fn lock(&self, debt: &AllocDebt) -> MeteredGuard<'_, T> {
        let now = self.retained.get();
        let before = self.charged.swap(now, Ordering::Relaxed);
        debt.add(
            isize::try_from(now).unwrap_or(isize::MAX)
                - isize::try_from(before).unwrap_or(isize::MAX),
        );
        self.lock_uncharged()
    }

    /// Lock without an account; see the type's docs.
    pub fn lock_uncharged(&self) -> MeteredGuard<'_, T> {
        MeteredGuard {
            guard: self.state.lock().unwrap_or_else(PoisonError::into_inner),
            retained: &self.retained,
        }
    }

    /// The payload's retained bytes as last published.
    pub fn retained_bytes(&self) -> usize {
        self.retained.get()
    }
}

/// Exclusive access through a [`MeteredMutex`]; publishes the payload's size
/// when dropped.
pub struct MeteredGuard<'a, T: RetainedFootprint> {
    guard: MutexGuard<'a, T>,
    retained: &'a RetainedBytes,
}

impl<T: RetainedFootprint> std::ops::Deref for MeteredGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.guard
    }
}

impl<T: RetainedFootprint> std::ops::DerefMut for MeteredGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.guard
    }
}

impl<T: RetainedFootprint> Drop for MeteredGuard<'_, T> {
    fn drop(&mut self) {
        self.retained.set(self.guard.retained_bytes());
    }
}

/// A payload with nothing to measure, for tests that only need some
/// `RustData` object.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TestRustData(pub u64);

impl BexRustData for TestRustData {
    fn measure(&self, _: &mut Meter) {}
}

// The resource crate sits below this one, so its payloads are sized here.

impl BexRustData for bex_resource_types::HostValueArc {
    fn measure(&self, _: &mut Meter) {
        // The value itself lives in the host.
    }
}

impl BexRustData for bex_resource_types::ResourceHandle {
    fn measure(&self, meter: &mut Meter) {
        // The resource lives in a registry the handle only names; what it
        // buffers on the handle's behalf is published by whoever fills it.
        meter.bytes(self.display_name().len() + self.retained_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Payload {
        bytes: Vec<u8>,
    }

    impl BexRustData for Payload {
        fn measure(&self, meter: &mut Meter) {
            meter.bytes(self.bytes.capacity());
        }
    }

    #[test]
    fn type_ids_name_the_payload_not_the_arc() {
        let payload: Arc<dyn BexRustData> = Arc::new(Payload { bytes: vec![0; 10] });
        assert_eq!(payload.payload_type_id(), TypeId::of::<Payload>());
        assert_ne!(
            payload.payload_type_id(),
            TypeId::of::<Arc<dyn BexRustData>>()
        );
        assert!(payload.is::<Payload>());
        assert!(!payload.is::<TestRustData>());
        assert_eq!(payload.downcast_ref::<Payload>().unwrap().bytes.len(), 10);
        assert_eq!(
            format!("{payload:?}"),
            "<rust data: bex_vm_types::rust_data::tests::Payload>"
        );
    }

    #[test]
    fn downcasting_shares_the_allocation() {
        let payload: Arc<dyn BexRustData> = Arc::new(Payload { bytes: Vec::new() });
        let address = Arc::as_ptr(&payload).cast::<()>();
        let other = Arc::clone(&payload)
            .downcast_payload::<TestRustData>()
            .expect_err("wrong type comes back");
        assert_eq!(Arc::as_ptr(&other).cast::<()>(), address);
        let typed = other.downcast_payload::<Payload>().unwrap();
        assert_eq!(Arc::as_ptr(&typed).cast::<()>(), address);
        assert_eq!(Arc::strong_count(&typed), 2);
    }

    struct Buffer(Vec<u8>);

    impl RetainedFootprint for Buffer {
        fn retained_bytes(&self) -> usize {
            self.0.capacity()
        }
    }

    /// Growth under one lock is published at unlock and charged by the next
    /// lock with an account; a shrink is credited the same way; an unlock
    /// with no account defers the charge rather than losing it.
    #[test]
    fn a_metered_mutex_charges_growth_one_lock_late() {
        let metered = MeteredMutex::new(Buffer(Vec::with_capacity(100)));
        let debt = AllocDebt::new();
        assert_eq!(
            metered.retained_bytes(),
            100,
            "the initial size is published"
        );

        metered.lock(&debt).0 = Vec::with_capacity(500);
        assert_eq!(metered.retained_bytes(), 500);
        assert_eq!(debt.balance(), 0, "not charged until the next lock");

        drop(metered.lock(&debt));
        assert_eq!(debt.balance(), 400);

        metered.lock_uncharged().0 = Vec::new();
        assert_eq!(metered.retained_bytes(), 0);
        assert_eq!(
            debt.balance(),
            400,
            "an unlock without an account charges nothing"
        );

        drop(metered.lock(&debt));
        assert_eq!(
            debt.balance(),
            -100,
            "the shrink is credited by the next charge"
        );
    }
}
